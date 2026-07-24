//! Task-handoff action handler — the Rust IO edge for durable custody transfer.
//!
//! Decision logic (which agent to hand off to, when to hand off) lives in
//! `.px` procedures.  This handler performs ONLY real side effects over a
//! [`ConditionalTaskStore`] backed by PluresDB / SledStorage:
//!
//! - `prepare_task_handoff` — marks a task `TransferPending` and returns an
//!   integrity-signed [`HandoffEnvelope`].
//! - `verify_task_handoff_digest` — verifies a serialised envelope's digest
//!   without mutating state (pure integrity check).
//! - `accept_task_handoff` — transitions custody to the target agent (the
//!   receiving side of the transfer).
//! - `conditional_claim_task` — atomically claims a task for a worker
//!   (compare-and-swap, returns claim token on success).
//!
//! All four verbs are gated behind [`is_task_handoff_action`]; the
//! [`CompositeActionHandler`](crate::spine::actions::CompositeActionHandler)
//! routes to this handler when the predicate is true.
//!
//! # Architecture (C-DEV-001)
//!
//! Branching/sequencing logic lives in `.px`, not here. This file is IO only.
//! No stub returns (C-NOSTUB-001): every arm either performs the real operation
//! or returns a structured [`ExecutionError::ActionFailed`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::warn;
use uuid::Uuid;

use crate::px_adapter::AsyncActionHandler;
use crate::task_handoff::{ConditionalTaskStore, HandoffEnvelope, TransferableTask};
use pares_radix_praxis::px::executor::ExecutionError;

/// Action verbs owned by this handler.
pub const TASK_HANDOFF_ACTIONS: &[&str] = &[
    "prepare_task_handoff",
    "verify_task_handoff_digest",
    "accept_task_handoff",
    "conditional_claim_task",
];

/// Returns `true` when `action` is handled by [`TaskHandoffActionHandler`].
pub fn is_task_handoff_action(action: &str) -> bool {
    TASK_HANDOFF_ACTIONS.contains(&action)
}

/// Rust IO boundary for task custody transfer.
///
/// Wraps a [`ConditionalTaskStore`] backed by a durable sled database.  The
/// store path is supplied at construction time (see
/// [`TaskHandoffActionHandler::open`]); all four handoff verbs share it.
pub struct TaskHandoffActionHandler {
    store: Arc<ConditionalTaskStore>,
}

impl TaskHandoffActionHandler {
    /// Open (or create) a handler backed by a sled database at `path`.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, crate::task_handoff::HandoffError> {
        let store = ConditionalTaskStore::open(path)?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// Construct from an already-open store (useful in tests).
    pub fn with_store(store: Arc<ConditionalTaskStore>) -> Self {
        Self { store }
    }

    // ── action implementations ─────────────────────────────────────────────

    /// `prepare_task_handoff` — transition a task to `TransferPending` and
    /// return the signed envelope so the `.px` procedure can relay it to the
    /// target agent.
    ///
    /// Required params:
    /// - `task_id`: string
    /// - `source_agent`: string (current owner)
    /// - `target_agent`: string (recipient)
    /// - `handoff_id`: UUID string (idempotency key)
    /// - `expected_generation`: u64
    async fn prepare_handoff(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_id = require_str(params, "task_id", "prepare_task_handoff")?;
        let source = require_str(params, "source_agent", "prepare_task_handoff")?;
        let target = require_str(params, "target_agent", "prepare_task_handoff")?;
        let handoff_id_str = require_str(params, "handoff_id", "prepare_task_handoff")?;
        let expected_gen = params
            .get("expected_generation")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let handoff_id = Uuid::parse_str(handoff_id_str).map_err(|e| {
            ExecutionError::ActionFailed {
                action: "prepare_task_handoff".into(),
                message: format!("invalid handoff_id UUID: {e}"),
            }
        })?;

        // Seed the task if params include a full task definition and the store
        // doesn't have it yet — allows `.px` to initiate a transfer without a
        // separate seed step.
        if let Some(task_val) = params.get("task") {
            let task: TransferableTask =
                serde_json::from_value(task_val.clone()).map_err(|e| {
                    ExecutionError::ActionFailed {
                        action: "prepare_task_handoff".into(),
                        message: format!("invalid task object: {e}"),
                    }
                })?;
            // seed_owned is idempotent on duplicate — ignore Conflict if the
            // exact record is already present.
            let _ = self.store.seed_owned(task, source);
        }

        let envelope = self
            .store
            .prepare_handoff(task_id, source, target, handoff_id, expected_gen)
            .map_err(|e| ExecutionError::ActionFailed {
                action: "prepare_task_handoff".into(),
                message: format!("{e}"),
            })?;

        serde_json::to_value(&envelope).map_err(|e| ExecutionError::ActionFailed {
            action: "prepare_task_handoff".into(),
            message: format!("serialisation error: {e}"),
        })
    }

