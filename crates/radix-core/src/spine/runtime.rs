//! Runtime assembly — wires the `.px` procedure engine into the live spine.
//!
//! This module is the **assembly seam** that turns the individually-tested
//! spine components (the `.px` engine, [`ReactiveRegistry`], the
//! [`CompositeActionHandler`], and the [`PluresDbStateStore`]) into a single
//! running system. Before this module existed, every piece was real and
//! covered by tests, but **nothing assembled them**:
//! [`CompositeActionHandler::new`] and
//! [`register_reactive_procedures`](crate::spine::bootstrap::register_reactive_procedures)
//! had zero non-test callers, so no shipped path actually executed `.px`.
//!
//! [`build_reactive_runtime`] is the first real (non-test) caller of both.
//!
//! # What it assembles
//!
//! ```text
//!  praxis/procedures/*.px ─load─▶ PxProcedureAdapter ─register─▶ ReactiveRegistry
//!                                                                      ▲
//!  PluresDbStateStore ─┐                                               │ on_write
//!                      ├─▶ CoreActionHandler ─┐                        │
//!  ConversationStore ──┘                      ├─▶ CompositeActionHandler
//!  ToolDispatcher ─────▶ ToolDispatchHandler ─┘            (handler the
//!                                                          procedures call)
//!
//!  Pipeline::with_reactive(registry)  ──run──▶  event loop drives on_write
//! ```
//!
//! The pipeline event loop (`Pipeline::run`) already calls
//! `reactive.on_write(...)` for every event (see `pipeline.rs`); this module
//! supplies the registry that loop fires into, populated with the real `.px`
//! procedures and a handler backed by the durable [`StateStore`].
//!
//! # Path resolution (no dev-only absolutes)
//!
//! * **State dir** — [`resolve_state_dir`]: `RADIX_STATE_DIR` env override,
//!   otherwise `<system-temp>/pares-radix/state`. Always overridable; the
//!   default is a stable, writable, per-user location.
//! * **Praxis dir** — [`resolve_praxis_dir`]: `RADIX_PRAXIS_DIR` env override,
//!   otherwise `./praxis/procedures` relative to the current working directory
//!   (the in-repo / bundled-resource layout). No hardcoded developer path.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use crate::model::ToolDispatcher;
use crate::px_adapter::ToolDispatchActionHandler;
use crate::spine::actions::CompositeActionHandler;
use crate::spine::bootstrap::register_reactive_procedures;
use crate::spine::conversation::ConversationStore;
use crate::spine::pipeline::Pipeline;
use crate::spine::reactive::ReactiveRegistry;
use crate::state::{PluresDbStateStore, StateStore};

/// Environment variable that overrides the durable state directory.
pub const STATE_DIR_ENV: &str = "RADIX_STATE_DIR";

/// Environment variable that overrides the `.px` procedure directory.
pub const PRAXIS_DIR_ENV: &str = "RADIX_PRAXIS_DIR";

/// Resolve the durable state directory.
///
/// Honors `RADIX_STATE_DIR` if set and non-empty; otherwise defaults to
/// `<system-temp>/pares-radix/state`. The directory is created if absent.
pub fn resolve_state_dir() -> PathBuf {
    let dir = match std::env::var(STATE_DIR_ENV) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => std::env::temp_dir().join("pares-radix").join("state"),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!(dir = %dir.display(), error = %e, "runtime: failed to create state dir");
    }
    dir
}

/// Resolve the `.px` procedure directory.
///
/// Honors `RADIX_PRAXIS_DIR` if set and non-empty; otherwise defaults to
/// `./praxis/procedures` relative to the current working directory.
pub fn resolve_praxis_dir() -> PathBuf {
    match std::env::var(PRAXIS_DIR_ENV) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => PathBuf::from("praxis").join("procedures"),
    }
}

/// The assembled reactive runtime: a [`ReactiveRegistry`] populated with the
/// repo's `.px` procedures, wired to a [`Pipeline`] whose event loop drives
/// `on_write`, backed by a durable [`StateStore`].
///
/// Hold onto [`state_store`](Self::state_store) to read/write durable state
/// directly; call [`spawn`](Self::spawn) to start the pipeline event loop.
pub struct ReactiveRuntime {
    /// The reactive registry (`.px` triggers). Shared with the pipeline.
    pub registry: Arc<ReactiveRegistry>,
    /// The spine pipeline (event bus) wired with the registry.
    pub pipeline: Arc<Pipeline>,
    /// The durable state store backing `read_state`/`write_state`.
    pub state_store: Arc<dyn StateStore>,
    /// Number of `.px` procedures registered as reactive triggers.
    pub registered: usize,
    /// The pipeline's inbound event receiver (consumed by [`spawn`]).
    rx: Option<tokio::sync::mpsc::Receiver<crate::spine::event::SpineEvent>>,
}

