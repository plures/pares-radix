//! Threaded delivery procedure — enriches DeliveryRequest with thread context.
//!
//! Sits in the pipeline listening for DeliveryRequest events and enriches
//! them with thread metadata. Channel adapters use this metadata to:
//! - Add topic indicators to messages
//! - Use reply chains for visual threading
//! - Present thread context in their native format

use std::sync::Arc;

use serde_json::json;
use tracing::debug;

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::threading::store::ThreadStore;

/// Style for thread topic indicators prepended to message content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorStyle {
    /// Emoji prefix: `[📎 topic] `
    Emoji,
    /// Bracket prefix: `[topic] `
    Bracket,
    /// No indicator prefix.
    None,
}

/// Enriches `DeliveryRequest` events with thread context before channel adapters deliver them.
///
/// When multiple threads exist for a chat, this procedure prepends a topic indicator
/// to the message content so users can visually distinguish which thread a response
/// belongs to. It also propagates channel anchor metadata (e.g., `reply_to_message_id`
/// for Telegram) so adapters can use native reply chains.
pub struct ThreadedDelivery {
    store: Arc<dyn ThreadStore>,
    /// Whether to include thread topic as indicator in message content.
    show_indicators: bool,
    /// Indicator style for thread topic prefix.
    indicator_style: IndicatorStyle,
}

impl ThreadedDelivery {
    /// Create a new ThreadedDelivery with defaults (indicators on, emoji style).
    pub fn new(store: Arc<dyn ThreadStore>) -> Self {
        Self {
            store,
            show_indicators: true,
            indicator_style: IndicatorStyle::Emoji,
        }
    }

    /// Set whether to show thread indicators.
    pub fn with_indicators(mut self, show: bool) -> Self {
        self.show_indicators = show;
        self
    }

    /// Set the indicator style.
    pub fn with_indicator_style(mut self, style: IndicatorStyle) -> Self {
        self.indicator_style = style;
        self
    }

