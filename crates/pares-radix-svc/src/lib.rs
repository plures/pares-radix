//! `pares-radix-svc` — headless service runtime for pares-radix.
//!
//! Implements ADR-0018 (`docs/adr/ADR-0018-radix-runtime-as-service.md`):
//! a thin binary crate that drives the existing `AgensRuntime`
//! (`poll_events` / `process_due_timers`) on a fixed tick, persists
//! exclusively through PluresDB (`CrdtStore` + `SledStorage`, no new
//! storage layer), and exposes a minimal loopback HTTP automation surface
//! for status/manual-trigger use. No Tauri, no Hyperswarm, no LLM
//! dependency — this crate only depends on `pares-radix-core` and the
//! `pluresdb*` crates it already vendors.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use pares_radix_core::px_adapter::{load_px_procedures, AsyncActionHandler, PxProcedureAdapter};
use pares_radix_core::spine::plugin_privilege_actions::PluginPrivilegeActionHandler;
use pares_radix_core::state::InMemoryStateStore;
use pares_radix_praxis::px::executor::ExecutionError;
use pluresdb::{CrdtStore, MemoryStorage, SledStorage, StorageEngine};
use pluresdb_procedures::agens::{AgensEvent, AgensRuntime, TimerEntry, TimerTrigger};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Notify;
use tracing::{error, info, warn};

use crate::supervisor::{PluginSpawnRequest, PluginSupervisor};

/// Actor name this service uses when writing CRDT nodes.
pub const SERVICE_ACTOR: &str = "pares-radix-svc";

pub mod supervisor;

/// Configuration for [`ServiceLifecycle`].
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Loopback bind address for the HTTP automation surface.
    ///
    /// Per ADR-0018 §4: default `127.0.0.1:8730`. Binding to a non-loopback
    /// address without a bearer token is refused (fail closed) — see
    /// [`ServiceConfig::validate`].
    pub bind_addr: SocketAddr,
    /// On-disk PluresDB data directory. `None` uses an in-memory store
    /// (for tests / dry-runs).
    pub data_dir: Option<PathBuf>,
    /// Scheduler tick interval.
    pub tick_interval: Duration,
    /// Bearer token required when `bind_addr` is not loopback.
    pub auth_token: Option<String>,
    /// Grace period for in-flight work during a drain (SIGTERM/Ctrl-C).
    pub drain_grace: Duration,
    /// Path (or bare name resolved via `PATH`) to the `pares-agens` binary
    /// to supervise as a special-privilege plugin child process. `None`
    /// (the default) means agens supervision is disabled — the service
    /// runs its own scheduler/HTTP surface without spawning any plugin
    /// child, which is the correct behavior for environments (tests, CI,
    /// minimal deployments) that don't ship agens alongside radix.
    pub agens_plugin_path: Option<PathBuf>,
    /// Channel id the supervised agens plugin requests exclusive ownership
    /// of. Only meaningful when `agens_plugin_path` is set.
    pub agens_channel_id: String,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8730".parse().expect("valid default addr"),
            data_dir: None,
            tick_interval: Duration::from_secs(5),
            auth_token: None,
            drain_grace: Duration::from_secs(10),
            agens_plugin_path: None,
            agens_channel_id: "agens".to_string(),
        }
    }
}