impl ReactiveRuntime {
    /// Spawn the pipeline event loop on the current tokio runtime.
    ///
    /// Returns the [`JoinHandle`](tokio::task::JoinHandle) for the loop. After
    /// this, emitting events into the pipeline (or writing keys via the
    /// registry's `on_write`) fires matching `.px` procedures.
    pub fn spawn(&mut self) -> tokio::task::JoinHandle<()> {
        let rx = self
            .rx
            .take()
            .expect("ReactiveRuntime::spawn called more than once");
        let pipeline = Arc::clone(&self.pipeline);
        tokio::spawn(async move {
            pipeline.run(rx).await;
        })
    }
}

/// Assemble the reactive `.px` runtime from real components.
///
/// This is the production assembly path and the **first non-test caller** of
/// [`CompositeActionHandler::new`] and
/// [`register_reactive_procedures`](crate::spine::bootstrap::register_reactive_procedures).
///
/// # Arguments
///
/// * `state_store`   — durable key/value store backing `read_state`/`write_state`.
/// * `conversation_store` — store backing `read_history`/`append_history`.
/// * `tool_dispatcher` — dispatcher for any non-core action (tool calls). May be
///   wired lazily; pass a real dispatcher here so procedure tool-calls work.
/// * `praxis_dir`    — directory of `.px` files to load (see [`resolve_praxis_dir`]).
/// * `capacity`      — pipeline event-channel buffer capacity.
///
/// # Returns
///
/// A [`ReactiveRuntime`] with the registry populated and the pipeline wired.
/// Call [`ReactiveRuntime::spawn`] to start driving events.
pub async fn build_reactive_runtime(
    state_store: Arc<dyn StateStore>,
    conversation_store: Arc<dyn ConversationStore>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    praxis_dir: &Path,
    capacity: usize,
) -> ReactiveRuntime {
    // 1. Tool handler — bridges `.px` action calls that aren't core/lifecycle
    //    into the tool dispatch pipeline.
    let tool_handler = Arc::new(ToolDispatchActionHandler::new(tool_dispatcher));

    // 2. The composite handler the procedures invoke. CoreActionHandler is now
    //    backed by the durable state store (read_state/write_state round-trip
    //    through PluresDB, not a stub).
    let composite = Arc::new(CompositeActionHandler::new(
        Arc::clone(&conversation_store),
        Arc::clone(&state_store),
        tool_handler,
    ));

    // 3. Build the registry and load every `.px` procedure against it.
    let registry = Arc::new(ReactiveRegistry::new());
    let registered = register_reactive_procedures(praxis_dir, &registry, composite).await;
    info!(
        registered,
        praxis_dir = %praxis_dir.display(),
        "runtime: reactive .px procedures registered against live registry"
    );

    // 4. Wire the pipeline to the registry and give the registry an emitter so
    //    procedure-emitted events can re-enter the pipeline.
    let (pipeline, rx) = Pipeline::with_reactive(capacity, Arc::clone(&registry));
    registry.set_emitter(pipeline.emitter()).await;

    ReactiveRuntime {
        registry,
        pipeline,
        state_store,
        registered,
        rx: Some(rx),
    }
}

