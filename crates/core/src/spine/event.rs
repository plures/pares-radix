//! Spine events — the messages that flow through the pipeline.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::ToolCall;

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
        /// The channel that originated this request (e.g. "telegram", "discord").
        source: String,
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
        /// The originating channel (propagated from ModelRequest).
        source: String,
        chat_id: String,
        /// Text content (may be empty if model only made tool calls).
        content: String,
        model: String,
        /// Tool calls requested by the model (empty if direct text response).
        tool_calls: Vec<ToolCall>,
        metadata: serde_json::Value,
    },

    /// A request to execute a tool (emitted by tool executor when processing tool calls).
    ToolRequest {
        id: String,
        chat_id: String,
        /// The tool call to execute.
        tool_call: ToolCall,
        metadata: serde_json::Value,
    },

    /// Result of a tool execution.
    ToolResult {
        id: String,
        chat_id: String,
        /// The tool_call.id this result correlates to.
        tool_call_id: String,
        /// The tool name that was called.
        tool_name: String,
        /// The result content.
        content: String,
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

    /// A periodic timer tick (used to trigger task evaluation).
    Timer {
        id: String,
        /// The timer's logical name (e.g. "task_eval").
        name: String,
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
            | Self::DeliveryFailure { id, .. }
            | Self::ToolRequest { id, .. }
            | Self::ToolResult { id, .. }
            | Self::Timer { id, .. } => id,
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
            Self::ToolRequest { .. } => "tool_request",
            Self::ToolResult { .. } => "tool_result",
            Self::Timer { .. } => "timer",
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
