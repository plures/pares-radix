//! Model invoker procedure — calls the LLM and emits ModelResponse.
//!
//! NOTE: This is a placeholder that echoes messages back. The full
//! implementation will integrate with the ModelRouter/CopilotClient.

use tracing::debug;

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Invokes the model for a ModelRequest and emits ModelResponse.
pub struct ModelInvoker;

#[async_trait::async_trait]
impl SpineProcedure for ModelInvoker {
    fn name(&self) -> &str {
        "model_invoker"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["model_request"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::ModelRequest {
            id,
            chat_id,
            content,
            ..
        } = event
        else {
            return;
        };

        debug!(event_id = %id, "model_invoker: processing model request");

        // TODO: Integrate with ModelRouter for real model calls.
        // For now, echo the content back as the response.
        emitter
            .emit(SpineEvent::ModelResponse {
                id: SpineEvent::new_id(),
                chat_id: chat_id.clone(),
                content: format!("[model placeholder] {}", content),
                model: "placeholder".into(),
                metadata: serde_json::json!({}),
            })
            .await;
    }
}
