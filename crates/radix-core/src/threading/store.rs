//! Thread-aware conversation store — PluresDB-backed thread persistence.

use std::collections::HashMap;
use std::sync::Arc;

use pluresdb::CrdtStore;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::debug;
use uuid::Uuid;

use crate::model::ChatMessage;

use super::types::{Thread, ThreadState};

/// Maximum messages to keep per thread (trim oldest when exceeded).
const MAX_HISTORY_PER_THREAD: usize = 50;

/// Actor name for PluresDB writes.
const ACTOR: &str = "thread_store";

/// Trait for thread-aware conversation storage.
#[async_trait::async_trait]
pub trait ThreadStore: Send + Sync {
    /// Get the currently active thread for a chat.
    async fn active_thread(&self, chat_id: &str) -> Option<Thread>;

    /// Switch the active thread for a chat.
    async fn switch_thread(
        &self,
        chat_id: &str,
        thread_id: &str,
    ) -> Result<Thread, ThreadStoreError>;

    /// Create a new thread in a chat.
    async fn create_thread(&self, chat_id: &str, topic: &str) -> Thread;

    /// Get the message history for a specific thread.
    async fn thread_history(&self, chat_id: &str, thread_id: &str) -> Vec<ChatMessage>;

    /// Add a message to the currently active thread.
    async fn add_message(&self, chat_id: &str, message: ChatMessage);

    /// Add a message to a specific thread.
    async fn add_message_to_thread(
        &self,
        chat_id: &str,
        thread_id: &str,
        message: ChatMessage,
    ) -> Result<(), ThreadStoreError>;

    /// List all threads for a chat.
    async fn list_threads(&self, chat_id: &str) -> Vec<Thread>;

    /// Archive a thread.
    async fn archive_thread(&self, chat_id: &str, thread_id: &str) -> Result<(), ThreadStoreError>;

    /// Find a thread matching a query string (by topic similarity).
    async fn find_matching_thread(&self, chat_id: &str, query: &str) -> Option<Thread>;
}

/// Errors from the thread store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadStoreError {
    /// The requested thread was not found.
    NotFound(String),
    /// A storage-level error occurred.
    StorageError(String),
}

impl std::fmt::Display for ThreadStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "thread not found: {id}"),
            Self::StorageError(msg) => write!(f, "thread store error: {msg}"),
        }
    }
}

impl std::error::Error for ThreadStoreError {}

// ── PluresDB-backed implementation ─────────────────────────────────────────

/// PluresDB-backed thread store.
///
/// Key scheme:
/// - `thread:{chat_id}:{thread_id}:meta` → Thread struct
/// - `thread:{chat_id}:{thread_id}:history` → Vec<ChatMessage>
/// - `thread:{chat_id}:index` → list of thread IDs
/// - `thread:{chat_id}:active` → current thread_id string
#[derive(Clone)]
pub struct PluresThreadStore {
    store: Arc<CrdtStore>,
}

impl PluresThreadStore {
    /// Create a new thread store backed by the given CrdtStore.
    pub fn new(store: Arc<CrdtStore>) -> Self {
        Self { store }
    }

    /// Create a store with a fresh in-memory CrdtStore (useful for tests).
    pub fn in_memory() -> Self {
        Self {
            store: Arc::new(CrdtStore::default()),
        }
    }

    fn meta_key(chat_id: &str, thread_id: &str) -> String {
        format!("thread:{chat_id}:{thread_id}:meta")
    }

    fn history_key(chat_id: &str, thread_id: &str) -> String {
        format!("thread:{chat_id}:{thread_id}:history")
    }

    fn index_key(chat_id: &str) -> String {
        format!("thread:{chat_id}:index")
    }

    fn active_key(chat_id: &str) -> String {
        format!("thread:{chat_id}:active")
    }

    fn legacy_history_key(chat_id: &str) -> String {
        format!("chat:{chat_id}:history")
    }

