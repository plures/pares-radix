//! Core types for the thread engine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A conversation thread — a logically grouped sequence of messages within a chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Unique thread identifier.
    pub id: String,
    /// The chat this thread belongs to.
    pub chat_id: String,
    /// Human-readable topic label.
    pub topic: String,
    /// Current lifecycle state.
    pub state: ThreadState,
    /// When this thread was created.
    pub created_at: DateTime<Utc>,
    /// When a message was last added to this thread.
    pub last_active_at: DateTime<Utc>,
    /// Number of messages in this thread.
    pub message_count: u64,
    /// Optional rolling summary of thread content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Channel-specific anchoring metadata (e.g., Telegram reply_to_message_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_anchor: Option<Value>,
}

impl Thread {
    /// Create a new thread with the given id, chat_id, and topic.
    pub fn new(id: impl Into<String>, chat_id: impl Into<String>, topic: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            chat_id: chat_id.into(),
            topic: topic.into(),
            state: ThreadState::Active,
            created_at: now,
            last_active_at: now,
            message_count: 0,
            summary: None,
            channel_anchor: None,
        }
    }
}

/// Lifecycle state of a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadState {
    /// Thread is actively receiving messages.
    Active,
    /// Thread is paused (user-initiated, not auto-archived).
    Paused,
    /// Thread is archived (stale or completed).
    Archived,
}

/// Decision output from the thread router — determines where a message goes.
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadDecision {
    /// Route to an existing thread.
    Existing { thread_id: String },
    /// Create a new thread with the given topic.
    New { topic: String },
    /// Continue in the current active thread (no routing needed).
    Continue,
}

/// Configuration for the thread engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadConfig {
    /// Whether threading is enabled at all.
    pub enabled: bool,
    /// Whether to auto-detect topic shifts.
    pub auto_detect: bool,
    /// Confidence threshold for auto-creating a new thread.
    pub auto_create_threshold: f64,
    /// Maximum number of active threads per chat.
    pub max_active: usize,
    /// Seconds of inactivity before a thread is auto-archived.
    pub archive_after_secs: u64,
    /// Summarize thread content after this many messages.
    pub summarize_after: u64,
}

impl Default for ThreadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_detect: true,
            auto_create_threshold: 0.75,
            max_active: 8,
            archive_after_secs: 48 * 3600, // 48 hours
            summarize_after: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_new_sets_defaults() {
        let t = Thread::new("t1", "chat-1", "debugging");
        assert_eq!(t.id, "t1");
        assert_eq!(t.chat_id, "chat-1");
        assert_eq!(t.topic, "debugging");
        assert_eq!(t.state, ThreadState::Active);
        assert_eq!(t.message_count, 0);
        assert!(t.summary.is_none());
        assert!(t.channel_anchor.is_none());
    }

    #[test]
    fn thread_state_serialization() {
        let active = serde_json::to_string(&ThreadState::Active).unwrap();
        assert_eq!(active, "\"active\"");
        let paused = serde_json::to_string(&ThreadState::Paused).unwrap();
        assert_eq!(paused, "\"paused\"");
        let archived = serde_json::to_string(&ThreadState::Archived).unwrap();
        assert_eq!(archived, "\"archived\"");
    }

    #[test]
    fn thread_state_deserialization() {
        let active: ThreadState = serde_json::from_str("\"active\"").unwrap();
        assert_eq!(active, ThreadState::Active);
        let paused: ThreadState = serde_json::from_str("\"paused\"").unwrap();
        assert_eq!(paused, ThreadState::Paused);
        let archived: ThreadState = serde_json::from_str("\"archived\"").unwrap();
        assert_eq!(archived, ThreadState::Archived);
    }

    #[test]
    fn thread_config_default() {
        let cfg = ThreadConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.auto_detect);
        assert!((cfg.auto_create_threshold - 0.75).abs() < f64::EPSILON);
        assert_eq!(cfg.max_active, 8);
        assert_eq!(cfg.archive_after_secs, 48 * 3600);
        assert_eq!(cfg.summarize_after, 5);
    }

    #[test]
    fn thread_config_serialization_roundtrip() {
        let cfg = ThreadConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: ThreadConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.enabled, cfg2.enabled);
        assert_eq!(cfg.max_active, cfg2.max_active);
        assert_eq!(cfg.archive_after_secs, cfg2.archive_after_secs);
    }

    #[test]
    fn thread_decision_equality() {
        let d1 = ThreadDecision::Existing {
            thread_id: "t1".to_string(),
        };
        let d2 = ThreadDecision::Existing {
            thread_id: "t1".to_string(),
        };
        assert_eq!(d1, d2);

        let d3 = ThreadDecision::New {
            topic: "testing".to_string(),
        };
        let d4 = ThreadDecision::Continue;
        assert_ne!(d3, d4);
    }

    #[test]
    fn thread_serialization_roundtrip() {
        let t = Thread::new("t1", "chat-1", "deployment");
        let json = serde_json::to_string(&t).unwrap();
        let t2: Thread = serde_json::from_str(&json).unwrap();
        assert_eq!(t.id, t2.id);
        assert_eq!(t.chat_id, t2.chat_id);
        assert_eq!(t.topic, t2.topic);
        assert_eq!(t.state, t2.state);
        assert_eq!(t.message_count, t2.message_count);
    }
}
