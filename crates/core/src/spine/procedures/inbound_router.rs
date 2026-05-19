//! Inbound router procedure — routes incoming messages to model invocation.

use tracing::debug;

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Routes inbound messages to model requests.
///
/// This is the first procedure in the pipeline. It receives `Inbound`
/// events from channel adapters and emits `ModelRequest` events for
/// the model invoker to process.
pub struct InboundRouter;

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
            "inbound_router: routing message to model"
        );

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
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn routes_inbound_to_model_request() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let router = InboundRouter;
        let event = SpineEvent::Inbound {
            id: "test-1".into(),
            source: "telegram".into(),
            chat_id: "123".into(),
            sender: "user".into(),
            content: "hello world".into(),
            metadata: serde_json::json!({}),
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
        }
    }
}