    fn get_thread_ids(&self, chat_id: &str) -> Vec<String> {
        let key = Self::index_key(chat_id);
        self.store
            .get(&key)
            .and_then(|record| {
                record
                    .data
                    .get("thread_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
            })
            .unwrap_or_default()
    }

    fn put_thread_ids(&self, chat_id: &str, ids: &[String]) {
        let key = Self::index_key(chat_id);
        self.store.put(
            key,
            ACTOR,
            json!({
                "_type": "thread:index",
                "chat_id": chat_id,
                "thread_ids": ids,
            }),
        );
    }

    fn get_active_thread_id(&self, chat_id: &str) -> Option<String> {
        let key = Self::active_key(chat_id);
        self.store.get(&key).and_then(|record| {
            record
                .data
                .get("thread_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
    }

    fn set_active_thread_id(&self, chat_id: &str, thread_id: &str) {
        let key = Self::active_key(chat_id);
        self.store.put(
            key,
            ACTOR,
            json!({
                "_type": "thread:active",
                "chat_id": chat_id,
                "thread_id": thread_id,
            }),
        );
    }

    fn get_thread_meta(&self, chat_id: &str, thread_id: &str) -> Option<Thread> {
        let key = Self::meta_key(chat_id, thread_id);
        self.store.get(&key).and_then(|record| {
            record
                .data
                .get("thread")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        })
    }

    fn put_thread_meta(&self, thread: &Thread) {
        let key = Self::meta_key(&thread.chat_id, &thread.id);
        self.store.put(
            key,
            ACTOR,
            json!({
                "_type": "thread:meta",
                "chat_id": &thread.chat_id,
                "thread_id": &thread.id,
                "thread": serde_json::to_value(thread).unwrap_or_default(),
            }),
        );
    }

    fn get_messages(&self, chat_id: &str, thread_id: &str) -> Vec<ChatMessage> {
        let key = Self::history_key(chat_id, thread_id);
        self.store
            .get(&key)
            .and_then(|record| {
                record
                    .data
                    .get("messages")
                    .and_then(|m| m.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| serde_json::from_value(m.clone()).ok())
                            .collect()
                    })
            })
            .unwrap_or_default()
    }

    fn put_messages(&self, chat_id: &str, thread_id: &str, messages: &[ChatMessage]) {
        let key = Self::history_key(chat_id, thread_id);
        let serialized: Vec<serde_json::Value> = messages
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();
        self.store.put(
            key,
            ACTOR,
            json!({
                "_type": "thread:history",
                "chat_id": chat_id,
                "thread_id": thread_id,
                "messages": serialized,
                "count": serialized.len(),
            }),
        );
    }

    /// Migrate legacy conversation history into a "default" thread.
    fn migrate_legacy(&self, chat_id: &str) -> Thread {
        let legacy_key = Self::legacy_history_key(chat_id);
        let legacy_messages: Vec<ChatMessage> = self
            .store
            .get(&legacy_key)
            .and_then(|record| {
                record
                    .data
                    .get("messages")
                    .and_then(|m| m.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| serde_json::from_value(m.clone()).ok())
                            .collect()
                    })
            })
            .unwrap_or_default();

        let thread_id = Uuid::new_v4().to_string();
        let mut thread = Thread::new(&thread_id, chat_id, "general");
        thread.message_count = legacy_messages.len() as u64;

        self.put_thread_meta(&thread);
        if !legacy_messages.is_empty() {
            self.put_messages(chat_id, &thread_id, &legacy_messages);
        }
        self.put_thread_ids(chat_id, std::slice::from_ref(&thread_id));
        self.set_active_thread_id(chat_id, &thread_id);

        debug!(
            chat_id = %chat_id,
            thread_id = %thread_id,
            migrated_messages = legacy_messages.len(),
            "thread_store: migrated legacy history into default thread"
        );

        thread
    }

    /// Ensure at least one thread exists for a chat, migrating legacy data if needed.
    fn ensure_initialized(&self, chat_id: &str) {
        let ids = self.get_thread_ids(chat_id);
        if ids.is_empty() {
            self.migrate_legacy(chat_id);
        }
    }
}

#[async_trait::async_trait]
impl ThreadStore for PluresThreadStore {
    async fn active_thread(&self, chat_id: &str) -> Option<Thread> {
        self.ensure_initialized(chat_id);
        let thread_id = self.get_active_thread_id(chat_id)?;
        self.get_thread_meta(chat_id, &thread_id)
    }

