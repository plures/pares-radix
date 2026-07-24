//! Thread routing procedure — routes inbound messages to the correct
//! conversation thread before they reach the model.
//!
//! Pipeline position: after InboundRouter, before ModelInvoker.
//! Enriches inbound event metadata with thread context so downstream
//! procedures (HistoryRecorder, ModelInvoker) operate on the correct thread.

use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::threading::router::{MessageMetadata, ThreadRouter};
use crate::threading::store::ThreadStore;
use crate::threading::types::ThreadDecision;

/// Routes inbound messages to the correct conversation thread.
///
/// When a message arrives, this procedure:
/// 1. Checks if it has already been thread-routed (prevents infinite loops)
/// 2. Converts the event metadata to `MessageMetadata` for the router
/// 3. Routes via `ThreadRouter` to get a `ThreadDecision`
/// 4. For `New` / `Existing` decisions, emits enriched events + lifecycle events
/// 5. For `Continue`, does nothing (original event passes through pipeline naturally)
pub struct ThreadRoutingProcedure {
    router: Arc<ThreadRouter>,
    store: Arc<dyn ThreadStore>,
}

impl ThreadRoutingProcedure {
    /// Create a new thread routing procedure.
    pub fn new(router: Arc<ThreadRouter>, store: Arc<dyn ThreadStore>) -> Self {
        Self { router, store }
    }

