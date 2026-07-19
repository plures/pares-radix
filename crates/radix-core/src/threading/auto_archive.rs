//! Auto-archive manager — periodically archives threads that exceed
//! the configured inactivity threshold.
//!
//! Designed to be called periodically (e.g., on a Timer event) to clean up
//! stale threads. Respects the following invariants:
//!
//! - Never archives the last active thread in a chat (always keep at least one)
//! - Enforces `max_active` cap by archiving the oldest threads first\n//! - Already-archived threads are never processed again

use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, info};

use super::store::ThreadStore;
use super::types::{Thread, ThreadConfig, ThreadState};

/// Auto-archives stale threads based on inactivity and cap configuration.
pub struct AutoArchiver {
    store: Arc<dyn ThreadStore>,
    config: ThreadConfig,
}

impl AutoArchiver {
    /// Create a new auto-archiver with the given store and config.
    pub fn new(store: Arc<dyn ThreadStore>, config: ThreadConfig) -> Self {
        Self { store, config }
    }

    /// Run one cleanup cycle for a specific chat. Returns the list of archived thread IDs.
    pub async fn cleanup_chat(&self, chat_id: &str) -> Vec<String> {
        let now = Utc::now();
        let threads = self.store.list_threads(chat_id).await;
        let mut archived = Vec::new();

        // Count currently active threads (before any archiving)
        let active_threads: Vec<&Thread> = threads
            .iter()
            .filter(|t| t.state == ThreadState::Active)
            .collect();

        let mut remaining_active_count = active_threads.len();

        // Phase 1: Archive threads that exceeded the inactivity threshold
        for thread in &active_threads {
            // Never archive the last active thread
            if remaining_active_count <= 1 {
                debug!(
                    chat_id = %chat_id,
                    thread_id = %thread.id,
                    "auto_archive: skipping last active thread"
                );
                break;
            }

            let inactive_secs = now
                .signed_duration_since(thread.last_active_at)
                .num_seconds()
                .max(0) as u64;

            if inactive_secs >= self.config.archive_after_secs {
                info!(
                    chat_id = %chat_id,
                    thread_id = %thread.id,
                    topic = %thread.topic,
                    inactive_secs = inactive_secs,
                    "auto_archive: archiving stale thread"
                );

                if self.store.archive_thread(chat_id, &thread.id).await.is_ok() {
                    archived.push(thread.id.clone());
                    remaining_active_count -= 1;
                }
            }
        }

        // Phase 2: Enforce max_active cap — archive oldest if over limit
        let remaining_active: Vec<&Thread> = active_threads
            .iter()
            .filter(|t| !archived.contains(&t.id))
            .copied()
            .collect();

        if remaining_active.len() > self.config.max_active {
            let mut by_activity: Vec<&&Thread> = remaining_active.iter().collect();
            by_activity.sort_by_key(|t| t.last_active_at);

            let excess = remaining_active.len() - self.config.max_active;
            for thread in by_activity.into_iter().take(excess) {
                // Still respect the "keep at least one" invariant
                if remaining_active_count <= 1 {
                    break;
                }

                info!(
                    chat_id = %chat_id,
                    thread_id = %thread.id,
                    topic = %thread.topic,
                    "auto_archive: archiving excess thread (over max_active cap)"
                );

                if self.store.archive_thread(chat_id, &thread.id).await.is_ok() {
                    archived.push(thread.id.clone());
                    remaining_active_count -= 1;
                }
            }
        }

        archived
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;
    use chrono::Duration;

    fn make_archiver(
        store: Arc<MemoryThreadStore>,
        config: ThreadConfig,
    ) -> AutoArchiver {
        AutoArchiver::new(store as Arc<dyn ThreadStore>, config)
    }

    #[tokio::test]
    async fn thread_inactive_beyond_threshold_is_archived() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "old-topic").await;
        // Create a second thread so the first isn't the only active one
        let _t2 = store.create_thread("chat-1", "new-topic").await;

        // Manually make t1 appear old by modifying its last_active_at
        {
            let mut threads = store.threads.write().await;
            let key = MemoryThreadStore::composite_key("chat-1", &t1.id);
            if let Some(thread) = threads.get_mut(&key) {
                thread.last_active_at = Utc::now() - Duration::hours(72);
            }
        }

        let config = ThreadConfig {
            archive_after_secs: 48 * 3600, // 48 hours
            ..Default::default()
        };
        let archiver = make_archiver(store.clone(), config);