impl ServiceConfig {
    /// Build a config from environment variables, falling back to defaults.
    ///
    /// - `RADIX_SVC_BIND_ADDR` (default `127.0.0.1:8730`)
    /// - `RADIX_SVC_DATA_DIR` (default: in-memory store)
    /// - `RADIX_SVC_TICK_SECS` (default `5`)
    /// - `RADIX_SVC_AUTH_TOKEN` (required if bind addr is non-loopback)
    /// - `RADIX_SVC_AGENS_PLUGIN_PATH` (optional: path to the `pares-agens`
    ///   binary to supervise as a special-privilege plugin child process;
    ///   unset disables agens supervision entirely). Mirrors the existing
    ///   `RADIX_SVC_*`-prefixed, env-driven external-path config pattern
    ///   already used for `RADIX_SVC_DATA_DIR` — no new config mechanism
    ///   introduced.
    /// - `RADIX_SVC_AGENS_CHANNEL_ID` (default `agens`)
    pub fn from_env() -> anyhow::Result<Self> {
        let mut cfg = Self::default();
        if let Ok(addr) = std::env::var("RADIX_SVC_BIND_ADDR") {
            cfg.bind_addr = addr.parse()?;
        }
        if let Ok(dir) = std::env::var("RADIX_SVC_DATA_DIR") {
            cfg.data_dir = Some(PathBuf::from(dir));
        }
        if let Ok(secs) = std::env::var("RADIX_SVC_TICK_SECS") {
            cfg.tick_interval = Duration::from_secs(secs.parse()?);
        }
        cfg.auth_token = std::env::var("RADIX_SVC_AUTH_TOKEN").ok();
        if let Ok(path) = std::env::var("RADIX_SVC_AGENS_PLUGIN_PATH") {
            cfg.agens_plugin_path = Some(PathBuf::from(path));
        }
        if let Ok(channel_id) = std::env::var("RADIX_SVC_AGENS_CHANNEL_ID") {
            cfg.agens_channel_id = channel_id;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// Enforce the hard gate from ADR-0018 §4: refuse to bind to a
    /// non-loopback address without a configured bearer token.
    pub fn validate(&self) -> anyhow::Result<()> {
        let is_loopback = self.bind_addr.ip().is_loopback();
        if !is_loopback && self.auth_token.is_none() {
            anyhow::bail!(
                "refusing to bind non-loopback address {} without RADIX_SVC_AUTH_TOKEN set",
                self.bind_addr
            );
        }
        Ok(())
    }
}

/// Explicit lifecycle state, mirroring ADR-0018 §3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Starting,
    Running,
    Degraded,
    Draining,
}

/// Shared state handed to every HTTP handler and the scheduler task.
///
/// `runtime` holds a single, long-lived `AgensRuntime` for the process
/// (rather than constructing a fresh one per call) because handler
/// registration lives in an in-process `RwLock<HashMap<..>>` on the
/// `AgensRuntime` instance itself, not in the CRDT store — a fresh instance
/// per call would silently drop any registered procedure handlers. The
/// `&'static CrdtStore` is obtained via a one-time `Box::leak`, which is
/// safe and intentional here: exactly one `CrdtStore` is created per
/// process for the lifetime of the service (there is no dynamic
/// creation/teardown of stores within a running process).
struct SharedState {
    store: &'static CrdtStore,
    runtime: AgensRuntime<'static>,
    ready: AtomicBool,
    draining: AtomicBool,
    tick_count: AtomicU64,
    last_tick_error: Mutex<Option<String>>,
    started_at: DateTime<Utc>,
    /// Live capability grants issued by [`supervisor::PluginSupervisor`] to
    /// supervised plugin children, keyed by `decision_id` (the value handed
    /// to the child as `RADIX_PLUGIN_GRANT_DECISION_ID`) -> `plugin_id`.
    /// This is the reverse-auth side of FIX-2: an incoming HTTP request that
    /// claims to be acting on behalf of a supervised plugin must present the
    /// matching `decision_id` via the `X-Radix-Plugin-Grant` header (see
    /// [`PluginGrantAuth`]); anything not in this map (never granted, or
    /// revoked because the child was shut down / crashed) is rejected.
    active_plugin_grants: Mutex<HashMap<String, String>>,
    /// Whether the last attempt to supervise-spawn the configured agens
    /// plugin child failed. Surfaced via `/healthz`/`/readyz` as `Degraded`
    /// rather than crashing the service — see FIX-3 startup wiring policy
    /// documented on [`ServiceLifecycle::run`].
    agens_supervision_error: Mutex<Option<String>>,
}

impl SharedState {
    fn new(store: CrdtStore) -> Self {
        let store: &'static CrdtStore = Box::leak(Box::new(store));
        let runtime = AgensRuntime::new(store, SERVICE_ACTOR);
        Self {
            store,
            runtime,
            ready: AtomicBool::new(false),
            draining: AtomicBool::new(false),
            tick_count: AtomicU64::new(0),
            last_tick_error: Mutex::new(None),
            started_at: Utc::now(),
            active_plugin_grants: Mutex::new(HashMap::new()),
            agens_supervision_error: Mutex::new(None),
        }
    }