    /// `verify_task_handoff_digest` — verify a serialised envelope's integrity
    /// digest without touching state.
    ///
    /// Required params:
    /// - `envelope`: the JSON-serialised [`HandoffEnvelope`]
    ///
    /// Returns `{ "ok": true }` on success or an [`ExecutionError`] on failure.
    async fn verify_digest(&self, params: &Value) -> Result<Value, ExecutionError> {
        let env_val = params.get("envelope").ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "verify_task_handoff_digest".into(),
                message: "missing `envelope` param".into(),
            }
        })?;

        let envelope: HandoffEnvelope =
            serde_json::from_value(env_val.clone()).map_err(|e| {
                ExecutionError::ActionFailed {
                    action: "verify_task_handoff_digest".into(),
                    message: format!("invalid envelope: {e}"),
                }
            })?;

        envelope.verify().map_err(|e| ExecutionError::ActionFailed {
            action: "verify_task_handoff_digest".into(),
            message: format!("{e}"),
        })?;

        Ok(json!({ "ok": true }))
    }

    /// `accept_task_handoff` — receive a transfer envelope and atomically
    /// commit custody to the target agent.
    ///
    /// Required params:
    /// - `envelope`: the JSON-serialised [`HandoffEnvelope`]
    /// - `target_agent`: string (must match envelope's target_agent_id)
    async fn accept_handoff(&self, params: &Value) -> Result<Value, ExecutionError> {
        let env_val = params.get("envelope").ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "accept_task_handoff".into(),
                message: "missing `envelope` param".into(),
            }
        })?;
        let target = require_str(params, "target_agent", "accept_task_handoff")?;

        let envelope: HandoffEnvelope =
            serde_json::from_value(env_val.clone()).map_err(|e| {
                ExecutionError::ActionFailed {
                    action: "accept_task_handoff".into(),
                    message: format!("invalid envelope: {e}"),
                }
            })?;

        let record = self
            .store
            .accept_handoff(&envelope, target)
            .map_err(|e| ExecutionError::ActionFailed {
                action: "accept_task_handoff".into(),
                message: format!("{e}"),
            })?;

        serde_json::to_value(&record).map_err(|e| ExecutionError::ActionFailed {
            action: "accept_task_handoff".into(),
            message: format!("serialisation error: {e}"),
        })
    }

    /// `conditional_claim_task` — atomically claim a task for a worker.
    ///
    /// Required params:
    /// - `task_id`: string
    /// - `agent_id`: string (must be current owner)
    /// - `worker_id`: string (sub-worker claiming the task)
    /// - `generation`: u64 (expected generation guard)
    ///
    /// Returns a `{ task_id, worker_id, token, generation }` claim object.
    async fn claim_task(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_id = require_str(params, "task_id", "conditional_claim_task")?;
        let agent_id = require_str(params, "agent_id", "conditional_claim_task")?;
        let worker_id = require_str(params, "worker_id", "conditional_claim_task")?;
        let generation = params
            .get("generation")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let claim = self
            .store
            .claim_task(task_id, agent_id, worker_id, generation)
            .map_err(|e| ExecutionError::ActionFailed {
                action: "conditional_claim_task".into(),
                message: format!("{e}"),
            })?;

        Ok(json!({
            "task_id":   claim.task_id,
            "worker_id": claim.worker_id,
            "token":     claim.token.to_string(),
            "generation": claim.generation,
        }))
    }
}

