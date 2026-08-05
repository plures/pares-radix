//! Plugin process supervisor — spawns and governs a privileged plugin
//! (in production: the `pares-agens` binary) as a **separate, supervised
//! child process** of `pares-radix-svc`, per the FIX-2 design in
//! `agens-umbra-integration-nixos-p1` (ADR-0037 OQ-1).
//!
//! # Why a child process, not an in-process crate
//!
//! `pares-agens` is a compiled Rust binary with its own dependency tree.
//! pares-radix's own crates must never gain a Cargo dependency on
//! pares-agens (that would invert the documented `agens-plugin ->
//! pares-radix (as lib)` dependency arrow). So "loading agens as a
//! privileged plugin inside pares-radix-svc" cannot mean linking its crate
//! into this binary — it means pares-radix-svc spawns and supervises the
//! agens binary as a child process, the same way agens already ships
//! today, but now **governs its privileges through the FIX-1a grant
//! handlers** (`plugin_check_privilege` / `plugin_grant_channel_ownership`
//! / `plugin_grant_model_invocation`) instead of letting it run
//! unsupervised with no privilege check at all.
//!
//! # Flow
//!
//! 1. [`PluginSupervisor::spawn`] first calls
//!    [`PluginPrivilegeActionHandler::call`] with `plugin_check_privilege`
//!    for the requested `plugin_id`. If the plugin is not on the durable
//!    `special_privilege_allowlist`, the child process is **never started**
//!    — this is the "unlisted plugin rejected end-to-end" requirement: the
//!    rejection happens before any process exists, not just at the
//!    unit-test level.
//! 2. If allowlisted, it requests a channel-ownership grant and a
//!    model-invocation grant from the same handler (both idempotent — a
//!    child that dies and gets respawned re-requests the same grants and
//!    gets the same durable decision).
//! 3. Only then does it spawn the child process via `tokio::process::Command`,
//!    health-checking that it started (didn't immediately exit).
//! 4. The returned [`SupervisedPlugin`] carries the grant's `decision_id`
//!    as a capability token the caller can pass to the child (e.g. via env
//!    var) so the child can present it as auth context on subsequent calls
//!    into pares-radix-svc's HTTP API. Wiring that presentation into the
//!    real `pares-agens` binary's HTTP client is out of scope for this
//!    stage (see module-level docs / final report for the honest gap).

use std::sync::Arc;
use std::time::Duration;

use pares_radix_core::px_adapter::AsyncActionHandler;
use pares_radix_core::spine::plugin_privilege_actions::PluginPrivilegeActionHandler;
use serde_json::json;
use tokio::process::{Child, Command};
use tracing::{info, warn};

/// A channel id to request ownership of when supervising a plugin. For the
/// real `pares-agens` integration this would be the actual external channel
/// (e.g. `"telegram"`); it is a parameter here so tests can use a distinct
/// value per case without colliding on the shared allowlist/ownership state.
#[derive(Debug, Clone)]
pub struct PluginSpawnRequest {
    /// The plugin identity checked against the durable
    /// `special_privilege_allowlist` (e.g. `"pares-agens"`).
    pub plugin_id: String,
    /// Channel id to request exclusive ownership of on the plugin's behalf.
    pub channel_id: String,
    /// Program to execute (path or name resolved via `PATH`).
    pub program: String,
    /// Arguments passed to the child process.
    pub args: Vec<String>,
}

/// Why a supervised spawn was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("plugin '{0}' is not on the special-privilege allowlist — refusing to spawn")]
    NotPrivileged(String),
    #[error("channel ownership grant denied: {0}")]
    ChannelGrantDenied(String),
    #[error("model invocation grant denied: {0}")]
    ModelGrantDenied(String),
    #[error("failed to spawn child process: {0}")]
    SpawnFailed(String),
    #[error("child process exited immediately (code={0:?}) — failed health check")]
    HealthCheckFailed(Option<i32>),
}

/// A running, privilege-governed child process plus the grant that
/// authorized it.
pub struct SupervisedPlugin {
    pub plugin_id: String,
    pub channel_id: String,
    /// Durable decision id from `plugin_grant_model_invocation`, usable as
    /// a capability token for downstream auth (see module docs).
    pub decision_id: String,
    child: Child,
}

