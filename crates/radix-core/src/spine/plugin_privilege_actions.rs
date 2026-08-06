//! Plugin-privilege action handlers — the real IO boundary for
//! `agens-plugin-lifecycle.px` (ADR-0037 OQ-1: pares-agens is a
//! special-privilege plugin, analogous to GitHub Copilot inside VS Code).
//!
//! # Why this exists
//!
//! `agens-plugin-lifecycle.px` decides *whether* a plugin activation request
//! qualifies for special-privilege tier (channel ownership, direct model
//! invocation) as pure dataflow, but cannot itself touch PluresDB state or
//! enforce exclusivity — that is Rust plumbing, mirroring
//! [`RepoHealthActionHandler`](crate::spine::repo_health_actions).
//!
//! Three actions:
//! - `plugin_check_privilege` — pure classification: is `plugin_id` present in
//!   the explicit `special_privilege_allowlist`? No implicit elevation
//!   (constraint `special_privilege_requires_explicit_grant`).
//! - `plugin_grant_channel_ownership` — the ONLY side effect that grants a
//!   plugin exclusive ownership of an external channel id. Enforces
//!   `channel_ownership_is_exclusive`: rejects the grant if a *different*
//!   plugin already owns the channel.
//! - `plugin_grant_model_invocation` — grants (or denies) direct
//!   model-invocation authority for a `special` tier plugin, recorded so the
//!   audit trail (`model_invocation_authority_is_logged`) can trace back to
//!   this decision.
//!
//! The `special_privilege_allowlist` config key is durable, PluresDB-backed
//! (via [`StateStore`]) under `config:special_privilege_allowlist`, seeded
//! with exactly `["pares-agens"]` (see [`PluginPrivilegeActionHandler::new`]).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// PluresDB key for the durable special-privilege allowlist.
pub const ALLOWLIST_KEY: &str = "config:special_privilege_allowlist";

/// The seeded default allowlist — exactly `pares-agens` per ADR-0037 OQ-1.
/// No other plugin is granted special-privilege tier implicitly.
pub const DEFAULT_ALLOWLIST: &[&str] = &["pares-agens"];

/// Actions handled by the plugin-privilege handler.
pub const PLUGIN_PRIVILEGE_ACTIONS: &[&str] = &[
    "plugin_check_privilege",
    "plugin_grant_channel_ownership",
    "plugin_grant_model_invocation",
];

/// Check whether an action name is handled by the plugin-privilege handler.
#[must_use]
pub fn is_plugin_privilege_action(action: &str) -> bool {
    PLUGIN_PRIVILEGE_ACTIONS.contains(&action)
}

fn err(action: &str, message: impl Into<String>) -> ExecutionError {
    ExecutionError::ActionFailed {
        action: action.to_string(),
        message: message.into(),
    }
}

/// Plugin-privilege action handler: enforces `special_privilege_requires_explicit_grant`
/// and `channel_ownership_is_exclusive` against a real, durable `StateStore`.
pub struct PluginPrivilegeActionHandler {
    state: Arc<dyn StateStore>,
}

impl PluginPrivilegeActionHandler {
    /// Construct the handler. Seeds `config:special_privilege_allowlist` with
    /// [`DEFAULT_ALLOWLIST`] if it does not already exist in the given store —
    /// callers that share a pre-seeded store (tests, alternate deployments)
    /// are not overwritten.
    pub fn new(state: Arc<dyn StateStore>) -> Self {
        Self { state }
    }

    /// Ensure the allowlist exists in the store, seeding the default if absent.
    /// Returns the current allowlist (existing or freshly seeded).
    async fn ensure_seeded_allowlist(&self) -> Vec<String> {
        if let Some(existing) = self.state.get(ALLOWLIST_KEY).await {
            if let Some(arr) = existing.as_array() {
                return arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
            }
        }
        let seeded: Vec<String> = DEFAULT_ALLOWLIST.iter().map(|s| s.to_string()).collect();
        self.state.set(ALLOWLIST_KEY, json!(seeded.clone())).await;
        seeded
    }

    /// `plugin_check_privilege {plugin_id, allowlist}` — pure classification.
    /// `allowlist` param (if supplied) is honored, but the actual enforcement
    /// authority is the durable store (re-read here) so the allowlist cannot
    /// be spoofed by a caller passing an arbitrary list.
    async fn check_privilege(&self, params: &Value) -> Result<Value, ExecutionError> {
        let plugin_id = params
            .get("plugin_id")
            .and_then(Value::as_str)
            .ok_or_else(|| err("plugin_check_privilege", "missing plugin_id"))?;

        let allowlist = self.ensure_seeded_allowlist().await;
        let is_allowlisted = allowlist.iter().any(|p| p == plugin_id);
        let tier = if is_allowlisted {
            "special"
        } else {
            "ordinary"
        };

        // The `.px` procedure binds this action output directly to `$tier`,
        // so the boundary contract is the tier scalar, not an envelope.
        Ok(json!(tier))
    }