    async fn switch_thread(
        &self,
        chat_id: &str,
        thread_id: &str,
    ) -> Result<Thread, ThreadStoreError> {
        self.ensure_initialized(chat_id);
        let thread = self
            .get_thread_meta(chat_id, thread_id)
            .ok_or_else(|| ThreadStoreError::NotFound(thread_id.to_string()))?;

        self.set_active_thread_id(chat_id, thread_id);
        debug!(chat_id = %chat_id, thread_id = %thread_id, "thread_store: switched active thread");
        Ok(thread)
    }

    async fn create_thread(&self, chat_id: &str, topic: &str) -> Thread {
        self.ensure_initialized(chat_id);
        let thread_id = Uuid::new_v4().to_string();
        let thread = Thread::new(&thread_id, chat_id, topic);

        self.put_thread_meta(&thread);

        let mut ids = self.get_thread_ids(chat_id);
        ids.push(thread_id.clone());
        self.put_thread_ids(chat_id, &ids);

        self.set_active_thread_id(chat_id, &thread_id);

        debug!(chat_id = %chat_id, thread_id = %thread_id, topic = %topic, "thread_store: created new thread");
        thread
    }

    async fn thread_history(&self, chat_id: &str, thread_id: &str) -> Vec<ChatMessage> {
        self.get_messages(chat_id, thread_id)
    }

    async fn add_message(&self, chat_id: &str, message: ChatMessage) {
        self.ensure_initialized(chat_id);
        let thread_id = match self.get_active_thread_id(chat_id) {
            Some(id) => id,
            None => return,
        };

        let _ = self
            .add_message_to_thread(chat_id, &thread_id, message)
            .await;
    }

    async fn add_message_to_thread(
        &self,
        chat_id: &str,
        thread_id: &str,
        message: ChatMessage,
    ) -> Result<(), ThreadStoreError> {
        let mut thread = self
            .get_thread_meta(chat_id, thread_id)
            .ok_or_else(|| ThreadStoreError::NotFound(thread_id.to_string()))?;

        let mut messages = self.get_messages(chat_id, thread_id);
        messages.push(message);

        if messages.len() > MAX_HISTORY_PER_THREAD {
            let excess = messages.len() - MAX_HISTORY_PER_THREAD;
            messages.drain(..excess);
        }

        self.put_messages(chat_id, thread_id, &messages);

        thread.message_count += 1;
        thread.last_active_at = chrono::Utc::now();
        self.put_thread_meta(&thread);

        debug!(
            chat_id = %chat_id,
            thread_id = %thread_id,
            count = messages.len(),
            "thread_store: message recorded"
        );

        Ok(())
    }

    async fn list_threads(&self, chat_id: &str) -> Vec<Thread> {
        self.ensure_initialized(chat_id);
        let ids = self.get_thread_ids(chat_id);
        ids.iter()
            .filter_map(|id| self.get_thread_meta(chat_id, id))
            .collect()
    }

    async fn archive_thread(&self, chat_id: &str, thread_id: &str) -> Result<(), ThreadStoreError> {
        let mut thread = self
            .get_thread_meta(chat_id, thread_id)
            .ok_or_else(|| ThreadStoreError::NotFound(thread_id.to_string()))?;

        thread.state = ThreadState::Archived;
        self.put_thread_meta(&thread);

        if self.get_active_thread_id(chat_id).as_deref() == Some(thread_id) {
            let ids = self.get_thread_ids(chat_id);
            let next_active = ids.iter().find(|id| {
                *id != thread_id
                    && self
                        .get_thread_meta(chat_id, id)
                        .map(|t| t.state == ThreadState::Active)
                        .unwrap_or(false)
            });
            if let Some(next_id) = next_active {
                self.set_active_thread_id(chat_id, next_id);
            }
        }

        debug!(chat_id = %chat_id, thread_id = %thread_id, "thread_store: archived thread");
        Ok(())
    }

    async fn find_matching_thread(&self, chat_id: &str, query: &str) -> Option<Thread> {
        self.ensure_initialized(chat_id);
        let ids = self.get_thread_ids(chat_id);
        let query_lower = query.to_lowercase();

        ids.iter()
            .filter_map(|id| self.get_thread_meta(chat_id, id))
            .find(|thread| {
                thread.state == ThreadState::Active
                    && thread.topic.to_lowercase().contains(&query_lower)
            })
    }
}

// ── In-memory implementation for tests ──────────────────────────────────────

