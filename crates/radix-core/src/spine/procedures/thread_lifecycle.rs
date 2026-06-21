//! Thread lifecycle procedure — handles auto-archiving and cleanup.
//!
//! Listens for Timer events to periodically archive stale threads.
//! Also handles ThreadCreated/ThreadSwitched events for logging/metrics.

use std::sync::Arc;

use tracing::{debug, info};

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::threading::store::ThreadStore;
use crate::threading::types::ThreadConfig;

/// Manages thread lifecycle events — logging, metrics, and cleanup.
///
/// Handles:
/// - `timer` events named "thread_cleanup" → archives stale threads
/// - `thread_created` events → logs new thread creation
/// - `thread_switched` events → logs thread switches
pub struct ThreadLifecycle {
    /// Thread store — used by cleanup timer for archiving stale threads.
    #[allow(dead_code)]
    store: Arc<dyn ThreadStore>,
    config: ThreadConfig,
}

impl ThreadLifecycle {
    /// Create a new ThreadLifecycle with the given store and config.
    pub fn new(store: Arc<dyn ThreadStore>, config: ThreadConfig) -> Self {
        Self { store, config }
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ThreadLifecycle {
    fn name(&self) -> &str {
        "thread_lifecycle"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["timer", "thread_created", "thread_switched"])
    }

    async fn handle(&self, event: &SpineEvent, _emitter: &PipelineEmitter) {
        match event {
            SpineEvent::Timer { name, .. } if name == "thread_cleanup" => {
                debug!(
                    archive_after_secs = self.config.archive_after_secs,
                    "thread_lifecycle: cleanup timer fired"
                );
                // The actual cleanup logic would iterate known chats and archive stale threads.
                // For now this is the hook point — full implementation requires a chat registry
                // to enumerate all active chat_ids.
            }
            SpineEvent::ThreadCreated {
                chat_id,
                thread_id,
                topic,
                ..
            } => {
                info!(
                    chat_id = %chat_id,
                    thread_id = %thread_id,
                    topic = %topic,
                    "thread_lifecycle: new thread created"
                );
            }
            SpineEvent::ThreadSwitched {
                chat_id,
                from_thread_id,
                to_thread_id,
                ..
            } => {
                info!(
                    chat_id = %chat_id,
                    from = %from_thread_id,
                    to = %to_thread_id,
                    "thread_lifecycle: thread switched"
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;
    use crate::threading::types::ThreadConfig;
    use tokio::sync::mpsc;

    fn make_emitter() -> PipelineEmitter {
        let (tx, _rx) = mpsc::channel(16);
        PipelineEmitter { tx }
    }

    fn setup() -> (ThreadLifecycle, PipelineEmitter) {
        let store = Arc::new(MemoryThreadStore::new());
        let config = ThreadConfig::default();
        let lifecycle = ThreadLifecycle::new(Arc::clone(&store) as Arc<dyn ThreadStore>, config);
        let emitter = make_emitter();
        (lifecycle, emitter)
    }

    #[tokio::test]
    async fn timer_thread_cleanup_handled() {
        let (lifecycle, emitter) = setup();

        let event = SpineEvent::Timer {
            id: "t-1".into(),
            name: "thread_cleanup".into(),
        };

        // Should not panic
        lifecycle.handle(&event, &emitter).await;
    }

    #[tokio::test]
    async fn timer_other_name_ignored() {
        let (lifecycle, emitter) = setup();

        let event = SpineEvent::Timer {
            id: "t-2".into(),
            name: "task_eval".into(),
        };

        // Should not panic — other timer names are simply not matched
        lifecycle.handle(&event, &emitter).await;
    }

    #[tokio::test]
    async fn thread_created_handled() {
        let (lifecycle, emitter) = setup();

        let event = SpineEvent::ThreadCreated {
            id: "tc-1".into(),
            chat_id: "chat-1".into(),
            thread_id: "thread-42".into(),
            topic: "new feature".into(),
            channel_anchor: serde_json::json!({}),
        };

        // Should not panic — logs the event
        lifecycle.handle(&event, &emitter).await;
    }

    #[tokio::test]
    async fn thread_switched_handled() {
        let (lifecycle, emitter) = setup();

        let event = SpineEvent::ThreadSwitched {
            id: "ts-1".into(),
            chat_id: "chat-1".into(),
            from_thread_id: "thread-a".into(),
            to_thread_id: "thread-b".into(),
        };

        // Should not panic — logs the event
        lifecycle.handle(&event, &emitter).await;
    }

    #[tokio::test]
    async fn handles_returns_expected_types() {
        let (lifecycle, _) = setup();
        let handles = lifecycle.handles().unwrap();
        assert!(handles.contains(&"timer"));
        assert!(handles.contains(&"thread_created"));
        assert!(handles.contains(&"thread_switched"));
        assert!(!handles.contains(&"inbound"));
    }

    #[tokio::test]
    async fn name_returns_expected() {
        let (lifecycle, _) = setup();
        assert_eq!(lifecycle.name(), "thread_lifecycle");
    }
}