    /// Record a live grant issued to a supervised plugin child so incoming
    /// requests presenting `decision_id` via `X-Radix-Plugin-Grant` can be
    /// authenticated. Called once the supervised child has passed its
    /// health check.
    fn record_plugin_grant(&self, decision_id: String, plugin_id: String) {
        self.active_plugin_grants
            .lock()
            .unwrap()
            .insert(decision_id, plugin_id);
    }

    /// Revoke a previously recorded grant (child shut down / crashed / was
    /// never healthy). Subsequent requests presenting this `decision_id`
    /// are rejected.
    fn revoke_plugin_grant(&self, decision_id: &str) {
        self.active_plugin_grants.lock().unwrap().remove(decision_id);
    }

    /// Look up the `plugin_id` a live, ungranted-since-revoked `decision_id`
    /// was issued to, or `None` if it was never granted or has since been
    /// revoked.
    fn plugin_id_for_grant(&self, decision_id: &str) -> Option<String> {
        self.active_plugin_grants
            .lock()
            .unwrap()
            .get(decision_id)
            .cloned()
    }

    fn runtime(&self) -> &AgensRuntime<'static> {
        &self.runtime
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    fn is_draining(&self) -> bool {
        self.draining.load(Ordering::SeqCst)
    }

    fn state(&self) -> LifecycleState {
        if self.is_draining() {
            LifecycleState::Draining
        } else if !self.is_ready() {
            LifecycleState::Starting
        } else if self.last_tick_error.lock().unwrap().is_some() {
            LifecycleState::Degraded
        } else {
            LifecycleState::Running
        }
    }
}

/// The service lifecycle: owns the store, the scheduler loop, and the HTTP
/// server, and drives them all to completion or clean shutdown.
pub struct ServiceLifecycle {
    config: ServiceConfig,
    shared: Arc<SharedState>,
}

impl ServiceLifecycle {
    /// Construct the lifecycle (`Starting` state): open the PluresDB store.
    ///
    /// Per ADR-0018 §3.1, a failure here is fatal — the caller should exit
    /// non-zero and let the process supervisor restart per its own policy.
    pub fn new(config: ServiceConfig) -> anyhow::Result<Self> {
        config.validate()?;

        let store = match &config.data_dir {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                let storage: Arc<dyn StorageEngine> =
                    Arc::new(SledStorage::open(dir).map_err(|e| {
                        anyhow::anyhow!("failed to open SledStorage at {dir:?}: {e}")
                    })?);
                CrdtStore::default().with_persistence(storage)
            }
            None => {
                let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
                CrdtStore::default().with_persistence(storage)
            }
        };

        let shared = Arc::new(SharedState::new(store));

        info!(bind_addr = %config.bind_addr, data_dir = ?config.data_dir, "pares-radix-svc: Starting");
        Ok(Self { config, shared })
    }