/// In-memory thread store for tests.
#[derive(Debug, Clone, Default)]
pub struct MemoryThreadStore {
    pub(crate) threads: Arc<RwLock<HashMap<String, Thread>>>,
    histories: Arc<RwLock<HashMap<String, Vec<ChatMessage>>>>,
    index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    active: Arc<RwLock<HashMap<String, String>>>,
}

impl MemoryThreadStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn composite_key(chat_id: &str, thread_id: &str) -> String {
        format!("{chat_id}:{thread_id}")
    }
}

#[async_trait::async_trait]
impl ThreadStore for MemoryThreadStore {
    async fn active_thread(&self, chat_id: &str) -> Option<Thread> {
        let active = self.active.read().await;
        let thread_id = active.get(chat_id)?;
        let threads = self.threads.read().await;
        threads
            .get(&Self::composite_key(chat_id, thread_id))
            .cloned()
    }

    async fn switch_thread(
        &self,
        chat_id: &str,
        thread_id: &str,
    ) -> Result<Thread, ThreadStoreError> {
        let threads = self.threads.read().await;
        let key = Self::composite_key(chat_id, thread_id);
        let thread = threads
            .get(&key)
            .cloned()
            .ok_or_else(|| ThreadStoreError::NotFound(thread_id.to_string()))?;
        drop(threads);

        let mut active = self.active.write().await;
        active.insert(chat_id.to_string(), thread_id.to_string());
        Ok(thread)
    }

    async fn create_thread(&self, chat_id: &str, topic: &str) -> Thread {
        let thread_id = Uuid::new_v4().to_string();
        let thread = Thread::new(&thread_id, chat_id, topic);

        let key = Self::composite_key(chat_id, &thread_id);
        let mut threads = self.threads.write().await;
        threads.insert(key, thread.clone());
        drop(threads);

        let mut index = self.index.write().await;
        index
            .entry(chat_id.to_string())
            .or_default()
            .push(thread_id.clone());
        drop(index);

        let mut active = self.active.write().await;
        active.insert(chat_id.to_string(), thread_id);

        thread
    }

    async fn thread_history(&self, chat_id: &str, thread_id: &str) -> Vec<ChatMessage> {
        let histories = self.histories.read().await;
        let key = Self::composite_key(chat_id, thread_id);
        histories.get(&key).cloned().unwrap_or_default()
    }

    async fn add_message(&self, chat_id: &str, message: ChatMessage) {
        let active = self.active.read().await;
        let thread_id = match active.get(chat_id) {
            Some(id) => id.clone(),
            None => return,
        };
        drop(active);

        let _ = self
            .add_message_to_thread(chat_id, &thread_id, message)
            .await;
    }

    async fn add_message_to_thread(
        &self,
        chat_id: &str,
        thread_id: &str,
        message: ChatMessage,
    ) -> Result<(), ThreadStoreError> {
        let key = Self::composite_key(chat_id, thread_id);

        {
            let threads = self.threads.read().await;
            if !threads.contains_key(&key) {
                return Err(ThreadStoreError::NotFound(thread_id.to_string()));
            }
        }

        let mut histories = self.histories.write().await;
        let history = histories.entry(key.clone()).or_default();
        history.push(message);
        if history.len() > MAX_HISTORY_PER_THREAD {
            let excess = history.len() - MAX_HISTORY_PER_THREAD;
            history.drain(..excess);
        }
        drop(histories);

        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(&key) {
            thread.message_count += 1;
            thread.last_active_at = chrono::Utc::now();
        }

        Ok(())
    }

    async fn list_threads(&self, chat_id: &str) -> Vec<Thread> {
        let index = self.index.read().await;
        let ids = match index.get(chat_id) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };
        drop(index);

