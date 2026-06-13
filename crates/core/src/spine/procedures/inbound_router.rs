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

use serde_json::json;
use tracing::{debug, info};

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
}

impl InboundRouter {
    /// Create a router without reactive support (legacy direct passthrough).
    pub fn new() -> Self {
        Self { reactive: None }
    }

    /// Create a router with reactive .px procedure support.
    pub fn with_reactive(reactive: Arc<ReactiveRegistry>) -> Self {
        Self {
            reactive: Some(reactive),
        }
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

        // If we have a reactive registry, notify it of the inbound write.
        // The registry will fire matching .px procedures (classify_message, etc.)
        // asynchronously. The procedures write their results back, which may
        // trigger further reactive chains (routing, context management).
        if let Some(ref reactive) = self.reactive {
            let write_key = format!("inbound:{id}");
            let write_value = json!({
                "source": source,
                "chat_id": chat_id,
                "sender": sender,
                "content": content,
                "metadata": metadata,
            });

            debug!(
                key = %write_key,
                "inbound_router: firing reactive triggers"
            );

            reactive.on_write(&write_key, &write_value).await;

            // The reactive procedures run asynchronously. For now, we still
            // emit the ModelRequest directly — the reactive path enriches
            // context and may modify routing, but the pipeline continues.
            // Phase 2 will make this await classification results before
            // choosing the model tier.
            info!(
                event_id = %id,
                "inbound_router: reactive triggers fired, emitting model request"
            );
        }

        // Emit ModelRequest downstream (current behavior preserved as fallback
        // and as the default path until Phase 2 wires classification-driven routing)
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
    use crate::px_adapter::AsyncActionHandler;
    use async_trait::async_trait;
    use pares_radix_praxis::px::executor::ExecutionError;
    use serde_json::Value;
    use std::time::Duration;
    use tokio::sync::mpsc;

    struct NoOpHandler;

    #[async_trait]
    impl AsyncActionHandler for NoOpHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(Value::Null)
        }
    }

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
    async fn routes_with_reactive_still_emits_model_request() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let reactive = Arc::new(ReactiveRegistry::new());
        let router = InboundRouter::with_reactive(reactive);

        let event = SpineEvent::Inbound {
            id: "test-2".into(),
            source: "telegram".into(),
            chat_id: "456".into(),
            sender: "user".into(),
            content: "build the API".into(),
            metadata: json!({}),
        };

        router.handle(&event, &emitter).await;

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
    async fn reactive_fires_triggers_for_registered_procedures() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let reactive = Arc::new(ReactiveRegistry::new());

        // We can't easily create a real PxProcedureAdapter in tests without
        // valid compiled data, but we verify the reactive.on_write path
        // is called by checking that no panic occurs with an empty registry.

        let router = InboundRouter::with_reactive(reactive);
        let event = SpineEvent::Inbound {
            id: "test-4".into(),
            source: "telegram".into(),
            chat_id: "789".into(),
            sender: "user".into(),
            content: "test reactive".into(),
            metadata: json!({}),
        };

        // Should not panic, should still emit ModelRequest
        router.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "model_request");
    }
}
