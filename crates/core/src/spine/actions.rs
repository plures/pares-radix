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
}

impl CoreActionHandler {
    pub fn new(conversation_store: Arc<dyn ConversationStore>) -> Self {
        Self { conversation_store }
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

    /// Read state from conversation store metadata.
    async fn read_state(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "read_state".into(),
                message: "missing key".into(),
            })?;

        // For now, conversation_tail and agent_promises map to conversation history
        // This will be replaced by PluresDB reads once fully wired.
        if key.starts_with("chat_history:") {
            let chat_id = key.strip_prefix("chat_history:").unwrap_or(key);
            return self
                .read_history(&serde_json::json!({"chat_id": chat_id}))
                .await;
        }

        // Unknown keys return null (not an error — state may not exist yet)
        debug!(key = %key, "action: read_state — key not found, returning null");
        Ok(Value::Null)
    }

    /// Write state (will become PluresDB write).
    async fn write_state(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        debug!(key = %key, "action: write_state (stub — will be PluresDB)");
        // TODO: Wire to PluresDB when available
        Ok(Value::Null)
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

/// Composite action handler that delegates to multiple handlers in priority order.
///
/// When a .px procedure calls an action:
/// 1. `CoreActionHandler` handles state/history actions (read_state, append_history, etc.)
/// 2. `ToolDispatchActionHandler` handles everything else as tool calls
///
/// This gives .px procedures access to both system state AND external tools.
pub struct CompositeActionHandler {
    core: CoreActionHandler,
    tool_handler: Arc<crate::px_adapter::ToolDispatchActionHandler>,
}

impl CompositeActionHandler {
    pub fn new(
        conversation_store: Arc<dyn ConversationStore>,
        tool_handler: Arc<crate::px_adapter::ToolDispatchActionHandler>,
    ) -> Self {
        Self {
            core: CoreActionHandler::new(conversation_store),
            tool_handler,
        }
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

    #[tokio::test]
    async fn append_and_read_history() {
        let store = Arc::new(MemoryConversationStore::new());
        let handler = CoreActionHandler::new(store);

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
        let handler = CoreActionHandler::new(Arc::clone(&store) as Arc<dyn ConversationStore>);

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
        let handler = CoreActionHandler::new(store);

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
        let handler = CoreActionHandler::new(store);

        let result = handler
            .call("some_unknown_action", &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result, Value::Null);
    }
}
