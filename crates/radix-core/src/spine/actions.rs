//! Action handlers — Rust IO boundary implementations for .px procedure actions.
//!
//! .px procedures call actions like `read_state`, `append_history`, `embed_text`,
//! `model_complete`, `channel_send`, etc. This module provides the Rust
//! implementations that perform actual side effects.
//!
//! Architecture principle: .px decides WHAT to do, Rust actors decide HOW.
//! Every action here is a thin wrapper around an existing capability.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, warn};

use crate::model::ChatMessage;
use crate::px_adapter::AsyncActionHandler;
use crate::spine::conversation::ConversationStore;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// Trait for handling actions dispatched from .px procedure execution.
///
/// Re-exported from px_adapter for convenience. The ReactiveRegistry calls this
/// when a .px procedure invokes an action like `read_state`, `append_history`, etc.
pub use crate::px_adapter::AsyncActionHandler as ActionHandler;


/// Core action handler that provides conversation/state management to .px procedures.
///
/// This is the minimal set of actions needed for the spine pipeline to function.
/// Additional handlers (model, tools, channel) are composed separately.
pub struct CoreActionHandler {
    conversation_store: Arc<dyn ConversationStore>,
    /// Durable key-value state store (PluresDB-backed in production).
    ///
    /// `write_state` / `read_state` round-trip general keys through this store,
    /// giving every `.px` procedure (dev-lifecycle.px included) real persistence.
    state_store: Arc<dyn StateStore>,
}

impl CoreActionHandler {
    /// Construct a core handler backed by a conversation store (for history
    /// actions) and a [`StateStore`] (for general `read_state`/`write_state`).
    pub fn new(
        conversation_store: Arc<dyn ConversationStore>,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        Self {
            conversation_store,
            state_store,
        }
    }

    /// Read conversation history for a chat.
    async fn read_history(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "read_history".into(),
                message: "missing chat_id".into(),
            })?;

        let history = self.conversation_store.get_history(chat_id).await;
        let json_history: Vec<Value> = history
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "role": msg.role,
                    "content": msg.content,
                })
            })
            .collect();

        Ok(Value::Array(json_history))
    }

    /// Append a message to conversation history.
    async fn append_history(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "append_history".into(),
                message: "missing chat_id".into(),
            })?;

        let role = params
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("user");

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let msg = match role {
            "assistant" => ChatMessage::assistant(content),
            "system" => ChatMessage::system(content),
            _ => ChatMessage::user(content),
        };

        self.conversation_store.add_message(chat_id, msg).await;
        debug!(chat_id = %chat_id, role = %role, "action: append_history");
        Ok(Value::Null)
    }

    /// Read state by key.
    ///
    /// The `chat_history:<chat_id>` prefix remains a virtual key projected from
    /// the conversation store (the pipeline relies on it). Every other key is a
    /// durable read from the [`StateStore`]; an absent key returns `Value::Null`
    /// (not an error — state may simply not exist yet).
    async fn read_state(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "read_state".into(),
                message: "missing key".into(),
            })?;

        // `chat_history:` is a virtual projection over the conversation store.
        if key.starts_with("chat_history:") {
            let chat_id = key.strip_prefix("chat_history:").unwrap_or(key);
            return self
                .read_history(&serde_json::json!({"chat_id": chat_id}))
                .await;
        }

        // General keys round-trip through the durable state store.
        let value = self.state_store.get(key).await.unwrap_or(Value::Null);
        debug!(key = %key, found = !value.is_null(), "action: read_state");
        Ok(value)
    }

    /// Write state by key into the durable [`StateStore`].
    ///
    /// Returns the value that was written so `.px` steps can bind it
    /// (e.g. `write_state {...} -> $written`).
    async fn write_state(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "write_state".into(),
                message: "missing key".into(),
            })?;

        let value = params.get("value").cloned().unwrap_or(Value::Null);
        self.state_store.set(key, value.clone()).await;
        debug!(key = %key, "action: write_state");
        Ok(value)
    }
}

#[async_trait]
impl AsyncActionHandler for CoreActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        match action {
            "read_state" => self.read_state(params).await,
            "write_state" => self.write_state(params).await,
            "read_history" => self.read_history(params).await,
            "append_history" => self.append_history(params).await,
            _ => {
                warn!(action = %action, "unknown action — returning null");
                Ok(Value::Null)
            }
        }
    }
}