#[async_trait]
impl AsyncActionHandler for TaskHandoffActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        match action {
            "prepare_task_handoff" => self.prepare_handoff(params).await,
            "verify_task_handoff_digest" => self.verify_digest(params).await,
            "accept_task_handoff" => self.accept_handoff(params).await,
            "conditional_claim_task" => self.claim_task(params).await,
            other => {
                warn!(action = %other, "task_handoff_actions: unrecognised action (routing bug)");
                Err(ExecutionError::ActionFailed {
                    action: other.to_string(),
                    message: "action not handled by TaskHandoffActionHandler".into(),
                })
            }
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

// ── store-path helper (used by runtime assembly) ──────────────────────────────

/// Resolve the default sled path for the handoff store.
///
/// Uses `RADIX_HANDOFF_DB_PATH` if set; otherwise places the store in
/// `<state_dir>/task-handoff` (relative to the runtime state directory).
pub fn resolve_handoff_db_path(state_dir: &Path) -> PathBuf {
    match std::env::var("RADIX_HANDOFF_DB_PATH") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => state_dir.join("task-handoff"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_handoff::{ConditionalTaskStore, TransferableTask};

    fn sample_task(id: &str) -> TransferableTask {
        TransferableTask {
            task_id: id.into(),
            objective: "test objective".into(),
            repo: "plures/test".into(),
            priority: "P1".into(),
            constraints: vec!["no stubs".into()],
            acceptance_criteria: vec!["tests pass".into()],
            next_action: "implement".into(),
            provenance: "test".into(),
            artifacts: vec![],
        }
    }

    fn temp_store() -> (tempfile::TempDir, Arc<ConditionalTaskStore>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ConditionalTaskStore::open(dir.path().join("handoff-store"))
            .expect("open store");
        (dir, Arc::new(store))
    }

    fn handler_with_seeded(task: TransferableTask, owner: &str) -> (tempfile::TempDir, TaskHandoffActionHandler) {
        let (dir, store) = temp_store();
        store.seed_owned(task, owner).expect("seed");
        let handler = TaskHandoffActionHandler::with_store(store);
        (dir, handler)
    }

    // ── prepare_task_handoff ─────────────────────────────────────────────────

    #[tokio::test]
    async fn prepare_produces_valid_envelope() {
        let (_dir, handler) = handler_with_seeded(sample_task("T-PREP"), "openclaw");

        let handoff_id = Uuid::new_v4().to_string();
        let params = json!({
            "task_id": "T-PREP",
            "source_agent": "openclaw",
            "target_agent": "praxisbot",
            "handoff_id": handoff_id,
            "expected_generation": 0_u64,
        });

        let result = handler.call("prepare_task_handoff", &params).await.unwrap();
        assert_eq!(result["schema"], "plures.task-handoff.v1");
        assert_eq!(result["record"]["task"]["task_id"], "T-PREP");
        assert_eq!(result["record"]["custody_state"], "transfer_pending");
        assert!(!result["digest"].as_str().unwrap_or("").is_empty());
    }

    #[tokio::test]
    async fn prepare_is_idempotent_for_same_handoff_id() {
        let (_dir, handler) = handler_with_seeded(sample_task("T-IDEMP"), "openclaw");

        let handoff_id = Uuid::new_v4().to_string();
        let params = json!({
            "task_id": "T-IDEMP",
            "source_agent": "openclaw",
            "target_agent": "praxisbot",
            "handoff_id": handoff_id,
            "expected_generation": 0_u64,
        });

        let first = handler.call("prepare_task_handoff", &params).await.unwrap();
        let second = handler.call("prepare_task_handoff", &params).await.unwrap();
        assert_eq!(first["digest"], second["digest"]);
    }

    #[tokio::test]
    async fn prepare_missing_param_returns_error() {
        let (_dir, handler) = handler_with_seeded(sample_task("T-MISSING"), "openclaw");

        let result = handler
            .call(
                "prepare_task_handoff",
                &json!({"task_id": "T-MISSING", "source_agent": "openclaw"}),
            )
            .await;
        assert!(result.is_err(), "expected error for missing params");
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("target_agent") || err.contains("missing"), "error: {err}");
    }

    // ── verify_task_handoff_digest ───────────────────────────────────────────

    #[tokio::test]
    async fn verify_accepts_valid_envelope() {
        let (_dir, handler) = handler_with_seeded(sample_task("T-VERIFY"), "openclaw");

        let handoff_id = Uuid::new_v4().to_string();
        let env_val = handler
            .call(
                "prepare_task_handoff",
                &json!({
                    "task_id": "T-VERIFY",
                    "source_agent": "openclaw",
                    "target_agent": "praxisbot",
                    "handoff_id": handoff_id,
                    "expected_generation": 0_u64,
                }),
            )
            .await
            .unwrap();

        let verify_result = handler
            .call(
                "verify_task_handoff_digest",
                &json!({ "envelope": env_val }),
            )
            .await
            .unwrap();

        assert_eq!(verify_result["ok"], true);
    }

    #[tokio::test]
    async fn verify_rejects_tampered_digest() {
        let (_dir, handler) = handler_with_seeded(sample_task("T-TAMPER"), "openclaw");

        let handoff_id = Uuid::new_v4().to_string();
        let mut env_val = handler
            .call(
                "prepare_task_handoff",
                &json!({
                    "task_id": "T-TAMPER",
                    "source_agent": "openclaw",
                    "target_agent": "praxisbot",
                    "handoff_id": handoff_id,
                    "expected_generation": 0_u64,
                }),
            )
            .await
            .unwrap();

        // Tamper: overwrite the digest with garbage.
        *env_val.get_mut("digest").unwrap() = json!("aaaa");

        let result = handler
            .call(
                "verify_task_handoff_digest",
                &json!({ "envelope": env_val }),
            )
            .await;
        assert!(result.is_err(), "expected error for tampered digest");
    }

    // ── accept_task_handoff ──────────────────────────────────────────────────

    #[tokio::test]
    async fn full_handoff_roundtrip_changes_owner() {
        // source store prepares, target store accepts.
        let source_dir = tempfile::tempdir().expect("tempdir");
        let target_dir = tempfile::tempdir().expect("tempdir");

        let source_store =
            Arc::new(ConditionalTaskStore::open(source_dir.path().join("src")).unwrap());
        let target_store =
            Arc::new(ConditionalTaskStore::open(target_dir.path().join("tgt")).unwrap());

        source_store.seed_owned(sample_task("T-ROUND"), "openclaw").unwrap();

        let src_handler = TaskHandoffActionHandler::with_store(Arc::clone(&source_store));
        let tgt_handler = TaskHandoffActionHandler::with_store(Arc::clone(&target_store));

        let handoff_id = Uuid::new_v4().to_string();

        let envelope = src_handler
            .call(
                "prepare_task_handoff",
                &json!({
                    "task_id": "T-ROUND",
                    "source_agent": "openclaw",
                    "target_agent": "praxisbot",
                    "handoff_id": handoff_id,
                    "expected_generation": 0_u64,
                }),
            )
            .await
            .unwrap();

        let accepted = tgt_handler
            .call(
                "accept_task_handoff",
                &json!({
                    "envelope": envelope,
                    "target_agent": "praxisbot",
                }),
            )
            .await
            .unwrap();

        assert_eq!(accepted["owner_agent_id"], "praxisbot");
        assert_eq!(accepted["custody_state"], "owned");
        assert_eq!(accepted["task"]["task_id"], "T-ROUND");
    }

    // ── conditional_claim_task ───────────────────────────────────────────────

    #[tokio::test]
    async fn claim_returns_token_and_only_one_worker_wins() {
        let (_dir, store) = temp_store();
        // Seed directly and accept (simulate a completed handoff — owner is praxisbot).
        store.seed_owned(sample_task("T-CLAIM"), "praxisbot").unwrap();

        let handler = TaskHandoffActionHandler::with_store(store);

        let claim = handler
            .call(
                "conditional_claim_task",
                &json!({
                    "task_id": "T-CLAIM",
                    "agent_id": "praxisbot",
                    "worker_id": "worker-1",
                    "generation": 0_u64,
                }),
            )
            .await
            .unwrap();

        assert_eq!(claim["task_id"], "T-CLAIM");
        assert_eq!(claim["worker_id"], "worker-1");
        assert!(!claim["token"].as_str().unwrap_or("").is_empty());

        // Second worker attempt must fail (already claimed).
        let second = handler
            .call(
                "conditional_claim_task",
                &json!({
                    "task_id": "T-CLAIM",
                    "agent_id": "praxisbot",
                    "worker_id": "worker-2",
                    "generation": 0_u64,
                }),
            )
            .await;
        assert!(second.is_err(), "second worker should fail");
    }

    #[tokio::test]
    async fn claim_task_idempotent_for_same_worker() {
        let (_dir, store) = temp_store();
        store.seed_owned(sample_task("T-IDEMP-CLAIM"), "praxisbot").unwrap();
        let handler = TaskHandoffActionHandler::with_store(store);

        let first = handler
            .call(
                "conditional_claim_task",
                &json!({
                    "task_id": "T-IDEMP-CLAIM",
                    "agent_id": "praxisbot",
                    "worker_id": "worker-1",
                    "generation": 0_u64,
                }),
            )
            .await
            .unwrap();

        // Same worker re-claiming returns same token.
        let second = handler
            .call(
                "conditional_claim_task",
                &json!({
                    "task_id": "T-IDEMP-CLAIM",
                    "agent_id": "praxisbot",
                    "worker_id": "worker-1",
                    "generation": 0_u64,
                }),
            )
            .await
            .unwrap();

        assert_eq!(first["token"], second["token"]);
    }

    // ── prepare with inline task definition ──────────────────────────────────

    #[tokio::test]
    async fn prepare_can_seed_task_inline() {
        let (_dir, store) = temp_store();
        let handler = TaskHandoffActionHandler::with_store(store);

        let handoff_id = Uuid::new_v4().to_string();
        let params = json!({
            "task_id": "T-INLINE",
            "source_agent": "openclaw",
            "target_agent": "praxisbot",
            "handoff_id": handoff_id,
            "expected_generation": 0_u64,
            "task": {
                "task_id": "T-INLINE",
                "objective": "inline seeded task",
                "repo": "plures/test",
                "priority": "P1",
                "constraints": [],
                "acceptance_criteria": [],
                "next_action": "test",
                "provenance": "inline",
                "artifacts": [],
            }
        });

        let result = handler.call("prepare_task_handoff", &params).await.unwrap();
        assert_eq!(result["record"]["task"]["task_id"], "T-INLINE");
        assert_eq!(result["record"]["custody_state"], "transfer_pending");
    }
}