    /// `plugin_grant_channel_ownership {plugin_id, tier, channel_id}` — the
    /// ONLY side effect that grants exclusive channel ownership. Rejects:
    /// - non-`special` tier (constraint `special_privilege_requires_explicit_grant`)
    /// - a channel already owned by a DIFFERENT plugin (`channel_ownership_is_exclusive`)
    ///
    /// Re-granting the SAME channel to the SAME plugin (idempotent re-activation)
    /// is allowed.
    async fn grant_channel_ownership(&self, params: &Value) -> Result<Value, ExecutionError> {
        let plugin_id = params
            .get("plugin_id")
            .and_then(Value::as_str)
            .ok_or_else(|| err("plugin_grant_channel_ownership", "missing plugin_id"))?;
        let tier = params
            .get("tier")
            .and_then(Value::as_str)
            .ok_or_else(|| err("plugin_grant_channel_ownership", "missing tier"))?;
        let channel_id = params
            .get("channel_id")
            .and_then(Value::as_str)
            .ok_or_else(|| err("plugin_grant_channel_ownership", "missing channel_id"))?;

        // Re-verify against the durable allowlist — never trust the caller's
        // `tier` claim alone (defense in depth for special_privilege_requires_explicit_grant).
        let allowlist = self.ensure_seeded_allowlist().await;
        let is_allowlisted = allowlist.iter().any(|p| p == plugin_id);
        if tier != "special" || !is_allowlisted {
            return Ok(json!({
                "granted": false,
                "reason": "plugin is not special-privilege tier (not on allowlist)",
                "plugin_id": plugin_id,
                "channel_id": channel_id,
            }));
        }

        let owner_key = format!("channel_ownership:{channel_id}");
        if let Some(existing) = self.state.get(&owner_key).await {
            let existing_owner = existing.get("plugin_id").and_then(Value::as_str);
            if existing_owner.is_some() && existing_owner != Some(plugin_id) {
                return Ok(json!({
                    "granted": false,
                    "reason": format!(
                        "channel already owned by {}",
                        existing_owner.unwrap_or("unknown")
                    ),
                    "plugin_id": plugin_id,
                    "channel_id": channel_id,
                }));
            }
        }

        let record = json!({
            "plugin_id": plugin_id,
            "channel_id": channel_id,
            "granted_at": now_unix(),
        });
        self.state.set(&owner_key, record.clone()).await;

        Ok(json!({
            "granted": true,
            "plugin_id": plugin_id,
            "channel_id": channel_id,
        }))
    }

    /// `plugin_grant_model_invocation {plugin_id, tier}` — grants direct
    /// model-invocation authority for `special` tier plugins. Records the
    /// grant durably so `model_invocation_authority_is_logged` can be
    /// satisfied by callers that stamp `granted_by_decision_id` on subsequent
    /// invocation events.
    async fn grant_model_invocation(&self, params: &Value) -> Result<Value, ExecutionError> {
        let plugin_id = params
            .get("plugin_id")
            .and_then(Value::as_str)
            .ok_or_else(|| err("plugin_grant_model_invocation", "missing plugin_id"))?;
        let tier = params
            .get("tier")
            .and_then(Value::as_str)
            .ok_or_else(|| err("plugin_grant_model_invocation", "missing tier"))?;

        let allowlist = self.ensure_seeded_allowlist().await;
        let is_allowlisted = allowlist.iter().any(|p| p == plugin_id);
        if tier != "special" || !is_allowlisted {
            return Ok(json!({
                "granted": false,
                "reason": "plugin is not special-privilege tier (not on allowlist)",
                "plugin_id": plugin_id,
            }));
        }

        let decision_id = format!("grant:{plugin_id}:{}", now_unix());
        let key = format!("model_invocation_grant:{plugin_id}");
        let record = json!({
            "plugin_id": plugin_id,
            "granted": true,
            "decision_id": decision_id,
            "granted_at": now_unix(),
        });
        self.state.set(&key, record.clone()).await;

        Ok(json!({
            "granted": true,
            "plugin_id": plugin_id,
            "decision_id": decision_id,
        }))
    }
}