use crate::spine::dev_lifecycle_actions::{is_dev_lifecycle_action, DevLifecycleActionHandler};
use crate::spine::briefing_actions::{is_briefing_action, BriefingActionHandler};
use crate::spine::run_command_actions::{is_run_command_action, RunCommandActionHandler};
use crate::spine::task_grounding_actions::{
    is_task_grounding_action, TaskGroundingActionHandler,
};
use crate::spine::subagent_actor::{is_subagent_action, SubagentActor};
use crate::spine::task_dispatch_actions::{is_task_dispatch_action, TaskDispatchActionHandler};
use crate::spine::worktask_actions::{is_worktask_action, WorktaskActionHandler};

/// Composite action handler that delegates to multiple handlers in priority order.
///
/// When a .px procedure calls an action:
/// 1. `CoreActionHandler` handles state/history actions (read_state, append_history, etc.)
/// 2. `DevLifecycleActionHandler` handles stage management actions
/// 3. `WorktaskActionHandler` handles worktask git/fs/quarantine effects
/// 4. `RunCommandActionHandler` handles `run_command` (real ShellExecutor, governed)
/// 5. `BriefingActionHandler` handles `assemble_briefing_report` (pure classify/format)
/// 6. `SubagentActor` handles spawn_subagent calls
/// 7. `ToolDispatchActionHandler` handles everything else as tool calls
///
/// This gives .px procedures access to system state, lifecycle logic, worktask
/// orchestration, shell commands, subagent spawning, AND external tools.
pub struct CompositeActionHandler {
    core: CoreActionHandler,
    dev_lifecycle: DevLifecycleActionHandler,
    worktask: WorktaskActionHandler,
    run_command: RunCommandActionHandler,
    briefing: BriefingActionHandler,
    /// Durable task-grounding handler (`read_open_tasks_block`). `None` when the
    /// runtime was assembled without a task store; the action then returns null
    /// and `.px` injects no block (honest absence, never a stub).
    task_grounding: Option<TaskGroundingActionHandler>,
    subagent: Option<Arc<SubagentActor>>,
    /// Autonomous task-dispatch IO edge (`dispatch_task`). `None` until wired
    /// via [`CompositeActionHandler::set_task_dispatch`] after the pipeline
    /// emitter exists. When absent the action returns a real "not wired" error
    /// (honest absence, never a stub).
    task_dispatch: Option<Arc<TaskDispatchActionHandler>>,
    tool_handler: Arc<crate::px_adapter::ToolDispatchActionHandler>,
}

impl CompositeActionHandler {
    pub fn new(
        conversation_store: Arc<dyn ConversationStore>,
        state_store: Arc<dyn StateStore>,
        tool_handler: Arc<crate::px_adapter::ToolDispatchActionHandler>,
    ) -> Self {
        Self {
            // Worktask shares the SAME durable state store as Core so worktask
            // records and general `.px` state co-locate in one PluresDB.
            worktask: WorktaskActionHandler::new(Arc::clone(&state_store)),
            core: CoreActionHandler::new(conversation_store, state_store),
            dev_lifecycle: DevLifecycleActionHandler::new(),
            run_command: RunCommandActionHandler::new(),
            briefing: BriefingActionHandler::new(),
            task_grounding: None,
            subagent: None,
            task_dispatch: None,
            tool_handler,
        }
    }

    /// Attach the durable [`TaskGroundingActionHandler`] so the live reactive
    /// `.px` path (`build_context`) can inject the persisted open-tasks block
    /// into the model system prompt each inbound turn (pares-radix#467).
    ///
    /// The manager must be backed by the SAME PluresDB store as the rest of the
    /// runtime so tasks written by the task procedures are read here
    /// (C-PLURES-003/004).
    pub fn with_task_grounding(
        mut self,
        task_manager: Arc<crate::task_manager::TaskManager>,
    ) -> Self {
        self.task_grounding = Some(TaskGroundingActionHandler::new(task_manager));
        self
    }

    /// Set the subagent actor after construction (breaks circular dependency).
    pub fn set_subagent_actor(&mut self, actor: Arc<SubagentActor>) {
        self.subagent = Some(actor);
    }

