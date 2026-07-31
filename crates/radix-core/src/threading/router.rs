//! Thread routing logic — determines where inbound messages are routed.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store::ThreadStore;
use super::types::{ThreadConfig, ThreadDecision};

/// Metadata associated with an inbound message for routing purposes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// If this message is a reply to another message, the original message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
    /// Thread ID resolved by the channel adapter (from native threading).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_thread_id: Option<String>,
    /// Raw metadata from the channel for additional context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

/// Output from a topic classifier (e.g., from topic-routing.px).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicClassification {
    /// The classified topic.
    pub topic: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
    /// Whether a topic shift was detected.
    pub is_shift: bool,
}

/// Thread router — resolves inbound messages to thread decisions.
pub struct ThreadRouter {
    store: Arc<dyn ThreadStore>,
    config: ThreadConfig,
}

impl ThreadRouter {
    /// Create a new thread router.
    pub fn new(store: Arc<dyn ThreadStore>, config: ThreadConfig) -> Self {
        Self { store, config }
    }

    /// Route an inbound message to a thread decision.
    ///
    /// Priority:
    /// 1. Explicit commands (`/thread new`, `/thread switch <id>`)
    /// 2. Channel metadata (reply-to, native thread ID)
    /// 3. Topic classification (if auto-detect is enabled)
    /// 4. Continue in current thread
    pub async fn route_message(
        &self,
        chat_id: &str,
        content: &str,
        metadata: &MessageMetadata,
    ) -> ThreadDecision {
        // 1. Check for explicit thread commands
        if let Some(decision) = self.parse_thread_command(content) {
            return decision;
        }

        // 2. Check channel metadata for reply-to routing
        if let Some(decision) = self.resolve_from_metadata(chat_id, metadata).await {
            return decision;
        }

        // 3. If auto-detect is disabled, always continue
        if !self.config.auto_detect {
            return ThreadDecision::Continue;
        }

        // Default: continue in current thread
        ThreadDecision::Continue
    }

    /// Route based on a topic classification result (from topic-routing.px or LLM).
    pub async fn route_from_classification(
        &self,
        chat_id: &str,
        classification: &TopicClassification,
    ) -> ThreadDecision {
        if !classification.is_shift {
            return ThreadDecision::Continue;
        }

        if classification.confidence < self.config.auto_create_threshold {
            return ThreadDecision::Continue;
        }

        // Check if there's an existing thread for this topic
        if let Some(thread) = self
            .store
            .find_matching_thread(chat_id, &classification.topic)
            .await
        {
            return ThreadDecision::Existing {
                thread_id: thread.id,
            };
        }

        // Check if we're at max active threads
        let threads = self.store.list_threads(chat_id).await;
        let active_count = threads
            .iter()
            .filter(|t| t.state == super::types::ThreadState::Active)
            .count();

        if active_count >= self.config.max_active {
            return ThreadDecision::Continue;
        }

        ThreadDecision::New {
            topic: classification.topic.clone(),
        }
    }

    /// Parse explicit thread commands.
    fn parse_thread_command(&self, content: &str) -> Option<ThreadDecision> {
        let trimmed = content.trim();

        if trimmed == "/thread new" || trimmed.starts_with("/thread new ") {
            let topic = trimmed
                .strip_prefix("/thread new")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("untitled");
            return Some(ThreadDecision::New {
                topic: topic.to_string(),
            });
        }

        if let Some(rest) = trimmed.strip_prefix("/thread switch ") {
            let thread_id = rest.trim();
            if !thread_id.is_empty() {
                return Some(ThreadDecision::Existing {
                    thread_id: thread_id.to_string(),
                });
            }
        }

        None
    }

    /// Resolve thread from message metadata (reply-to, channel thread ID).
    async fn resolve_from_metadata(
        &self,
        _chat_id: &str,
        metadata: &MessageMetadata,
    ) -> Option<ThreadDecision> {
        // If the channel resolved a thread ID directly, use it
        if let Some(thread_id) = &metadata.channel_thread_id {
            return Some(ThreadDecision::Existing {
                thread_id: thread_id.clone(),
            });
        }

        // Reply-to routing would require looking up which thread a message belongs to.
        // This is a future extension point — for now, reply-to doesn't force thread routing.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;

    fn make_router(store: Arc<dyn ThreadStore>) -> ThreadRouter {
        ThreadRouter::new(store, ThreadConfig::default())
    }

    fn make_router_with_config(store: Arc<dyn ThreadStore>, config: ThreadConfig) -> ThreadRouter {
        ThreadRouter::new(store, config)
    }

    #[tokio::test]
    async fn explicit_new_thread_command() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let decision = router
            .route_message(
                "chat-1",
                "/thread new debugging",
                &MessageMetadata::default(),
            )
            .await;
        assert_eq!(
            decision,
            ThreadDecision::New {
                topic: "debugging".to_string()
            }
        );
    }

