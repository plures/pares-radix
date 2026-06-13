//! Topic Routing action handlers.
//!
//! Provides boundary actors for the topic-routing.px procedure:
//! - `classify_message_topic` — LLM-delegated classification (placeholder → routes to generate)
//! - `evaluate_topic_confidence` — pure confidence threshold check
//! - `build_steering_context` — assemble context for steering
//! - `format_topic_switch` — format topic transition message
//! - `extract_topic_signals` — extract topic signals from message content

use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use pares_radix_praxis::px::executor::ExecutionError;

/// Helper to construct ActionFailed errors concisely.
#[allow(dead_code)]
fn err(action: &str, message: impl Into<String>) -> ExecutionError {
    ExecutionError::ActionFailed {
        action: action.to_string(),
        message: message.into(),
    }
}

/// Topic routing action handler.
pub struct TopicRoutingActionHandler;

impl Default for TopicRoutingActionHandler {
    fn default() -> Self {
        Self
    }
}

impl TopicRoutingActionHandler {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate topic confidence — pure threshold check.
    /// Input: {confidence: 0.85, threshold: 0.7}
    /// Output: {above_threshold: true, confidence: 0.85}
    fn evaluate_topic_confidence(&self, params: &Value) -> Result<Value, ExecutionError> {
        let confidence = params
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let threshold = params
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7);

        Ok(json!({
            "above_threshold": confidence >= threshold,
            "confidence": confidence,
            "threshold": threshold,
            "margin": confidence - threshold
        }))
    }

    /// Build steering context for topic continuation.
    /// Assembles recent messages + current topic state into a context block.
    fn build_steering_context(&self, params: &Value) -> Result<Value, ExecutionError> {
        let current_topic = params
            .get("current_topic")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        let recent_messages = params
            .get("recent_messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let topic_history = params
            .get("topic_history")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Build a steering context block
        let context = json!({
            "current_topic": current_topic,
            "recent_message_count": recent_messages.len(),
            "topic_switches": topic_history.len(),
            "last_messages": recent_messages.iter().take(5).cloned().collect::<Vec<Value>>(),
            "steering_instruction": format!(
                "Continue in the context of topic '{}'. {} recent messages, {} topic switches in session.",
                current_topic, recent_messages.len(), topic_history.len()
            )
        });

        Ok(context)
    }

    /// Format a topic switch notification.
    fn format_topic_switch(&self, params: &Value) -> Result<Value, ExecutionError> {
        let from_topic = params
            .get("from_topic")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let to_topic = params
            .get("to_topic")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let reason = params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("user initiated");

        Ok(json!({
            "switch": {
                "from": from_topic,
                "to": to_topic,
                "reason": reason
            },
            "context_instruction": format!(
                "Topic switched from '{}' to '{}'. Reason: {}. Adjust context accordingly.",
                from_topic, to_topic, reason
            )
        }))
    }

    /// Extract topic signals from message content.
    /// This is a heuristic pre-filter before LLM classification.
    fn extract_topic_signals(&self, params: &Value) -> Result<Value, ExecutionError> {
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut signals: Vec<Value> = Vec::new();

        // Simple keyword-based signal extraction (heuristic, not classification)
        let code_keywords = [
            "function", "class", "impl", "fn ", "def ", "const ", "let ", "var ", "bug", "error",
            "compile", "cargo", "npm", "git",
        ];
        let architecture_keywords = [
            "design",
            "architecture",
            "system",
            "component",
            "service",
            "infrastructure",
            "deploy",
        ];
        let conversation_keywords = [
            "how are you",
            "what do you think",
            "opinion",
            "feel",
            "hey",
            "hi",
            "thanks",
        ];

        let lower = message.to_lowercase();

        let code_hits: usize = code_keywords
            .iter()
            .filter(|kw| lower.contains(*kw))
            .count();
        let arch_hits: usize = architecture_keywords
            .iter()
            .filter(|kw| lower.contains(*kw))
            .count();
        let conv_hits: usize = conversation_keywords
            .iter()
            .filter(|kw| lower.contains(*kw))
            .count();

        if code_hits > 0 {
            signals.push(json!({"type": "code", "strength": code_hits}));
        }
        if arch_hits > 0 {
            signals.push(json!({"type": "architecture", "strength": arch_hits}));
        }
        if conv_hits > 0 {
            signals.push(json!({"type": "conversation", "strength": conv_hits}));
        }

        // Determine strongest signal
        let dominant = if code_hits >= arch_hits && code_hits >= conv_hits {
            "code"
        } else if arch_hits >= conv_hits {
            "architecture"
        } else {
            "conversation"
        };

        Ok(json!({
            "signals": signals,
            "dominant_signal": dominant,
            "total_signal_strength": code_hits + arch_hits + conv_hits,
            "needs_llm_classification": signals.is_empty() || (code_hits + arch_hits + conv_hits) < 2
        }))
    }

    /// Check if topic has changed (comparing new classification to current).
    fn topic_changed(&self, params: &Value) -> Result<Value, ExecutionError> {
        let current = params
            .get("current_topic")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let new_topic = params
            .get("new_topic")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let changed = !current.is_empty() && !new_topic.is_empty() && current != new_topic;

        Ok(json!({
            "changed": changed,
            "current": current,
            "new": new_topic
        }))
    }
}

