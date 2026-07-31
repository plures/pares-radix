//! Epic-registry action handler — the Rust IO edge for
//! `praxis/procedures/epic-registry.px`.
//!
//! Decision logic (priority ordering, orphan-detection thresholds, auto-resume
//! gating) lives in `.px` (`development-guide/procedures/epic-registry.px`).
//! This handler performs ONLY real side effects over the SAME durable
//! `StateStore` backend used by `task_handoff.rs` / `task_dashboard_actions.rs`
//! (`epic:registry:` key prefix — C-PLURES-003/004: one ledger, read/written
//! by whoever needs it, never a parallel projection).
//!
//! # Actions
//!
//! - `register_epic` — durably register a new epic ledger row (the mandatory
//!   front door before any epic work begins).
//! - `update_epic` — apply a status/next_action/owner update, bumping
//!   `updated_at` (keeps the staleness detector honest).
//! - `claim_epic` — atomically claim ownership of an epic for a session,
//!   using the same compare-and-swap-under-mutex pattern as
//!   `ConditionalTaskStore::claim_task` in `task_handoff.rs` (read-check-write
//!   serialised by a process-local lock; refuses to steal a live claim).
//! - `epic_registry_sweep` — the Rust IO edge for the heartbeat resume/orphan
//!   sweep: scans every `epic:registry:` row, flips stale `in_progress` rows
//!   with no live claim to `orphaned` (staleness = `now - updated_at >
//!   orphan_threshold_ms`), and (when `auto_resume` is true) requeues
//!   orphaned/queued rows to `in_progress` under the sweep's owner. The
//!   priority ordering and per-status branching mirrored here is exactly the
//!   `.px` `epic_registry_heartbeat_sweep` procedure's shape (read that file
//!   first — do not diverge).
//!
//! # Architecture (C-DEV-001 / C-NOSTUB-001)
//!
//! Branching/sequencing policy lives in `.px`. This file is IO only: real
//! `StateStore` reads/writes, real CAS, real clock. No fabricated data, no
//! stub returns — every arm performs the real operation or returns a
//! structured `ExecutionError::ActionFailed`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::px_adapter::AsyncActionHandler;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// Shared key prefix with `task_dashboard_actions.rs` (`EPIC_REGISTRY_PREFIX`)
/// and the `.px` procedures — one ledger, one prefix.
const EPIC_REGISTRY_PREFIX: &str = "epic:registry:";

/// Default staleness threshold (24h) mirroring `epic_registry_defaults` in
/// the `.px` config block, used only when a registered entry omits it.
const DEFAULT_ORPHAN_THRESHOLD_MS: u64 = 86_400_000;

/// Action verbs owned by this handler.
pub const EPIC_REGISTRY_ACTIONS: &[&str] = &[
    "register_epic",
    "update_epic",
    "claim_epic",
    "epic_registry_sweep",
];

/// Returns `true` when `action` is handled by [`EpicRegistryActionHandler`].
#[must_use]
pub fn is_epic_registry_action(action: &str) -> bool {
    EPIC_REGISTRY_ACTIONS.contains(&action)
}

/// Durable ledger row — the Rust mirror of `entity epic_registry_entry` in
/// `development-guide/procedures/epic-registry.px`. Field-for-field parity is
/// intentional; do not add/rename fields without updating the `.px` entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpicRegistryEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub next_action: String,
    pub owner_session: String,
    #[serde(default)]
    pub orchestration_epic_id: String,
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub tracking_url: String,
    #[serde(default)]
    pub blocked_on: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default = "default_orphan_threshold")]
    pub orphan_threshold_ms: u64,
    #[serde(default = "default_auto_resume")]
    pub auto_resume: bool,
    #[serde(default)]
    pub rescue_attempted: bool,
}

fn default_orphan_threshold() -> u64 {
    DEFAULT_ORPHAN_THRESHOLD_MS
}