    /// Handle to the underlying CRDT store (mostly for tests).
    pub fn store(&self) -> &'static CrdtStore {
        self.shared.store
    }

    /// Run the service until `shutdown` resolves (SIGTERM/Ctrl-C in
    /// `main.rs`, or a test-controlled future in integration tests).
    ///
    /// # Agens plugin supervision (FIX-3)
    ///
    /// If `config.agens_plugin_path` is set, this spawns the configured
    /// binary as a privilege-governed child via [`PluginSupervisor`] (the
    /// same real spawn/grant/health-check flow validated in
    /// `tests/supervisor_integration.rs`), using `"pares-agens"` as the
    /// `plugin_id` checked against the durable
    /// `special_privilege_allowlist` and `config.agens_channel_id` as the
    /// channel it requests ownership of.
    ///
    /// **Restart policy (explicit decision, not silently assumed):** this
    /// is a *log-and-surface-unhealthy*, not a bounded-retry restart loop.
    /// There is no existing precedent elsewhere in this codebase for
    /// process-level retry/backoff supervision (the scheduler's tick loop
    /// retries on the *next tick*, which is a fixed-interval poll, not a
    /// crash-restart loop for an external process) — introducing one here
    /// would be new, unreviewed process-supervision policy bolted onto an
    /// unrelated stage. If the child fails to spawn/pass its health check,
    /// or dies later, the failure/exit is logged and reflected in
    /// `agens_supervision_error` (visible via `/healthz` as `Degraded`);
    /// the service itself keeps running its own scheduler/HTTP surface
    /// rather than crash-looping. A bounded-retry policy is a legitimate
    /// follow-up but needs its own design decision (backoff shape, retry
    /// cap, what "too many restarts" means) — named here as an explicit
    /// gap rather than guessed at.
    pub async fn run(
        self,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let (addr, server_task) = self.spawn_http_server().await?;
        info!(%addr, "pares-radix-svc: HTTP automation surface listening");

        let scheduler_shared = Arc::clone(&self.shared);
        let tick_interval = self.config.tick_interval;
        let scheduler_task = tokio::spawn(run_scheduler(scheduler_shared, tick_interval));

        let mut supervised_plugin = None;
        if let Some(program) = self.config.agens_plugin_path.clone() {
            let privilege = Arc::new(PluginPrivilegeActionHandler::new(Arc::new(
                InMemoryStateStore::new(),
            )));
            let supervisor = PluginSupervisor::new(privilege);
            let req = PluginSpawnRequest {
                plugin_id: "pares-agens".to_string(),
                channel_id: self.config.agens_channel_id.clone(),
                program: program.to_string_lossy().to_string(),
                args: Vec::new(),
            };
            match supervisor.spawn(req).await {
                Ok(plugin) => {
                    info!(
                        plugin_id = %plugin.plugin_id,
                        decision_id = %plugin.decision_id,
                        pid = ?plugin.pid(),
                        "pares-radix-svc: agens plugin supervised and healthy"
                    );
                    self.shared
                        .record_plugin_grant(plugin.decision_id.clone(), plugin.plugin_id.clone());
                    supervised_plugin = Some(plugin);
                }
                Err(e) => {
                    error!(error = %e, program = %program.display(), "pares-radix-svc: failed to supervise agens plugin, continuing without it");
                    *self.shared.agens_supervision_error.lock().unwrap() = Some(e.to_string());
                }
            }
        }

        shutdown.await;

        // Draining: stop new mutating work, let in-flight ticks/requests
        // finish up to the configured grace deadline, then flush & exit.
        self.shared.draining.store(true, Ordering::SeqCst);
        info!(
            "pares-radix-svc: Draining (grace={:?})",
            self.config.drain_grace
        );

        let drained = tokio::time::timeout(self.config.drain_grace, async {
            if let Some(mut plugin) = supervised_plugin {
                self.shared.revoke_plugin_grant(&plugin.decision_id);
                let _ = plugin.shutdown().await;
            }
            scheduler_task.abort();
            let _ = scheduler_task.await;
            server_task.abort();
            let _ = server_task.await;
        })
        .await;

        if drained.is_err() {
            warn!("pares-radix-svc: drain grace period exceeded, forcing shutdown");
        }

        info!("pares-radix-svc: Stopped");
        Ok(())
    }

    /// Bind the HTTP listener and return its (possibly OS-assigned) address
    /// plus the running server task, without starting the scheduler or
    /// waiting for shutdown. Exposed for tests that want a live server
    /// without driving the full `run()` lifecycle.
    pub async fn spawn_http_server(
        &self,
    ) -> anyhow::Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
        let router = build_router(Arc::clone(&self.shared));
        let listener = tokio::net::TcpListener::bind(self.config.bind_addr).await?;
        let addr = listener.local_addr()?;
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!(error = %e, "pares-radix-svc: HTTP server task exited with error");
            }
        });
        Ok((addr, task))
    }

    /// Spawn just the scheduler loop against this lifecycle's store,
    /// returning its join handle. Exposed for integration tests.
    pub fn spawn_scheduler(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(run_scheduler(
            Arc::clone(&self.shared),
            self.config.tick_interval,
        ))
    }
}

