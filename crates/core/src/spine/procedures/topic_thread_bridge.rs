//! Topic-to-thread bridge — converts reactive topic classification results
//! into thread routing decisions.
//!
//! Listens for "topic_classified" reactive events (from topic-routing.px)
//! and feeds them into the ThreadRouter to create/switch threads automatically.
//!
//! Pipeline flow:
//!   Inbound → topic-routing.px fires → topic_classified written to reactive
//!   → TopicThreadBridge reads it → feeds ThreadRouter → emits thread events\n
use std::sync::Arc;

use serde_json::json;
use tracing::{debug, info, warn};

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::threading::router::{ThreadRouter, TopicClassification};
use crate::threading::store::ThreadStore;
use crate::threading::types::ThreadDecision;

/// Bridges topic classification events into the thread router.
///
/// When a `TopicClassified` spine event arrives (emitted by the reactive bridge
/// in thread-management.px), this procedure:
/// 1. Parses the classification JSON into a `TopicClassification`
/// 2. Calls `ThreadRouter::route_from_classification`
/// 3. Based on the `ThreadDecision`, creates/switches threads and emits lifecycle events
pub struct TopicThreadBridge {
    router: Arc<ThreadRouter>,
    store: Arc<dyn ThreadStore>,
}

impl TopicThreadBridge {
    /// Create a new topic-to-thread bridge.
    pub fn new(router: Arc<ThreadRouter>, store: Arc<dyn ThreadStore>) -> Self {
        Self { router, store }
    }

