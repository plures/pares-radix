//! Conversation memory — stores chat history per chat_id for multi-turn context.

use std::collections::HashMap;
use std::sync::Arc;

use pluresdb::CrdtStore;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::debug;

use crate::model::ChatMessage;

/// Maximum messages to keep per conversation (trim oldest when exceeded).
const MAX_HISTORY_PER_CHAT: usize = 50;

/// Actor name for PluresDB writes.
const ACTOR: &str = "conversation_store";

/// Trait for storing and retrieving conversation history.
#[async_trait::async_trait]
pub trait ConversationStore: Send + Sync {
    /// Get the conversation history for a chat.
    async fn get_history(&self, chat_id: &str) -> Vec<ChatMessage>;

    /// Add a message to a chat's history.
    async fn add_message(&self, chat_id: &str, message: ChatMessage);

    /// Clear a chat's history.
    async fn clear(&self, chat_id: &str);
}

/// PluresDB-backed conversation store. Messages are stored as a JSON array
/// under key `chat:{chat_id}:history` in the CrdtStore.
#[derive(Clone)]
pub struct PluresConversationStore {
    store: Arc<CrdtStore>,
}

impl PluresConversationStore {
    /// Create a new store backed by the given CrdtStore.
    pub fn new(store: Arc<CrdtStore>) -> Self {
        Self { store }
    }

    /// Create a store with a fresh in-memory CrdtStore.
    pub fn in_memory() -> Self {
        Self {
            store: Arc::new(CrdtStore::default()),
        }
    }

    /// Create a store with persistent disk-backed storage.
    ///
    /// Uses SledStorage for durable on-disk persistence. Falls back to
    /// in-memory if the path cannot be opened.
    pub fn persistent(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        use pluresdb::{SledStorage, StorageEngine};
        let storage: Arc<dyn StorageEngine> =
            Arc::new(SledStorage::open(path).map_err(|e| format!("open conversation store: {e}"))?);
        let store = Arc::new(CrdtStore::default().with_persistence(storage));
        Ok(Self { store })
    }

    fn key_for(chat_id: &str) -> String {
        format!("chat:{}:history", chat_id)
    }
}

#[async_trait::async_trait]
impl ConversationStore for PluresConversationStore {
    async fn get_history(&self, chat_id: &str) -> Vec<ChatMessage> {
        let key = Self::key_for(chat_id);
        match self.store.get(&key) {
            Some(record) => {
                if let Some(messages) = record.data.get("messages").and_then(|m| m.as_array()) {
                    messages
                        .iter()
                        .filter_map(|m| serde_json::from_value(m.clone()).ok())
                        .collect()
                } else {
                    vec![]
                }
            }
            None => vec![],
        }
    }

    async fn add_message(&self, chat_id: &str, message: ChatMessage) {
        let key = Self::key_for(chat_id);

        // Read current history
        let mut messages: Vec<serde_json::Value> = self
            .store
            .get(&key)
            .and_then(|record| {
                record
                    .data
                    .get("messages")
                    .and_then(|m| m.as_array().cloned())
            })
            .unwrap_or_default();

        // Append new message
        if let Ok(serialized) = serde_json::to_value(&message) {
            messages.push(serialized);
        }

        // Trim to max size
        if messages.len() > MAX_HISTORY_PER_CHAT {
            let excess = messages.len() - MAX_HISTORY_PER_CHAT;
            messages.drain(..excess);
        }

        // Write back
        self.store.put(
            key,
            ACTOR,
            json!({
                "_type": "conversation:history",
                "chat_id": chat_id,
                "messages": messages,
                "count": messages.len(),
            }),
        );

        debug!(chat_id = %chat_id, count = messages.len(), "conversation_store: message recorded");
    }

    async fn clear(&self, chat_id: &str) {
        let key = Self::key_for(chat_id);
        self.store.put(
            key,
            ACTOR,
            json!({
                "_type": "conversation:history",
                "chat_id": chat_id,
                "messages": [],
                "count": 0,
            }),
        );
    }
}

/// In-memory conversation store for tests.
#[derive(Debug, Clone, Default)]
pub struct MemoryConversationStore {
    histories: Arc<RwLock<HashMap<String, Vec<ChatMessage>>>>,
}

impl MemoryConversationStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl ConversationStore for MemoryConversationStore {
    async fn get_history(&self, chat_id: &str) -> Vec<ChatMessage> {
        let histories = self.histories.read().await;
        histories.get(chat_id).cloned().unwrap_or_default()
    }

    async fn add_message(&self, chat_id: &str, message: ChatMessage) {
        let mut histories = self.histories.write().await;
        let history = histories.entry(chat_id.to_string()).or_default();
        history.push(message);

        if history.len() > MAX_HISTORY_PER_CHAT {
            let excess = history.len() - MAX_HISTORY_PER_CHAT;
            history.drain(..excess);
        }
    }

    async fn clear(&self, chat_id: &str) {
        let mut histories = self.histories.write().await;
        histories.remove(chat_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_and_retrieve_messages() {
        let store = MemoryConversationStore::new();

        store
            .add_message("chat-1", ChatMessage::user("Hello"))
            .await;
        store
            .add_message("chat-1", ChatMessage::assistant("Hi there"))
            .await;

        let history = store.get_history("chat-1").await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "Hi there");
    }

    #[tokio::test]
    async fn memory_separate_chats_are_isolated() {
        let store = MemoryConversationStore::new();

        store.add_message("chat-1", ChatMessage::user("msg1")).await;
        store.add_message("chat-2", ChatMessage::user("msg2")).await;

        assert_eq!(store.get_history("chat-1").await.len(), 1);
        assert_eq!(store.get_history("chat-2").await.len(), 1);
    }

    #[tokio::test]
    async fn memory_trims_to_max_size() {
        let store = MemoryConversationStore::new();

        for i in 0..60 {
            store
                .add_message("chat-1", ChatMessage::user(format!("msg-{}", i)))
                .await;
        }

        let history = store.get_history("chat-1").await;
        assert_eq!(history.len(), MAX_HISTORY_PER_CHAT);
        assert_eq!(history[0].content, "msg-10");
    }

    #[tokio::test]
    async fn plures_store_roundtrip() {
        let crdt_store = Arc::new(CrdtStore::default());
        let store = PluresConversationStore::new(crdt_store);

        store
            .add_message("chat-1", ChatMessage::user("Hello"))
            .await;
        store
            .add_message("chat-1", ChatMessage::assistant("Hi"))
            .await;

        let history = store.get_history("chat-1").await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "Hi");
    }

    #[tokio::test]
    async fn plures_store_clear() {
        let crdt_store = Arc::new(CrdtStore::default());
        let store = PluresConversationStore::new(crdt_store);

        store
            .add_message("chat-1", ChatMessage::user("Hello"))
            .await;
        store.clear("chat-1").await;

        let history = store.get_history("chat-1").await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn plures_store_trims() {
        let crdt_store = Arc::new(CrdtStore::default());
        let store = PluresConversationStore::new(crdt_store);

        for i in 0..60 {
            store
                .add_message("chat-1", ChatMessage::user(format!("msg-{}", i)))
                .await;
        }

        let history = store.get_history("chat-1").await;
        assert_eq!(history.len(), MAX_HISTORY_PER_CHAT);
        assert_eq!(history[0].content, "msg-10");
    }

    #[tokio::test]
    async fn plures_store_empty_for_unknown() {
        let crdt_store = Arc::new(CrdtStore::default());
        let store = PluresConversationStore::new(crdt_store);
        assert!(store.get_history("nonexistent").await.is_empty());
    }
}