/// One tick: poll events (informational, at-least-once, logged) and process
/// due timers. Errors are recorded as `Degraded`, never panic the process —
/// per ADR-0018 §3.1 the tick loop retries on the next interval rather than
/// crash-looping on a transient store hiccup.
async fn run_scheduler(shared: Arc<SharedState>, tick_interval: Duration) {
    let mut ticker = tokio::time::interval(tick_interval);
    loop {
        ticker.tick().await;
        if shared.is_draining() {
            break;
        }

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let runtime = shared.runtime();
            let now = Utc::now();
            let fired = runtime.process_due_timers(now);
            let events = runtime.poll_events(now - chrono::Duration::seconds(1));
            (fired, events.len())
        }));

        match outcome {
            Ok((fired, polled)) => {
                *shared.last_tick_error.lock().unwrap() = None;
                shared.tick_count.fetch_add(1, Ordering::SeqCst);
                shared.ready.store(true, Ordering::SeqCst);
                if fired > 0 || polled > 0 {
                    info!(
                        fired_timers = fired,
                        polled_events = polled,
                        "pares-radix-svc: tick"
                    );
                }
            }
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic in scheduler tick".to_string());
                warn!(error = %msg, "pares-radix-svc: scheduler tick failed, will retry next interval");
                *shared.last_tick_error.lock().unwrap() = Some(msg);
                shared.tick_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP automation surface
// ---------------------------------------------------------------------------

/// Header a supervised plugin child presents to prove it holds a live
/// capability grant issued by [`supervisor::PluginSupervisor`] (FIX-3
/// reverse-auth wiring). The value must equal the `decision_id` recorded in
/// [`SharedState::active_plugin_grants`] at spawn time (see
/// [`ServiceLifecycle::run`]).
const PLUGIN_GRANT_HEADER: &str = "X-Radix-Plugin-Grant";

/// Middleware enforcing that requests to privileged endpoints (currently
/// `/v1/ssh/authorize`; extend the route list in [`build_router`] if more
/// privileged endpoints are added) present a live plugin grant.
///
/// This is deliberately conservative for v1: **any** request missing the
/// header, or presenting a `decision_id` not currently recorded as a live
/// grant (never issued, or revoked because the owning child was shut down /
/// crashed / never started), is rejected with 401. There is no notion yet
/// of "privileged endpoints reachable without a plugin grant" (e.g. a human
/// operator calling `/v1/ssh/authorize` directly) — if that's a real use
/// case, it needs its own explicit bypass, not an implicit one. Named here
/// as an honest scope boundary rather than silently assumed.
async fn require_plugin_grant(
    State(shared): State<Arc<SharedState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let decision_id = request
        .headers()
        .get(PLUGIN_GRANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let Some(decision_id) = decision_id else {
        warn!("pares-radix-svc: rejected privileged request missing {PLUGIN_GRANT_HEADER}");
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": format!("missing {PLUGIN_GRANT_HEADER} header")})),
        )
            .into_response();
    };

    match shared.plugin_id_for_grant(&decision_id) {
        Some(plugin_id) => {
            info!(plugin_id = %plugin_id, decision_id = %decision_id, "pares-radix-svc: privileged request authenticated via plugin grant");
            next.run(request).await
        }
        None => {
            warn!(decision_id = %decision_id, "pares-radix-svc: rejected privileged request with unknown/revoked plugin grant");
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "grant is not live (never issued, or revoked)"})),
            )
                .into_response()
        }
    }
}

fn build_router(shared: Arc<SharedState>) -> Router {
    let privileged = Router::new()
        .route("/v1/ssh/authorize", post(post_ssh_authorize))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&shared),
            require_plugin_grant,
        ));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/events", get(get_events).post(post_event))
        .route("/timers", get(list_timers).post(post_timer))
        .route("/timers/{id}", delete(delete_timer))
        .merge(privileged)
        .with_state(shared)
}

async fn healthz(State(shared): State<Arc<SharedState>>) -> impl IntoResponse {
    let state = shared.state();
    (
        StatusCode::OK,
        Json(json!({
            "state": state,
            "started_at": shared.started_at,
            "ticks": shared.tick_count.load(Ordering::SeqCst),
        })),
    )
}

async fn readyz(State(shared): State<Arc<SharedState>>) -> impl IntoResponse {
    let ready = shared.is_ready() && shared.last_tick_error.lock().unwrap().is_none();
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "ready": ready,
            "state": shared.state(),
            "last_tick_error": *shared.last_tick_error.lock().unwrap(),
        })),
    )
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<DateTime<Utc>>,
}