    /// Parse a classification JSON value into a `TopicClassification`.
    fn parse_classification(value: &serde_json::Value) -> Option<TopicClassification> {
        let topic_changed = value
            .get("topic_changed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let new_topic = value
            .get("new_topic")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let confidence = value
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Require at minimum a topic string when topic_changed is true
        if topic_changed && new_topic.is_empty() {
            return None;
        }

        Some(TopicClassification {
            topic: new_topic,
            confidence,
            is_shift: topic_changed,
        })
    }
}

#[async_trait::async_trait]
impl SpineProcedure for TopicThreadBridge {
    fn name(&self) -> &str {
        "topic_thread_bridge"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["topic_classified"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::TopicClassified {
            id: _,
            chat_id,
            classification,
            original_metadata: _,
        } = event
        else {
            return;
        };

        // ── Parse classification ───────────────────────────────────────────
        let parsed = match Self::parse_classification(classification) {
            Some(c) => c,
            None => {
                warn!(
                    chat_id = %chat_id,
                    classification = %classification,
                    "topic_thread_bridge: malformed classification JSON, skipping"
                );
                return;
            }
        };

        debug!(
            chat_id = %chat_id,
            topic = %parsed.topic,
            confidence = %parsed.confidence,
            is_shift = %parsed.is_shift,
            "topic_thread_bridge: processing classification"
        );

        // ── Route via ThreadRouter ─────────────────────────────────────────
        let decision = self
            .router
            .route_from_classification(chat_id, &parsed)
            .await;

        match decision {
            ThreadDecision::New { topic } => {
                // Create the new thread
                let thread = self.store.create_thread(chat_id, &topic).await;
                info!(
                    chat_id = %chat_id,
                    thread_id = %thread.id,
                    topic = %topic,
                    confidence = %parsed.confidence,
                    "topic_thread_bridge: created new thread from topic classification"
                );

                // Emit ThreadCreated lifecycle event
                emitter
                    .emit(SpineEvent::ThreadCreated {
                        id: SpineEvent::new_id(),
                        chat_id: chat_id.clone(),
                        thread_id: thread.id.clone(),
                        topic,
                        channel_anchor: json!({}),
                    })
                    .await;
            }

            ThreadDecision::Existing { thread_id } => {
                // Check if this is actually a switch
                let current_active = self.store.active_thread(chat_id).await;
                let is_switch = current_active
                    .as_ref()
                    .map(|t| t.id != thread_id)
                    .unwrap_or(true);

                if is_switch {
                    let from_thread_id = current_active
                        .as_ref()
                        .map(|t| t.id.clone())
                        .unwrap_or_default();

                    match self.store.switch_thread(chat_id, &thread_id).await {
                        Ok(_) => {
                            info!(
                                chat_id = %chat_id,
                                from = %from_thread_id,
                                to = %thread_id,
                                "topic_thread_bridge: switched thread based on topic match"
                            );

                            emitter
                                .emit(SpineEvent::ThreadSwitched {
                                    id: SpineEvent::new_id(),
                                    chat_id: chat_id.clone(),
                                    from_thread_id,
                                    to_thread_id: thread_id,
                                })
                                .await;
                        }
                        Err(e) => {
                            warn!(
                                chat_id = %chat_id,
                                thread_id = %thread_id,
                                error = %e,
                                "topic_thread_bridge: failed to switch thread, continuing in current"
                            );
                        }
                    }
                } else {
                    debug!(
                        chat_id = %chat_id,
                        thread_id = %thread_id,
                        "topic_thread_bridge: already in matching thread, no switch needed"
                    );
                }
            }

            ThreadDecision::Continue => {
                debug!(
                    chat_id = %chat_id,
                    "topic_thread_bridge: no routing action (continue)"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;
    use crate::threading::types::ThreadConfig;
    use tokio::sync::mpsc;

    /// Helper: create router + store with default config.
    fn make_test_setup() -> (Arc<ThreadRouter>, Arc<MemoryThreadStore>) {
        let store = Arc::new(MemoryThreadStore::new());
        let router = Arc::new(ThreadRouter::new(
            store.clone() as Arc<dyn ThreadStore>,
            ThreadConfig::default(),
        ));
        (router, store)
    }

    /// Helper: create router + store with a custom config.
    fn make_test_setup_with_config(config: ThreadConfig) -> (Arc<ThreadRouter>, Arc<MemoryThreadStore>) {
        let store = Arc::new(MemoryThreadStore::new());
        let router = Arc::new(ThreadRouter::new(
            store.clone() as Arc<dyn ThreadStore>,
            config,
        ));
        (router, store)
    }

    /// Helper: create the bridge + emitter/receiver.
    fn make_bridge_and_emitter(
        router: Arc<ThreadRouter>,
        store: Arc<MemoryThreadStore>,
    ) -> (TopicThreadBridge, PipelineEmitter, mpsc::Receiver<SpineEvent>) {
        let (tx, rx) = mpsc::channel(32);
        let emitter = PipelineEmitter { tx };
        let bridge = TopicThreadBridge::new(router, store as Arc<dyn ThreadStore>);
        (bridge, emitter, rx)
    }

    fn make_topic_classified_event(
        chat_id: &str,
        topic_changed: bool,
        new_topic: &str,
        confidence: f64,
    ) -> SpineEvent {
        SpineEvent::TopicClassified {
            id: SpineEvent::new_id(),
            chat_id: chat_id.to_string(),
            classification: json!({
                "topic_changed": topic_changed,
                "new_topic": new_topic,
                "confidence": confidence,
            }),
            original_metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn topic_changed_high_confidence_creates_thread() {
        let (router, store) = make_test_setup();
        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store.clone());

        let event = make_topic_classified_event("chat-1", true, "deployment", 0.9);
        bridge.handle(&event, &emitter).await;

        // Should emit ThreadCreated
        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "thread_created");
        if let SpineEvent::ThreadCreated {
            chat_id, topic, ..
        } = &emitted
        {
            assert_eq!(chat_id, "chat-1");
            assert_eq!(topic, "deployment");
        } else {
            panic!("expected ThreadCreated, got {:?}", emitted);
        }

        // Verify thread was created in store
        let threads = store.list_threads("chat-1").await;
        assert!(threads.iter().any(|t| t.topic == "deployment"));
    }

    #[tokio::test]
    async fn topic_changed_low_confidence_no_action() {
        let (router, store) = make_test_setup();
        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        // Confidence 0.5 is below default threshold 0.75
        let event = make_topic_classified_event("chat-1", true, "deployment", 0.5);
        bridge.handle(&event, &emitter).await;

        // Should not emit anything (Continue decision)
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "low confidence should not trigger thread creation"
        );
    }

    #[tokio::test]
    async fn topic_not_changed_no_action() {
        let (router, store) = make_test_setup();
        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        let event = make_topic_classified_event("chat-1", false, "same-topic", 0.95);
        bridge.handle(&event, &emitter).await;

        // is_shift=false → Continue
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "no topic change should not trigger any action"
        );
    }

    #[tokio::test]
    async fn existing_thread_matches_topic_switches() {
        let (router, store) = make_test_setup();
        // Create two threads; the second is active
        let t1 = store.create_thread("chat-1", "deployment").await;
        let _t2 = store.create_thread("chat-1", "debugging").await;

        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        // Classify topic that matches t1 — should switch back to t1
        let event = make_topic_classified_event("chat-1", true, "deployment", 0.9);
        bridge.handle(&event, &emitter).await;

        // Should emit ThreadSwitched
        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "thread_switched");
        if let SpineEvent::ThreadSwitched {
            to_thread_id, ..
        } = &emitted
        {
            assert_eq!(to_thread_id, &t1.id);
        } else {
            panic!("expected ThreadSwitched, got {:?}", emitted);
        }
    }

    #[tokio::test]
    async fn malformed_classification_logged_no_crash() {
        let (router, store) = make_test_setup();
        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        // Missing fields / bad JSON shape
        let event = SpineEvent::TopicClassified {
            id: SpineEvent::new_id(),
            chat_id: "chat-1".to_string(),
            classification: json!({
                "topic_changed": true,
                // Missing "new_topic" — or empty
                "new_topic": "",
                "confidence": 0.9,
            }),
            original_metadata: json!({}),
        };

        bridge.handle(&event, &emitter).await;

        // Should not crash, should not emit (malformed)
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "malformed classification should produce no output"
        );
    }

    #[tokio::test]
    async fn completely_invalid_classification_json() {
        let (router, store) = make_test_setup();
        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        // Completely nonsensical classification
        let event = SpineEvent::TopicClassified {
            id: SpineEvent::new_id(),
            chat_id: "chat-1".to_string(),
            classification: json!("not an object"),
            original_metadata: json!({}),
        };

        bridge.handle(&event, &emitter).await;

        // Should not crash
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn non_topic_classified_event_ignored() {
        let (router, store) = make_test_setup();
        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        let event = SpineEvent::Inbound {
            id: SpineEvent::new_id(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "hello".into(),
            metadata: json!({}),
        };

        bridge.handle(&event, &emitter).await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err(), "non-TopicClassified events should be ignored");
    }

    #[tokio::test]
    async fn max_active_cap_prevents_creation() {
        let config = ThreadConfig {
            max_active: 2,
            ..Default::default()
        };
        let (router, store) = make_test_setup_with_config(config);

        // Fill up to max
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let (bridge, emitter, mut rx) = make_bridge_and_emitter(router, store);

        // Try to classify a new topic — should be blocked by max_active
        let event = make_topic_classified_event("chat-1", true, "brand-new-topic", 0.95);
        bridge.handle(&event, &emitter).await;

        // Should not emit anything (Continue due to max_active)
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "max_active cap should prevent thread creation"
        );
    }
}
