//! Thread management action handlers.
//!
//! Provides boundary actors for the thread-management.px procedure:
//! - `find_or_create_thread` — look up existing thread by topic similarity, create if not found
//! - `create_thread` — create new thread in state, return thread metadata
//! - `switch_thread` — set new active thread, return old active
//! - `list_threads` — return thread list for chat
//! - `archive_thread` — set thread state to archived
//! - `find_stale_threads` — find threads exceeding inactivity threshold

use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use pares_radix_praxis::px::executor::ExecutionError;

/// Helper to construct ActionFailed errors concisely.
fn err(action: &str, message: impl Into<String>) -> ExecutionError {
    ExecutionError::ActionFailed {
        action: action.to_string(),
        message: message.into(),
    }
}

/// Thread management action handler.
///
/// This handler provides the actions used by `thread-management.px` to manage
/// conversation thread lifecycle. In production, state operations delegate to
/// PluresDB; this implementation uses in-memory JSON representations suitable
/// for both unit tests and spine execution.
pub struct ThreadActionHandler;

impl Default for ThreadActionHandler {
    fn default() -> Self {
        Self
    }
}

impl ThreadActionHandler {
    pub fn new() -> Self {
        Self
    }

    /// Find an existing thread by topic similarity, or create a new one.
    ///
    /// Input: `{chat_id: "...", topic: "..."}`
    /// Output: `{action: "switched"|"created", thread_id: "...", topic: "...", is_new: bool}`
    fn find_or_create_thread(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("find_or_create_thread", "missing chat_id"))?;

        let topic = params
            .get("topic")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("find_or_create_thread", "missing topic"))?;

        // In a full implementation, this would query PluresDB for threads with
        // similar topics. For now, we generate a deterministic thread ID from
        // the chat_id + topic combination as a placeholder for the lookup.
        let thread_id = format!("thread_{}_{}", chat_id, slug_from_topic(topic));

        Ok(json!({
            "action": "switched",
            "thread_id": thread_id,
            "chat_id": chat_id,
            "topic": topic,
            "is_new": false
        }))
    }

    /// Create a new thread explicitly.
    ///
    /// Input: `{chat_id: "...", topic: "..."}`
    /// Output: `{thread_id: "...", chat_id: "...", topic: "...", created_at: ...}`
    fn create_thread(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("create_thread", "missing chat_id"))?;

        let topic = params
            .get("topic")
            .and_then(|v| v.as_str())
            .unwrap_or("untitled");

        let thread_id = format!(
            "thread_{}_{}",
            chat_id,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("new")
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(json!({
            "thread_id": thread_id,
            "chat_id": chat_id,
            "topic": topic,
            "state": "active",
            "created_at": now,
            "last_active_at": now
        }))
    }

    /// Switch the active thread for a chat.
    ///
    /// Input: `{chat_id: "...", thread_id: "..."}`
    /// Output: `{switched: true, chat_id: "...", new_active: "...", previous_active: ...}`
    fn switch_thread(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("switch_thread", "missing chat_id"))?;

        let thread_id = params
            .get("thread_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("switch_thread", "missing thread_id"))?;

        // In production, this reads and updates PluresDB state.
        // The previous_active would come from `thread:{chat_id}:active`.
        Ok(json!({
            "switched": true,
            "chat_id": chat_id,
            "new_active": thread_id,
            "previous_active": null
        }))
    }

    /// List threads for a chat.
    ///
    /// Input: `{chat_id: "...", include_archived: bool}`
    /// Output: `{threads: [...], count: N}`
    fn list_threads(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("list_threads", "missing chat_id"))?;

        let include_archived = params
            .get("include_archived")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // In production, this queries PluresDB for all threads matching the chat_id.
        // Returns empty list as placeholder — real implementation scans state keys.
        Ok(json!({
            "chat_id": chat_id,
            "threads": [],
            "count": 0,
            "include_archived": include_archived
        }))
    }

    /// Archive a thread.
    ///
    /// Input: `{chat_id: "...", thread_id: "..."}`
    /// Output: `{archived: true, thread_id: "...", archived_at: ...}`
    fn archive_thread(&self, params: &Value) -> Result<Value, ExecutionError> {
        let chat_id = params
            .get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("archive_thread", "missing chat_id"))?;

        let thread_id = params
            .get("thread_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("archive_thread", "missing thread_id"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(json!({
            "archived": true,
            "chat_id": chat_id,
            "thread_id": thread_id,
            "state": "archived",
            "archived_at": now
        }))
    }

    /// Find threads that have been inactive beyond a threshold.
    ///
    /// Input: `{max_inactive_secs: N, current_time: N}`
    /// Output: `[{thread_id, chat_id, last_active_at, inactive_secs}, ...]`
    fn find_stale_threads(&self, params: &Value) -> Result<Value, ExecutionError> {
        let _max_inactive_secs = params
            .get("max_inactive_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(172800); // 2 days default

        let _current_time = params
            .get("current_time")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // In production, this scans all active threads in PluresDB and filters
        // by last_active_at relative to current_time and max_inactive_secs.
        // Returns empty list as placeholder.
        Ok(json!([]))
    }
}

