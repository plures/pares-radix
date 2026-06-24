//! Message classifier contract (platform seam).
//!
//! This module holds the **interface** for cerebellum message classification:
//! the [`ClassifierBackend`] trait plus its result DTOs
//! ([`MessageClassification`], [`MessageIntent`]). It carries no inference
//! logic and no model dependency — concrete backends (BitNet, OpenAI, the
//! heuristic `CerebellumClassifier`, etc.) live elsewhere and merely implement
//! the trait.
//!
//! ## Why this lives in `pares-radix-core`
//!
//! Platform crates that wire a model-backed classifier (e.g. the
//! `pares-radix-cli-runtime` host runtime, which builds a BitNet-backed
//! `ClassifierBackend`) must depend only on this contract — never on the
//! cognition crate. The cognition crate (`pares-agens-core`) owns the higher
//! level `CerebellumClassifier` orchestration and re-exports these types for
//! backward compatibility, so both sides share one definition without the
//! platform importing cognition.
//!
//! This mirrors the [`crate::memory_client`] and [`crate::subagent_spawn`]
//! seams: platform owns the trait + DTOs, cognition owns the orchestration.

use serde::{Deserialize, Serialize};

// ── classification result ────────────────────────────────────────────────────

/// The result of classifying a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageClassification {
    /// Detected intent category.
    pub intent: MessageIntent,
    /// Estimated complexity (1 = trivial, 5 = very complex).
    pub complexity: u8,
    /// Extracted topic summary (up to 3 content words).
    pub topic: String,
    /// Whether the topic shifted from the previous message.
    pub topic_shift: bool,
    /// Named entities extracted from the message.
    pub entities: Vec<String>,
    /// Name of a matching plugin, if any.
    pub plugin_match: Option<String>,
    /// Suggested completion hint for task-type messages.
    pub completion_hint: Option<String>,
    /// Whether the message likely requires tool use.
    pub needs_tools: bool,
    /// Whether the message warrants a deep/expensive model.
    pub needs_deep_model: bool,
}

/// Intent categories for inbound messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageIntent {
    /// A question expecting an answer.
    Question,
    /// A slash command or direct instruction.
    Command,
    /// A multi-step task requiring planning.
    Task,
    /// Casual conversation / chat.
    Chat,
    /// Acknowledgement or feedback on a prior response.
    Feedback,
}

// ── backend trait ────────────────────────────────────────────────────────────

/// Trait for model-backed classification.
///
/// Implementations receive a system prompt and user message and must return
/// a JSON string matching the [`MessageClassification`] schema.
pub trait ClassifierBackend: Send + Sync {
    /// Classify a user message, returning raw JSON.
    fn classify(&self, system_prompt: &str, user_message: &str) -> Result<String, String>;
}

// ── system prompt ────────────────────────────────────────────────────────────

/// System prompt instructing the model to output ONLY JSON matching the
/// `MessageClassification` schema.
pub const CLASSIFIER_SYSTEM_PROMPT: &str = r#"You are a message classifier. Output ONLY valid JSON matching this schema, no other text:
{
  "intent": "Question"|"Command"|"Task"|"Chat"|"Feedback",
  "complexity": 1-5,
  "topic": "short topic summary",
  "topic_shift": true|false,
  "entities": ["entity1", "entity2"],
  "plugin_match": "plugin-name"|null,
  "completion_hint": "expected completion description"|null,
  "needs_tools": true|false,
  "needs_deep_model": true|false
}
Rules:
- intent: Question (asks something), Command (starts with /), Task (asks to do something), Chat (casual), Feedback (ack/approval/rejection)
- complexity: 1=trivial, 2=simple, 3=moderate, 4=complex, 5=very complex
- topic: 1-3 word summary of the main subject
- topic_shift: true only if the topic is unrelated to prior context
- needs_tools: true if the task likely requires file/code/search/system tools
- needs_deep_model: true if the task requires deep reasoning (complexity >= 4)"#;