    fn format_indicator(&self, topic: &str) -> String {
        match self.indicator_style {
            IndicatorStyle::Emoji => format!("[📎 {}] ", topic),
            IndicatorStyle::Bracket => format!("[{}] ", topic),
            IndicatorStyle::None => String::new(),
        }
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ThreadedDelivery {
    fn name(&self) -> &str {
        "threaded_delivery"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["delivery_request"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::DeliveryRequest {
            id,
            channel,
            chat_id,
            content,
            metadata,
        } = event
        else {
            return;
        };

        // Skip if already processed (prevent infinite loop — pipeline re-dispatches emitted events)
        if metadata
            .get("thread_formatted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            debug!(event_id = %id, "threaded_delivery: already formatted, skipping");
            return;
        }

        // Get active thread for this chat
        let active = match self.store.active_thread(chat_id).await {
            Some(t) => t,
            None => {
                debug!(chat_id = %chat_id, "threaded_delivery: no active thread, skipping");
                return;
            }
        };

        // Only add indicators when multiple threads exist (don't clutter single-thread chats)
        let threads = self.store.list_threads(chat_id).await;
        let should_indicate = self.show_indicators && threads.len() > 1;

        let formatted_content = if should_indicate {
            format!("{}{}", self.format_indicator(&active.topic), content)
        } else {
            content.clone()
        };

        // Enrich metadata with thread delivery info
        let mut enriched_meta = metadata.clone();
        if let Some(obj) = enriched_meta.as_object_mut() {
            obj.insert("thread_formatted".to_string(), json!(true));
            obj.insert("thread_id".to_string(), json!(active.id));
            obj.insert("thread_topic".to_string(), json!(active.topic));
            // Channel adapters use this to set reply_to_message_id (Telegram)
            // or thread indicators (TUI tabs, etc.)
            if let Some(anchor) = &active.channel_anchor {
                if let Some(anchor_obj) = anchor.as_object() {
                    if let Some(msg_id) = anchor_obj.get("message_id") {
                        obj.insert("reply_to_message_id".to_string(), msg_id.clone());
                    }
                }
            }
        }

        debug!(
            event_id = %id,
            chat_id = %chat_id,
            thread_id = %active.id,
            thread_topic = %active.topic,
            indicated = should_indicate,
            "threaded_delivery: enriched delivery with thread context"
        );

        // Re-emit with formatted content and enriched metadata
        emitter
            .emit(SpineEvent::DeliveryRequest {
                id: id.clone(),
                channel: channel.clone(),
                chat_id: chat_id.clone(),
                content: formatted_content,
                metadata: enriched_meta,
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;
    use serde_json::json;
    use tokio::sync::mpsc;

    fn make_emitter() -> (PipelineEmitter, mpsc::Receiver<SpineEvent>) {
        let (tx, rx) = mpsc::channel(16);
        (PipelineEmitter { tx }, rx)
    }

    #[tokio::test]
    async fn single_thread_no_indicator() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "general").await;

        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-1".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Hello!".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        if let SpineEvent::DeliveryRequest {
            content, metadata, ..
        } = emitted
        {
            // Single thread → no indicator prepended
            assert_eq!(content, "Hello!");
            assert_eq!(metadata["thread_formatted"], json!(true));
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn multi_thread_emoji_indicator() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "debugging").await;
        store.create_thread("chat-1", "architecture").await;

        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-2".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Here's the fix.".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        if let SpineEvent::DeliveryRequest {
            content, metadata, ..
        } = emitted
        {
            // Active thread is "architecture" (last created), multi-thread → emoji indicator
            assert_eq!(content, "[📎 architecture] Here's the fix.");
            assert_eq!(metadata["thread_formatted"], json!(true));
            assert_eq!(metadata["thread_topic"], json!("architecture"));
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn multi_thread_bracket_indicator() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>)
            .with_indicator_style(IndicatorStyle::Bracket);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-3".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Response".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        if let SpineEvent::DeliveryRequest { content, .. } = emitted {
            assert_eq!(content, "[topic-b] Response");
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn none_style_no_indicator_even_with_multi_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>)
            .with_indicator_style(IndicatorStyle::None);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-4".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Plain".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        if let SpineEvent::DeliveryRequest { content, .. } = emitted {
            // None style → format_indicator returns "" so content is unchanged
            assert_eq!(content, "Plain");
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn already_formatted_skipped() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "general").await;
        store.create_thread("chat-1", "other").await;

        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let (emitter, mut rx) = make_emitter();

        // Event with thread_formatted already set
        let event = SpineEvent::DeliveryRequest {
            id: "d-5".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "[📎 other] Already formatted".into(),
            metadata: json!({"thread_formatted": true}),
        };

        delivery.handle(&event, &emitter).await;

        // Nothing should be emitted (skipped)
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn channel_anchor_propagated() {
        let store = Arc::new(MemoryThreadStore::new());
        let mut thread = store.create_thread("chat-1", "debugging").await;
        // Set a channel anchor on the thread
        thread.channel_anchor = Some(json!({"message_id": 12345}));
        // Update via creating a second thread so we have multi-thread, then switch back
        store.create_thread("chat-1", "other").await;
        // Directly manipulate the store to set anchor on first thread
        // Use switch_thread then set anchor manually — since MemoryThreadStore uses RwLock
        // we'll work around by checking the propagation through a custom store setup.
        // For this test, use PluresThreadStore::in_memory which gives us more control.
        drop(store);

        // Set up with a store that has anchor data
        let store = Arc::new(MemoryThreadStoreWithAnchor::new());
        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-6".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Reply".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        if let SpineEvent::DeliveryRequest { metadata, .. } = emitted {
            assert_eq!(metadata["reply_to_message_id"], json!(98765));
            assert_eq!(metadata["thread_id"], json!("anchor-thread"));
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn show_indicators_false_no_indicator() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>)
            .with_indicators(false);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-7".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "No prefix".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        if let SpineEvent::DeliveryRequest { content, .. } = emitted {
            assert_eq!(content, "No prefix");
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn no_active_thread_skips() {
        let store = Arc::new(MemoryThreadStore::new());
        // Don't create any threads for chat-1 — active_thread returns None
        let delivery = ThreadedDelivery::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let (emitter, mut rx) = make_emitter();

        let event = SpineEvent::DeliveryRequest {
            id: "d-8".into(),
            channel: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "Orphan".into(),
            metadata: json!({}),
        };

        delivery.handle(&event, &emitter).await;

        // Nothing emitted when no active thread
        assert!(rx.try_recv().is_err());
    }

    // ── Helper: MemoryThreadStore variant with anchor data ──────────────────

    use crate::model::ChatMessage;
    use crate::threading::store::ThreadStoreError;
    use crate::threading::types::Thread;

    /// A minimal thread store that returns a thread with a channel anchor, for testing.
    struct MemoryThreadStoreWithAnchor;

    impl MemoryThreadStoreWithAnchor {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait::async_trait]
    impl ThreadStore for MemoryThreadStoreWithAnchor {
        async fn active_thread(&self, _chat_id: &str) -> Option<Thread> {
            let mut thread = Thread::new("anchor-thread", "chat-1", "anchored");
            thread.channel_anchor = Some(json!({"message_id": 98765}));
            Some(thread)
        }

        async fn switch_thread(
            &self,
            _chat_id: &str,
            _thread_id: &str,
        ) -> Result<Thread, ThreadStoreError> {
            Err(ThreadStoreError::NotFound("n/a".into()))
        }

        async fn create_thread(&self, _chat_id: &str, topic: &str) -> Thread {
            Thread::new("new", "chat-1", topic)
        }

        async fn thread_history(&self, _chat_id: &str, _thread_id: &str) -> Vec<ChatMessage> {
            vec![]
        }

        async fn add_message(&self, _chat_id: &str, _message: ChatMessage) {}

        async fn add_message_to_thread(
            &self,
            _chat_id: &str,
            _thread_id: &str,
            _message: ChatMessage,
        ) -> Result<(), ThreadStoreError> {
            Ok(())
        }

        async fn list_threads(&self, _chat_id: &str) -> Vec<Thread> {
            // Return 2 threads to trigger indicator logic
            vec![
                Thread::new("anchor-thread", "chat-1", "anchored"),
                Thread::new("other", "chat-1", "other"),
            ]
        }

        async fn archive_thread(
            &self,
            _chat_id: &str,
            _thread_id: &str,
        ) -> Result<(), ThreadStoreError> {
            Ok(())
        }

        async fn find_matching_thread(&self, _chat_id: &str, _query: &str) -> Option<Thread> {
            None
        }
    }
}