async fn get_events(
    State(shared): State<Arc<SharedState>>,
    Query(q): Query<EventsQuery>,
) -> impl IntoResponse {
    let since = q
        .since
        .unwrap_or_else(|| Utc::now() - chrono::Duration::hours(1));
    let runtime = shared.runtime();
    let events = runtime.poll_events(since);
    Json(json!({ "events": events }))
}

async fn post_event(
    State(shared): State<Arc<SharedState>>,
    Json(event): Json<AgensEvent>,
) -> impl IntoResponse {
    if shared.is_draining() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "draining"})),
        )
            .into_response();
    }
    let runtime = shared.runtime();
    let node_id = runtime.emit_event(&event);
    (StatusCode::CREATED, Json(json!({ "node_id": node_id }))).into_response()
}

async fn list_timers(State(shared): State<Arc<SharedState>>) -> impl IntoResponse {
    let runtime = shared.runtime();
    let timers: Vec<TimerEntry> = runtime.timers().list();
    Json(json!({ "timers": timers }))
}

#[derive(Deserialize)]
struct NewTimerRequest {
    name: String,
    trigger: TimerTrigger,
    interval_secs: Option<u64>,
    cron_expr: Option<String>,
    run_at: Option<DateTime<Utc>>,
    #[serde(default = "default_payload")]
    payload: serde_json::Value,
}

fn default_payload() -> serde_json::Value {
    serde_json::Value::Null
}