    /// Extract `MessageMetadata` from the event's JSON metadata field.
    fn extract_message_metadata(metadata: &Value) -> MessageMetadata {
        // Try to deserialize directly; fall back to default if the shape doesn't match.
        serde_json::from_value(metadata.clone()).unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ThreadRoutingProcedure {
    fn name(&self) -> &str {
        "thread_routing"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["inbound"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::Inbound {
            id,
            source,
            chat_id,
            sender,
            content,
            metadata,
        } = event
        else {
            return;
        };

        // ── Guard: skip already-routed events to prevent infinite loops ────
        if metadata
            .get("thread_routed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            debug!(
                event_id = %id,
                "thread_routing: skipping already-routed event"
            );
            return;
        }

        // ── Route the message ──────────────────────────────────────────────
        let msg_meta = Self::extract_message_metadata(metadata);
        let decision = self.router.route_message(chat_id, content, &msg_meta).await;

        match decision {
            ThreadDecision::New { topic } => {
                // Create new thread
                let thread = self.store.create_thread(chat_id, &topic).await;
                info!(
                    chat_id = %chat_id,
                    thread_id = %thread.id,
                    topic = %topic,
                    "thread_routing: created new thread"
                );

                // Emit ThreadCreated lifecycle event
                emitter
                    .emit(SpineEvent::ThreadCreated {
                        id: SpineEvent::new_id(),
                        chat_id: chat_id.clone(),
                        thread_id: thread.id.clone(),
                        topic: topic.clone(),
                        channel_anchor: json!({}),
                    })
                    .await;

                // Emit enriched inbound with thread context
                emitter
                    .emit(SpineEvent::Inbound {
                        id: id.clone(),
                        source: source.clone(),
                        chat_id: chat_id.clone(),
                        sender: sender.clone(),
                        content: content.clone(),
                        metadata: enrich_metadata(metadata, &thread.id, &topic),
                    })
                    .await;
            }

            ThreadDecision::Existing { thread_id } => {
                // Check if this is a thread switch
                let current_active = self.store.active_thread(chat_id).await;

                let is_switch = current_active
                    .as_ref()
                    .map(|t| t.id != thread_id)
                    .unwrap_or(true);

                if is_switch {
                    let old_id = current_active
                        .as_ref()
                        .map(|t| t.id.clone())
                        .unwrap_or_default();

                    match self.store.switch_thread(chat_id, &thread_id).await {
                        Ok(_) => {
                            info!(
                                chat_id = %chat_id,
                                from = %old_id,
                                to = %thread_id,
                                "thread_routing: switched thread"
                            );

                            emitter
                                .emit(SpineEvent::ThreadSwitched {
                                    id: SpineEvent::new_id(),
                                    chat_id: chat_id.clone(),
                                    from_thread_id: old_id,
                                    to_thread_id: thread_id.clone(),
                                })
                                .await;
                        }
                        Err(e) => {
                            warn!(
                                chat_id = %chat_id,
                                thread_id = %thread_id,
                                error = %e,
                                "thread_routing: failed to switch thread, continuing in current"
                            );
                            // Fall through — emit with current active thread info
                            if let Some(active) = &current_active {
                                emitter
                                    .emit(SpineEvent::Inbound {
                                        id: id.clone(),
                                        source: source.clone(),
                                        chat_id: chat_id.clone(),
                                        sender: sender.clone(),
                                        content: content.clone(),
                                        metadata: enrich_metadata(
                                            metadata,
                                            &active.id,
                                            &active.topic,
                                        ),
                                    })
                                    .await;
                            }
                            return;
                        }
                    }
                }

                // Get the thread topic for enrichment
                let topic = self
                    .store
                    .active_thread(chat_id)
                    .await
                    .map(|t| t.topic)
                    .unwrap_or_else(|| "unknown".to_string());

                // Emit enriched inbound
                emitter
                    .emit(SpineEvent::Inbound {
                        id: id.clone(),
                        source: source.clone(),
                        chat_id: chat_id.clone(),
                        sender: sender.clone(),
                        content: content.clone(),
                        metadata: enrich_metadata(metadata, &thread_id, &topic),
                    })
                    .await;
            }

            ThreadDecision::Continue => {
                // No routing change needed — the original event continues through
                // the pipeline naturally. We don't re-emit anything.
                debug!(
                    chat_id = %chat_id,
                    event_id = %id,
                    "thread_routing: continuing in current thread"
                );
            }
        }
    }
}

/// Enrich event metadata with thread routing information.
fn enrich_metadata(original: &Value, thread_id: &str, topic: &str) -> Value {
    let mut meta = original.clone();
    if let Some(obj) = meta.as_object_mut() {
        obj.insert("thread_id".to_string(), json!(thread_id));
        obj.insert("thread_topic".to_string(), json!(topic));
        obj.insert("thread_routed".to_string(), json!(true));
    } else {
        meta = json!({
            "thread_id": thread_id,
            "thread_topic": topic,
            "thread_routed": true,
        });
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;
    use crate::threading::types::ThreadConfig;
    use tokio::sync::mpsc;

    /// Helper: create a router + store with default config.
    fn make_test_setup() -> (Arc<ThreadRouter>, Arc<MemoryThreadStore>) {
        let store = Arc::new(MemoryThreadStore::new());
        let router = Arc::new(ThreadRouter::new(
            store.clone() as Arc<dyn ThreadStore>,
            ThreadConfig::default(),
        ));
        (router, store)
    }

    /// Helper: create the procedure + an emitter/receiver pair.
    fn make_procedure_and_emitter(
        router: Arc<ThreadRouter>,
        store: Arc<MemoryThreadStore>,
    ) -> (
        ThreadRoutingProcedure,
        PipelineEmitter,
        mpsc::Receiver<SpineEvent>,
    ) {
        let (tx, rx) = mpsc::channel(32);
        let emitter = PipelineEmitter { tx };
        let procedure = ThreadRoutingProcedure::new(router, store as Arc<dyn ThreadStore>);
        (procedure, emitter, rx)
    }

    #[tokio::test]
    async fn normal_message_continues_no_emit() {
        let (router, store) = make_test_setup();
        // Create an active thread so there's something to "continue" in
        store.create_thread("chat-1", "general").await;

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        let event = SpineEvent::Inbound {
            id: "evt-1".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "Hello world".into(),
            metadata: json!({}),
        };

        procedure.handle(&event, &emitter).await;

        // Continue decision → no events emitted
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "Continue decision should not emit any events"
        );
    }

    #[tokio::test]
    async fn explicit_new_thread_command_creates_thread() {
        let (router, store) = make_test_setup();
        store.create_thread("chat-1", "general").await;

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store.clone());

        let event = SpineEvent::Inbound {
            id: "evt-2".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "/thread new debugging".into(),
            metadata: json!({}),
        };

        procedure.handle(&event, &emitter).await;

        // Should emit ThreadCreated
        let first = rx.recv().await.unwrap();
        assert_eq!(first.event_type(), "thread_created");
        if let SpineEvent::ThreadCreated { chat_id, topic, .. } = &first {
            assert_eq!(chat_id, "chat-1");
            assert_eq!(topic, "debugging");
        } else {
            panic!("expected ThreadCreated, got {:?}", first);
        }

        // Should emit enriched Inbound
        let second = rx.recv().await.unwrap();
        assert_eq!(second.event_type(), "inbound");
        if let SpineEvent::Inbound { metadata, .. } = &second {
            assert_eq!(metadata["thread_routed"], true);
            assert_eq!(metadata["thread_topic"], "debugging");
            assert!(metadata["thread_id"].as_str().is_some());
        } else {
            panic!("expected enriched Inbound, got {:?}", second);
        }

        // Verify the thread was actually created in the store
        let threads = store.list_threads("chat-1").await;
        assert!(
            threads.iter().any(|t| t.topic == "debugging"),
            "thread should exist in store"
        );
    }

    #[tokio::test]
    async fn explicit_switch_command_emits_switch_event() {
        let (router, store) = make_test_setup();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let _t2 = store.create_thread("chat-1", "topic-b").await;
        // t2 is now active (last created)

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        let event = SpineEvent::Inbound {
            id: "evt-3".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: format!("/thread switch {}", t1.id),
            metadata: json!({}),
        };

        procedure.handle(&event, &emitter).await;

        // Should emit ThreadSwitched
        let first = rx.recv().await.unwrap();
        assert_eq!(first.event_type(), "thread_switched");
        if let SpineEvent::ThreadSwitched {
            from_thread_id,
            to_thread_id,
            ..
        } = &first
        {
            assert_eq!(to_thread_id, &t1.id);
            assert_ne!(from_thread_id, &t1.id);
        } else {
            panic!("expected ThreadSwitched, got {:?}", first);
        }

        // Should emit enriched Inbound
        let second = rx.recv().await.unwrap();
        assert_eq!(second.event_type(), "inbound");
        if let SpineEvent::Inbound { metadata, .. } = &second {
            assert_eq!(metadata["thread_routed"], true);
            assert_eq!(metadata["thread_id"], t1.id);
            assert_eq!(metadata["thread_topic"], "topic-a");
        } else {
            panic!("expected enriched Inbound, got {:?}", second);
        }
    }

    #[tokio::test]
    async fn channel_thread_id_in_metadata_routes_to_existing() {
        let (router, store) = make_test_setup();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let _t2 = store.create_thread("chat-1", "topic-b").await;
        // t2 is active

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        // Simulate metadata with a channel_thread_id pointing to t1
        let event = SpineEvent::Inbound {
            id: "evt-4".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "replying in thread".into(),
            metadata: json!({
                "channel_thread_id": t1.id,
            }),
        };

        procedure.handle(&event, &emitter).await;

        // Should emit ThreadSwitched (from t2 to t1)
        let first = rx.recv().await.unwrap();
        assert_eq!(first.event_type(), "thread_switched");

        // Should emit enriched Inbound
        let second = rx.recv().await.unwrap();
        assert_eq!(second.event_type(), "inbound");
        if let SpineEvent::Inbound { metadata, .. } = &second {
            assert_eq!(metadata["thread_routed"], true);
            assert_eq!(metadata["thread_id"], t1.id);
        } else {
            panic!("expected enriched Inbound");
        }
    }

    #[tokio::test]
    async fn already_routed_event_is_skipped() {
        let (router, store) = make_test_setup();
        store.create_thread("chat-1", "general").await;

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        // Event already has thread_routed: true
        let event = SpineEvent::Inbound {
            id: "evt-5".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "/thread new should-be-ignored".into(),
            metadata: json!({
                "thread_routed": true,
                "thread_id": "existing-thread",
                "thread_topic": "already-set",
            }),
        };

        procedure.handle(&event, &emitter).await;

        // Should NOT emit anything — infinite loop prevention
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "already-routed event should be skipped entirely"
        );
    }

