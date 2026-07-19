//! Inbound router procedure — routes incoming messages through reactive .px
//! classification before falling back to direct model invocation.
//!
//! # Architecture
//!
//! ```text
//! Inbound event arrives
//!   → Write to ReactiveRegistry key "inbound:{id}"
//!   → .px classify_message fires (async, non-blocking)
//!   → Classification result written to "classification:{id}"
//!   → .px route_event fires on classification
//!   → Routing result determines pipeline path
//!
//! Fallback (no reactive result within timeout):
//!   → Emit ModelRequest directly (legacy behavior)
//! ```

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::spine::reactive::ReactiveRegistry;

/// Routes inbound messages through the reactive .px pipeline.
///
/// When a `ReactiveRegistry` is attached, inbound events are written to it
/// triggering .px classification and routing procedures. The router then
/// emits the appropriate downstream event based on the .px result.
///
/// When no registry is attached (or the reactive path times out), falls back
/// to emitting a `ModelRequest` directly — preserving pre-rewiring behavior.
pub struct InboundRouter {
    /// Optional reactive registry for triggering .px procedures.
    reactive: Option<Arc<ReactiveRegistry>>,
    /// Timeout for awaiting reactive routing decisions.
    reactive_timeout: Duration,
}

impl InboundRouter {
    /// Default reactive timeout (5 seconds).
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

    /// Create a router without reactive support (legacy direct passthrough).
    pub fn new() -> Self {
        Self {
            reactive: None,
            reactive_timeout: Self::DEFAULT_TIMEOUT,
        }
    }

    /// Create a router with reactive .px procedure support.
    pub fn with_reactive(reactive: Arc<ReactiveRegistry>) -> Self {
        Self {
            reactive: Some(reactive),
            reactive_timeout: Self::DEFAULT_TIMEOUT,
        }
    }

    /// Set a custom timeout for reactive routing decisions.
    #[cfg(test)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.reactive_timeout = timeout;
        self
    }
}