async fn post_timer(
    State(shared): State<Arc<SharedState>>,
    Json(req): Json<NewTimerRequest>,
) -> impl IntoResponse {
    if shared.is_draining() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "draining"})),
        )
            .into_response();
    }
    let runtime = shared.runtime();
    let timers = runtime.timers();
    let result = match req.trigger {
        TimerTrigger::Interval => {
            let secs = req.interval_secs.unwrap_or(60);
            Ok(timers.schedule_interval(&req.name, secs, req.payload))
        }
        TimerTrigger::Cron => match req.cron_expr.as_deref() {
            Some(expr) => timers
                .schedule_cron(&req.name, expr, req.payload)
                .map_err(|e| e.to_string()),
            None => Err("cron_expr required for cron trigger".to_string()),
        },
        TimerTrigger::Once => {
            let run_at = req.run_at.unwrap_or_else(Utc::now);
            Ok(timers.schedule_once(&req.name, run_at, req.payload))
        }
    };
    match result {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

async fn delete_timer(
    State(shared): State<Arc<SharedState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let runtime = shared.runtime();
    let cancelled = runtime.timers().cancel(&id);
    if cancelled {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// Wire contract consumed by `jit-ssh-jitd`'s `PolicyClient`.
#[derive(Debug, Deserialize)]
struct SshAuthorizeRequest {
    pubkey: String,
    target_host: String,
    role: String,
    user: String,
}

/// A response is always explicit: `allowed` is never inferred from HTTP status.
#[derive(Debug, Serialize)]
struct SshAuthorizeResponse {
    allowed: bool,
    ttl_seconds: Option<u64>,
    principals: Vec<String>,
    extensions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deny_reason: Option<String>,
}

impl SshAuthorizeResponse {
    fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            ttl_seconds: None,
            principals: Vec::new(),
            extensions: Vec::new(),
            deny_reason: Some(reason.into()),
        }
    }

    fn from_policy(value: serde_json::Value) -> Self {
        let Some(obj) = value.as_object() else {
            return Self::deny("policy procedure returned a malformed decision");
        };
        if obj.get("allowed") != Some(&serde_json::Value::Bool(true)) {
            return Self::deny(
                obj.get("deny_reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("policy denied authorization"),
            );
        }
        let ttl_seconds = obj.get("ttl_seconds").and_then(serde_json::Value::as_u64);
        let principals = json_string_list(obj.get("principals"));
        let extensions = match obj.get("extensions") {
            None | Some(serde_json::Value::Null) => Some(Vec::new()),
            value => json_string_list(value),
        };
        match (ttl_seconds, principals, extensions) {
            (Some(ttl), Some(principals), Some(extensions))
                if ttl > 0 && !principals.is_empty() =>
            {
                Self {
                    allowed: true,
                    ttl_seconds: Some(ttl),
                    principals,
                    extensions,
                    deny_reason: None,
                }
            }
            _ => Self::deny("policy produced an incomplete authorization grant"),
        }
    }
}

fn json_string_list(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    value?
        .as_array()?
        .iter()
        .map(|item| {
            let value = item.as_str()?;
            (!value.is_empty()).then(|| value.to_owned())
        })
        .collect()
}

/// Action bridge for the `authorize_ssh` PX procedure.  It owns all parsing of
/// persisted policy data, while the PX program owns the authorize/deny flow.
struct SshAuthorizePxActions {
    store: &'static CrdtStore,
}

#[async_trait::async_trait]
impl AsyncActionHandler for SshAuthorizePxActions {
    async fn call(
        &self,
        name: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, ExecutionError> {
        let error = |message: &str| ExecutionError::ActionFailed {
            action: name.to_owned(),
            message: message.to_owned(),
        };
        match name {
            "db_get" => {
                let role = params
                    .get("role")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| error("missing role"))?;
                Ok(self
                    .store
                    .get(format!("policy:global:ssh-authorize-role:{role}"))
                    .map(|record| record.data)
                    .unwrap_or(serde_json::Value::Null))
            }
            "check_user_in_allowlist" => {
                let policy = params.get("policy").and_then(serde_json::Value::as_object);
                let user = params.get("user").and_then(serde_json::Value::as_str);
                let Some((policy, user)) = policy.zip(user) else {
                    return Ok(json!({"allowed": false}));
                };
                let value = policy.get("value").and_then(serde_json::Value::as_object);
                let valid_kind =
                    policy.get("kind").and_then(serde_json::Value::as_str) == Some("setting");
                let allowed_users = value
                    .and_then(|value| value.get("allowed_users"))
                    .and_then(|v| json_string_list(Some(v)));
                let principals = value
                    .and_then(|value| value.get("principals"))
                    .and_then(|v| json_string_list(Some(v)));
                let extensions = match value.and_then(|value| value.get("extensions")) {
                    None | Some(serde_json::Value::Null) => Some(Vec::new()),
                    extension_value => json_string_list(extension_value),
                };
                match (valid_kind, allowed_users, principals, extensions) {
                    (true, Some(users), Some(principals), Some(extensions))
                        if users.iter().any(|entry| entry == user) && !principals.is_empty() =>
                    {
                        Ok(
                            json!({"allowed": true, "principals": principals, "extensions": extensions}),
                        )
                    }
                    _ => Ok(json!({"allowed": false})),
                }
            }
            "resolve_grant_ttl" => {
                let ttl = params
                    .get("policy")
                    .and_then(|policy| policy.get("value"))
                    .and_then(|value| value.get("ttl_seconds"))
                    .and_then(serde_json::Value::as_u64)
                    .filter(|ttl| *ttl > 0);
                match ttl {
                    Some(ttl_seconds) => Ok(json!({"ttl_seconds": ttl_seconds})),
                    None => Err(error("policy is missing a positive ttl_seconds")),
                }
            }
            other => Err(ExecutionError::UnknownAction(other.to_owned())),
        }
    }
}

async fn post_ssh_authorize(
    State(shared): State<Arc<SharedState>>,
    Json(req): Json<SshAuthorizeRequest>,
) -> impl IntoResponse {
    if shared.is_draining() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SshAuthorizeResponse::deny("service is draining")),
        )
            .into_response();
    }

    let handler: Arc<dyn AsyncActionHandler> = Arc::new(SshAuthorizePxActions {
        store: shared.store,
    });
    let adapter: Option<PxProcedureAdapter> = load_px_procedures(
        include_str!("../../../praxis/procedures/ssh-authorize.px"),
        handler,
    )
    .ok()
    .and_then(|procedures| {
        procedures
            .into_iter()
            .find(|procedure| procedure.name() == "authorize_ssh")
    });
    let Some(adapter) = adapter else {
        return Json(SshAuthorizeResponse::deny(
            "ssh authorization policy is unavailable",
        ))
        .into_response();
    };
    let vars = HashMap::from([
        ("pubkey".to_owned(), json!(req.pubkey)),
        ("target_host".to_owned(), json!(req.target_host)),
        ("role".to_owned(), json!(req.role)),
        ("user".to_owned(), json!(req.user)),
    ]);
    let response = match adapter.execute_with_vars(vars).await {
        Ok(result) => result
            .step_results
            .last()
            .and_then(|step| step.output.clone())
            .map(SshAuthorizeResponse::from_policy)
            .unwrap_or_else(|| SshAuthorizeResponse::deny("policy returned no decision")),
        Err(error) => {
            warn!(error = %error, "ssh authorization policy execution failed");
            SshAuthorizeResponse::deny("policy execution failed")
        }
    };
    Json(response).into_response()
}