impl SupervisedPlugin {
    /// Whether the child process is still alive (best-effort, non-blocking).
    pub async fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Terminate the supervised child process.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        self.child.kill().await
    }

    /// The OS process id of the supervised child, if still running.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}

/// Spawns and governs privileged plugin child processes through the FIX-1a
/// privilege-grant handlers.
pub struct PluginSupervisor {
    privilege: Arc<PluginPrivilegeActionHandler>,
    /// How long to wait after spawning before checking that the child is
    /// still alive (a crude but real health check for a generic child
    /// process — enough to catch "binary not found" / "crashed on start"
    /// without depending on the child exposing its own health endpoint).
    health_check_delay: Duration,
}

impl PluginSupervisor {
    pub fn new(privilege: Arc<PluginPrivilegeActionHandler>) -> Self {
        Self {
            privilege,
            health_check_delay: Duration::from_millis(150),
        }
    }

    /// Override the health-check delay (tests use a short one).
    pub fn with_health_check_delay(mut self, delay: Duration) -> Self {
        self.health_check_delay = delay;
        self
    }

    /// Spawn and supervise a plugin child process, enforcing privilege
    /// grants before the process is ever started.
    pub async fn spawn(
        &self,
        req: PluginSpawnRequest,
    ) -> Result<SupervisedPlugin, SupervisorError> {
        // Step 1: pure classification. Rejects unlisted plugins before any
        // process exists.
        let tier = self
            .privilege
            .call(
                "plugin_check_privilege",
                &json!({"plugin_id": req.plugin_id}),
            )
            .await
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;
        let tier = tier.as_str().unwrap_or("ordinary").to_string();
        if tier != "special" {
            warn!(plugin_id = %req.plugin_id, tier = %tier, "supervisor: refusing to spawn non-privileged plugin");
            return Err(SupervisorError::NotPrivileged(req.plugin_id.clone()));
        }

        // Step 2: channel ownership grant.
        let channel_grant = self
            .privilege
            .call(
                "plugin_grant_channel_ownership",
                &json!({
                    "plugin_id": req.plugin_id,
                    "tier": tier,
                    "channel_id": req.channel_id,
                }),
            )
            .await
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;
        if channel_grant.get("granted") != Some(&serde_json::Value::Bool(true)) {
            let reason = channel_grant
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("channel ownership grant denied")
                .to_string();
            return Err(SupervisorError::ChannelGrantDenied(reason));
        }

        // Step 3: model-invocation grant.
        let model_grant = self
            .privilege
            .call(
                "plugin_grant_model_invocation",
                &json!({"plugin_id": req.plugin_id, "tier": tier}),
            )
            .await
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;
        if model_grant.get("granted") != Some(&serde_json::Value::Bool(true)) {
            let reason = model_grant
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("model invocation grant denied")
                .to_string();
            return Err(SupervisorError::ModelGrantDenied(reason));
        }
        let decision_id = model_grant
            .get("decision_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Step 4: only now spawn the child process, carrying the grant's
        // decision id so it can present it as a capability token.
        info!(plugin_id = %req.plugin_id, program = %req.program, decision_id = %decision_id, "supervisor: spawning privileged plugin child process");
        let mut child = Command::new(&req.program)
            .args(&req.args)
            .env("RADIX_PLUGIN_GRANT_DECISION_ID", &decision_id)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;

        // Step 5: minimal real health check — the process must still be
        // running a short interval after spawn (catches "binary missing" /
        // "crashed immediately" without requiring the child to expose its
        // own health endpoint).
        tokio::time::sleep(self.health_check_delay).await;
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                return Err(SupervisorError::HealthCheckFailed(status.code()));
            }
            Err(e) => return Err(SupervisorError::SpawnFailed(e.to_string())),
        }

        Ok(SupervisedPlugin {
            plugin_id: req.plugin_id,
            channel_id: req.channel_id,
            decision_id,
            child,
        })
    }
}