    /// Attach the autonomous task-dispatch IO edge after construction.
    ///
    /// Mirrors [`set_subagent_actor`](Self::set_subagent_actor): the handler
    /// wraps a [`TaskDispatcher`](crate::task_executor::TaskDispatcher) built
    /// over the live pipeline emitter, which only exists after the pipeline is
    /// constructed — so it is attached post-construction.
    pub fn set_task_dispatch(&mut self, handler: Arc<TaskDispatchActionHandler>) {
        self.task_dispatch = Some(handler);
    }
}

/// Actions that the core handler knows about (state/conversation).
const CORE_ACTIONS: &[&str] = &[
    "read_state",
    "write_state",
    "read_history",
    "append_history",
];

#[async_trait]
impl AsyncActionHandler for CompositeActionHandler {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, ExecutionError> {
        if CORE_ACTIONS.contains(&action) {
            self.core.call(action, params).await
        } else if is_dev_lifecycle_action(action) {
            self.dev_lifecycle.call(action, params).await
        } else if is_worktask_action(action) {
            self.worktask.call(action, params).await
        } else if is_run_command_action(action) {
            self.run_command.call(action, params).await
        } else if is_briefing_action(action) {
            self.briefing.call(action, params).await
        } else if is_task_grounding_action(action) {
            if let Some(ref h) = self.task_grounding {
                h.call(action, params).await
            } else {
                // No task store wired — degrade gracefully. If `.px` supplied a base prompt, pass it through.
                warn!(action = %action, "task grounding not wired — passing through base prompt");
                let base = params.get("base").and_then(|v| v.as_str());
                Ok(Value::String(base.unwrap_or("").to_string()))
            }
        } else if is_subagent_action(action) {
            if let Some(ref actor) = self.subagent {
                actor.call(action, params).await
            } else {
                warn!(action = %action, "subagent actor not configured");
                Err(ExecutionError::ActionFailed {
                    action: action.to_string(),
                    message: "subagent actor not wired — SubAgentManager not available".into(),
                })
            }
        } else if is_task_dispatch_action(action) {
            if let Some(ref h) = self.task_dispatch {
                h.call(action, params).await
            } else {
                // Not wired (no pipeline emitter yet) — honest error, not a stub.
                warn!(action = %action, "task-dispatch not wired — TaskDispatcher unavailable");
                Err(ExecutionError::ActionFailed {
                    action: action.to_string(),
                    message: "task-dispatch not wired — TaskDispatcher/pipeline emitter not available".into(),
                })
            }
        } else {
            // Delegate to tool dispatcher for unknown actions (tool calls)
            self.tool_handler.call(action, params).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spine::conversation::MemoryConversationStore;
    use crate::state::InMemoryStateStore;

    fn test_state() -> Arc<dyn StateStore> {
        Arc::new(InMemoryStateStore::new())
    }

    #[tokio::test]
    async fn append_and_read_history() {
        let store = Arc::new(MemoryConversationStore::new());
        let handler = CoreActionHandler::new(store, test_state());

        // Append a user message
        let result = handler
            .call(
                "append_history",
                &serde_json::json!({"chat_id": "test-1", "role": "user", "content": "hello"}),
            )
            .await
            .unwrap();
        assert_eq!(result, Value::Null);

        // Append an assistant response
        handler
            .call(
                "append_history",
                &serde_json::json!({"chat_id": "test-1", "role": "assistant", "content": "hi"}),
            )
            .await
            .unwrap();

        // Read history
        let history = handler
            .call(
                "read_history",
                &serde_json::json!({"chat_id": "test-1"}),
            )
            .await
            .unwrap();

        let arr = history.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "user");
        assert_eq!(arr[0]["content"], "hello");
        assert_eq!(arr[1]["role"], "assistant");
        assert_eq!(arr[1]["content"], "hi");
    }

    #[tokio::test]
    async fn read_state_chat_history_prefix() {
        let store = Arc::new(MemoryConversationStore::new());
        let handler = CoreActionHandler::new(Arc::clone(&store) as Arc<dyn ConversationStore>, test_state());

        // Add a message directly to store
        store
            .add_message("chat-42", ChatMessage::user("test msg"))
            .await;

        // read_state with chat_history: prefix
        let result = handler
            .call(
                "read_state",
                &serde_json::json!({"key": "chat_history:chat-42"}),
            )
            .await
            .unwrap();

        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["content"], "test msg");
    }

