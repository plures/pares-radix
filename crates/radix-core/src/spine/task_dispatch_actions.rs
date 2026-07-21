//! Task-dispatch action handler — the Rust IO edge that closes the autonomous
//! task-execution loop (design: praxis/spine/spine.px IO boundary #5).
//!
//! Decision logic lives in `.px` (`autonomous-dispatch.px::evaluate_dispatch`).
//! That procedure decides WHICH task to run and builds the execution prompt,
//! then invokes the action verbs exposed here. This handler performs
//! ONLY side effects over real runtime state:
//! - `dispatch_task` hands (task_id, prompt) to [`TaskDispatcher`], which
//!   injects a `SpineEvent::Inbound{source:"task_executor", autonomous:true}`
//!   into the SAME pipeline that handles user messages, then records dispatch.
//! - `read_evaluable_tasks` reads durable task state via [`TaskManager`].
//! - `mark_task_in_progress` updates durable task status/attempt accounting.
//!
//! If you're tempted to add "which task / should we dispatch" logic here, STOP.
//! It belongs in `.px` (C-DEV-001). This file is IO only.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::warn;

use crate::px_adapter::AsyncActionHandler;
use crate::task::{Task, TaskStatus};
use crate::task_executor::TaskDispatcher;
use crate::task_manager::TaskManager;
use pares_radix_praxis::px::executor::ExecutionError;

/// Action verb(s) this handler owns.
const TASK_DISPATCH_ACTIONS: &[&str] = &[
    "dispatch_task",
    "read_evaluable_tasks",
    "mark_task_in_progress",
];

/// Returns true if `action` is handled by [`TaskDispatchActionHandler`].
pub fn is_task_dispatch_action(action: &str) -> bool {
    TASK_DISPATCH_ACTIONS.contains(&action)
}

/// Rust IO boundary that dispatches an autonomous task chosen by `.px`.
///
/// Wraps a [`TaskDispatcher`] built over the live [`StateStore`] and
/// [`PipelineEmitter`], plus the shared durable [`TaskManager`].
pub struct TaskDispatchActionHandler {
    dispatcher: Arc<TaskDispatcher>,
    task_manager: Option<Arc<TaskManager>>,
}

impl TaskDispatchActionHandler {
    /// Create a handler over an already-constructed [`TaskDispatcher`] and
    /// optional durable task manager.
    pub fn new(dispatcher: Arc<TaskDispatcher>, task_manager: Option<Arc<TaskManager>>) -> Self {
        Self {
            dispatcher,
            task_manager,
        }
    }

    fn require_task_manager(&self, action: &str) -> Result<&TaskManager, ExecutionError> {
        self.task_manager
            .as_deref()
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: action.to_string(),
                message: "task store not wired — TaskManager unavailable".into(),
            })
    }

    fn status_to_px(status: &TaskStatus) -> &'static str {
        match status {
            TaskStatus::Open => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Delegated => "delegated",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    fn task_to_evaluable(task: Task) -> Value {
        let conditions = task
            .completion_conditions
            .into_iter()
            .map(|c| {
                json!({
                    "description": c.description,
                    "satisfied": c.satisfied,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "id": task.id,
            "description": task.description,
            "priority": task.priority,
            "created_at": task.created_at,
            "last_evaluated_at": task.last_evaluated_at,
            "attempts": task.attempts,
            "status": Self::status_to_px(&task.status),
            "conditions": conditions,
        })
    }

    /// IO: dispatch the chosen task prompt into the spine pipeline.
    ///
    /// Params:
    /// ```json
    /// { "task_id": "task-123", "prompt": "## Execute task ...\n..." }
    /// ```
    ///
    /// Returns `{ "dispatched": bool, "task_id": "..." }`. On a successful
    /// dispatch the dispatch timestamp is recorded to durable state via
    /// `TaskDispatcher::record_dispatch` (matches autonomous-dispatch.px's
    /// `record_dispatch` step in the design).
    async fn dispatch_task(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_id = params
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "dispatch_task".into(),
                message: "missing 'task_id'".into(),
            })?;

        let prompt = params
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "dispatch_task".into(),
                message: "missing 'prompt'".into(),
            })?;

        let dispatched = self.dispatcher.dispatch(task_id, prompt);
        if dispatched {
            self.dispatcher.record_dispatch(task_id).await;
        } else {
            warn!(
                task_id = %task_id,
                "dispatch_task: TaskDispatcher had no pipeline emitter — not dispatched"
            );
        }

        Ok(json!({ "dispatched": dispatched, "task_id": task_id }))
    }

    /// IO: read tasks currently eligible for autonomous evaluation.
    async fn read_evaluable_tasks(&self) -> Result<Value, ExecutionError> {
        let manager = self.require_task_manager("read_evaluable_tasks")?;
        let tasks = manager
            .evaluable_tasks()
            .into_iter()
            .map(Self::task_to_evaluable)
            .collect::<Vec<_>>();
        Ok(Value::Array(tasks))
    }

    /// IO: mark a task as in-progress and persist a dispatch evaluation attempt.
    async fn mark_task_in_progress(&self, params: &Value) -> Result<Value, ExecutionError> {
        let manager = self.require_task_manager("mark_task_in_progress")?;
        let task_id = params
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "mark_task_in_progress".into(),
                message: "missing 'task_id'".into(),
            })?;

        if manager.get_task(task_id).is_none() {
            return Err(ExecutionError::ActionFailed {
                action: "mark_task_in_progress".into(),
                message: format!("task not found: {task_id}"),
            });
        }

        manager.update_status(task_id, TaskStatus::InProgress);
        manager.record_evaluation(task_id, "autonomous_dispatch");

        Ok(json!({
            "ok": true,
            "task_id": task_id,
            "status": "in_progress"
        }))
    }
}

