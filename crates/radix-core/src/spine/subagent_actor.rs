//! Subagent spawn actor for dev-lifecycle orchestration.
//!
//! Bridges between the reactive .px procedure system and the platform
//! sub-agent spawn seam. When `spawn_subagent` is called by a .px procedure,
//! this actor:
//!
//! 1. Spawns a session via a [`SubAgentSpawner`] (implemented by cognition's
//!    delegation `SubAgentManager`)
//! 2. Returns immediately with `{spawned: true, session_id: "..."}`
//! 3. On completion, writes to PluresDB key `stage_complete:{task_id}:{stage_name}`
//! 4. That PluresDB write triggers `evaluate_gate` via the reactive registry
//!
//! This creates the async feedback loop:
//! ```text
//! plan_task → spawn_subagent → (agent executes) → stage_complete write → evaluate_gate → spawn_subagent → ...
//! ```
//!
//! The actor depends only on platform types ([`crate::subagent_spawn`]); it has
//! no dependency on cognition's `delegation` module.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

use crate::px_adapter::AsyncActionHandler;
use crate::spine::reactive::ReactiveRegistry;
use crate::subagent_spawn::{SessionStatus, SpawnOptions, SubAgentSpawner};
use crate::task_manager::TaskManager;
use pares_radix_praxis::px::executor::ExecutionError;

/// Actor that spawns subagent sessions and wires their completion back
/// into the reactive registry via PluresDB writes.
pub struct SubagentActor {
    /// Sub-agent spawner for spawning and tracking sessions (platform seam).
    manager: Arc<dyn SubAgentSpawner>,
    /// Reactive registry for triggering `evaluate_gate` on stage completion.
    registry: Arc<ReactiveRegistry>,
    /// Owning task manager — drives the World-A `Task` terminal on subagent
    /// finish (the missing seam this actor now closes). Optional so tests /
    /// early-bootstrap that don't need task finalization can omit it, but the
    /// production wiring always supplies it.
    task_manager: Option<Arc<TaskManager>>,
}

impl SubagentActor {
    /// Create a new subagent actor without task finalization.
    ///
    /// # Arguments
    /// * `manager` — The sub-agent spawner that handles spawning
    /// * `registry` — The reactive registry for writing stage_complete events
    pub fn new(manager: Arc<dyn SubAgentSpawner>, registry: Arc<ReactiveRegistry>) -> Self {
        Self {
            manager,
            registry,
            task_manager: None,
        }
    }

    /// Create a new subagent actor wired to a [`TaskManager`] so that on the
    /// final stage completing, the owning World-A `Task` is driven terminal.
    ///
    /// This is the production constructor: it closes the task-completion seam
    /// (subagent finish → `Task` → `Completed`/`Failed`).
    pub fn with_task_manager(
        manager: Arc<dyn SubAgentSpawner>,
        registry: Arc<ReactiveRegistry>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        Self {
            manager,
            registry,
            task_manager: Some(task_manager),
        }
    }

    /// True when the gate reports no next stage (the dev task is fully done).
    ///
    /// `.px` action `is_final_stage {next_stage: $new_value.next_stage}`.
    fn is_final_stage(&self, params: &Value) -> Result<Value, ExecutionError> {
        // A missing key or explicit JSON null both mean "no next stage".
        let next = params.get("next_stage").cloned().unwrap_or(Value::Null);
        Ok(json!(next.is_null()))
    }

    /// Map a gate stage status onto a terminal [`TaskStatus`] string.
    ///
    /// `passed` → `completed`; `failed | blocked | timed_out | killed` →
    /// `failed`. Anything else is treated as non-terminal and returns null so
    /// callers do not force a premature transition.
    ///
    /// `.px` action `map_gate_to_terminal {gate_status: $new_value.status}`.
    fn map_gate_to_terminal(&self, params: &Value) -> Result<Value, ExecutionError> {
        let gate_status = params
            .get("gate_status")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let terminal = match gate_status {
            "passed" => Value::String("completed".into()),
            "failed" | "blocked" | "timed_out" | "killed" => Value::String("failed".into()),
            _ => Value::Null,
        };
        Ok(terminal)
    }