        let threads = self.threads.read().await;
        ids.iter()
            .filter_map(|id| threads.get(&Self::composite_key(chat_id, id)).cloned())
            .collect()
    }

    async fn archive_thread(&self, chat_id: &str, thread_id: &str) -> Result<(), ThreadStoreError> {
        let key = Self::composite_key(chat_id, thread_id);
        let mut threads = self.threads.write().await;
        let thread = threads
            .get_mut(&key)
            .ok_or_else(|| ThreadStoreError::NotFound(thread_id.to_string()))?;
        thread.state = ThreadState::Archived;
        drop(threads);

        let mut active = self.active.write().await;
        if active.get(chat_id).map(|s| s.as_str()) == Some(thread_id) {
            let index = self.index.read().await;
            if let Some(ids) = index.get(chat_id) {
                let threads = self.threads.read().await;
                let next = ids.iter().find(|id| {
                    *id != thread_id
                        && threads
                            .get(&Self::composite_key(chat_id, id))
                            .map(|t| t.state == ThreadState::Active)
                            .unwrap_or(false)
                });
                if let Some(next_id) = next {
                    active.insert(chat_id.to_string(), next_id.clone());
                } else {
                    active.remove(chat_id);
                }
            }
        }

        Ok(())
    }

    async fn find_matching_thread(&self, chat_id: &str, query: &str) -> Option<Thread> {
        let index = self.index.read().await;
        let ids = index.get(chat_id)?;
        let threads = self.threads.read().await;
        let query_lower = query.to_lowercase();

        ids.iter()
            .filter_map(|id| threads.get(&Self::composite_key(chat_id, id)))
            .find(|thread| {
                thread.state == ThreadState::Active
                    && thread.topic.to_lowercase().contains(&query_lower)
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_create_and_get_active() {
        let store = MemoryThreadStore::new();
        let thread = store.create_thread("chat-1", "debugging").await;
        assert_eq!(thread.topic, "debugging");

        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, thread.id);
    }

    #[tokio::test]
    async fn memory_switch_thread() {
        let store = MemoryThreadStore::new();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t2.id);

        let switched = store.switch_thread("chat-1", &t1.id).await.unwrap();
        assert_eq!(switched.id, t1.id);

        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t1.id);
    }

    #[tokio::test]
    async fn memory_switch_nonexistent() {
        let store = MemoryThreadStore::new();
        store.create_thread("chat-1", "topic-a").await;

        let err = store
            .switch_thread("chat-1", "nonexistent")
            .await
            .unwrap_err();
        assert_eq!(err, ThreadStoreError::NotFound("nonexistent".to_string()));
    }

    #[tokio::test]
    async fn memory_add_message_and_history() {
        let store = MemoryThreadStore::new();
        let thread = store.create_thread("chat-1", "testing").await;

        store
            .add_message("chat-1", ChatMessage::user("Hello"))
            .await;
        store
            .add_message("chat-1", ChatMessage::assistant("Hi"))
            .await;

        let history = store.thread_history("chat-1", &thread.id).await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].content, "Hi");
    }

    #[tokio::test]
    async fn memory_add_to_specific_thread() {
        let store = MemoryThreadStore::new();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        store
            .add_message_to_thread("chat-1", &t1.id, ChatMessage::user("for t1"))
            .await
            .unwrap();
        store
            .add_message_to_thread("chat-1", &t2.id, ChatMessage::user("for t2"))
            .await
            .unwrap();

        assert_eq!(
            store.thread_history("chat-1", &t1.id).await[0].content,
            "for t1"
        );
        assert_eq!(
            store.thread_history("chat-1", &t2.id).await[0].content,
            "for t2"
        );
    }

    #[tokio::test]
    async fn memory_list_threads() {
        let store = MemoryThreadStore::new();
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;
        store.create_thread("chat-2", "other").await;

        assert_eq!(store.list_threads("chat-1").await.len(), 2);
        assert_eq!(store.list_threads("chat-2").await.len(), 1);
    }

    #[tokio::test]
    async fn memory_archive_switches_active() {
        let store = MemoryThreadStore::new();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        store.archive_thread("chat-1", &t2.id).await.unwrap();

        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t1.id);

        let threads = store.list_threads("chat-1").await;
        let archived = threads.iter().find(|t| t.id == t2.id).unwrap();
        assert_eq!(archived.state, ThreadState::Archived);
    }

    #[tokio::test]
    async fn memory_find_matching() {
        let store = MemoryThreadStore::new();
        store.create_thread("chat-1", "debugging rust").await;
        store.create_thread("chat-1", "architecture design").await;

        let found = store.find_matching_thread("chat-1", "rust").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().topic, "debugging rust");

        assert!(store
            .find_matching_thread("chat-1", "python")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn memory_trims_history() {
        let store = MemoryThreadStore::new();
        let thread = store.create_thread("chat-1", "flood").await;

        for i in 0..60 {
            store
                .add_message_to_thread("chat-1", &thread.id, ChatMessage::user(format!("msg-{i}")))
                .await
                .unwrap();
        }

        let history = store.thread_history("chat-1", &thread.id).await;
        assert_eq!(history.len(), MAX_HISTORY_PER_THREAD);
        assert_eq!(history[0].content, "msg-10");
    }

    #[tokio::test]
    async fn memory_no_active_for_unknown() {
        let store = MemoryThreadStore::new();
        assert!(store.active_thread("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn plures_create_and_get_active() {
        let store = PluresThreadStore::in_memory();
        let thread = store.create_thread("chat-1", "deployment").await;
        assert_eq!(thread.topic, "deployment");

        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, thread.id);
    }

    #[tokio::test]
    async fn plures_switch_thread() {
        let store = PluresThreadStore::in_memory();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let _t2 = store.create_thread("chat-1", "topic-b").await;

        store.switch_thread("chat-1", &t1.id).await.unwrap();
        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t1.id);
    }

    #[tokio::test]
    async fn plures_add_message_roundtrip() {
        let store = PluresThreadStore::in_memory();
        let thread = store.create_thread("chat-1", "testing").await;

        store
            .add_message("chat-1", ChatMessage::user("Hello"))
            .await;
        store
            .add_message("chat-1", ChatMessage::assistant("Hi"))
            .await;

        let history = store.thread_history("chat-1", &thread.id).await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].content, "Hi");
    }

    #[tokio::test]
    async fn plures_archive_thread() {
        let store = PluresThreadStore::in_memory();
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        // t2 is currently active (last created)
        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t2.id);

        store.archive_thread("chat-1", &t2.id).await.unwrap();

        // After archiving t2, active should switch to another non-archived thread
        let active = store.active_thread("chat-1").await.unwrap();
        assert_ne!(active.id, t2.id);
        assert_eq!(active.state, ThreadState::Active);

        // Verify t2 is indeed archived
        let threads = store.list_threads("chat-1").await;
        let archived = threads.iter().find(|t| t.id == t2.id).unwrap();
        assert_eq!(archived.state, ThreadState::Archived);

        // Verify t1 still exists and is active
        let t1_found = threads.iter().find(|t| t.id == t1.id).unwrap();
        assert_eq!(t1_found.state, ThreadState::Active);
    }

    #[tokio::test]
    async fn plures_list_threads() {
        let store = PluresThreadStore::in_memory();
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let threads = store.list_threads("chat-1").await;
        // list_threads calls ensure_initialized which creates a "general" thread
        // then we create 2 more = 3 total
        assert!(threads.len() >= 2);
    }

    #[tokio::test]
    async fn plures_legacy_migration() {
        let crdt = Arc::new(CrdtStore::default());

        // Seed legacy data
        crdt.put(
            "chat:legacy-chat:history".to_string(),
            "conversation_store",
            json!({
                "_type": "conversation:history",
                "chat_id": "legacy-chat",
                "messages": [
                    {"role": "user", "content": "old msg 1"},
                    {"role": "assistant", "content": "old msg 2"}
                ],
                "count": 2
            }),
        );

        let store = PluresThreadStore::new(crdt);

        // First access should migrate
        let active = store.active_thread("legacy-chat").await.unwrap();
        assert_eq!(active.topic, "general");
        assert_eq!(active.message_count, 2);

        let history = store.thread_history("legacy-chat", &active.id).await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "old msg 1");
    }

    #[tokio::test]
    async fn plures_find_matching() {
        let store = PluresThreadStore::in_memory();
        store.create_thread("chat-1", "debugging rust").await;
        store.create_thread("chat-1", "architecture").await;

        let found = store.find_matching_thread("chat-1", "rust").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().topic, "debugging rust");
    }

    #[tokio::test]
    async fn plures_trims_history() {
        let store = PluresThreadStore::in_memory();
        let thread = store.create_thread("chat-1", "flood").await;

        for i in 0..60 {
            store
                .add_message_to_thread("chat-1", &thread.id, ChatMessage::user(format!("msg-{i}")))
                .await
                .unwrap();
        }

        let history = store.thread_history("chat-1", &thread.id).await;
        assert_eq!(history.len(), MAX_HISTORY_PER_THREAD);
        assert_eq!(history[0].content, "msg-10");
    }
}