/// Wait for a Ctrl-C signal (used by `main.rs`); split out so tests can
/// supply their own shutdown future instead.
pub async fn ctrl_c_shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

/// A shutdown handle a test (or a future admin endpoint) can trigger
/// programmatically instead of waiting on a real OS signal.
#[derive(Clone, Default)]
pub struct ShutdownHandle(Arc<Notify>);

impl ShutdownHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trigger(&self) {
        self.0.notify_waiters();
    }

    pub async fn wait(&self) {
        self.0.notified().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ServiceConfig {
        ServiceConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            data_dir: None,
            tick_interval: Duration::from_millis(50),
            auth_token: None,
            drain_grace: Duration::from_secs(2),
            agens_plugin_path: None,
            agens_channel_id: "agens".to_string(),
        }
    }

    #[test]
    fn validate_refuses_nonloopback_without_token() {
        let cfg = ServiceConfig {
            bind_addr: "0.0.0.0:8730".parse().unwrap(),
            ..ServiceConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_allows_nonloopback_with_token() {
        let cfg = ServiceConfig {
            bind_addr: "0.0.0.0:8730".parse().unwrap(),
            auth_token: Some("secret".to_string()),
            ..ServiceConfig::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_allows_loopback_without_token() {
        let cfg = ServiceConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[tokio::test]
    async fn lifecycle_new_opens_in_memory_store() {
        let lifecycle = ServiceLifecycle::new(test_config()).unwrap();
        lifecycle
            .store()
            .put("probe", SERVICE_ACTOR, json!({"ok": true}));
        assert!(lifecycle.store().get("probe").is_some());
    }

    #[test]
    fn build_router_accepts_current_axum_path_syntax() {
        let shared = Arc::new(SharedState::new(CrdtStore::default()));
        let _router = build_router(shared);
    }

    #[tokio::test]
    async fn scheduler_flips_ready_after_first_tick() {
        let shared = Arc::new(SharedState::new(CrdtStore::default()));
        assert!(!shared.is_ready());
        let handle = tokio::spawn(run_scheduler(
            Arc::clone(&shared),
            Duration::from_millis(10),
        ));
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(
            shared.is_ready(),
            "scheduler should flip ready after first tick"
        );
        assert!(shared.tick_count.load(Ordering::SeqCst) > 0);
        shared.draining.store(true, Ordering::SeqCst);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn scheduler_fires_due_once_timer() {
        let shared = Arc::new(SharedState::new(CrdtStore::default()));

        // Schedule a one-shot timer due immediately, and register a handler
        // that records firing via a CRDT node so the test can assert on it
        // without racing a channel/atomic captured only in the closure.
        {
            let runtime = shared.runtime();
            let store = shared.store;
            runtime.register_procedure(
                "timer",
                Arc::new(move |event: &AgensEvent| {
                    if let AgensEvent::Timer { name, .. } = event {
                        if name == "fires-once" {
                            store.put("test:timer-fired", SERVICE_ACTOR, json!({"fired": true}));
                        }
                    }
                    Ok(())
                }),
            );
            runtime
                .timers()
                .schedule_once("fires-once", Utc::now(), json!({}));
        }

        let handle = tokio::spawn(run_scheduler(
            Arc::clone(&shared),
            Duration::from_millis(10),
        ));
        tokio::time::sleep(Duration::from_millis(80)).await;
        shared.draining.store(true, Ordering::SeqCst);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;

        assert!(
            shared.store.get("test:timer-fired").is_some(),
            "expected the due one-shot timer to have fired within the tick window"
        );
    }
}
