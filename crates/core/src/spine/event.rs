//! Spine events — the messages that flow through the pipeline.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A spine event — the unit of communication between pipeline procedures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpineEvent {
    /// An inbound message from a channel adapter.
    Inbound {
        id: String,
        source: String,
        chat_id: String,
        sender: String,
        content: String,
        metadata: serde_json::Value,
    },

    /// A request to invoke the model (emitted by inbound router).
    ModelRequest {
        id: String,
        chat_id: String,
        sender: String,
        content: String,
        /// Optional system prompt override.
        system_prompt: Option<String>,
        metadata: serde_json::Value,
    },

    /// The model's response (emitted by model invoker).
    ModelResponse {
        id: String,
        chat_id: String,
        content: String,
        model: String,
        metadata: serde_json::Value,
    },

    /// A request to deliver a message to a channel.
    DeliveryRequest {
        id: String,
        channel: String,
        chat_id: String,
        content: String,
        metadata: serde_json::Value,
    },

    /// Confirmation that delivery succeeded.
    DeliverySuccess {
        id: String,
        channel: String,
        chat_id: String,
        platform_message_id: Option<String>,
    },

    /// Notification that delivery failed.
    DeliveryFailure {
        id: String,
        channel: String,
        chat_id: String,
        error: String,
    },
}

impl SpineEvent {
    /// Generate a new unique event ID.
    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Get this event's ID.
    pub fn id(&self) -> &str {
        match self {
            Self::Inbound { id, .. }
            | Self::ModelRequest { id, .. }
            | Self::ModelResponse { id, .. }
            | Self::DeliveryRequest { id, .. }
            | Self::DeliverySuccess { id, .. }
            | Self::DeliveryFailure { id, .. } => id,
        }
    }

    /// Get the event type as a string.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Inbound { .. } => "inbound",
            Self::ModelRequest { .. } => "model_request",
            Self::ModelResponse { .. } => "model_response",
            Self::DeliveryRequest { .. } => "delivery_request",
            Self::DeliverySuccess { .. } => "delivery_success",
            Self::DeliveryFailure { .. } => "delivery_failure",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_id_is_unique() {
        let a = SpineEvent::new_id();
        let b = SpineEvent::new_id();
        assert_ne!(a, b);
    }

    #[test]
    fn event_type_matches() {
        let ev = SpineEvent::Inbound {
            id: SpineEvent::new_id(),
            source: "test".into(),
            chat_id: "123".into(),
            sender: "user".into(),
            content: "hello".into(),
            metadata: serde_json::json!({}),
        };
        assert_eq!(ev.event_type(), "inbound");
    }
}