#[async_trait]
impl AsyncActionHandler for TaskDispatchActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        match action {
            "dispatch_task" => self.dispatch_task(params).await,
            "read_evaluable_tasks" => self.read_evaluable_tasks().await,
            "mark_task_in_progress" => self.mark_task_in_progress(params).await,
            _ => Err(ExecutionError::ActionFailed {
                action: action.to_string(),
                message: format!("unknown task-dispatch action: {action}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{InMemoryStateStore, StateStore};
    use pluresdb::{CrdtStore, MemoryStorage, StorageEngine};

    fn manager() -> Arc<TaskManager> {
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        Arc::new(TaskManager::new(Arc::new(store)))
    }

    fn handler_with_manager(task_manager: Arc<TaskManager>) -> TaskDispatchActionHandler {
        let state: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let dispatcher = Arc::new(TaskDispatcher::new(state));
        TaskDispatchActionHandler::new(dispatcher, Some(task_manager))
    }

    #[tokio::test]
    async fn read_evaluable_tasks_maps_open_status_to_pending_vocab() {
        let tm = manager();
        tm.create_task("finish p0", "chat-1", vec![]);

        let h = handler_with_manager(tm);
        let out = h.call("read_evaluable_tasks", &json!({})).await.unwrap();
        let arr = out.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["status"], "pending");
    }

    #[tokio::test]
    async fn mark_task_in_progress_updates_task_manager_state() {
        let tm = manager();
        let seeded = tm.create_task("run loop", "chat-1", vec![]);

        let h = handler_with_manager(Arc::clone(&tm));
        h.call("mark_task_in_progress", &json!({"task_id": seeded.id}))
            .await
            .unwrap();

        let after = tm.get_task(&seeded.id).expect("task exists");
        assert_eq!(after.status, TaskStatus::InProgress);
        assert_eq!(after.attempts, 1);
        assert!(after.last_evaluated_at.is_some());
    }

    #[test]
    fn autonomous_dispatch_px_uses_registered_task_dispatch_verbs() {
        let px = include_str!("../../../../praxis/procedures/autonomous-dispatch.px");

        // C-SPINE-001: .px task-dispatch IO must resolve to real Rust handlers.
        assert!(
            px.contains("read_evaluable_tasks {}"),
            "autonomous-dispatch must load tasks via read_evaluable_tasks"
        );
        assert!(
            px.contains("mark_task_in_progress {task_id: $best.id}"),
            "evaluate_dispatch must mark selected task in-progress via task seam"
        );
        assert!(
            px.contains("mark_task_in_progress {task_id: steer.task_id}"),
            "build_steered_prompt must mark steered task in-progress via task seam"
        );

        for required in ["dispatch_task", "read_evaluable_tasks", "mark_task_in_progress"] {
            assert!(
                TASK_DISPATCH_ACTIONS.contains(&required),
                "TASK_DISPATCH_ACTIONS missing required verb: {required}"
            );
        }
    }
}