    #[tokio::test]
    async fn explicit_new_thread_no_topic() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let decision = router
            .route_message("chat-1", "/thread new", &MessageMetadata::default())
            .await;
        assert_eq!(
            decision,
            ThreadDecision::New {
                topic: "untitled".to_string()
            }
        );
    }

    #[tokio::test]
    async fn explicit_switch_command() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let decision = router
            .route_message(
                "chat-1",
                "/thread switch abc-123",
                &MessageMetadata::default(),
            )
            .await;
        assert_eq!(
            decision,
            ThreadDecision::Existing {
                thread_id: "abc-123".to_string()
            }
        );
    }

    #[tokio::test]
    async fn channel_thread_id_in_metadata() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let meta = MessageMetadata {
            channel_thread_id: Some("thread-xyz".to_string()),
            ..Default::default()
        };

        let decision = router.route_message("chat-1", "hello", &meta).await;
        assert_eq!(
            decision,
            ThreadDecision::Existing {
                thread_id: "thread-xyz".to_string()
            }
        );
    }

    #[tokio::test]
    async fn normal_message_continues() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let decision = router
            .route_message("chat-1", "Hello world", &MessageMetadata::default())
            .await;
        assert_eq!(decision, ThreadDecision::Continue);
    }

    #[tokio::test]
    async fn auto_detect_disabled_always_continues() {
        let store = Arc::new(MemoryThreadStore::new());
        let config = ThreadConfig {
            auto_detect: false,
            ..Default::default()
        };
        let router = make_router_with_config(store, config);

        let decision = router
            .route_message(
                "chat-1",
                "something about a new topic",
                &MessageMetadata::default(),
            )
            .await;
        assert_eq!(decision, ThreadDecision::Continue);
    }

    #[tokio::test]
    async fn classification_below_threshold_continues() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let classification = TopicClassification {
            topic: "new topic".to_string(),
            confidence: 0.5, // below default 0.75
            is_shift: true,
        };

        let decision = router
            .route_from_classification("chat-1", &classification)
            .await;
        assert_eq!(decision, ThreadDecision::Continue);
    }

    #[tokio::test]
    async fn classification_no_shift_continues() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let classification = TopicClassification {
            topic: "same topic".to_string(),
            confidence: 0.9,
            is_shift: false,
        };

        let decision = router
            .route_from_classification("chat-1", &classification)
            .await;
        assert_eq!(decision, ThreadDecision::Continue);
    }

    #[tokio::test]
    async fn classification_creates_new_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let router = make_router(store);

        let classification = TopicClassification {
            topic: "deployment".to_string(),
            confidence: 0.9,
            is_shift: true,
        };

        let decision = router
            .route_from_classification("chat-1", &classification)
            .await;
        assert_eq!(
            decision,
            ThreadDecision::New {
                topic: "deployment".to_string()
            }
        );
    }

    #[tokio::test]
    async fn classification_routes_to_existing() {
        let store = Arc::new(MemoryThreadStore::new());
        let thread = store.create_thread("chat-1", "deployment").await;
        let router = make_router(store.clone() as Arc<dyn ThreadStore>);

        let classification = TopicClassification {
            topic: "deployment".to_string(),
            confidence: 0.9,
            is_shift: true,
        };

        let decision = router
            .route_from_classification("chat-1", &classification)
            .await;
        assert_eq!(
            decision,
            ThreadDecision::Existing {
                thread_id: thread.id
            }
        );
    }

    #[tokio::test]
    async fn classification_respects_max_active() {
        let store = Arc::new(MemoryThreadStore::new());
        let config = ThreadConfig {
            max_active: 2,
            ..Default::default()
        };

        // Create 2 threads (max)
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let router = make_router_with_config(store.clone() as Arc<dyn ThreadStore>, config);

        let classification = TopicClassification {
            topic: "brand new".to_string(),
            confidence: 0.95,
            is_shift: true,
        };

        let decision = router
            .route_from_classification("chat-1", &classification)
            .await;
        assert_eq!(decision, ThreadDecision::Continue);
    }
}