impl Default for InboundRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SpineProcedure for InboundRouter {
    fn name(&self) -> &str {
        "inbound_router"
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

        debug!(
            event_id = %id,
            source = %source,
            chat_id = %chat_id,
            sender = %sender,
            "inbound_router: processing inbound message"
        );

        // If we have a reactive registry, subscribe to the routing decision
        // BEFORE firing the inbound write, then await the result with timeout.
        if let Some(ref reactive) = self.reactive {
            let write_key = format!("inbound:{id}");
            let result_key = format!("route_decision:{id}");
            let write_value = json!({
                "source": source,
                "chat_id": chat_id,
                "sender": sender,
                "content": content,
                "metadata": metadata,
            });

            // Subscribe BEFORE write so we don't miss the notification
            let result_rx = reactive.subscribe_result(&result_key).await;

            debug!(
                key = %write_key,
                await_key = %result_key,
                "inbound_router: firing reactive triggers, awaiting route decision"
            );

            // Fire the inbound write — triggers classify_and_route
            reactive.on_write(&write_key, &write_value).await;

            // Await the routing decision with timeout
            match tokio::time::timeout(self.reactive_timeout, result_rx).await {
                Ok(Ok(route_decision)) => {
                    // .px routing succeeded — emit based on the decision
                    info!(
                        event_id = %id,
                        decision = %route_decision,
                        "inbound_router: .px routing decision received"
                    );

                    // Extract routing info from the .px result
                    let tier = route_decision
                        .get("tier")
                        .and_then(Value::as_str)
                        .unwrap_or("standard");
                    let destination = route_decision
                        .get("destination")
                        .and_then(Value::as_str)
                        .unwrap_or("conversation");

                    // For procedural responses, the .px chain handles delivery
                    // directly via the reactive cascade — don't emit ModelRequest
                    if destination == "procedural" {
                        debug!(
                            event_id = %id,
                            "inbound_router: procedural route — .px handles response"
                        );
                        return;
                    }

                    // For heartbeat, emit with fast tier in metadata
                    if destination == "heartbeat" {
                        debug!(
                            event_id = %id,
                            "inbound_router: heartbeat route — emitting fast model request"
                        );
                        emitter
                            .emit(SpineEvent::ModelRequest {
                                id: SpineEvent::new_id(),
                                source: source.clone(),
                                chat_id: chat_id.clone(),
                                sender: sender.clone(),
                                content: content.clone(),
                                system_prompt: None,
                                metadata: json!({
                                    "model_tier": "fast",
                                    "route_reason": "heartbeat",
                                    "routed_by": "px"
                                }),
                            })
                            .await;
                        return;
                    }

                    // Conversation / task_steering — emit ModelRequest with the
                    // tier determined by .px classification in metadata
                    let reason = route_decision
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    emitter
                        .emit(SpineEvent::ModelRequest {
                            id: SpineEvent::new_id(),
                            source: source.clone(),
                            chat_id: chat_id.clone(),
                            sender: sender.clone(),
                            content: content.clone(),
                            system_prompt: None,
                            metadata: json!({
                                "model_tier": tier,
                                "route_destination": destination,
                                "route_reason": reason,
                                "routed_by": "px"
                            }),
                        })
                        .await;
                    return;
                }
                Ok(Err(_)) => {
                    // Receiver dropped — reactive chain didn't produce output
                    warn!(
                        event_id = %id,
                        "inbound_router: reactive chain dropped without result, falling back"
                    );
                }
                Err(_) => {
                    // Timeout — fall back to direct model request
                    warn!(
                        event_id = %id,
                        timeout_ms = self.reactive_timeout.as_millis() as u64,
                        "inbound_router: reactive timeout, falling back to direct routing"
                    );
                }
            }
        }

        // Fallback: emit ModelRequest directly (no reactive, or reactive timed out)
        emitter
            .emit(SpineEvent::ModelRequest {
                id: SpineEvent::new_id(),
                source: source.clone(),
                chat_id: chat_id.clone(),
                sender: sender.clone(),
                content: content.clone(),
                system_prompt: None,
                metadata: metadata.clone(),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn routes_inbound_to_model_request_without_reactive() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let router = InboundRouter::new();
        let event = SpineEvent::Inbound {
            id: "test-1".into(),
            source: "telegram".into(),
            chat_id: "123".into(),
            sender: "user".into(),
            content: "hello world".into(),
            metadata: json!({}),
        };

        router.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "model_request");
        if let SpineEvent::ModelRequest {
            chat_id, content, ..
        } = emitted
        {
            assert_eq!(chat_id, "123");
            assert_eq!(content, "hello world");
        } else {
            panic!("expected ModelRequest");
        }
    }

    #[tokio::test]
    async fn routes_with_reactive_falls_back_on_empty_registry() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let reactive = Arc::new(ReactiveRegistry::new());
        // Use short timeout so test doesn't wait 5s
        let router = InboundRouter::with_reactive(reactive).with_timeout(Duration::from_millis(50));

        let event = SpineEvent::Inbound {
            id: "test-2".into(),
            source: "telegram".into(),
            chat_id: "456".into(),
            sender: "user".into(),
            content: "build the API".into(),
            metadata: json!({}),
        };

        router.handle(&event, &emitter).await;

        // Should fall back to direct ModelRequest after timeout
        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "model_request");
        if let SpineEvent::ModelRequest {
            chat_id, content, ..
        } = emitted
        {
            assert_eq!(chat_id, "456");
            assert_eq!(content, "build the API");
        } else {
            panic!("expected ModelRequest");
        }
    }

    #[tokio::test]
    async fn reactive_route_decision_drives_model_request() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let reactive = Arc::new(ReactiveRegistry::new());
        let reactive_clone = Arc::clone(&reactive);
        let router = InboundRouter::with_reactive(reactive).with_timeout(Duration::from_millis(500));

        // Simulate a .px procedure writing a route_decision by spawning a task
        // that writes to the registry after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            // Simulate classify_and_route producing a decision
            let decision = json!({
                "destination": "conversation",
                "tier": "premium",
                "reason": "high complexity"
            });
            reactive_clone.on_write("route_decision:test-5", &decision).await;
        });

        let event = SpineEvent::Inbound {
            id: "test-5".into(),
            source: "telegram".into(),
            chat_id: "999".into(),
            sender: "user".into(),
            content: "complex question about architecture".into(),
            metadata: json!({}),
        };

        router.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "model_request");
        if let SpineEvent::ModelRequest {
            chat_id, metadata, ..
        } = emitted
        {
            assert_eq!(chat_id, "999");
            // Verify the .px routing decision was used
            assert_eq!(metadata["model_tier"], "premium");
            assert_eq!(metadata["routed_by"], "px");
            assert_eq!(metadata["route_reason"], "high complexity");
        } else {
            panic!("expected ModelRequest with px routing metadata");
        }
    }

    #[tokio::test]
    async fn reactive_procedural_route_skips_model_request() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let reactive = Arc::new(ReactiveRegistry::new());
        let reactive_clone = Arc::clone(&reactive);
        let router = InboundRouter::with_reactive(reactive).with_timeout(Duration::from_millis(500));

        // Simulate classify_and_route producing a "procedural" decision
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let decision = json!({
                "destination": "procedural",
                "tier": "fast",
                "reason": "greeting"
            });
            reactive_clone.on_write("route_decision:test-6", &decision).await;
        });

        let event = SpineEvent::Inbound {
            id: "test-6".into(),
            source: "telegram".into(),
            chat_id: "111".into(),
            sender: "user".into(),
            content: "hello".into(),
            metadata: json!({}),
        };

        router.handle(&event, &emitter).await;

        // Procedural route should NOT emit any ModelRequest
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err(), "procedural route should not emit ModelRequest");
    }

    #[tokio::test]
    async fn ignores_non_inbound_events() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let router = InboundRouter::new();
        let event = SpineEvent::ModelResponse {
            id: "test-3".into(),
            source: "telegram".into(),
            chat_id: "123".into(),
            content: "response".into(),
            model: "test-model".into(),
            tool_calls: vec![],
            metadata: json!({}),
        };

        router.handle(&event, &emitter).await;

        // Should not emit anything for non-inbound events
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn reactive_fires_triggers_and_falls_back_on_no_result() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let reactive = Arc::new(ReactiveRegistry::new());

        // Empty registry — triggers fire but no procedures produce output
        let router = InboundRouter::with_reactive(reactive).with_timeout(Duration::from_millis(50));
        let event = SpineEvent::Inbound {
            id: "test-4".into(),
            source: "telegram".into(),
            chat_id: "789".into(),
            sender: "user".into(),
            content: "test reactive".into(),
            metadata: json!({}),
        };

        // Should not panic, should fall back to ModelRequest after timeout
        router.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "model_request");
    }
}