    #[tokio::test]
    async fn non_inbound_event_is_ignored() {
        let (router, store) = make_test_setup();
        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        let event = SpineEvent::ModelResponse {
            id: "evt-6".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            content: "response".into(),
            model: "test".into(),
            tool_calls: vec![],
            metadata: json!({}),
        };

        procedure.handle(&event, &emitter).await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "non-inbound events should be ignored"
        );
    }

    #[tokio::test]
    async fn switch_to_nonexistent_thread_falls_back_gracefully() {
        let (router, store) = make_test_setup();
        let active = store.create_thread("chat-1", "general").await;

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        // Try to switch to a thread that doesn't exist
        let event = SpineEvent::Inbound {
            id: "evt-7".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "/thread switch nonexistent-id".into(),
            metadata: json!({}),
        };

        procedure.handle(&event, &emitter).await;

        // Should emit enriched Inbound with current active thread info (graceful fallback)
        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "inbound");
        if let SpineEvent::Inbound { metadata, .. } = &emitted {
            assert_eq!(metadata["thread_routed"], true);
            assert_eq!(metadata["thread_id"], active.id);
            assert_eq!(metadata["thread_topic"], "general");
        } else {
            panic!("expected enriched Inbound fallback");
        }
    }

    #[tokio::test]
    async fn existing_decision_same_thread_no_switch_event() {
        let (router, store) = make_test_setup();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        // t1 is both the target and the active thread

        let (procedure, emitter, mut rx) = make_procedure_and_emitter(router, store);

        // Metadata points to the currently-active thread (no switch needed)
        let event = SpineEvent::Inbound {
            id: "evt-8".into(),
            source: "telegram".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "message in current thread".into(),
            metadata: json!({
                "channel_thread_id": t1.id,
            }),
        };

        procedure.handle(&event, &emitter).await;

        // Should emit enriched Inbound but NOT ThreadSwitched (already in correct thread)
        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "inbound");
        if let SpineEvent::Inbound { metadata, .. } = &emitted {
            assert_eq!(metadata["thread_routed"], true);
            assert_eq!(metadata["thread_id"], t1.id);
        } else {
            panic!("expected enriched Inbound");
        }

        // No ThreadSwitched event
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            rx.try_recv().is_err(),
            "no ThreadSwitched when target == current active"
        );
    }

    #[tokio::test]
    async fn enrich_metadata_preserves_existing_fields() {
        let original = json!({
            "source_platform": "telegram",
            "message_id": 12345,
        });

        let enriched = enrich_metadata(&original, "thread-abc", "debugging");

        assert_eq!(enriched["source_platform"], "telegram");
        assert_eq!(enriched["message_id"], 12345);
        assert_eq!(enriched["thread_id"], "thread-abc");
        assert_eq!(enriched["thread_topic"], "debugging");
        assert_eq!(enriched["thread_routed"], true);
    }

    #[tokio::test]
    async fn enrich_metadata_handles_null() {
        let original = Value::Null;
        let enriched = enrich_metadata(&original, "thread-xyz", "topic");

        assert_eq!(enriched["thread_id"], "thread-xyz");
        assert_eq!(enriched["thread_topic"], "topic");
        assert_eq!(enriched["thread_routed"], true);
    }
}
