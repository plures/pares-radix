//! Channel threading trait — abstracts platform-specific thread presentation.

use serde_json::Value;

use super::types::Thread;

/// Type alias for channel-specific thread anchoring metadata.
pub type ChannelAnchor = Value;

/// Capabilities of a channel's threading support.
#[derive(Debug, Clone, Default)]
pub struct ThreadCapabilities {
    /// Channel supports visual thread indicators (e.g., colored bars, labels).
    pub indicators: bool,
    /// Channel supports reply chains (reply-to-message).
    pub reply_chains: bool,
    /// Channel has native thread support (e.g., Slack threads, Discord threads).
    pub native_threads: bool,
    /// Channel can present an interactive thread switcher UI.
    pub thread_switcher: bool,
    /// Channel can display multiple threads concurrently.
    pub concurrent_display: bool,
}

/// Errors from channel threading operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadError {
    /// The requested thread was not found.
    NotFound(String),
    /// A storage-level error occurred.
    StorageError(String),
    /// A channel-level error occurred (e.g., API failure).
    ChannelError(String),
}

impl std::fmt::Display for ThreadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "thread not found: {id}"),
            Self::StorageError(msg) => write!(f, "storage error: {msg}"),
            Self::ChannelError(msg) => write!(f, "channel error: {msg}"),
        }
    }
}

impl std::error::Error for ThreadError {}

/// Trait for channel-specific thread presentation and interaction.
///
/// Each channel adapter implements this to provide its native threading UX.
/// For example, Telegram uses reply-to chains and inline keyboards,
/// while a CLI adapter might use colored prefixes.
#[async_trait::async_trait]
pub trait ChannelThreading: Send + Sync {
    /// Report what threading capabilities this channel supports.
    fn capabilities(&self) -> ThreadCapabilities;

    /// Called when a new thread is created — the channel may need to
    /// send a notification or create a visual anchor.
    async fn on_thread_created(
        &self,
        thread: &Thread,
    ) -> Result<Option<ChannelAnchor>, ThreadError>;

    /// Called when the active thread switches — update visual indicators.
    async fn on_thread_switched(
        &self,
        from: Option<&Thread>,
        to: &Thread,
    ) -> Result<(), ThreadError>;

    /// Deliver a message within the context of a specific thread.
    /// The channel may use reply-to, thread indicators, or native threading.
    async fn deliver_in_thread(
        &self,
        thread: &Thread,
        content: &str,
    ) -> Result<Option<ChannelAnchor>, ThreadError>;

    /// Given an inbound message's metadata, resolve which thread it belongs to.
    /// Returns None if the message doesn't have thread-routing metadata.
    async fn resolve_thread_from_message(
        &self,
        message_metadata: &Value,
    ) -> Result<Option<String>, ThreadError>;

    /// Present the thread list to the user (e.g., inline keyboard, menu).
    async fn present_thread_list(&self, threads: &[Thread]) -> Result<(), ThreadError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal test implementation of ChannelThreading.
    struct MockChannel;

    #[async_trait::async_trait]
    impl ChannelThreading for MockChannel {
        fn capabilities(&self) -> ThreadCapabilities {
            ThreadCapabilities {
                indicators: true,
                reply_chains: true,
                native_threads: false,
                thread_switcher: true,
                concurrent_display: false,
            }
        }

        async fn on_thread_created(
            &self,
            _thread: &Thread,
        ) -> Result<Option<ChannelAnchor>, ThreadError> {
            Ok(Some(serde_json::json!({"message_id": 42})))
        }

        async fn on_thread_switched(
            &self,
            _from: Option<&Thread>,
            _to: &Thread,
        ) -> Result<(), ThreadError> {
            Ok(())
        }

        async fn deliver_in_thread(
            &self,
            _thread: &Thread,
            _content: &str,
        ) -> Result<Option<ChannelAnchor>, ThreadError> {
            Ok(Some(serde_json::json!({"message_id": 43})))
        }

        async fn resolve_thread_from_message(
            &self,
            metadata: &Value,
        ) -> Result<Option<String>, ThreadError> {
            Ok(metadata
                .get("thread_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()))
        }

        async fn present_thread_list(&self, _threads: &[Thread]) -> Result<(), ThreadError> {
            Ok(())
        }
    }

    #[test]
    fn capabilities_reported_correctly() {
        let channel = MockChannel;
        let caps = channel.capabilities();
        assert!(caps.indicators);
        assert!(caps.reply_chains);
        assert!(!caps.native_threads);
        assert!(caps.thread_switcher);
        assert!(!caps.concurrent_display);
    }

    #[tokio::test]
    async fn on_thread_created_returns_anchor() {
        let channel = MockChannel;
        let thread = Thread::new("t1", "chat-1", "test");
        let anchor = channel.on_thread_created(&thread).await.unwrap();
        assert!(anchor.is_some());
        assert_eq!(anchor.unwrap()["message_id"], 42);
    }

    #[tokio::test]
    async fn resolve_thread_from_metadata() {
        let channel = MockChannel;
        let meta = serde_json::json!({"thread_id": "t1"});
        let resolved = channel.resolve_thread_from_message(&meta).await.unwrap();
        assert_eq!(resolved, Some("t1".to_string()));

        let empty_meta = serde_json::json!({});
        let resolved = channel
            .resolve_thread_from_message(&empty_meta)
            .await
            .unwrap();
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn deliver_in_thread_returns_anchor() {
        let channel = MockChannel;
        let thread = Thread::new("t1", "chat-1", "test");
        let anchor = channel.deliver_in_thread(&thread, "hello").await.unwrap();
        assert!(anchor.is_some());
    }

    #[test]
    fn thread_error_display() {
        let e = ThreadError::NotFound("t1".into());
        assert_eq!(e.to_string(), "thread not found: t1");

        let e = ThreadError::ChannelError("timeout".into());
        assert_eq!(e.to_string(), "channel error: timeout");
    }

    #[test]
    fn default_capabilities_all_false() {
        let caps = ThreadCapabilities::default();
        assert!(!caps.indicators);
        assert!(!caps.reply_chains);
        assert!(!caps.native_threads);
        assert!(!caps.thread_switcher);
        assert!(!caps.concurrent_display);
    }
}