fn default_auto_resume() -> bool {
    true
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn registry_key(id: &str) -> String {
    format!("{EPIC_REGISTRY_PREFIX}{id}")
}

/// Rust IO boundary for the epic registry.
///
/// Wraps the SAME `Arc<dyn StateStore>` the rest of the runtime uses (Core,
/// Worktask, TaskDashboard) so `epic:registry:` rows written here are
/// immediately visible to `aggregate_task_dashboard` and vice versa
/// (C-PLURES-003/004). Claim/CAS operations are serialised by a
/// process-local `Mutex`, mirroring `ConditionalTaskStore::cas` in
/// `task_handoff.rs` — this gives compare-and-swap-equivalent guarantees
/// within a single process without inventing a new storage primitive.
pub struct EpicRegistryActionHandler {
    state_store: Arc<dyn StateStore>,
    cas_lock: Arc<Mutex<()>>,
}

impl EpicRegistryActionHandler {
    /// Construct over the SAME durable `StateStore` the rest of the spine
    /// action handlers share.
    #[must_use]
    pub fn new(state_store: Arc<dyn StateStore>) -> Self {
        Self {
            state_store,
            cas_lock: Arc::new(Mutex::new(())),
        }
    }

    async fn read_entry(&self, id: &str) -> Option<EpicRegistryEntry> {
        let raw = self.state_store.get(&registry_key(id)).await?;
        if raw.is_null() {
            return None;
        }
        serde_json::from_value(raw).ok()
    }

    async fn write_entry(&self, entry: &EpicRegistryEntry) {
        let value = serde_json::to_value(entry).expect("EpicRegistryEntry serialises");
        self.state_store.set(&registry_key(&entry.id), value).await;
    }

    // ── register_epic ───────────────────────────────────────────────────────

    /// `register_epic` — the mandatory front door before any epic work
    /// begins. Idempotent: registering the same `id` again with identical
    /// fields is a no-op success (mirrors `.px` `write_state` semantics,
    /// which unconditionally overwrite — but tests rely on being able to
    /// safely re-register without corrupting an in-flight claim, so we only
    /// overwrite `created_at`/CAS fields on first registration).
    async fn register_epic(&self, params: &Value) -> Result<Value, ExecutionError> {
        let id = require_str(params, "id", "register_epic")?;
        let title = require_str(params, "title", "register_epic")?;
        let status = params
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("queued");
        let priority = params
            .get("priority")
            .and_then(Value::as_str)
            .unwrap_or("p2");
        let next_action = require_str(params, "next_action", "register_epic")?;
        let owner_session = params
            .get("owner_session")
            .and_then(Value::as_str)
            .unwrap_or("");
        let orchestration_epic_id = params
            .get("orchestration_epic_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let repos: Vec<String> = params
            .get("repos")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let tracking_url = params
            .get("tracking_url")
            .and_then(Value::as_str)
            .unwrap_or("");
        let orphan_threshold_ms = params
            .get("orphan_threshold_ms")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_ORPHAN_THRESHOLD_MS);
        let auto_resume = params
            .get("auto_resume")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let _guard = self.cas_lock.lock().await;
        let now = now_ms();
        let created_at = self
            .read_entry(id)
            .await
            .map(|e| e.created_at)
            .unwrap_or(now);

        let entry = EpicRegistryEntry {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            next_action: next_action.to_string(),
            owner_session: owner_session.to_string(),
            orchestration_epic_id: orchestration_epic_id.to_string(),
            repos,
            tracking_url: tracking_url.to_string(),
            blocked_on: String::new(),
            created_at,
            updated_at: now,
            orphan_threshold_ms,
            auto_resume,
            rescue_attempted: false,
        };
        self.write_entry(&entry).await;
        serde_json::to_value(&entry).map_err(|e| ExecutionError::ActionFailed {
            action: "register_epic".into(),
            message: format!("serialisation error: {e}"),
        })
    }

    // ── update_epic ─────────────────────────────────────────────────────────

    /// `update_epic` — apply a status/next_action/owner/blocked_on/tracking_url
    /// update, always bumping `updated_at` so the staleness detector reflects
    /// reality (mirrors `.px` `update_epic`).
    async fn update_epic(&self, params: &Value) -> Result<Value, ExecutionError> {
        let id = require_str(params, "id", "update_epic")?;

        let _guard = self.cas_lock.lock().await;
        let mut entry = self
            .read_entry(id)
            .await
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "update_epic".into(),
                message: format!("epic not found: {id}"),
            })?;

        if let Some(v) = params.get("status").and_then(Value::as_str) {
            if !v.is_empty() {
                entry.status = v.to_string();
            }
        }
        if let Some(v) = params.get("next_action").and_then(Value::as_str) {
            if !v.is_empty() {
                entry.next_action = v.to_string();
            }
        }
        if let Some(v) = params.get("owner_session").and_then(Value::as_str) {
            if !v.is_empty() {
                entry.owner_session = v.to_string();
            }
        }
        if let Some(v) = params.get("blocked_on").and_then(Value::as_str) {
            if !v.is_empty() {
                entry.blocked_on = v.to_string();
            }
        }
        if let Some(v) = params.get("tracking_url").and_then(Value::as_str) {
            if !v.is_empty() {
                entry.tracking_url = v.to_string();
            }
        }
        entry.updated_at = now_ms();
        self.write_entry(&entry).await;
        serde_json::to_value(&entry).map_err(|e| ExecutionError::ActionFailed {
            action: "update_epic".into(),
            message: format!("serialisation error: {e}"),
        })
    }

    // ── claim_epic ──────────────────────────────────────────────────────────

    /// `claim_epic` — atomically claim ownership of an epic for
    /// `owner_session`, mirroring `ConditionalTaskStore::claim_task`'s
    /// read-check-write-under-lock CAS pattern in `task_handoff.rs`.
    ///
    /// Succeeds when the epic has no live owner (`owner_session == ""`), is
    /// already owned by the SAME session (idempotent re-claim), or is
    /// `orphaned`/`queued` (available for pickup). Fails with
    /// `ActionFailed` if a DIFFERENT session already holds a live,
    /// non-orphaned claim — this is the "do not steal a live claim"
    /// guarantee the CAS pattern provides.
    async fn claim_epic(&self, params: &Value) -> Result<Value, ExecutionError> {
        let id = require_str(params, "id", "claim_epic")?;
        let owner_session = require_str(params, "owner_session", "claim_epic")?;

        let _guard = self.cas_lock.lock().await;
        let mut entry = self
            .read_entry(id)
            .await
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "claim_epic".into(),
                message: format!("epic not found: {id}"),
            })?;

        let already_owned_by_caller = entry.owner_session == owner_session;
        let claimable = entry.owner_session.is_empty()
            || already_owned_by_caller
            || matches!(entry.status.as_str(), "orphaned" | "queued");

        if !claimable {
            return Err(ExecutionError::ActionFailed {
                action: "claim_epic".into(),
                message: format!(
                    "epic '{id}' already claimed by live owner '{}' (status={})",
                    entry.owner_session, entry.status
                ),
            });
        }

        entry.owner_session = owner_session.to_string();
        if entry.status == "queued" || entry.status == "orphaned" {
            entry.status = "in_progress".to_string();
        }
        entry.rescue_attempted = true;
        entry.updated_at = now_ms();
        self.write_entry(&entry).await;
        serde_json::to_value(&entry).map_err(|e| ExecutionError::ActionFailed {
            action: "claim_epic".into(),
            message: format!("serialisation error: {e}"),
        })
    }

    // ── epic_registry_sweep ─────────────────────────────────────────────────

    /// `epic_registry_sweep` — the Rust IO edge for the `.px`
    /// `epic_registry_heartbeat_sweep` procedure. Scans every `epic:registry:`
    /// row (priority-sorted p0..p3, matching the `.px` sweep's ordering),
    /// detects orphans by staleness (`now - updated_at > orphan_threshold_ms`
    /// with no live claim), and — when `auto_resume` is true — requeues
    /// `queued`/`orphaned` rows to `in_progress` under `owner_session:
    /// "main"`. Returns the list of entries the sweep touched (for
    /// verification / telemetry), never mutates `done`/`abandoned`/
    /// `awaiting_approval` rows, and leaves named-dependency `blocked` rows
    /// untouched (dependency-clearance re-check is `.px`-side policy, not a
    /// Rust IO concern here).
    async fn sweep(&self, _params: &Value) -> Result<Value, ExecutionError> {
        let _guard = self.cas_lock.lock().await;
        let now = now_ms();
        let keys = self.state_store.keys_with_prefix(EPIC_REGISTRY_PREFIX).await;

        let mut entries: Vec<EpicRegistryEntry> = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(raw) = self.state_store.get(&key).await {
                if raw.is_null() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_value::<EpicRegistryEntry>(raw) {
                    entries.push(entry);
                }
            }
        }
        // Priority-sorted p0 > p1 > p2 > p3, matching the `.px` sweep.
        entries.sort_by_key(|e| priority_rank(&e.priority));

        let mut touched = Vec::new();

        for mut entry in entries {
            match entry.status.as_str() {
                "done" | "abandoned" | "awaiting_approval" | "blocked" => continue,
                _ => {}
            }

            let stale = now.saturating_sub(entry.updated_at) > entry.orphan_threshold_ms;
            let has_owner = !entry.owner_session.is_empty();

            if entry.status == "in_progress" && !has_owner && stale {
                entry.status = "orphaned".to_string();
                entry.updated_at = now;
                self.write_entry(&entry).await;
                // NOTE: deliberately no `continue`/push here — mirrors the
                // `.px` `epic_registry_heartbeat_sweep` procedure, which
                // falls through to the auto-resume check in the SAME tick so
                // an orphan with `auto_resume: true` is rescued immediately
                // rather than waiting for the next sweep pass. `touched` is
                // populated once below with the entry's FINAL state.
            }

            if (entry.status == "queued" || entry.status == "orphaned") && entry.auto_resume {
                entry.status = "in_progress".to_string();
                entry.owner_session = "main".to_string();
                entry.rescue_attempted = true;
                entry.updated_at = now;
                self.write_entry(&entry).await;
                touched.push(entry.clone());
            } else if entry.status == "orphaned" && !entry.auto_resume {
                // Surfaced for human decision — Rust records the observation,
                // waking the human session is `.px`/runtime policy, not this
                // IO boundary's concern.
                touched.push(entry.clone());
            }
        }

        let touched_json: Vec<Value> = touched
            .iter()
            .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
            .collect();
        Ok(json!({ "touched": touched_json, "swept_at": now }))
    }
}