#[async_trait]
impl AsyncActionHandler for PluginPrivilegeActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "plugin_check_privilege" => self.check_privilege(params).await,
            "plugin_grant_channel_ownership" => self.grant_channel_ownership(params).await,
            "plugin_grant_model_invocation" => self.grant_model_invocation(params).await,
            other => Err(ExecutionError::UnknownAction(other.to_string())),
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InMemoryStateStore;

    fn handler() -> PluginPrivilegeActionHandler {
        PluginPrivilegeActionHandler::new(Arc::new(InMemoryStateStore::new()))
    }

    #[tokio::test]
    async fn allowlisted_plugin_gets_special_tier() {
        let h = handler();
        let result = h
            .check_privilege(&json!({"plugin_id": "pares-agens", "allowlist": []}))
            .await
            .unwrap();
        assert_eq!(result, json!("special"));
    }

    #[tokio::test]
    async fn unlisted_plugin_id_rejected() {
        let h = handler();
        let result = h
            .check_privilege(&json!({"plugin_id": "some-random-plugin"}))
            .await
            .unwrap();
        assert_eq!(result, json!("ordinary"));

        // Even if it fakes tier=special, the grant path re-verifies and rejects.
        let grant = h
            .grant_channel_ownership(&json!({
                "plugin_id": "some-random-plugin",
                "tier": "special",
                "channel_id": "telegram"
            }))
            .await
            .unwrap();
        assert_eq!(grant["granted"], json!(false));

        let model_grant = h
            .grant_model_invocation(&json!({
                "plugin_id": "some-random-plugin",
                "tier": "special"
            }))
            .await
            .unwrap();
        assert_eq!(model_grant["granted"], json!(false));
    }

    #[tokio::test]
    async fn allowlisted_plugin_channel_grant_succeeds() {
        let h = handler();
        let grant = h
            .grant_channel_ownership(&json!({
                "plugin_id": "pares-agens",
                "tier": "special",
                "channel_id": "telegram"
            }))
            .await
            .unwrap();
        assert_eq!(grant["granted"], json!(true));
    }

    #[tokio::test]
    async fn two_plugins_cannot_claim_same_channel() {
        let state = Arc::new(InMemoryStateStore::new());
        let h = PluginPrivilegeActionHandler::new(Arc::clone(&state) as Arc<dyn StateStore>);

        // Seed allowlist with two special-tier plugins to isolate the
        // exclusivity check from the allowlist check.
        state
            .set(
                ALLOWLIST_KEY,
                json!(["pares-agens", "second-special-plugin"]),
            )
            .await;

        let first = h
            .grant_channel_ownership(&json!({
                "plugin_id": "pares-agens",
                "tier": "special",
                "channel_id": "telegram"
            }))
            .await
            .unwrap();
        assert_eq!(first["granted"], json!(true));

        let second = h
            .grant_channel_ownership(&json!({
                "plugin_id": "second-special-plugin",
                "tier": "special",
                "channel_id": "telegram"
            }))
            .await
            .unwrap();
        assert_eq!(second["granted"], json!(false));
        assert!(second["reason"].as_str().unwrap().contains("already owned"));
    }

    #[tokio::test]
    async fn same_plugin_can_regrant_same_channel_idempotently() {
        let h = handler();
        let params = json!({
            "plugin_id": "pares-agens",
            "tier": "special",
            "channel_id": "telegram"
        });
        let first = h.grant_channel_ownership(&params).await.unwrap();
        assert_eq!(first["granted"], json!(true));
        let second = h.grant_channel_ownership(&params).await.unwrap();
        assert_eq!(second["granted"], json!(true));
    }

    #[tokio::test]
    async fn allowlist_is_seeded_with_pares_agens_only() {
        let state = Arc::new(InMemoryStateStore::new());
        let h = PluginPrivilegeActionHandler::new(Arc::clone(&state) as Arc<dyn StateStore>);
        let _ = h
            .check_privilege(&json!({"plugin_id": "pares-agens"}))
            .await;

        let stored = state.get(ALLOWLIST_KEY).await.expect("allowlist seeded");
        let arr: Vec<String> = stored
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(arr, vec!["pares-agens".to_string()]);
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
            .join("agens-plugin-lifecycle.px");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let handler: StdArc<dyn AsyncActionHandler> = StdArc::new(Noop);
        let adapters = load_px_procedures(&source, handler)
            .unwrap_or_else(|e| panic!("agens-plugin-lifecycle.px must parse+compile: {e}"));
        assert_eq!(adapters.len(), 3, "expected three procedures to compile");
    }
}
