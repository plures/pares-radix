//! GUI-launch-mode action handler — the real IO boundary for
//! `gui-launch-policy.px` (ADR-0037 OQ-2: GUI launches on-demand by default,
//! user-configurable via a durable config toggle for autostart).
//!
//! # Why this exists
//!
//! `gui-launch-policy.px` decides which launch mode applies as pure dataflow,
//! but cannot itself read the durable `config:gui_launch_mode` toggle or
//! apply the OQ-2 default — that is Rust plumbing, mirroring
//! [`RepoHealthActionHandler`](crate::spine::repo_health_actions) and
//! [`PluginPrivilegeActionHandler`](crate::spine::plugin_privilege_actions).
//!
//! Only `gui_resolve_launch_mode` is implemented here (per FIX-1a scope):
//! `gui_launch_process` (actually spawning the Tauri binary) and
//! `gui_poll_backend_ready` (the `/readyz` poll) are separate Rust/OS-level
//! side effects not part of this slice — they remain "not wired" and MUST
//! NOT be fabricated (C-NOSTUB-001).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// PluresDB key for the durable GUI launch-mode configuration toggle.
pub const LAUNCH_MODE_CONFIG_KEY: &str = "config:gui_launch_mode";

/// Default launch mode per ADR-0037 OQ-2 — on-demand unless explicitly
/// overridden by the user.
pub const DEFAULT_LAUNCH_MODE: &str = "on_demand";

/// Valid launch mode values.
const VALID_MODES: &[&str] = &["on_demand", "autostart"];

/// Actions handled by the GUI-launch handler.
pub const GUI_LAUNCH_ACTIONS: &[&str] = &["gui_resolve_launch_mode"];

/// Check whether an action name is handled by the GUI-launch handler.
#[must_use]
pub fn is_gui_launch_action(action: &str) -> bool {
    GUI_LAUNCH_ACTIONS.contains(&action)
}

fn err(action: &str, message: impl Into<String>) -> ExecutionError {
    ExecutionError::ActionFailed {
        action: action.to_string(),
        message: message.into(),
    }
}

/// GUI-launch action handler: resolves launch mode from durable config,
/// defaulting to on-demand per OQ-2 (`on_demand_is_default` constraint).
pub struct GuiLaunchActionHandler {
    state: Arc<dyn StateStore>,
}

impl GuiLaunchActionHandler {
    #[must_use]
    pub fn new(state: Arc<dyn StateStore>) -> Self {
        Self { state }
    }

    /// `gui_resolve_launch_mode {trigger, configured_mode}` — resolve the
    /// effective launch mode. The `configured_mode` param (read by the .px
    /// procedure via `pluresdb_read`) is honored if it is one of the valid
    /// modes; otherwise (missing/unset/malformed) the OQ-2 default applies
    /// and — if the config key was entirely absent — it is durably seeded
    /// with the default so the config surface is discoverable, without ever
    /// silently overwriting an explicit user setting.
    async fn resolve_launch_mode(&self, params: &Value) -> Result<Value, ExecutionError> {
        let trigger = params
            .get("trigger")
            .and_then(Value::as_str)
            .ok_or_else(|| err("gui_resolve_launch_mode", "missing trigger"))?;

        // Re-read the durable config directly — never trust a caller-supplied
        // `configured_mode` alone, mirroring the allowlist re-verification
        // pattern in plugin_privilege_actions.
        let existing = self.state.get(LAUNCH_MODE_CONFIG_KEY).await;
        let mode = match existing.as_ref().and_then(Value::as_str) {
            Some(m) if VALID_MODES.contains(&m) => m.to_string(),
            Some(_) | None => {
                if existing.is_none() {
                    // Seed the config surface with the default so it's
                    // discoverable, but this is NOT "explicitly set" — the
                    // on_demand_is_default constraint's alternate clause
                    // (mode == on_demand) is satisfied directly instead.
                    self.state
                        .set(LAUNCH_MODE_CONFIG_KEY, json!(DEFAULT_LAUNCH_MODE))
                        .await;
                }
                DEFAULT_LAUNCH_MODE.to_string()
            }
        };

        // The `.px` procedure binds this result directly to `$mode`, then
        // persists it and passes it to `gui_launch_process`; it must therefore
        // be the scalar mode, not an implementation-detail envelope.
        let _ = trigger;
        Ok(json!(mode))
    }
}

#[async_trait]
impl AsyncActionHandler for GuiLaunchActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "gui_resolve_launch_mode" => self.resolve_launch_mode(params).await,
            other => Err(ExecutionError::UnknownAction(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InMemoryStateStore;

    fn handler() -> (GuiLaunchActionHandler, Arc<InMemoryStateStore>) {
        let store = Arc::new(InMemoryStateStore::new());
        (
            GuiLaunchActionHandler::new(Arc::clone(&store) as Arc<dyn StateStore>),
            store,
        )
    }

    #[tokio::test]
    async fn defaults_to_on_demand_when_unconfigured() {
        let (h, _store) = handler();
        let result = h
            .resolve_launch_mode(&json!({"trigger": "login"}))
            .await
            .unwrap();
        assert_eq!(result, json!("on_demand"));
    }

    #[tokio::test]
    async fn seeds_config_key_on_first_resolution() {
        let (h, store) = handler();
        let _ = h
            .resolve_launch_mode(&json!({"trigger": "login"}))
            .await
            .unwrap();
        let seeded = store.get(LAUNCH_MODE_CONFIG_KEY).await;
        assert_eq!(seeded, Some(json!("on_demand")));
    }

    #[tokio::test]
    async fn honors_explicit_autostart_override() {
        let (h, store) = handler();
        store.set(LAUNCH_MODE_CONFIG_KEY, json!("autostart")).await;
        let result = h
            .resolve_launch_mode(&json!({"trigger": "login"}))
            .await
            .unwrap();
        assert_eq!(result, json!("autostart"));
    }

    #[tokio::test]
    async fn malformed_config_falls_back_to_default_without_overwriting() {
        let (h, store) = handler();
        store.set(LAUNCH_MODE_CONFIG_KEY, json!("bogus_mode")).await;
        let result = h
            .resolve_launch_mode(&json!({"trigger": "login"}))
            .await
            .unwrap();
        assert_eq!(result, json!("on_demand"));
        // Malformed value is left untouched — we never overwrite existing config.
        let stored = store.get(LAUNCH_MODE_CONFIG_KEY).await;
        assert_eq!(stored, Some(json!("bogus_mode")));
    }

    #[test]
    fn full_px_procedure_parses_and_compiles() {
        use crate::px_adapter::load_px_procedures;
        use std::sync::Arc as StdArc;

        struct Noop;
        #[async_trait::async_trait]
        impl AsyncActionHandler for Noop {
            async fn call(&self, name: &str, _p: &Value) -> Result<Value, ExecutionError> {
                Err(ExecutionError::UnknownAction(name.to_string()))
            }
        }

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("praxis")
            .join("procedures")
            .join("gui-launch-policy.px");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let handler: StdArc<dyn AsyncActionHandler> = StdArc::new(Noop);
        let adapters = load_px_procedures(&source, handler)
            .unwrap_or_else(|e| panic!("gui-launch-policy.px must parse+compile: {e}"));
        assert_eq!(adapters.len(), 3, "expected three procedures to compile");
    }
}
