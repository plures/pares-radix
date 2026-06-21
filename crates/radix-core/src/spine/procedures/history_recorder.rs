//! History recorder procedure — records messages into the ConversationStore.
//!
//! Subscribes to Inbound events (records user messages) and ModelResponse events
//! with content but no tool_calls (records final assistant responses).

use std::sync::Arc;

use tracing::debug;

use crate::model::ChatMessage;
use crate::spine::conversation::ConversationStore;
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Records user and assistant messages into a ConversationStore for multi-turn context.
pub struct HistoryRecorder {
    store: Arc<dyn ConversationStore>,
}

impl HistoryRecorder {
    /// Create a new HistoryRecorder with the given store.
    pub fn new(store: Arc<dyn ConversationStore>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl SpineProcedure for HistoryRecorder {
    fn name(&self) -> &str {
        "history_recorder"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["inbound", "model_response"])
    }

    async fn handle(&self, event: &SpineEvent, _emitter: &PipelineEmitter) {
        match event {
            SpineEvent::Inbound {
                chat_id, content, ..
            } => {
                debug!(chat_id = %chat_id, "history_recorder: recording user message");
                self.store
                    .add_message(chat_id, ChatMessage::user(content))
                    .await;
            }
            SpineEvent::ModelResponse {
                chat_id,
                content,
                tool_calls,
                ..
            } if tool_calls.is_empty() && !content.is_empty() => {
                debug!(chat_id = %chat_id, "history_recorder: recording assistant message");
                self.store
                    .add_message(chat_id, ChatMessage::assistant(content))
                    .await;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolCall;
    use crate::spine::conversation::MemoryConversationStore;
    use serde_json::json;
    use tokio::sync::mpsc;

    fn make_emitter() -> PipelineEmitter {
        let (tx, _rx) = mpsc::channel(16);
        PipelineEmitter { tx }
    }

    #[tokio::test]
    async fn records_user_message_on_inbound() {
        let store = Arc::new(MemoryConversationStore::new());
        let recorder = HistoryRecorder::new(Arc::clone(&store) as Arc<dyn ConversationStore>);
        let emitter = make_emitter();

        let event = SpineEvent::Inbound {
            id: "1".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "Hello!".into(),
            metadata: json!({}),
        };

        recorder.handle(&event, &emitter).await;

        let history = store.get_history("chat-1").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello!");
    }

    #[tokio::test]
    async fn records_final_assistant_response() {
        let store = Arc::new(MemoryConversationStore::new());
        let recorder = HistoryRecorder::new(Arc::clone(&store) as Arc<dyn ConversationStore>);
        let emitter = make_emitter();

        let event = SpineEvent::ModelResponse {
            id: "2".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Hi there!".into(),
            model: "gpt-4o".into(),
            tool_calls: vec![],
            metadata: json!({}),
        };

        recorder.handle(&event, &emitter).await;

        let history = store.get_history("chat-1").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "assistant");
        assert_eq!(history[0].content, "Hi there!");
    }

    #[tokio::test]
    async fn does_not_record_tool_call_responses() {
        let store = Arc::new(MemoryConversationStore::new());
        let recorder = HistoryRecorder::new(Arc::clone(&store) as Arc<dyn ConversationStore>);
        let emitter = make_emitter();

        let event = SpineEvent::ModelResponse {
            id: "3".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "".into(),
            model: "gpt-4o".into(),
            tool_calls: vec![ToolCall {
                id: "tc-1".into(),
                name: "web_search".into(),
                arguments: json!({"query": "test"}),
            }],
            metadata: json!({}),
        };

        recorder.handle(&event, &emitter).await;

        let history = store.get_history("chat-1").await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn does_not_record_empty_content_response() {
        let store = Arc::new(MemoryConversationStore::new());
        let recorder = HistoryRecorder::new(Arc::clone(&store) as Arc<dyn ConversationStore>);
        let emitter = make_emitter();

        let event = SpineEvent::ModelResponse {
            id: "4".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "".into(),
            model: "gpt-4o".into(),
            tool_calls: vec![],
            metadata: json!({}),
        };

        recorder.handle(&event, &emitter).await;

        let history = store.get_history("chat-1").await;
        assert!(history.is_empty());
    }
}
