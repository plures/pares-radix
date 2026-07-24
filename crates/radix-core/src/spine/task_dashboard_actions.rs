//! Task dashboard aggregation handler — the Rust IO boundary for
//! `praxis/procedures/task-dashboard.px` (ADR-0036).
//!
//! # Why this exists
//!
//! ADR-0036 (`.praxis/decisions/ADR-0036-praxisbot-native-task-dashboard.md`)
//! defines a **read-only aggregation view** over four independent, unreconciled
//! PluresDB namespaces (`task:{id}`, `worktask:task:{task_id}`,
//! `epic:registry:{id}`, `epic:gate:{epic_id}:{stage}`). Per C-PLURES-003 the
//! dashboard owns exactly one derived-cache namespace of its own
//! (`dashboard:tasks:{surface_id}`) and never writes to any of the four source
//! namespaces it reads. This handler is the aggregation + status-translation
//! boundary that both `task_dashboard_tick` and `task_dashboard_get` consult
//! (ADR-0036 §2: "never re-derived independently on either side").
//!
//! # Safety (C-NOSTUB-001)
//!
//! Pure aggregation over real `StateStore` reads — every namespace scan uses
//! the actual `keys_with_prefix`/`get` round trip through the same durable
//! store the source procedures write to. No fabricated counts, no synthetic
//! records. When a namespace has zero matching keys that is an honest zero,
//! not an error.
//!
//! # Write-cache guard (ADR-0036 §3, `task_dashboard_never_writes_source_namespaces`)
//!
//! `write_dashboard_cache` is the ONLY write path this handler exposes, and it
//! refuses any key that does not start with `dashboard:tasks:`. This makes the
//! "dashboard quietly becomes a fifth ledger" failure mode a build-time
//! (well, run-time-checked) violation rather than a matter of code-review
//! discipline — mirroring the `dashboard-stream.px` pattern of one owned
//! render-cache namespace and nothing else.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// The four source namespaces the dashboard is allowed to READ.
const TASK_PREFIX: &str = "task:";
const WORKTASK_PREFIX: &str = "worktask:task:";
const EPIC_REGISTRY_PREFIX: &str = "epic:registry:";
const EPIC_GATE_PREFIX: &str = "epic:gate:";

/// The ONE namespace the dashboard is allowed to WRITE.
const DASHBOARD_CACHE_PREFIX: &str = "dashboard:tasks:";

/// Actions handled by [`TaskDashboardActionHandler`].
pub const TASK_DASHBOARD_ACTIONS: &[&str] = &[
    "aggregate_task_dashboard",
    "write_dashboard_cache",
    "read_dashboard_cache",
];

/// Returns `true` if `action` is handled by [`TaskDashboardActionHandler`].
#[must_use]
pub fn is_task_dashboard_action(action: &str) -> bool {
    TASK_DASHBOARD_ACTIONS.contains(&action)
}

/// Unified presentation-only status, per ADR-0036 §2's translation table.
///
/// This is the single source of truth for folding the four namespaces'
/// incompatible status vocabularies into one tri... quad-state used by the
/// dashboard render. It is consulted here AND nowhere else (ADR-0034 drift
/// class this ADR explicitly closes off).
#[must_use]
fn unified_status(namespace: &str, raw_status: &str) -> &'static str {
    match namespace {
        "task" => match raw_status {
            "pending" => "open",
            "complete" => "done",
            _ => "open",
        },
        "worktask" => match raw_status {
            "planned" | "active" => "open",
            "in_review" | "merging" => "waiting",
            "done" => "done",
            "abandoned" => "stopped",
            _ => "open",
        },
        "epic_registry" => match raw_status {
            "queued" | "in_progress" => "open",
            "blocked" | "awaiting_approval" => "waiting",
            "done" => "done",
            "orphaned" | "abandoned" => "stopped",
            _ => "open",
        },
        "epic_gate" => match raw_status {
            "open" => "open",
            "passed" => "done",
            "failed" => "stopped",
            _ => "open",
        },
        _ => "open",
    }
}

/// One aggregated record surfaced by the dashboard, tagged with its source
/// namespace and translated `dashboard_status`.
#[derive(Debug, Clone)]
struct DashboardRecord {
    namespace: &'static str,
    key: String,
    dashboard_status: &'static str,
    raw: Value,
}

/// The read-only aggregation + write-cache-guard handler for the native task
/// dashboard (ADR-0036).
pub struct TaskDashboardActionHandler {
    state_store: Arc<dyn StateStore>,
}