#[async_trait::async_trait]
impl AsyncActionHandler for TopicRoutingActionHandler {
    async fn call(
        &self,
        name: &str,
        params: &Value,
    ) -> Result<Value, ExecutionError> {
        match name {
            "evaluate_topic_confidence" => self.evaluate_topic_confidence(params),
            "build_steering_context" => self.build_steering_context(params),
            "format_topic_switch" => self.format_topic_switch(params),
            "extract_topic_signals" => self.extract_topic_signals(params),
            "topic_changed" => self.topic_changed(params),
            _ => Err(ExecutionError::UnknownAction(name.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_confidence_above_threshold() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .evaluate_topic_confidence(&json!({"confidence": 0.85, "threshold": 0.7}))
            .unwrap();
        assert_eq!(result["above_threshold"], true);
    }

    #[test]
    fn evaluate_confidence_below_threshold() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .evaluate_topic_confidence(&json!({"confidence": 0.5, "threshold": 0.7}))
            .unwrap();
        assert_eq!(result["above_threshold"], false);
    }

    #[test]
    fn extract_code_signals() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .extract_topic_signals(&json!({
                "message": "there's a compile error in the function, cargo test fails"
            }))
            .unwrap();
        assert_eq!(result["dominant_signal"], "code");
        assert!(result["total_signal_strength"].as_u64().unwrap() >= 2);
    }

    #[test]
    fn extract_conversation_signals() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .extract_topic_signals(&json!({
                "message": "hey, what do you think about this? thanks!"
            }))
            .unwrap();
        assert_eq!(result["dominant_signal"], "conversation");
    }

    #[test]
    fn topic_changed_detects_switch() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .topic_changed(&json!({
                "current_topic": "code",
                "new_topic": "architecture"
            }))
            .unwrap();
        assert_eq!(result["changed"], true);
    }

    #[test]
    fn topic_changed_same_topic() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .topic_changed(&json!({
                "current_topic": "code",
                "new_topic": "code"
            }))
            .unwrap();
        assert_eq!(result["changed"], false);
    }

    #[test]
    fn format_topic_switch_output() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .format_topic_switch(&json!({
                "from_topic": "debugging",
                "to_topic": "architecture",
                "reason": "user asked about system design"
            }))
            .unwrap();
        assert_eq!(result["switch"]["from"], "debugging");
        assert_eq!(result["switch"]["to"], "architecture");
    }

    #[test]
    fn build_steering_context_output() {
        let handler = TopicRoutingActionHandler::new();
        let result = handler
            .build_steering_context(&json!({
                "current_topic": "deployment",
                "recent_messages": [{"role": "user", "content": "deploy to prod"}],
                "topic_history": ["code", "testing"]
            }))
            .unwrap();
        assert_eq!(result["current_topic"], "deployment");
        assert_eq!(result["recent_message_count"], 1);
        assert_eq!(result["topic_switches"], 2);
    }
}