    #[tokio::test]
    async fn read_state_unknown_key_returns_null() {
        let store = Arc::new(MemoryConversationStore::new());
        let handler = CoreActionHandler::new(store, test_state());

        let result = handler
            .call(
                "read_state",
                &serde_json::json!({"key": "nonexistent_key"}),
            )
            .await
            .unwrap();

        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn unknown_action_returns_null() {
        let store = Arc::new(MemoryConversationStore::new());
        let handler = CoreActionHandler::new(store, test_state());

        let result = handler
            .call("some_unknown_action", &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result, Value::Null);
    }

    /// END-TO-END LIVE-PATH PROOF (pares-radix#467 — task amnesia):
    ///
    /// Assemble the [`CompositeActionHandler`] EXACTLY as the shipped serve
    /// runtime does (`build_reactive_runtime_with_tasks`): a real
    /// [`TaskManager`](crate::task_manager::TaskManager) over the SAME PluresDB
    /// `CrdtStore` the task procedures write to, wired via `with_task_grounding`.
    /// Persist an open task, then invoke the `read_open_tasks_block` action the
    /// live `praxis/spine/conversation.px::build_context` step calls — passing
    /// the base system prompt — and assert the durable open-tasks block is
    /// injected ahead of the base prompt in the string that becomes the inbound
    /// turn's `system_prompt`. This proves the grounding reaches every inbound
    /// turn on the live reactive path, NOT via the (test-only) Rust ModelInvoker.
    #[tokio::test]
    async fn live_path_injects_open_tasks_block_into_system_prompt() {
        use crate::model::{ToolDefinition, ToolDispatcher};
        use crate::px_adapter::ToolDispatchActionHandler;
        use crate::task_manager::TaskManager;
        use pluresdb::{CrdtStore, MemoryStorage};

        // A trivial tool dispatcher — the read_open_tasks_block action never
        // reaches it (it's handled before the tool fallthrough).
        struct NullDispatcher;
        #[async_trait]
        impl ToolDispatcher for NullDispatcher {
            async fn available_tools(&self) -> Vec<ToolDefinition> {
                vec![]
            }
            async fn call_tool(&self, _name: &str, _args: Value) -> String {
                "null".to_string()
            }
        }

        // Shared store used by BOTH the task manager and (conceptually) state.
        let storage: Arc<dyn pluresdb::StorageEngine> = Arc::new(MemoryStorage::default());
        let crdt = Arc::new(CrdtStore::default().with_persistence(storage));
        let task_manager = Arc::new(TaskManager::new(Arc::clone(&crdt)));

        // Persist a real open task for this chat.
        task_manager.create_task("finish the praxisbot 467 fix", "tg-chat-1", vec![]);

        // Build the composite exactly like the runtime, with task grounding.
        let conv: Arc<dyn ConversationStore> = Arc::new(MemoryConversationStore::new());
        let tool_handler = Arc::new(ToolDispatchActionHandler::new(Arc::new(NullDispatcher)));
        let composite = CompositeActionHandler::new(conv, test_state(), tool_handler)
            .with_task_grounding(Arc::clone(&task_manager));

        // Drive the live action the .px build_context step calls.
        let base = "You are praxisbot, a helpful agent.";
        let out = composite
            .call(
                "read_open_tasks_block",
                &serde_json::json!({"chat_id": "tg-chat-1", "base": base}),
            )
            .await
            .unwrap();
        let grounded = out.as_str().expect("action returns a string prompt");

        assert!(
            grounded.contains("finish the praxisbot 467 fix"),
            "live system prompt must carry the persisted open task; got: {grounded}"
        );
        assert!(
            grounded.contains("open tasks/commitments"),
            "live system prompt must carry the grounding header; got: {grounded}"
        );
        assert!(
            grounded.contains(base),
            "base system prompt must be preserved; got: {grounded}"
        );
        // Grounding block precedes the base prompt.
        assert!(
            grounded.find("finish the praxisbot 467 fix").unwrap()
                < grounded.find(base).unwrap(),
            "grounding block must be prepended before the base prompt"
        );
    }
}