impl TaskDashboardActionHandler {
    /// Construct over the SAME durable [`StateStore`] the four source
    /// namespaces are written through (C-PLURES-003/004: read from the store
    /// the writers actually use, never a parallel projection).
    #[must_use]
    pub fn new(state_store: Arc<dyn StateStore>) -> Self {
        Self { state_store }
    }

    /// Scan one source namespace and translate every record's status.
    async fn scan_namespace(&self, namespace: &'static str, prefix: &str) -> Vec<DashboardRecord> {
        let keys = self.state_store.keys_with_prefix(prefix).await;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let raw = self.state_store.get(&key).await.unwrap_or(Value::Null);
            let raw_status = raw
                .get("status")
                .or_else(|| raw.get("state"))
                .and_then(Value::as_str)
                .unwrap_or("");
            out.push(DashboardRecord {
                namespace,
                key,
                dashboard_status: unified_status(namespace, raw_status),
                raw,
            });
        }
        out
    }

    /// Aggregate all four source namespaces, translate statuses, and produce
    /// the counts + record list the render actor (tick or on-demand get)
    /// needs. Never writes anything — read-only per ADR-0036 §1.
    async fn aggregate_task_dashboard(&self, _params: &Value) -> Result<Value, ExecutionError> {
        let mut records = Vec::new();
        records.extend(self.scan_namespace("task", TASK_PREFIX).await);
        records.extend(self.scan_namespace("worktask", WORKTASK_PREFIX).await);
        records.extend(
            self.scan_namespace("epic_registry", EPIC_REGISTRY_PREFIX)
                .await,
        );
        records.extend(self.scan_namespace("epic_gate", EPIC_GATE_PREFIX).await);

        let mut open_count = 0usize;
        let mut waiting_count = 0usize;
        let mut done_count = 0usize;
        let mut stopped_count = 0usize;
        let mut items = Vec::with_capacity(records.len());

        for r in &records {
            match r.dashboard_status {
                "open" => open_count += 1,
                "waiting" => waiting_count += 1,
                "done" => done_count += 1,
                "stopped" => stopped_count += 1,
                _ => {}
            }
            items.push(json!({
                "namespace": r.namespace,
                "key": r.key,
                "dashboard_status": r.dashboard_status,
                "raw": r.raw,
            }));
        }

        Ok(json!({
            "items": items,
            "open_count": open_count,
            "waiting_count": waiting_count,
            "done_count": done_count,
            "stopped_count": stopped_count,
            "total_count": records.len(),
        }))
    }

    /// Write-cache guard: the ONLY write path this handler exposes.
    ///
    /// Enforces `task_dashboard_never_writes_source_namespaces` (ADR-0036 §3)
    /// at the Rust boundary — refuses any key that is not under
    /// `dashboard:tasks:`, returning a real `ExecutionError`, never silently
    /// dropping or redirecting the write.
    async fn write_dashboard_cache(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params.get("key").and_then(Value::as_str).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "write_dashboard_cache".into(),
                message: "missing key".into(),
            }
        })?;

        if !key.starts_with(DASHBOARD_CACHE_PREFIX) {
            return Err(ExecutionError::ActionFailed {
                action: "write_dashboard_cache".into(),
                message: format!(
                    "task_dashboard_never_writes_source_namespaces: refused write to \
                     '{key}' — the task dashboard may only write under \
                     '{DASHBOARD_CACHE_PREFIX}'"
                ),
            });
        }

        let value = params.get("value").cloned().unwrap_or(Value::Null);
        self.state_store.set(key, value.clone()).await;
        Ok(value)
    }

    /// Read back a dashboard render-cache record (for freeze/tick-resume
    /// logic that needs the previous `message_id`/`frozen` state).
    async fn read_dashboard_cache(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params.get("key").and_then(Value::as_str).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "read_dashboard_cache".into(),
                message: "missing key".into(),
            }
        })?;

        if !key.starts_with(DASHBOARD_CACHE_PREFIX) {
            return Err(ExecutionError::ActionFailed {
                action: "read_dashboard_cache".into(),
                message: format!(
                    "task_dashboard_never_writes_source_namespaces: refused read-cache lookup \
                     outside '{DASHBOARD_CACHE_PREFIX}' for key '{key}'"
                ),
            });
        }

        let value = self.state_store.get(key).await.unwrap_or(Value::Null);
        Ok(value)
    }
}

