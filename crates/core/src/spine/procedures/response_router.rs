//! Response router procedure — converts ModelResponse into DeliveryRequest.

use tracing::debug;

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Routes model responses to channel delivery.
///
/// Listens for `ModelResponse` events and emits `DeliveryRequest` events
/// targeting the channel that originated the conversation.
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
            source,
            chat_id,
            content,
            tool_calls,
            metadata,
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

        debug!(event_id = %id, channel = %source, "response_router: routing response to delivery");

        emitter
            .emit(SpineEvent::DeliveryRequest {
                id: SpineEvent::new_id(),
                channel: source.clone(),
                chat_id: chat_id.clone(),
                content: content.clone(),
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
    async fn routes_model_response_to_delivery() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let router = ResponseRouter;
        let event = SpineEvent::ModelResponse {
            id: "resp-1".into(),
            source: "discord".into(),
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
            assert_eq!(channel, "discord");
            assert_eq!(chat_id, "456");
            assert_eq!(content, "Hello back!");
        }
    }

    #[tokio::test]
    async fn skips_responses_with_tool_calls() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        let router = ResponseRouter;
        let event = SpineEvent::ModelResponse {
            id: "resp-2".into(),
            source: "telegram".into(),
            chat_id: "789".into(),
            content: String::new(),
            model: "gpt-4".into(),
            tool_calls: vec![crate::model::ToolCall {
                id: "call-1".into(),
                name: "search".into(),
                arguments: serde_json::json!({"q": "test"}),
            }],
            metadata: serde_json::json!({}),
        };

        router.handle(&event, &emitter).await;

        // Nothing should be emitted
        assert!(rx.try_recv().is_err());
    }
}
