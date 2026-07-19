//! Thread-aware history recorder — records messages into the active thread.
//!
//! Replaces the flat HistoryRecorder by routing user and assistant messages
//! into the currently active thread for a given chat_id. Supports explicit
//! thread targeting via `thread_id` in event metadata (set by ThreadRoutingProcedure).

use std::sync::Arc;

use tracing::debug;

use crate::model::ChatMessage;
use crate::threading::store::ThreadStore;
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Records user and assistant messages into thread-aware storage.
///
/// When `metadata.thread_id` is present, messages are routed to that specific thread.
/// Otherwise, messages go to the currently active thread for the chat.
pub struct ThreadedHistoryRecorder {
    store: Arc<dyn ThreadStore>,
}

impl ThreadedHistoryRecorder {
    /// Create a new ThreadedHistoryRecorder with the given thread store.
    pub fn new(store: Arc<dyn ThreadStore>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ThreadedHistoryRecorder {
    fn name(&self) -> &str {
        "threaded_history_recorder"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["inbound", "model_response"])
    }

    async fn handle(&self, event: &SpineEvent, _emitter: &PipelineEmitter) {
        match event {
            SpineEvent::Inbound {
                chat_id,
                content,
                metadata,
                ..
            } => {
                let thread_id = metadata
                    .get("thread_id")
                    .and_then(|v| v.as_str());

                if let Some(tid) = thread_id {
                    debug!(
                        chat_id = %chat_id,
                        thread_id = %tid,
                        "threaded_history_recorder: recording user message to specific thread"
                    );
                    let _ = self
                        .store
                        .add_message_to_thread(chat_id, tid, ChatMessage::user(content))
                        .await;
                } else {
                    debug!(
                        chat_id = %chat_id,
                        "threaded_history_recorder: recording user message to active thread"
                    );
                    self.store
                        .add_message(chat_id, ChatMessage::user(content))
                        .await;
                }
            }
            SpineEvent::ModelResponse {
                chat_id,
                content,
                tool_calls,
                metadata,
                ..
            } if tool_calls.is_empty() && !content.is_empty() => {
                let thread_id = metadata
                    .get("thread_id")
                    .and_then(|v| v.as_str());

                if let Some(tid) = thread_id {
                    debug!(
                        chat_id = %chat_id,
                        thread_id = %tid,
                        "threaded_history_recorder: recording assistant message to specific thread"
                    );
                    let _ = self
                        .store
                        .add_message_to_thread(chat_id, tid, ChatMessage::assistant(content))
                        .await;
                } else {
                    debug!(
                        chat_id = %chat_id,
                        "threaded_history_recorder: recording assistant message to active thread"
                    );
                    self.store
                        .add_message(chat_id, ChatMessage::assistant(content))
                        .await;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolCall;
    use crate::threading::store::MemoryThreadStore;
    use serde_json::json;
    use tokio::sync::mpsc;

    fn make_emitter() -> PipelineEmitter {
        let (tx, _rx) = mpsc::channel(16);
        PipelineEmitter { tx }
    }

    async fn setup() -> (Arc<MemoryThreadStore>, ThreadedHistoryRecorder, PipelineEmitter) {
        let store = Arc::new(MemoryThreadStore::new());
        // Create a default thread so add_message has somewhere to go
        store.create_thread("chat-1", "general").await;
        let recorder = ThreadedHistoryRecorder::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let emitter = make_emitter();
        (store, recorder, emitter)
    }

    #[tokio::test]
    async fn records_user_message_to_active_thread() {
        let (store, recorder, emitter) = setup().await;

        let event = SpineEvent::Inbound {
            id: "1".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "Hello!".into(),
            metadata: json!({}),
        };

        recorder.handle(&event, &emitter).await;

        let active = store.active_thread("chat-1").await.unwrap();
        let history = store.thread_history("chat-1", &active.id).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello!");
    }

    #[tokio::test]
    async fn records_user_message_to_specific_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;
        let recorder = ThreadedHistoryRecorder::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let emitter = make_emitter();

        // Active thread is t2 (last created), but we target t1 explicitly
        let event = SpineEvent::Inbound {
            id: "1".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "For thread A".into(),
            metadata: json!({"thread_id": t1.id}),
        };

        recorder.handle(&event, &emitter).await;

        // t1 should have the message
        let history_t1 = store.thread_history("chat-1", &t1.id).await;
        assert_eq!(history_t1.len(), 1);
        assert_eq!(history_t1[0].content, "For thread A");

        // t2 should be empty
        let history_t2 = store.thread_history("chat-1", &t2.id).await;
        assert!(history_t2.is_empty());
    }

    #[tokio::test]
    async fn records_assistant_response_to_active_thread() {
        let (store, recorder, emitter) = setup().await;

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

        let active = store.active_thread("chat-1").await.unwrap();
        let history = store.thread_history("chat-1", &active.id).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "assistant");
        assert_eq!(history[0].content, "Hi there!");
    }

    #[tokio::test]
    async fn records_assistant_response_to_specific_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let _t2 = store.create_thread("chat-1", "topic-b").await;
        let recorder = ThreadedHistoryRecorder::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let emitter = make_emitter();

        let event = SpineEvent::ModelResponse {
            id: "2".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Response for A".into(),
            model: "gpt-4o".into(),
            tool_calls: vec![],
            metadata: json!({"thread_id": t1.id}),
        };

        recorder.handle(&event, &emitter).await;

        let history_t1 = store.thread_history("chat-1", &t1.id).await;
        assert_eq!(history_t1.len(), 1);
        assert_eq!(history_t1[0].content, "Response for A");
    }

    #[tokio::test]
    async fn does_not_record_tool_call_responses() {
        let (store, recorder, emitter) = setup().await;

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

        let active = store.active_thread("chat-1").await.unwrap();
        let history = store.thread_history("chat-1", &active.id).await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn does_not_record_empty_content_response() {
        let (store, recorder, emitter) = setup().await;

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

        let active = store.active_thread("chat-1").await.unwrap();
        let history = store.thread_history("chat-1", &active.id).await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn ignores_unhandled_events() {
        let (store, recorder, emitter) = setup().await;

        let event = SpineEvent::Timer {
            id: "5".into(),
            name: "task_eval".into(),
        };

        recorder.handle(&event, &emitter).await;

        let active = store.active_thread("chat-1").await.unwrap();
        let history = store.thread_history("chat-1", &active.id).await;
        assert!(history.is_empty());
    }
}