#[async_trait]
impl AsyncActionHandler for TaskDashboardActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "aggregate_task_dashboard" => self.aggregate_task_dashboard(params).await,
            "write_dashboard_cache" => self.write_dashboard_cache(params).await,
            "read_dashboard_cache" => self.read_dashboard_cache(params).await,
            other => Err(ExecutionError::UnknownAction(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InMemoryStateStore;

    fn handler() -> TaskDashboardActionHandler {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        TaskDashboardActionHandler::new(store)
    }

    #[test]
    fn action_gate_matches_only_dashboard_actions() {
        assert!(is_task_dashboard_action("aggregate_task_dashboard"));
        assert!(is_task_dashboard_action("write_dashboard_cache"));
        assert!(is_task_dashboard_action("read_dashboard_cache"));
        assert!(!is_task_dashboard_action("write_state"));
        assert!(!is_task_dashboard_action("new_epic"));
    }

    // ── status translation table (ADR-0036 §2) ────────────────────────────

    #[test]
    fn task_status_translation() {
        assert_eq!(unified_status("task", "pending"), "open");
        assert_eq!(unified_status("task", "complete"), "done");
    }

    #[test]
    fn worktask_status_translation() {
        assert_eq!(unified_status("worktask", "planned"), "open");
        assert_eq!(unified_status("worktask", "active"), "open");
        assert_eq!(unified_status("worktask", "in_review"), "waiting");
        assert_eq!(unified_status("worktask", "merging"), "waiting");
        assert_eq!(unified_status("worktask", "done"), "done");
        assert_eq!(unified_status("worktask", "abandoned"), "stopped");
    }

    #[test]
    fn epic_registry_status_translation() {
        assert_eq!(unified_status("epic_registry", "queued"), "open");
        assert_eq!(unified_status("epic_registry", "in_progress"), "open");
        assert_eq!(unified_status("epic_registry", "blocked"), "waiting");
        assert_eq!(
            unified_status("epic_registry", "awaiting_approval"),
            "waiting"
        );
        assert_eq!(unified_status("epic_registry", "done"), "done");
        assert_eq!(unified_status("epic_registry", "orphaned"), "stopped");
        assert_eq!(unified_status("epic_registry", "abandoned"), "stopped");
    }

    #[test]
    fn epic_gate_status_translation() {
        assert_eq!(unified_status("epic_gate", "open"), "open");
        assert_eq!(unified_status("epic_gate", "passed"), "done");
        assert_eq!(unified_status("epic_gate", "failed"), "stopped");
    }

    #[test]
    fn unknown_namespace_or_status_defaults_to_open_never_panics() {
        assert_eq!(unified_status("bogus", "whatever"), "open");
        assert_eq!(unified_status("task", "unknown_status"), "open");
    }

    // ── aggregation over real StateStore reads ─────────────────────────────

    #[tokio::test]
    async fn aggregates_across_all_four_namespaces_with_correct_counts() {
        let h = handler();
        h.state_store
            .set("task:1", json!({"status": "pending", "text": "do a thing"}))
            .await;
        h.state_store
            .set("task:2", json!({"status": "complete"}))
            .await;
        h.state_store
            .set(
                "worktask:task:wt-1",
                json!({"status": "in_review", "repo": "pares-radix"}),
            )
            .await;
        h.state_store
            .set(
                "epic:registry:e-1",
                json!({"status": "in_progress", "title": "epic one"}),
            )
            .await;
        h.state_store
            .set("epic:gate:e-1:design", json!({"status": "passed"}))
            .await;
        h.state_store
            .set("epic:gate:e-1:dev", json!({"status": "failed"}))
            .await;

        let out = h
            .aggregate_task_dashboard(&json!({}))
            .await
            .expect("aggregation must succeed");

        assert_eq!(out["total_count"], json!(6));
        // open: task:1(pending->open), epic:registry:e-1(in_progress->open),
        //       epic:gate design(passed is done, not open) -- recompute:
        // done: task:2, epic:gate design(passed)
        // waiting: worktask in_review
        // stopped: epic:gate dev(failed)
        assert_eq!(out["done_count"], json!(2));
        assert_eq!(out["waiting_count"], json!(1));
        assert_eq!(out["stopped_count"], json!(1));
        assert_eq!(out["open_count"], json!(2));

        let items = out["items"].as_array().unwrap();
        assert_eq!(items.len(), 6);
        assert!(items
            .iter()
            .any(|i| i["key"] == "task:1" && i["dashboard_status"] == "open"));
    }

    #[tokio::test]
    async fn empty_store_yields_honest_zero_never_fabricated() {
        let h = handler();
        let out = h.aggregate_task_dashboard(&json!({})).await.unwrap();
        assert_eq!(out["total_count"], json!(0));
        assert_eq!(out["open_count"], json!(0));
        assert_eq!(out["items"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn missing_status_field_defaults_to_open_not_error() {
        let h = handler();
        h.state_store
            .set("task:no-status", json!({"text": "no status field"}))
            .await;
        let out = h.aggregate_task_dashboard(&json!({})).await.unwrap();
        assert_eq!(out["open_count"], json!(1));
    }

    // ── write-cache guard: task_dashboard_never_writes_source_namespaces ────

    #[tokio::test]
    async fn write_dashboard_cache_accepts_dashboard_tasks_prefix() {
        let h = handler();
        let out = h
            .write_dashboard_cache(&json!({
                "key": "dashboard:tasks:tg-main",
                "value": {"message_id": 42, "frozen": false}
            }))
            .await
            .expect("write under dashboard:tasks: must succeed");
        assert_eq!(out["message_id"], json!(42));

        let read_back = h.state_store.get("dashboard:tasks:tg-main").await.unwrap();
        assert_eq!(read_back["message_id"], json!(42));
    }

    #[tokio::test]
    async fn write_dashboard_cache_refuses_task_namespace() {
        let h = handler();
        let err = h
            .write_dashboard_cache(&json!({"key": "task:99", "value": {"status": "complete"}}))
            .await
            .expect_err("write to task: must be refused");
        match err {
            ExecutionError::ActionFailed { action, message } => {
                assert_eq!(action, "write_dashboard_cache");
                assert!(message.contains("task_dashboard_never_writes_source_namespaces"));
            }
            other => panic!("expected ActionFailed, got {other:?}"),
        }
        // Confirm the refusal actually prevented the write.
        assert!(h.state_store.get("task:99").await.is_none());
    }

    #[tokio::test]
    async fn write_dashboard_cache_refuses_worktask_namespace() {
        let h = handler();
        assert!(h
            .write_dashboard_cache(&json!({"key": "worktask:task:wt-1", "value": {}}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn write_dashboard_cache_refuses_epic_registry_namespace() {
        let h = handler();
        assert!(h
            .write_dashboard_cache(&json!({"key": "epic:registry:e-1", "value": {}}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn write_dashboard_cache_refuses_epic_gate_namespace() {
        let h = handler();
        assert!(h
            .write_dashboard_cache(&json!({"key": "epic:gate:e-1:design", "value": {}}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn write_dashboard_cache_missing_key_errors() {
        let h = handler();
        let err = h
            .write_dashboard_cache(&json!({"value": {}}))
            .await
            .expect_err("missing key must error");
        matches!(err, ExecutionError::ActionFailed { .. });
    }

    #[tokio::test]
    async fn read_dashboard_cache_round_trips() {
        let h = handler();
        h.write_dashboard_cache(&json!({
            "key": "dashboard:tasks:surface-1",
            "value": {"frozen": true}
        }))
        .await
        .unwrap();

        let out = h
            .read_dashboard_cache(&json!({"key": "dashboard:tasks:surface-1"}))
            .await
            .unwrap();
        assert_eq!(out["frozen"], json!(true));
    }

    #[tokio::test]
    async fn read_dashboard_cache_absent_key_returns_null() {
        let h = handler();
        let out = h
            .read_dashboard_cache(&json!({"key": "dashboard:tasks:nope"}))
            .await
            .unwrap();
        assert_eq!(out, Value::Null);
    }

    #[tokio::test]
    async fn read_dashboard_cache_refuses_non_dashboard_prefix() {
        let h = handler();
        assert!(h
            .read_dashboard_cache(&json!({"key": "task:1"}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn call_dispatches_all_three_actions() {
        let h = handler();
        assert!(h.call("aggregate_task_dashboard", &json!({})).await.is_ok());
        assert!(h
            .call(
                "write_dashboard_cache",
                &json!({"key": "dashboard:tasks:x", "value": {}})
            )
            .await
            .is_ok());
        assert!(h
            .call("read_dashboard_cache", &json!({"key": "dashboard:tasks:x"}))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn call_unknown_action_errors() {
        let h = handler();
        let err = h.call("nope", &json!({})).await.expect_err("must error");
        matches!(err, ExecutionError::UnknownAction(_));
    }
}