/// Convenience constructor that opens a durable [`PluresDbStateStore`] at the
/// resolved [`resolve_state_dir`] location and co-locates the conversation
/// store in the same CRDT store, then calls [`build_reactive_runtime`] with the
/// `.px` directory from [`resolve_praxis_dir`].
///
/// This is what a shipped runtime driver (e.g. the cognition `serve` loop or a
/// Tauri integration) calls at startup. Errors only if the durable store can't
/// be opened.
pub async fn build_default_reactive_runtime(
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    capacity: usize,
) -> Result<ReactiveRuntime, String> {
    let state_dir = resolve_state_dir();
    let praxis_dir = resolve_praxis_dir();

    let pdb = PluresDbStateStore::open(&state_dir)
        .map_err(|e| format!("open state store at {}: {e}", state_dir.display()))?;
    // Co-locate conversation history in the same CRDT store as agent state.
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(
        crate::spine::conversation::PluresConversationStore::new(pdb.crdt_store()),
    );
    let state_store: Arc<dyn StateStore> = Arc::new(pdb);

    Ok(build_reactive_runtime(
        state_store,
        conversation_store,
        tool_dispatcher,
        &praxis_dir,
        capacity,
    )
    .await)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ToolDefinition, ToolDispatcher};
    use crate::spine::conversation::MemoryConversationStore;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::time::Duration;
    use tempfile::TempDir;

    /// A dispatcher that records nothing and returns null — the runtime's
    /// `.px` procedures under test use only core `write_state`/`read_state`,
    /// which route to the CoreActionHandler, NOT the dispatcher.
    struct NullDispatcher;

    #[async_trait]
    impl ToolDispatcher for NullDispatcher {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        async fn call_tool(&self, _name: &str, _args: Value) -> String {
            "null".to_string()
        }
    }

    fn dispatcher() -> Arc<dyn ToolDispatcher> {
        Arc::new(NullDispatcher)
    }

    #[test]
    fn resolve_state_dir_honors_env() {
        // SAFETY: single-threaded test; we set then immediately read.
        // Build the override with the OS-native separator so the assertion is
        // portable: a hardcoded "C:\\..\\state" string is a SINGLE path
        // component on Linux (backslash is not a separator there), which made
        // `ends_with("state")` fail in CI. Use a real joined path instead.
        let override_dir = std::env::temp_dir().join("radix-override").join("state");
        std::env::set_var(STATE_DIR_ENV, &override_dir);
        let dir = resolve_state_dir();
        assert!(dir.ends_with("state"));
        assert_eq!(dir, override_dir);
        std::env::remove_var(STATE_DIR_ENV);
    }

    #[test]
    fn resolve_praxis_dir_default_is_relative() {
        std::env::remove_var(PRAXIS_DIR_ENV);
        let dir = resolve_praxis_dir();
        assert!(dir.ends_with(Path::new("praxis").join("procedures")));
    }

    /// END-TO-END PROOF (assembled path, real handler + real store):
    ///
    /// Build the real [`CompositeActionHandler`] + a real [`PluresDbStateStore`]
    /// via [`build_reactive_runtime`], register a real `.px` procedure that, on
    /// `on_write`, calls `write_state` to persist a derived node, then fire the
    /// pipeline's reactive `on_write` and read the persisted node back out of
    /// the SAME durable store.
    ///
    /// This exercises: write → reactive trigger match → `.px` execution →
    /// `write_state` effect through the real CoreActionHandler → durable
    /// PluresDB node → read-back. No NoOpHandler, no mock store.
    #[tokio::test]
    async fn end_to_end_write_triggers_px_procedure_persists_state() {
        let tmp = TempDir::new().unwrap();
        let praxis = tmp.path().join("procedures");
        std::fs::create_dir_all(&praxis).unwrap();

        // A minimal REAL procedure. trigger: on_write → registered under
        // "task_request:*" via the bootstrap trigger map (name == plan_task is
        // mapped there), but we give it a distinct name + explicit pattern via
        // its declared trigger kind. To keep it deterministic we name it so the
        // default-trigger fallback registers it under "on_write:*" — then we
        // fire an "on_write:..." key directly.
        //
        // The procedure reads the reactive-provided `$value` (NOT `$new_value`,
        // which the reactive binder does not set) and writes a node keyed by it.
        std::fs::write(
            praxis.join("proof.px"),
            r#"procedure persist_proof:
  trigger: on_write
  given: "Persist a node derived from the write that triggered us"
  write_state {key: "proof:landed", value: $value} -> $written
  return {ok: true}
"#,
        )
        .unwrap();

        // Real durable store (on-disk sled under the temp dir).
        let pdb = PluresDbStateStore::open(tmp.path().join("state")).unwrap();
        let state_store: Arc<dyn StateStore> = Arc::new(pdb);
        let conversation_store: Arc<dyn ConversationStore> =
            Arc::new(MemoryConversationStore::new());

        let runtime = build_reactive_runtime(
            Arc::clone(&state_store),
            conversation_store,
            dispatcher(),
            &praxis,
            16,
        )
        .await;

        // The proof procedure must have registered (trigger kind on_write →
        // "on_write:*" fallback pattern).
        assert!(
            runtime.registered >= 1,
            "expected the real .px procedure to register, got {}",
            runtime.registered
        );

        // Fire the registry exactly as the pipeline event loop does.
        let payload = json!({"task": "wire-px-runtime", "n": 42});
        runtime
            .registry
            .on_write("on_write:proof-1", &payload)
            .await;

        // The procedure executes on a spawned task; poll the durable store for
        // the node it should have written.
        let mut landed: Option<Value> = None;
        for _ in 0..50 {
            if let Some(v) = state_store.get("proof:landed").await {
                if !v.is_null() {
                    landed = Some(v);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let landed = landed.expect(
            "procedure-driven write_state did not land a durable node — \
             the assembled write→.px→state path is not working",
        );
        assert_eq!(landed["task"], "wire-px-runtime");
        assert_eq!(landed["n"], 42);
    }
}
