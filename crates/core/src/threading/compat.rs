//! Compatibility adapter — wraps a ThreadStore to implement ConversationStore.
//!
//! This allows the existing ModelInvoker to use thread-aware storage
//! transparently. It always reads/writes to the active thread for the
//! given chat_id.

use std::sync::Arc;

use async_trait::async_trait;

use crate::model::ChatMessage;
use crate::spine::conversation::ConversationStore;
use crate::threading::store::ThreadStore;

/// Adapts a [`ThreadStore`] into a [`ConversationStore`] by routing all
/// operations to the active thread for the given `chat_id`.
///
/// This is the bridge that lets existing code (e.g. `ModelInvoker`) that
/// takes `Arc<dyn ConversationStore>` work with thread-aware storage without
/// modification.
pub struct ThreadStoreAdapter {
    inner: Arc<dyn ThreadStore>,
}

impl ThreadStoreAdapter {
    /// Create a new adapter wrapping the given thread store.
    pub fn new(store: Arc<dyn ThreadStore>) -> Self {
        Self { inner: store }
    }
}

#[async_trait]
impl ConversationStore for ThreadStoreAdapter {
    async fn get_history(&self, chat_id: &str) -> Vec<ChatMessage> {
        match self.inner.active_thread(chat_id).await {
            Some(thread) => self.inner.thread_history(chat_id, &thread.id).await,
            None => vec![],
        }
    }

    async fn add_message(&self, chat_id: &str, message: ChatMessage) {
        self.inner.add_message(chat_id, message).await;
    }

    async fn clear(&self, chat_id: &str) {
        // Clear means we're done with the active thread — archive it
        if let Some(thread) = self.inner.active_thread(chat_id).await {
            let _ = self.inner.archive_thread(chat_id, &thread.id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;

    #[tokio::test]
    async fn get_history_returns_active_thread_history() {
        let store = Arc::new(MemoryThreadStore::new());
        let thread = store.create_thread("chat-1", "general").await;
        store
            .add_message_to_thread("chat-1", &thread.id, ChatMessage::user("Hello"))
            .await
            .unwrap();
        store
            .add_message_to_thread("chat-1", &thread.id, ChatMessage::assistant("Hi"))
            .await
            .unwrap();

        let adapter = ThreadStoreAdapter::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        let history = adapter.get_history("chat-1").await;

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "Hi");
    }

    #[tokio::test]
    async fn get_history_returns_empty_when_no_active_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let adapter = ThreadStoreAdapter::new(Arc::clone(&store) as Arc<dyn ThreadStore>);

        let history = adapter.get_history("nonexistent").await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn add_message_goes_to_active_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let thread = store.create_thread("chat-1", "general").await;

        let adapter = ThreadStoreAdapter::new(Arc::clone(&store) as Arc<dyn ThreadStore>);
        adapter
            .add_message("chat-1", ChatMessage::user("Test message"))
            .await;

        let history = store.thread_history("chat-1", &thread.id).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Test message");
    }

    #[tokio::test]
    async fn after_switch_get_history_returns_new_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        // Add messages to each thread
        store
            .add_message_to_thread("chat-1", &t1.id, ChatMessage::user("msg in t1"))
            .await
            .unwrap();
        store
            .add_message_to_thread("chat-1", &t2.id, ChatMessage::user("msg in t2"))
            .await
            .unwrap();

        let adapter = ThreadStoreAdapter::new(Arc::clone(&store) as Arc<dyn ThreadStore>);

        // t2 is active (last created)
        let history = adapter.get_history("chat-1").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "msg in t2");

        // Switch to t1
        store.switch_thread("chat-1", &t1.id).await.unwrap();

        let history = adapter.get_history("chat-1").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "msg in t1");
    }

    #[tokio::test]
    async fn clear_archives_active_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        let adapter = ThreadStoreAdapter::new(Arc::clone(&store) as Arc<dyn ThreadStore>);

        // t2 is active, clear it (archives it)
        adapter.clear("chat-1").await;

        // t2 should be archived, t1 should now be active
        let threads = store.list_threads("chat-1").await;
        let t2_found = threads.iter().find(|t| t.id == t2.id).unwrap();
        assert_eq!(
            t2_found.state,
            crate::threading::types::ThreadState::Archived
        );

        // Active should now be t1
        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t1.id);
    }
}
