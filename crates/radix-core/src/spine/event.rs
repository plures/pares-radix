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

    /// A periodic heartbeat tick emitted by the heartbeat runner (Rust IO,
    /// design: praxis/spine/spine.px IO boundary #4). The pipeline loop turns
    /// this into a `heartbeat_tick:<id>` reactive write, which
    /// `autonomous-dispatch.px::evaluate_dispatch` consumes to decide whether
    /// to dispatch an autonomous task this tick. Decision logic lives in `.px`;
    /// this event only carries the tick counter.
    HeartbeatTick {
        id: String,
        /// Monotonic tick counter (payload consumed by evaluate_dispatch as `tick: int`).
        tick: i64,
    },

    /// A new conversation thread was created.
    ThreadCreated {
        id: String,
        chat_id: String,
        thread_id: String,
        topic: String,
        /// Channel-specific anchor data (opaque to core).
        channel_anchor: serde_json::Value,
    },

    /// Active thread switched for a chat.
    ThreadSwitched {
        id: String,
        chat_id: String,
        from_thread_id: String,
        to_thread_id: String,
    },

    /// A thread was archived (inactivity or user action).
    ThreadArchived {
        id: String,
        chat_id: String,
        thread_id: String,
    },

    /// Topic classification result from .px procedure (reactive bridge).
    TopicClassified {
        id: String,
        chat_id: String,
        /// The classification result from topic-routing.px.
        classification: serde_json::Value,
        /// Original inbound event metadata for correlation.
        original_metadata: serde_json::Value,
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
            | Self::Timer { id, .. }
            | Self::HeartbeatTick { id, .. }
            | Self::ThreadCreated { id, .. }
            | Self::ThreadSwitched { id, .. }
            | Self::ThreadArchived { id, .. }
            | Self::TopicClassified { id, .. } => id,
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
            Self::HeartbeatTick { .. } => "heartbeat_tick",
            Self::ThreadCreated { .. } => "thread_created",
            Self::ThreadSwitched { .. } => "thread_switched",
            Self::ThreadArchived { .. } => "thread_archived",
            Self::TopicClassified { .. } => "topic_classified",
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