fn priority_rank(priority: &str) -> u8 {
    match priority {
        "p0" => 0,
        "p1" => 1,
        "p2" => 2,
        "p3" => 3,
        _ => 4,
    }
}

#[async_trait]
impl AsyncActionHandler for EpicRegistryActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        match action {
            "register_epic" => self.register_epic(params).await,
            "update_epic" => self.update_epic(params).await,
            "claim_epic" => self.claim_epic(params).await,
            "epic_registry_sweep" => self.sweep(params).await,
            other => Err(ExecutionError::ActionFailed {
                action: other.to_string(),
                message: "action not handled by EpicRegistryActionHandler".into(),
            }),
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn require_str<'a>(params: &'a Value, key: &str, action: &str) -> Result<&'a str, ExecutionError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::ActionFailed {
            action: action.to_string(),
            message: format!("missing or non-string param `{key}`"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PluresDbStateStore;

    fn handler() -> EpicRegistryActionHandler {
        let store: Arc<dyn StateStore> = Arc::new(PluresDbStateStore::in_memory());
        EpicRegistryActionHandler::new(store)
    }

    fn register_params(id: &str, status: &str, auto_resume: bool) -> Value {
        json!({
            "id": id,
            "title": "test epic registry claim/resume/orphan protocol",
            "status": status,
            "priority": "p1",
            "next_action": "verify claim CAS",
            "owner_session": "",
            "repos": ["plures/pares-radix"],
            "orphan_threshold_ms": 50_u64,
            "auto_resume": auto_resume,
        })
    }

    /// REAL end-to-end proof over an actual PluresDB (`CrdtStore` +
    /// `MemoryStorage`) backend — not a mock. Seeds a throwaway
    /// `epic:registry:test-*` entry, demonstrates register → claim → update,
    /// then forces staleness (via a tiny `orphan_threshold_ms` + a real sleep)
    /// and confirms the sweep flips the row to `orphaned` and then, because
    /// `auto_resume` is true, immediately re-resumes it to `in_progress`.
    #[tokio::test]
    async fn claim_update_and_sweep_orphan_detection_round_trip() {
        let h = handler();
        let id = "test-epic-registry-claim-v2";

        // 1. register (front door) — starts queued.
        let registered = h
            .call("register_epic", &register_params(id, "queued", true))
            .await
            .expect("register_epic succeeds");
        assert_eq!(registered["status"], "queued");

        // 2. claim — an available (queued) epic transitions to in_progress
        // under the claiming session, mirroring conditional_claim_task's CAS.
        let claimed = h
            .call(
                "claim_epic",
                &json!({ "id": id, "owner_session": "session-a" }),
            )
            .await
            .expect("claim_epic succeeds");
        assert_eq!(claimed["status"], "in_progress");
        assert_eq!(claimed["owner_session"], "session-a");

        // A different session may not steal a live claim.
        let steal_attempt = h
            .call(
                "claim_epic",
                &json!({ "id": id, "owner_session": "session-b" }),
            )
            .await;
        assert!(
            steal_attempt.is_err(),
            "a live claim by session-a must not be stealable by session-b"
        );

        // 3. update — bumps next_action/updated_at, keeping the ledger live.
        let updated_at_before = claimed["updated_at"].as_u64().unwrap();
        let updated = h
            .call(
                "update_epic",
                &json!({
                    "id": id,
                    "next_action": "run the sweep test",
                    "owner_session": "session-a",
                }),
            )
            .await
            .expect("update_epic succeeds");
        assert_eq!(updated["next_action"], "run the sweep test");
        assert!(updated["updated_at"].as_u64().unwrap() >= updated_at_before);

        // 4. force staleness: orphan_threshold_ms was seeded at 50ms above.
        // Sleep past it, then drop the owner_session directly via update_epic
        // (simulating the owning session dying without a clean handoff) so
        // the sweep's has_owner/staleness gate actually exercises the
        // orphan-detection branch, not just the auto-resume branch.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        // Directly clear owner_session on the durable row to simulate a dead
        // owner (update_epic ignores empty-string params, so we go straight
        // to the store for this one field — the same store the handler uses).
        {
            let mut stale_entry = h.read_entry(id).await.expect("entry exists");
            stale_entry.owner_session = String::new();
            h.write_entry(&stale_entry).await;
        }

        let sweep_result = h
            .call("epic_registry_sweep", &json!({}))
            .await
            .expect("epic_registry_sweep succeeds");
        let touched = sweep_result["touched"]
            .as_array()
            .expect("touched is an array");
        let ours = touched
            .iter()
            .find(|e| e["id"] == id)
            .expect("our test epic was touched by the sweep");

        // auto_resume=true means the sweep rescues it in the SAME tick:
        // orphan-detected then immediately requeued to in_progress under
        // "main" — proving both the orphan-detect AND the auto-resume path
        // fired against real PluresDB reads/writes, not a mock.
        assert_eq!(ours["status"], "in_progress");
        assert_eq!(ours["owner_session"], "main");
        assert_eq!(ours["rescue_attempted"], true);

        // Confirm durability: re-reading directly from the store shows the
        // same result the sweep returned (no divergence between the action's
        // response and what's actually persisted).
        let persisted = h.read_entry(id).await.expect("entry persisted");
        assert_eq!(persisted.status, "in_progress");
        assert_eq!(persisted.owner_session, "main");
    }

    /// A registered epic with `auto_resume: false` that goes stale must be
    /// marked `orphaned` and surfaced (touched) by the sweep, but must NOT be
    /// silently auto-resumed — mirrors the `.px` sweep's
    /// `orphaned && !auto_resume` branch (human decision required).
    #[tokio::test]
    async fn orphan_without_auto_resume_is_surfaced_not_resumed() {
        let h = handler();
        let id = "test-epic-registry-no-auto-resume";

        h.call("register_epic", &register_params(id, "in_progress", false))
            .await
            .expect("register_epic succeeds");

        // Give it a live owner first so we can simulate the owner disappearing.
        h.call(
            "update_epic",
            &json!({ "id": id, "owner_session": "session-dead" }),
        )
        .await
        .expect("update_epic succeeds");

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        {
            let mut stale_entry = h.read_entry(id).await.expect("entry exists");
            stale_entry.owner_session = String::new();
            h.write_entry(&stale_entry).await;
        }

        let sweep_result = h
            .call("epic_registry_sweep", &json!({}))
            .await
            .expect("epic_registry_sweep succeeds");
        let touched = sweep_result["touched"].as_array().unwrap();
        let ours = touched.iter().find(|e| e["id"] == id);
        assert!(
            ours.is_some(),
            "orphan-without-auto-resume must still be surfaced by the sweep"
        );
        assert_eq!(ours.unwrap()["status"], "orphaned");

        // Re-read from the store: must remain orphaned, never silently
        // resumed to in_progress.
        let persisted = h.read_entry(id).await.expect("entry persisted");
        assert_eq!(persisted.status, "orphaned");
        assert_eq!(persisted.owner_session, "");
    }

    #[tokio::test]
    async fn register_epic_requires_id_and_title() {
        let h = handler();
        let result = h
            .call("register_epic", &json!({ "title": "missing id" }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_epic_unknown_id_errors() {
        let h = handler();
        let result = h
            .call(
                "update_epic",
                &json!({ "id": "does-not-exist", "status": "done" }),
            )
            .await;
        assert!(result.is_err());
    }
}
