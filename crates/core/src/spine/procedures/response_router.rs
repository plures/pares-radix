//! Response router procedure — converts ModelResponse into DeliveryRequest.

use tracing::debug;

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Routes model responses to channel delivery.
///
/// Listens for `ModelResponse` events and emits `DeliveryRequest` events
/// targeting the appropriate channel for the chat.
pub struct ResponseRouter;

#[async_trait::async_trait]
impl SpineProcedure for ResponseRouter {
    fn name(&self) -> &str {
        "response_router"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["model_response"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::ModelResponse {
            id,
            chat_id,
            content,
            tool_calls,
            ..
        } = event
        else {
            return;
        };

        // If the model made tool calls, don't deliver — the ToolExecutor handles it.
        if !tool_calls.is_empty() {
            debug!(event_id = %id, tool_count = tool_calls.len(), "response_router: skipping (has tool calls)");
            return;
        }

        debug!(event_id = %id, "response_router: routing response to delivery");

        // TODO: Look up which channel the chat_id belongs to.
        // For now, assume Telegram.
        emitter
            .emit(SpineEvent::DeliveryRequest {
                id: SpineEvent::new_id(),
                channel: "telegram".into(),
                chat_id: chat_id.clone(),
                content: content.clone(),
                metadata: serde_json::json!({}),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn routes_model_response_to_delivery() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let router = ResponseRouter;
        let event = SpineEvent::ModelResponse {
            id: "resp-1".into(),
            chat_id: "456".into(),
            content: "Hello back!".into(),
            model: "gpt-4".into(),
            tool_calls: vec![],
            metadata: serde_json::json!({}),
        };

        router.handle(&event, &emitter).await;

        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "delivery_request");
        if let SpineEvent::DeliveryRequest {
            channel,
            chat_id,
            content,
            ..
        } = emitted
        {
            assert_eq!(channel, "telegram");
            assert_eq!(chat_id, "456");
            assert_eq!(content, "Hello back!");
        }
    }
}
