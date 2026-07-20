//! Task-dispatch action handler — the Rust IO edge that closes the autonomous
//! task-execution loop (design: praxis/spine/spine.px IO boundary #5).
//!
//! Decision logic lives in `.px` (`autonomous-dispatch.px::evaluate_dispatch`).
//! That procedure decides WHICH task to run and builds the execution prompt,
//! then invokes the `dispatch_task` action exposed here. This handler performs
//! ONLY the side effect: hand the (task_id, prompt) to [`TaskDispatcher`], which
//! injects a `SpineEvent::Inbound{source:"task_executor", autonomous:true}` into
//! the SAME pipeline that handles user messages, then records the dispatch
//! timestamp to durable state.
//!
//! This mirrors the [`crate::spine::subagent_actor::SubagentActor`] convention:
//! an action handler holding runtime IO state (StateStore + PipelineEmitter),
//! composed into [`crate::spine::actions::CompositeActionHandler`] via a
//! post-construction setter (the emitter only exists after the pipeline is
//! built, so the handler is attached after `Pipeline::with_reactive`).
//!
//! If you're tempted to add "which task / should we dispatch" logic here, STOP.
//! It belongs in `.px` (C-DEV-001). This file is IO only.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::warn;

use crate::px_adapter::AsyncActionHandler;
use crate::task_executor::TaskDispatcher;
use pares_radix_praxis::px::executor::ExecutionError;

/// Action verb(s) this handler owns.
const TASK_DISPATCH_ACTIONS: &[&str] = &["dispatch_task"];

/// Returns true if `action` is handled by [`TaskDispatchActionHandler`].
pub fn is_task_dispatch_action(action: &str) -> bool {
    TASK_DISPATCH_ACTIONS.contains(&action)
}

/// Rust IO boundary that dispatches an autonomous task chosen by `.px`.
///
/// Wraps a [`TaskDispatcher`] built over the live [`StateStore`] and
/// [`PipelineEmitter`]. Exposes the `dispatch_task` action so
/// `evaluate_dispatch` (and any future decision procedure) can trigger a
/// dispatch inline once it has selected a task and built its prompt.
pub struct TaskDispatchActionHandler {
    dispatcher: Arc<TaskDispatcher>,
}

impl TaskDispatchActionHandler {
    /// Create a handler over an already-constructed [`TaskDispatcher`].
    ///
    /// The dispatcher MUST have a pipeline emitter set
    /// (`TaskDispatcher::with_pipeline_emitter`) or `dispatch` will no-op with a
    /// warning — it is the caller's responsibility to build it over the live
    /// pipeline emitter (see `runtime.rs::with_task_dispatch`).
    pub fn new(dispatcher: Arc<TaskDispatcher>) -> Self {
        Self { dispatcher }
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
}

#[async_trait]
impl AsyncActionHandler for TaskDispatchActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        match action {
            "dispatch_task" => self.dispatch_task(params).await,
            _ => Err(ExecutionError::ActionFailed {
                action: action.to_string(),
                message: format!("unknown task-dispatch action: {action}"),
            }),
        }
    }
}