        let archived = archiver.cleanup_chat("chat-1").await;
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0], t1.id);

        // Verify the thread is actually archived
        let threads = store.list_threads("chat-1").await;
        let t1_found = threads.iter().find(|t| t.id == t1.id).unwrap();
        assert_eq!(t1_found.state, ThreadState::Archived);
    }

    #[tokio::test]
    async fn thread_within_threshold_not_archived() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "recent-topic").await;
        store.create_thread("chat-1", "another-topic").await;

        let config = ThreadConfig {
            archive_after_secs: 48 * 3600,
            ..Default::default()
        };
        let archiver = make_archiver(store.clone(), config);

        let archived = archiver.cleanup_chat("chat-1").await;
        assert!(archived.is_empty(), "fresh threads should not be archived");
    }

    #[tokio::test]
    async fn last_active_thread_never_archived() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "only-topic").await;

        // Make it appear very old
        {
            let mut threads = store.threads.write().await;
            let key = MemoryThreadStore::composite_key("chat-1", &t1.id);
            if let Some(thread) = threads.get_mut(&key) {
                thread.last_active_at = Utc::now() - Duration::hours(200);
            }
        }

        let config = ThreadConfig {
            archive_after_secs: 3600, // 1 hour — way less than 200 hours
            ..Default::default()
        };
        let archiver = make_archiver(store.clone(), config);

        let archived = archiver.cleanup_chat("chat-1").await;
        assert!(
            archived.is_empty(),
            "last active thread must never be archived even if stale"
        );

        // Verify it's still active
        let threads = store.list_threads("chat-1").await;
        let t1_found = threads.iter().find(|t| t.id == t1.id).unwrap();
        assert_eq!(t1_found.state, ThreadState::Active);
    }

    #[tokio::test]
    async fn max_active_cap_enforced_oldest_first() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;
        let _t3 = store.create_thread("chat-1", "topic-c").await;

        // Make t1 the oldest, t2 second oldest, t3 most recent
        {
            let mut threads = store.threads.write().await;
            let key1 = MemoryThreadStore::composite_key("chat-1", &t1.id);
            let key2 = MemoryThreadStore::composite_key("chat-1", &t2.id);
            if let Some(thread) = threads.get_mut(&key1) {
                thread.last_active_at = Utc::now() - Duration::hours(3);
            }
            if let Some(thread) = threads.get_mut(&key2) {
                thread.last_active_at = Utc::now() - Duration::hours(2);
            }
        }

        let config = ThreadConfig {
            max_active: 2,
            archive_after_secs: 999999, // Very high — won't trigger time-based archiving
            ..Default::default()
        };
        let archiver = make_archiver(store.clone(), config);

        let archived = archiver.cleanup_chat("chat-1").await;
        // Should archive 1 thread (3 active, max is 2)
        assert_eq!(archived.len(), 1);
        // Should archive the oldest (t1)
        assert_eq!(archived[0], t1.id);
    }

    #[tokio::test]
    async fn already_archived_threads_not_double_archived() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let _t2 = store.create_thread("chat-1", "topic-b").await;

        // Archive t1 manually
        store.archive_thread("chat-1", &t1.id).await.unwrap();

        let config = ThreadConfig {
            archive_after_secs: 1, // Very low threshold
            ..Default::default()
        };
        let archiver = make_archiver(store.clone(), config);

        // Even with a very low threshold, t1 shouldn't appear in results
        // because it's already archived (filtered by state == Active)
        let archived = archiver.cleanup_chat("chat-1").await;

        // t1 should NOT be in the archived list (already archived)
        assert!(
            !archived.contains(&t1.id),
            "already-archived threads should not be re-processed"
        );
    }

    #[tokio::test]
    async fn empty_chat_returns_empty() {
        let store = Arc::new(MemoryThreadStore::new());
        let config = ThreadConfig::default();
        let archiver = make_archiver(store, config);

        let archived = archiver.cleanup_chat("nonexistent-chat").await;
        assert!(archived.is_empty());
    }

    #[tokio::test]
    async fn multiple_stale_threads_archived_preserving_one() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;
        let t3 = store.create_thread("chat-1", "topic-c").await;

        // Make all of them stale
        {
            let mut threads = store.threads.write().await;
            for id in [&t1.id, &t2.id, &t3.id] {
                let key = MemoryThreadStore::composite_key("chat-1", id);
                if let Some(thread) = threads.get_mut(&key) {
                    thread.last_active_at = Utc::now() - Duration::hours(100);
                }
            }
        }

        let config = ThreadConfig {
            archive_after_secs: 3600, // 1 hour
            ..Default::default()
        };
        let archiver = make_archiver(store.clone(), config);

        let archived = archiver.cleanup_chat("chat-1").await;
        // Should archive 2 of the 3 (keep at least 1)
        assert_eq!(archived.len(), 2);

        // Verify at least one thread remains active
        let remaining = store.list_threads("chat-1").await;
        let active_count = remaining.iter().filter(|t| t.state == ThreadState::Active).count();
        assert_eq!(active_count, 1, "must keep at least one active thread");
    }
}