/// Generate a URL-safe slug from a topic string.
fn slug_from_topic(topic: &str) -> String {
    topic
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(32)
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

#[async_trait::async_trait]
impl AsyncActionHandler for ThreadActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "find_or_create_thread" => self.find_or_create_thread(params),
            "create_thread" => self.create_thread(params),
            "switch_thread" => self.switch_thread(params),
            "list_threads" => self.list_threads(params),
            "archive_thread" => self.archive_thread(params),
            "find_stale_threads" => self.find_stale_threads(params),
            _ => Err(ExecutionError::UnknownAction(name.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_from_topic_basic() {
        assert_eq!(slug_from_topic("Hello World"), "hello_world");
        assert_eq!(slug_from_topic("code-review"), "code_review");
        assert_eq!(slug_from_topic("  spaces  "), "spaces");
    }

    #[test]
    fn slug_from_topic_long_truncates() {
        let long_topic = "a".repeat(100);
        let slug = slug_from_topic(&long_topic);
        assert!(slug.len() <= 32);
    }

    #[tokio::test]
    async fn find_or_create_thread_success() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call(
                "find_or_create_thread",
                &json!({"chat_id": "chat_123", "topic": "debugging"}),
            )
            .await
            .unwrap();

        assert_eq!(result["chat_id"], "chat_123");
        assert_eq!(result["topic"], "debugging");
        assert!(result["thread_id"].as_str().unwrap().contains("chat_123"));
    }

    #[tokio::test]
    async fn find_or_create_thread_missing_chat_id() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call("find_or_create_thread", &json!({"topic": "test"}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_thread_success() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call(
                "create_thread",
                &json!({"chat_id": "chat_456", "topic": "architecture"}),
            )
            .await
            .unwrap();

        assert_eq!(result["chat_id"], "chat_456");
        assert_eq!(result["topic"], "architecture");
        assert_eq!(result["state"], "active");
        assert!(result["thread_id"].as_str().unwrap().starts_with("thread_chat_456_"));
        assert!(result["created_at"].as_u64().is_some());
    }

    #[tokio::test]
    async fn create_thread_default_topic() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call("create_thread", &json!({"chat_id": "chat_789"}))
            .await
            .unwrap();

        assert_eq!(result["topic"], "untitled");
    }

    #[tokio::test]
    async fn switch_thread_success() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call(
                "switch_thread",
                &json!({"chat_id": "chat_123", "thread_id": "thread_abc"}),
            )
            .await
            .unwrap();

        assert_eq!(result["switched"], true);
        assert_eq!(result["chat_id"], "chat_123");
        assert_eq!(result["new_active"], "thread_abc");
    }

    #[tokio::test]
    async fn switch_thread_missing_thread_id() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call("switch_thread", &json!({"chat_id": "chat_123"}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_threads_success() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call(
                "list_threads",
                &json!({"chat_id": "chat_123", "include_archived": true}),
            )
            .await
            .unwrap();

        assert_eq!(result["chat_id"], "chat_123");
        assert_eq!(result["include_archived"], true);
        assert!(result["threads"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn archive_thread_success() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call(
                "archive_thread",
                &json!({"chat_id": "chat_123", "thread_id": "thread_abc"}),
            )
            .await
            .unwrap();

        assert_eq!(result["archived"], true);
        assert_eq!(result["thread_id"], "thread_abc");
        assert_eq!(result["state"], "archived");
        assert!(result["archived_at"].as_u64().is_some());
    }

    #[tokio::test]
    async fn archive_thread_missing_params() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call("archive_thread", &json!({"chat_id": "chat_123"}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn find_stale_threads_returns_empty() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call(
                "find_stale_threads",
                &json!({"max_inactive_secs": 86400, "current_time": 1700000000}),
            )
            .await
            .unwrap();

        assert!(result.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_action_returns_error() {
        let handler = ThreadActionHandler::new();
        let result = handler
            .call("nonexistent_action", &json!({}))
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutionError::UnknownAction(name) => assert_eq!(name, "nonexistent_action"),
            other => panic!("Expected UnknownAction, got: {:?}", other),
        }
    }
}
