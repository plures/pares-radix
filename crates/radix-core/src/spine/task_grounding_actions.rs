//! Task-grounding action handler for the live reactive `.px` path.
//!
//! # Why this exists (pares-radix#467 — conversational task amnesia)
//!
//! The Rust [`ModelInvoker`](crate::spine::procedures::model_invoker::ModelInvoker)
//! already knows how to inject a durable open-tasks block into the model prompt
//! (via [`render_open_tasks_block`]), but that SpineProcedure is **not on the
//! live serve path** — the shipped runtime drives the model through the reactive
//! `.px` engine (`praxis/spine/conversation.px::build_context` →
//! `praxis/spine/model_invoke.px::invoke_model`). So the task-grounding block
//! never reached an inbound Telegram turn and the agent "forgot" its
//! obligations across turns.
//!
//! This handler closes that gap by exposing a real `read_open_tasks_block`
//! action that `build_context` calls. It reads the SAME durable PluresDB
//! [`CrdtStore`] the [`TaskManager`] writes to (C-PLURES-003/004: task state
//! lives in PluresDB, read straight from the store — no Rust-memory task
//! state), renders the shared block, and returns it as a string the `.px`
//! procedure prepends to the system prompt. When there are no open tasks it
//! returns an empty string so no noise block is injected.

use std::sync::Arc;

use async_trait::async_trait;
use pares_radix_praxis::px::executor::ExecutionError;
use serde_json::Value;
use tracing::debug;

use crate::px_adapter::AsyncActionHandler;
use crate::task_manager::{render_open_tasks_block, TaskManager};

/// The action name this handler owns.
pub const READ_OPEN_TASKS_BLOCK: &str = "read_open_tasks_block";

/// Returns `true` if `action` is handled by [`TaskGroundingActionHandler`].
pub fn is_task_grounding_action(action: &str) -> bool {
    action == READ_OPEN_TASKS_BLOCK
}

/// Reads the agent's durable open tasks from the shared PluresDB store and
/// renders a grounding block for injection into the model system prompt.
pub struct TaskGroundingActionHandler {
    task_manager: Arc<TaskManager>,
}

impl TaskGroundingActionHandler {
    /// Construct over a [`TaskManager`] backed by the shared PluresDB store.
    pub fn new(task_manager: Arc<TaskManager>) -> Self {
        Self { task_manager }
    }
}

#[async_trait]
impl AsyncActionHandler for TaskGroundingActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        if action != READ_OPEN_TASKS_BLOCK {
            return Err(ExecutionError::ActionFailed {
                action: action.to_string(),
                message: "TaskGroundingActionHandler only handles read_open_tasks_block".into(),
            });
        }

        // chat_id is optional: absent/empty falls back to global open tasks
        // (render_open_tasks_block already unions chat-scoped + global).
        let chat_id = params.get("chat_id").and_then(|v| v.as_str()).unwrap_or("");

        // Optional base system prompt. When provided, we return the combined
        // prompt (grounding block prepended) so the `.px` `build_context` step
        // can bind a single ready-to-send `system_prompt`. When absent we return
        // just the block (or empty string) for callers that combine themselves.
        let base = params.get("base").and_then(|v| v.as_str());

        let block = render_open_tasks_block(&self.task_manager, chat_id);
        debug!(
            chat_id = %chat_id,
            has_tasks = block.is_some(),
            with_base = base.is_some(),
            "action: read_open_tasks_block"
        );

        let out = match (block, base) {
            // Prepend the durable block to the base prompt (blank line between).
            (Some(b), Some(base)) => format!("{b}\n\n{base}"),
            (Some(b), None) => b,
            // No open tasks: pass the base prompt through unchanged (no noise).
            (None, Some(base)) => base.to_string(),
            (None, None) => String::new(),
        };
        Ok(Value::String(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluresdb::{CrdtStore, MemoryStorage};

    fn manager() -> Arc<TaskManager> {
        let storage: Arc<dyn pluresdb::StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        Arc::new(TaskManager::new(Arc::new(store)))
    }

    #[tokio::test]
    async fn empty_when_no_tasks() {
        let h = TaskGroundingActionHandler::new(manager());
        let out = h
            .call(READ_OPEN_TASKS_BLOCK, &serde_json::json!({"chat_id": "c1"}))
            .await
            .unwrap();
        assert_eq!(out, Value::String(String::new()));
    }

    #[tokio::test]
    async fn renders_block_for_open_task() {
        let mgr = manager();
        mgr.create_task("ship the 467 fix", "c1", vec![]);
        let h = TaskGroundingActionHandler::new(Arc::clone(&mgr));
        let out = h
            .call(READ_OPEN_TASKS_BLOCK, &serde_json::json!({"chat_id": "c1"}))
            .await
            .unwrap();
        let s = out.as_str().unwrap();
        assert!(s.contains("ship the 467 fix"), "block missing task: {s}");
        assert!(
            s.contains("open tasks/commitments"),
            "block missing header: {s}"
        );
    }

    #[tokio::test]
    async fn combines_block_with_base_prompt() {
        let mgr = manager();
        mgr.create_task("finish 467", "c1", vec![]);
        let h = TaskGroundingActionHandler::new(Arc::clone(&mgr));
        let out = h
            .call(
                READ_OPEN_TASKS_BLOCK,
                &serde_json::json!({"chat_id": "c1", "base": "You are praxisbot."}),
            )
            .await
            .unwrap();
        let s = out.as_str().unwrap();
        assert!(s.contains("finish 467"), "missing task: {s}");
        assert!(s.contains("You are praxisbot."), "missing base prompt: {s}");
        // Block comes first, base prompt after.
        let task_pos = s.find("finish 467").unwrap();
        let base_pos = s.find("You are praxisbot.").unwrap();
        assert!(
            task_pos < base_pos,
            "grounding block must precede base prompt"
        );
    }

    #[tokio::test]
    async fn base_passthrough_when_no_tasks() {
        let h = TaskGroundingActionHandler::new(manager());
        let out = h
            .call(
                READ_OPEN_TASKS_BLOCK,
                &serde_json::json!({"chat_id": "c1", "base": "BASE"}),
            )
            .await
            .unwrap();
        assert_eq!(out, Value::String("BASE".to_string()));
    }

    #[tokio::test]
    async fn rejects_unknown_action() {
        let h = TaskGroundingActionHandler::new(manager());
        assert!(h.call("nope", &serde_json::json!({})).await.is_err());
    }

    #[test]
    fn action_predicate() {
        assert!(is_task_grounding_action(READ_OPEN_TASKS_BLOCK));
        assert!(!is_task_grounding_action("read_state"));
    }
}
