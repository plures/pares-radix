//! Session persistence — save and restore conversation sessions.
//!
//! [`SessionManager`] uses the [`StateStore`] trait to persist conversation
//! state across restarts.  Active sessions are stored under
//! `session:active:{chat_id}`, and archived sessions under
//! `session:archive:{chat_id}:{timestamp}`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::model::ChatMessage;
use crate::state::StateStore;

/// Manages session persistence via a [`StateStore`] backend.
pub struct SessionManager {
    store: Arc<dyn StateStore>,
}

/// A saved session including messages and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub messages: Vec<ChatMessage>,
    pub metadata: SessionMetadata,
}

/// Metadata about a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub started_at: u64,
    pub last_message_at: u64,
    pub message_count: usize,
    pub topic_summary: Option<String>,
}

/// Summary of a session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub key: String,
    pub started_at: u64,
    pub message_count: usize,
    pub topic_summary: Option<String>,
}

impl SessionManager {
    /// Create a new session manager backed by the given state store.
    pub fn new(store: Arc<dyn StateStore>) -> Self {
        Self { store }
    }

    fn active_key(chat_id: &str) -> String {
        format!("session:active:{chat_id}")
    }

    fn archive_key(chat_id: &str, timestamp: u64) -> String {
        format!("session:archive:{chat_id}:{timestamp}")
    }

    fn index_key(chat_id: &str) -> String {
        format!("session:index:{chat_id}")
    }

    /// Save the current session state for a chat.
    pub async fn save_session(
        &self,
        chat_id: &str,
        messages: &[ChatMessage],
        metadata: SessionMetadata,
    ) {
        let session = SavedSession {
            messages: messages.to_vec(),
            metadata,
        };
        match serde_json::to_value(&session) {
            Ok(value) => {
                self.store.set(&Self::active_key(chat_id), value).await;
                debug!(chat_id, "saved active session");
            }
            Err(e) => {
                warn!(error = %e, chat_id, "failed to serialize session");
            }
        }
    }

    /// Load the most recent active session for a chat.
    pub async fn load_active_session(&self, chat_id: &str) -> Option<SavedSession> {
        let value = self.store.get(&Self::active_key(chat_id)).await?;
        match serde_json::from_value(value) {
            Ok(session) => Some(session),
            Err(e) => {
                warn!(error = %e, chat_id, "failed to deserialize active session");
                None
            }
        }
    }

    /// Archive the current active session (called on /clear or session end).
    ///
    /// Moves the active session to an archive key and updates the session index.
    pub async fn archive_session(&self, chat_id: &str) {
        let Some(session) = self.load_active_session(chat_id).await else {
            return;
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Store the archive entry.
        let archive_key = Self::archive_key(chat_id, now);
        if let Ok(value) = serde_json::to_value(&session) {
            self.store.set(&archive_key, value).await;
        }

        // Update the session index (list of archive keys).
        let index_key = Self::index_key(chat_id);
        let mut index: Vec<String> = self
            .store
            .get(&index_key)
            .await
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        index.push(archive_key);
        if let Ok(value) = serde_json::to_value(&index) {
            self.store.set(&index_key, value).await;
        }

        // Clear the active session.
        self.store
            .set(&Self::active_key(chat_id), serde_json::Value::Null)
            .await;

        debug!(chat_id, "archived session");
    }

    /// List recent sessions (both active and archived) for a chat.
    pub async fn list_sessions(&self, chat_id: &str, limit: usize) -> Vec<SessionSummary> {
        let mut summaries = Vec::new();

        // Check active session.
        if let Some(session) = self.load_active_session(chat_id).await {
            summaries.push(SessionSummary {
                key: "active".to_string(),
                started_at: session.metadata.started_at,
                message_count: session.metadata.message_count,
                topic_summary: session.metadata.topic_summary,
            });
        }

        // Load archived sessions from index.
        let index_key = Self::index_key(chat_id);
        let index: Vec<String> = self
            .store
            .get(&index_key)
            .await
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Take most recent archives.
        for key in index.iter().rev().take(limit.saturating_sub(summaries.len())) {
            if let Some(value) = self.store.get(key).await {
                if let Ok(session) = serde_json::from_value::<SavedSession>(value) {
                    summaries.push(SessionSummary {
                        key: key.clone(),
                        started_at: session.metadata.started_at,
                        message_count: session.metadata.message_count,
                        topic_summary: session.metadata.topic_summary,
                    });
                }
            }
        }

        summaries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InMemoryStateStore;

    fn make_manager() -> SessionManager {
        SessionManager::new(Arc::new(InMemoryStateStore::new()))
    }

    fn make_metadata(count: usize) -> SessionMetadata {
        SessionMetadata {
            started_at: 1000,
            last_message_at: 2000,
            message_count: count,
            topic_summary: Some("test topic".into()),
        }
    }

    #[tokio::test]
    async fn save_and_load_active_session() {
        let mgr = make_manager();
        let messages = vec![
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi there"),
        ];
        mgr.save_session("chat1", &messages, make_metadata(2))
            .await;

        let loaded = mgr.load_active_session("chat1").await.unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.metadata.message_count, 2);
    }

    #[tokio::test]
    async fn load_returns_none_when_empty() {
        let mgr = make_manager();
        assert!(mgr.load_active_session("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn archive_moves_active_to_archive() {
        let mgr = make_manager();
        let messages = vec![ChatMessage::user("test")];
        mgr.save_session("chat1", &messages, make_metadata(1))
            .await;

        mgr.archive_session("chat1").await;

        // Active session should be cleared.
        assert!(mgr.load_active_session("chat1").await.is_none());

        // Should appear in session list.
        let sessions = mgr.list_sessions("chat1", 10).await;
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].key.starts_with("session:archive:"));
    }

    #[tokio::test]
    async fn list_sessions_includes_active_and_archived() {
        let mgr = make_manager();

        // Create and archive a session.
        let messages = vec![ChatMessage::user("old")];
        mgr.save_session("chat1", &messages, make_metadata(1))
            .await;
        mgr.archive_session("chat1").await;

        // Create a new active session.
        let messages = vec![ChatMessage::user("new")];
        mgr.save_session("chat1", &messages, make_metadata(1))
            .await;

        let sessions = mgr.list_sessions("chat1", 10).await;
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].key, "active");
    }
}