    /// Resolve the owning [`Task`] by id and drive it terminal — the real
    /// side-effect boundary that closes the seam.
    ///
    /// `.px` action
    /// `finalize_owning_task {task_id, terminal_status, when, result}`.
    ///
    /// Only acts when `when` is true (final stage) and `terminal_status` is a
    /// real terminal (`completed`/`failed`); otherwise it is a no-op so the
    /// non-final ticks of a multi-stage lifecycle don't disturb the Task.
    fn finalize_owning_task(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "finalize_owning_task".into(),
                message: "missing 'task_id'".into(),
            })?;

        let is_final = params
            .get("when")
            .map(|v| match v {
                Value::Bool(b) => *b,
                Value::String(s) => s == "true",
                _ => false,
            })
            .unwrap_or(false);

        let terminal_status = params
            .get("terminal_status")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let result = params.get("result").and_then(|v| v.as_str());

        if !is_final || terminal_status.is_empty() {
            // Not the final stage (or non-terminal status) — nothing to do.
            return Ok(json!({
                "finalized": false,
                "task_id": task_id,
                "reason": "not final stage or non-terminal status"
            }));
        }

        let Some(task_manager) = self.task_manager.as_ref() else {
            warn!(
                task_id = %task_id,
                "finalize_owning_task: no TaskManager wired — owning Task will \
                 NOT be terminated (seam open)"
            );
            return Ok(json!({
                "finalized": false,
                "task_id": task_id,
                "reason": "no task_manager wired"
            }));
        };

        self.drive_task_terminal(task_manager, task_id, terminal_status, result)
    }

    /// Shared terminal-transition logic used by both the `.px` action path and
    /// the poll-loop terminal arms. Returns whether a transition was applied.
    fn drive_task_terminal(
        &self,
        task_manager: &Arc<TaskManager>,
        task_id: &str,
        terminal_status: &str,
        result: Option<&str>,
    ) -> Result<Value, ExecutionError> {
        // Resolve the owning Task; if it doesn't exist the seam has no target
        // (devtask id != any Task.id) — report honestly, don't fabricate.
        if task_manager.get_task(task_id).is_none() {
            debug!(
                task_id = %task_id,
                "finalize_owning_task: no owning Task with this id — nothing to \
                 finalize (devtask id may not correlate to a TaskManager Task)"
            );
            return Ok(json!({
                "finalized": false,
                "task_id": task_id,
                "reason": "no owning Task with this id"
            }));
        }

        match terminal_status {
            "completed" => {
                task_manager.complete_task(task_id, result);
                info!(task_id = %task_id, "seam: owning Task → Completed on subagent finish");
            }
            "failed" => {
                task_manager.fail_task(task_id, result);
                info!(task_id = %task_id, "seam: owning Task → Failed on subagent finish");
            }
            other => {
                warn!(task_id = %task_id, status = %other, "finalize_owning_task: unknown terminal status");
                return Ok(json!({
                    "finalized": false,
                    "task_id": task_id,
                    "reason": "unknown terminal status"
                }));
            }
        }

        Ok(json!({
            "finalized": true,
            "task_id": task_id,
            "terminal_status": terminal_status
        }))
    }

    /// Spawn a subagent for a dev-lifecycle stage.
    ///
    /// Params:
    /// ```json
    /// {
    ///   "task_id": "TASK-2024-01-01-001",
    ///   "stage_name": "fix",
    ///   "prompt": "## Dev Lifecycle: fix stage\n...",
    ///   "workdir": "/projects/pares-radix",
    ///   "timeout_seconds": 600
    /// }
    /// ```
    ///
    /// Returns immediately with `{spawned: true, session_id: "..."}`.
    /// On completion, writes `stage_complete:{task_id}:{stage_name}` to trigger
    /// the reactive gate evaluation.
    async fn spawn_subagent(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "spawn_subagent".into(),
                message: "missing 'task_id'".into(),
            })?
            .to_string();

        let stage_name = params
            .get("stage_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "spawn_subagent".into(),
                message: "missing 'stage_name'".into(),
            })?
            .to_string();

        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "spawn_subagent".into(),
                message: "missing 'prompt'".into(),
            })?
            .to_string();

        let timeout_seconds = params
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(600);

        let _workdir = params
            .get("workdir")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        info!(
            task_id = %task_id,
            stage = %stage_name,
            timeout_s = timeout_seconds,
            "spawn_subagent: spawning stage execution"
        );

        // Configure spawn options
        let options = SpawnOptions::default()
            .with_timeout(Duration::from_secs(timeout_seconds))
            .with_label(format!("dev-lifecycle:{task_id}:{stage_name}"))
            .with_parent_context(format!(
                "Dev lifecycle stage execution. Task: {task_id}, Stage: {stage_name}"
            ));

        // Spawn the subagent — uses the "coder" agent for dev work
        let agent_name = match stage_name.as_str() {
            "analyze" => "analyst",
            "fix" | "test" | "deploy" => "coder",
            "verify" => "researcher",
            _ => "coder",
        };

        let session_id = self.manager.spawn(agent_name, &prompt, options).await;

        // Set up completion listener that writes to stage_complete
        let registry = Arc::clone(&self.registry);
        let tid = task_id.clone();
        let sname = stage_name.clone();
        let sid = session_id.clone();
        let mgr = Arc::clone(&self.manager);

        tokio::spawn(async move {
            // Poll for completion (the manager pushes events, but we need to
            // check the session status since we don't own the rx channel)
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Some(info) = mgr.get(&sid).await {
                    match &info.status {
                        SessionStatus::Completed => {
                            let output = info.output.unwrap_or_default();
                            info!(
                                task_id = %tid,
                                stage = %sname,
                                "subagent completed successfully"
                            );
                            // Write stage_complete to trigger evaluate_gate
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "passed",
                                "output": output,
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::Failed(err) => {
                            warn!(
                                task_id = %tid,
                                stage = %sname,
                                error = %err,
                                "subagent failed"
                            );
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "failed",
                                "output": err,
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::TimedOut => {
                            warn!(
                                task_id = %tid,
                                stage = %sname,
                                "subagent timed out"
                            );
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "failed",
                                "output": "Stage execution timed out",
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::Killed => {
                            warn!(
                                task_id = %tid,
                                stage = %sname,
                                "subagent killed"
                            );
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "blocked",
                                "output": "Stage execution was killed",
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::Running => {
                            // Still running, continue polling
                            continue;
                        }
                    }
                } else {
                    error!(
                        task_id = %tid,
                        stage = %sname,
                        session_id = %sid,
                        "subagent session not found — lost?"
                    );
                    break;
                }
            }
        });

        debug!(
            session_id = %session_id,
            task_id = %task_id,
            stage = %stage_name,
            "spawn_subagent: spawned successfully"
        );

        Ok(json!({
            "spawned": true,
            "session_id": session_id,
            "task_id": task_id,
            "stage_name": stage_name
        }))
    }
}

/// Actions handled by the subagent actor.
const SUBAGENT_ACTIONS: &[&str] = &[
    "spawn_subagent",
    "is_final_stage",
    "map_gate_to_terminal",
    "finalize_owning_task",
];

/// Check if an action name is handled by the subagent actor.
pub fn is_subagent_action(action: &str) -> bool {
    SUBAGENT_ACTIONS.contains(&action)
}

#[async_trait]
impl AsyncActionHandler for SubagentActor {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "spawn_subagent" => self.spawn_subagent(params).await,
            "is_final_stage" => self.is_final_stage(params),
            "map_gate_to_terminal" => self.map_gate_to_terminal(params),
            "finalize_owning_task" => self.finalize_owning_task(params),
            _ => Err(ExecutionError::ActionFailed {
                action: name.to_string(),
                message: "not a subagent action".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent_spawn::{SessionStatus, SpawnedInfo};

    /// Minimal in-file spawner used to exercise the actor without any
    /// cognition (`delegation`) scaffolding.
    struct MockSpawner;

    #[async_trait]
    impl SubAgentSpawner for MockSpawner {
        async fn spawn(&self, _agent: &str, _prompt: &str, _options: SpawnOptions) -> String {
            "mock-session-1".to_string()
        }

        async fn get(&self, _session_id: &str) -> Option<SpawnedInfo> {
            Some(SpawnedInfo {
                status: SessionStatus::Completed,
                output: Some("Result: PASS".into()),
            })
        }
    }

    fn make_actor() -> SubagentActor {
        let manager: Arc<dyn SubAgentSpawner> = Arc::new(MockSpawner);
        let registry = Arc::new(ReactiveRegistry::new());
        SubagentActor::new(manager, registry)
    }

    #[tokio::test]
    async fn spawn_subagent_returns_immediately() {
        let actor = make_actor();
        let params = json!({
            "task_id": "TASK-001",
            "stage_name": "fix",
            "prompt": "Fix the bug",
            "workdir": "/tmp",
            "timeout_seconds": 60
        });

        let result = actor.call("spawn_subagent", &params).await.unwrap();
        assert_eq!(result["spawned"], true);
        assert!(result["session_id"].is_string());
        assert_eq!(result["task_id"], "TASK-001");
        assert_eq!(result["stage_name"], "fix");
    }

    #[tokio::test]
    async fn spawn_subagent_missing_task_id_errors() {
        let actor = make_actor();
        let params = json!({
            "stage_name": "fix",
            "prompt": "Fix the bug"
        });

        let result = actor.call("spawn_subagent", &params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn unknown_action_errors() {
        let actor = make_actor();
        let result = actor.call("unknown_action", &json!({})).await;
        assert!(result.is_err());
    }

    // ─────────────────────────────────────────────────────────────────────
    // Task-completion seam (S2): subagent finish → owning Task terminal
    // ─────────────────────────────────────────────────────────────────────

    use crate::task::{CompletionCondition, ConditionType, TaskStatus};
    use crate::task_manager::TaskManager;
    use pluresdb::{CrdtStore, MemoryStorage};

    /// Spawner test-double whose reported completion status is configurable.
    /// This is a test double at a real seam (allowed); the thing under test —
    /// the finalize transition — is REAL, not mocked.
    struct StatusSpawner {
        status: SessionStatus,
    }

    #[async_trait]
    impl SubAgentSpawner for StatusSpawner {
        async fn spawn(&self, _agent: &str, _prompt: &str, _options: SpawnOptions) -> String {
            "mock-session-seam".to_string()
        }
        async fn get(&self, _session_id: &str) -> Option<SpawnedInfo> {
            Some(SpawnedInfo {
                status: self.status.clone(),
                output: Some("stage output".into()),
            })
        }
    }

    fn make_task_manager() -> Arc<TaskManager> {
        let storage: Arc<dyn pluresdb::StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        Arc::new(TaskManager::new(Arc::new(store)))
    }

    /// Seed a real Open Task and return its id (== the devtask correlation id).
    fn seed_open_task(tm: &TaskManager) -> String {
        let task = tm.create_task(
            "Seam test task",
            "chat_seam",
            vec![CompletionCondition {
                description: "model eval".into(),
                condition_type: ConditionType::ModelEvaluation("done?".into()),
                satisfied: false,
            }],
        );
        // Task starts non-terminal and is visible in open_tasks().
        assert!(!task.is_terminal());
        task.id
    }

    fn actor_with_tm(status: SessionStatus, tm: Arc<TaskManager>) -> SubagentActor {
        let manager: Arc<dyn SubAgentSpawner> = Arc::new(StatusSpawner { status });
        let registry = Arc::new(ReactiveRegistry::new());
        SubagentActor::with_task_manager(manager, registry, tm)
    }

    /// Drive the REAL finalize action-handler chain exactly as the reactive
    /// `finalize_task` procedure would (is_final_stage → map_gate_to_terminal →
    /// finalize_owning_task) off a gate_decision value, and return the
    /// finalize_owning_task result.
    async fn drive_finalize(
        actor: &SubagentActor,
        task_id: &str,
        gate_status: &str,
        next_stage: Value,
    ) -> Value {
        let final_flag = actor
            .call("is_final_stage", &json!({ "next_stage": next_stage }))
            .await
            .unwrap();
        let terminal = actor
            .call("map_gate_to_terminal", &json!({ "gate_status": gate_status }))
            .await
            .unwrap();
        actor
            .call(
                "finalize_owning_task",
                &json!({
                    "task_id": task_id,
                    "terminal_status": terminal,
                    "when": final_flag,
                    "result": "stage output"
                }),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn seam_completed_final_stage_drives_task_completed() {
        let tm = make_task_manager();
        let id = seed_open_task(&tm);
        let actor = actor_with_tm(SessionStatus::Completed, Arc::clone(&tm));

        // Final stage passed: next_stage == null, gate status == passed.
        let res = drive_finalize(&actor, &id, "passed", Value::Null).await;
        assert_eq!(res["finalized"], true, "finalize must apply on final passed stage");

        // REAL assertion: owning Task is now Completed and absent from open_tasks.
        let task = tm.get_task(&id).expect("task still exists");
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.result.as_deref(), Some("stage output"));
        assert!(
            !tm.open_tasks().iter().any(|t| t.id == id),
            "completed task must NOT remain in open_tasks() (no stale in_progress)"
        );
    }

    #[tokio::test]
    async fn seam_failed_final_stage_drives_task_failed() {
        let tm = make_task_manager();
        let id = seed_open_task(&tm);
        let actor = actor_with_tm(SessionStatus::Failed("boom".into()), Arc::clone(&tm));

        let res = drive_finalize(&actor, &id, "failed", Value::Null).await;
        assert_eq!(res["finalized"], true);

        let task = tm.get_task(&id).expect("task still exists");
        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(task.error.as_deref(), Some("stage output"));
        assert!(
            !tm.open_tasks().iter().any(|t| t.id == id),
            "failed task must NOT remain in open_tasks()"
        );
    }

    #[tokio::test]
    async fn seam_timed_out_final_stage_drives_task_failed() {
        let tm = make_task_manager();
        let id = seed_open_task(&tm);
        let actor = actor_with_tm(SessionStatus::TimedOut, Arc::clone(&tm));

        // A timed_out gate status maps to a failed terminal.
        let res = drive_finalize(&actor, &id, "timed_out", Value::Null).await;
        assert_eq!(res["finalized"], true);
        assert_eq!(tm.get_task(&id).unwrap().status, TaskStatus::Failed);
        assert!(tm.open_tasks().is_empty());
    }

    #[tokio::test]
    async fn seam_non_final_stage_does_not_terminate_task() {
        let tm = make_task_manager();
        let id = seed_open_task(&tm);
        let actor = actor_with_tm(SessionStatus::Completed, Arc::clone(&tm));

        // Non-final: next_stage is a real stage name → must NOT finalize.
        let res = drive_finalize(&actor, &id, "passed", json!("test")).await;
        assert_eq!(res["finalized"], false, "non-final stage must not terminate the Task");

        let task = tm.get_task(&id).unwrap();
        assert!(!task.is_terminal(), "Task must remain non-terminal mid-lifecycle");
        assert!(tm.open_tasks().iter().any(|t| t.id == id));
    }

    #[tokio::test]
    async fn seam_map_gate_to_terminal_mapping() {
        let tm = make_task_manager();
        let actor = actor_with_tm(SessionStatus::Completed, tm);
        async fn m(actor: &SubagentActor, s: &str) -> Value {
            actor
                .call("map_gate_to_terminal", &json!({ "gate_status": s }))
                .await
                .unwrap()
        }
        assert_eq!(m(&actor, "passed").await, json!("completed"));
        assert_eq!(m(&actor, "failed").await, json!("failed"));
        assert_eq!(m(&actor, "blocked").await, json!("failed"));
        assert_eq!(m(&actor, "timed_out").await, json!("failed"));
        assert_eq!(m(&actor, "killed").await, json!("failed"));
        assert_eq!(m(&actor, "running").await, Value::Null);
    }

    #[tokio::test]
    async fn seam_finalize_without_task_manager_is_noop() {
        // No TaskManager wired → finalize reports not-finalized rather than
        // panicking or fabricating a transition (honest absence).
        let actor = make_actor();
        let res = drive_finalize(&actor, "no-such-id", "passed", Value::Null).await;
        assert_eq!(res["finalized"], false);
    }
}
