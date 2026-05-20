//! Model invoker procedure — calls the LLM and emits ModelResponse.
//!
//! Integrates with the `ModelClient` trait for real model calls.
//! Builds conversation context from event metadata (tool results, history)
//! and passes available tool definitions so the model can make tool calls.

use std::sync::Arc;

use tracing::{debug, error, info};

use crate::model::{ChatMessage, ChatOptions, ModelClient, ToolDispatcher};
use crate::spine::conversation::ConversationStore;
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Invokes the language model for a ModelRequest and emits ModelResponse.
///
/// Holds references to the model client and tool dispatcher, building
/// conversation context from the event content and accumulated history.
pub struct ModelInvoker {
    model_client: Arc<dyn ModelClient>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    /// Default system prompt used when none is provided in the event.
    default_system_prompt: Option<String>,
    /// Optional conversation store for multi-turn history.
    conversation_store: Option<Arc<dyn ConversationStore>>,
}

impl ModelInvoker {
    /// Create a new ModelInvoker with the given model client and tool dispatcher.
    pub fn new(
        model_client: Arc<dyn ModelClient>,
        tool_dispatcher: Arc<dyn ToolDispatcher>,
    ) -> Self {
        Self {
            model_client,
            tool_dispatcher,
            default_system_prompt: None,
            conversation_store: None,
        }
    }

    /// Create a ModelInvoker with a custom default system prompt.
    pub fn with_system_prompt(
        model_client: Arc<dyn ModelClient>,
        tool_dispatcher: Arc<dyn ToolDispatcher>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            model_client,
            tool_dispatcher,
            default_system_prompt: Some(system_prompt.into()),
            conversation_store: None,
        }
    }

    /// Attach a conversation store for multi-turn history.
    pub fn with_conversation_store(mut self, store: Arc<dyn ConversationStore>) -> Self {
        self.conversation_store = Some(store);
        self
    }

    /// Build the message list for the model from the spine event.
    ///
    /// If the event metadata contains `conversation_history`, those messages
    /// are prepended to provide full context for multi-turn tool loops.
    /// Additionally, if `prior_history` is provided (from ConversationStore),
    /// it is included before the current turn's messages.
    fn build_messages(
        &self,
        content: &str,
        system_prompt: Option<&str>,
        metadata: &serde_json::Value,
        prior_history: &[ChatMessage],
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System prompt
        if let Some(sp) = system_prompt.or(self.default_system_prompt.as_deref()) {
            messages.push(ChatMessage::system(sp));
        }

        // Prior conversation history from ConversationStore (multi-turn context)
        if !prior_history.is_empty() {
            messages.extend(prior_history.iter().cloned());
        }

        // If this is a follow-up from tool_executor, include conversation history
        if let Some(history) = metadata.get("conversation_history").and_then(|h| h.as_array()) {
            for entry in history {
                let role = entry["role"].as_str().unwrap_or("user");
                let msg_content = entry["content"].as_str().unwrap_or("");
                let tool_call_id = entry["tool_call_id"].as_str();

                match role {
                    "assistant" => {
                        let mut msg = ChatMessage::assistant(msg_content);
                        // Restore structured tool_calls if present
                        if let Some(tcs) = entry.get("tool_calls").and_then(|v| v.as_array()) {
                            let tool_calls: Vec<crate::model::ToolCall> = tcs
                                .iter()
                                .filter_map(|tc| serde_json::from_value(tc.clone()).ok())
                                .collect();
                            if !tool_calls.is_empty() {
                                msg.tool_calls = Some(tool_calls);
                            }
                        }
                        messages.push(msg);
                    }
                    "tool" => {
                        if let Some(tc_id) = tool_call_id {
                            messages.push(ChatMessage::tool_result(tc_id, msg_content));
                        } else {
                            // Fallback: wrap as a user-visible tool result
                            messages.push(ChatMessage::tool_result("unknown", msg_content));
                        }
                    }
                    "system" => messages.push(ChatMessage::system(msg_content)),
                    _ => messages.push(ChatMessage::user(msg_content)),
                }
            }
        }

        // The current message content (from user or tool results summary)
        if !content.is_empty() {
            // If this is from the tool_executor (has "source": "tool_executor"),
            // the content is already tool results — add as user context
            let source = metadata
                .get("source")
                .and_then(|s| s.as_str())
                .unwrap_or("");
            if source == "tool_executor" {
                messages.push(ChatMessage::user(format!(
                    "Tool results:\n\n{}",
                    content
                )));
            } else {
                messages.push(ChatMessage::user(content));
            }
        }

        messages
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ModelInvoker {
    fn name(&self) -> &str {
        "model_invoker"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["model_request"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::ModelRequest {
            id,
            source,
            chat_id,
            content,
            system_prompt,
            metadata,
            ..
        } = event
        else {
            return;
        };

        debug!(event_id = %id, chat_id = %chat_id, "model_invoker: processing model request");

        // Fetch conversation history from store if available
        let prior_history = if let Some(store) = &self.conversation_store {
            store.get_history(chat_id).await
        } else {
            vec![]
        };

        // Build messages
        let messages =
            self.build_messages(content, system_prompt.as_deref(), metadata, &prior_history);

        if messages.is_empty() || (messages.len() == 1 && messages[0].role == "system") {
            error!(event_id = %id, "model_invoker: no user content to send to model");
            return;
        }

        // Get available tools
        let tool_defs = self.tool_dispatcher.available_tools().await;

        // Call the model
        let options = ChatOptions::default();
        let result = self.model_client.complete(&messages, &tool_defs, &options).await;

        match result {
            Ok(completion) => {
                let response_content = completion.content.unwrap_or_default();
                let tool_calls = completion.tool_calls;

                info!(
                    event_id = %id,
                    chat_id = %chat_id,
                    content_len = response_content.len(),
                    tool_call_count = tool_calls.len(),
                    "model_invoker: model responded"
                );

                emitter
                    .emit(SpineEvent::ModelResponse {
                        id: SpineEvent::new_id(),
                        source: source.clone(),
                        chat_id: chat_id.clone(),
                        content: response_content,
                        model: completion.model.unwrap_or_else(|| "unknown".into()),
                        tool_calls,
                        metadata: serde_json::json!({}),
                    })
                    .await;
            }
            Err(e) => {
                error!(
                    event_id = %id,
                    chat_id = %chat_id,
                    error = %e,
                    "model_invoker: model call failed"
                );

                // Emit a delivery request with the error
                emitter
                    .emit(SpineEvent::DeliveryRequest {
                        id: SpineEvent::new_id(),
                        channel: source.clone(),
                        chat_id: chat_id.clone(),
                        content: format!("⚠️ Model error: {}", e),
                        metadata: serde_json::json!({
                            "source": "model_invoker",
                            "error": e.to_string(),
                        }),
                    })
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelCompletion, ToolCall, ToolDefinition};
    use async_trait::async_trait;
    use serde_json::json;
    use tokio::sync::mpsc;

    // ── Mock ModelClient ──────────────────────────────────────────────────────

    /// A mock that returns a simple text response.
    struct TextModelClient {
        response: String,
    }

    impl TextModelClient {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
            }
        }
    }

    #[async_trait]
    impl ModelClient for TextModelClient {
        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<ModelCompletion, String> {
            Ok(ModelCompletion {
                content: Some(self.response.clone()),
                tool_calls: vec![],
                logprobs: None,
                model: Some("gpt-4o-test".into()),
            })
        }
    }

    /// A mock that returns tool calls.
    struct ToolCallingModelClient;

    #[async_trait]
    impl ModelClient for ToolCallingModelClient {
        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<ModelCompletion, String> {
            Ok(ModelCompletion {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "call-123".into(),
                    name: "web_search".into(),
                    arguments: json!({"query": "rust programming"}),
                }],
                logprobs: None,
                model: Some("claude-sonnet-4-20250514".into()),
            })
        }
    }

    /// A mock that always errors.
    struct FailingModelClient;

    #[async_trait]
    impl ModelClient for FailingModelClient {
        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<ModelCompletion, String> {
            Err("connection timeout".into())
        }
    }

    /// A mock that captures the messages it receives.
    struct CapturingModelClient {
        captured: tokio::sync::Mutex<Vec<Vec<ChatMessage>>>,
    }

    impl CapturingModelClient {
        fn new() -> Self {
            Self {
                captured: tokio::sync::Mutex::new(Vec::new()),
            }
        }

        async fn last_messages(&self) -> Vec<ChatMessage> {
            let locked = self.captured.lock().await;
            locked.last().cloned().unwrap_or_default()
        }
    }

    #[async_trait]
    impl ModelClient for CapturingModelClient {
        async fn complete(
            &self,
            messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<ModelCompletion, String> {
            self.captured.lock().await.push(messages.to_vec());
            Ok(ModelCompletion {
                content: Some("captured".into()),
                tool_calls: vec![],
                logprobs: None,
                model: None,
            })
        }
    }

    // ── Mock ToolDispatcher ───────────────────────────────────────────────────

    struct MockTools;

    #[async_trait]
    impl ToolDispatcher for MockTools {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "web_search".into(),
                description: "Search the web".into(),
                parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}}),
            }]
        }

        async fn call_tool(&self, _name: &str, _arguments: serde_json::Value) -> String {
            "mock result".into()
        }
    }

    struct EmptyTools;

    #[async_trait]
    impl ToolDispatcher for EmptyTools {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        async fn call_tool(&self, _name: &str, _arguments: serde_json::Value) -> String {
            String::new()
        }
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    fn make_emitter() -> (PipelineEmitter, mpsc::Receiver<SpineEvent>) {
        let (tx, rx) = mpsc::channel(64);
        (PipelineEmitter { tx }, rx)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn emits_model_response_with_text() {
        let (emitter, mut rx) = make_emitter();
        let invoker = ModelInvoker::new(
            Arc::new(TextModelClient::new("Hello, world!")),
            Arc::new(MockTools),
        );

        let event = SpineEvent::ModelRequest {
            source: "test".into(),
            id: "req-1".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "Hi there".into(),
            system_prompt: None,
            metadata: json!({}),
        };

        invoker.handle(&event, &emitter).await;

        let response = rx.recv().await.unwrap();
        assert_eq!(response.event_type(), "model_response");
        if let SpineEvent::ModelResponse {
            content,
            tool_calls,
            chat_id,
            model,
            ..
        } = response
        {
            assert_eq!(content, "Hello, world!");
            assert!(tool_calls.is_empty());
            assert_eq!(chat_id, "chat-1");
            assert_eq!(model, "gpt-4o-test");
        } else {
            panic!("expected ModelResponse");
        }
    }

    #[tokio::test]
    async fn emits_model_response_with_tool_calls() {
        let (emitter, mut rx) = make_emitter();
        let invoker = ModelInvoker::new(
            Arc::new(ToolCallingModelClient),
            Arc::new(MockTools),
        );

        let event = SpineEvent::ModelRequest {
            source: "test".into(),
            id: "req-2".into(),
            chat_id: "chat-2".into(),
            sender: "user".into(),
            content: "Search for rust".into(),
            system_prompt: None,
            metadata: json!({}),
        };

        invoker.handle(&event, &emitter).await;

        let response = rx.recv().await.unwrap();
        if let SpineEvent::ModelResponse {
            content,
            tool_calls,
            ..
        } = response
        {
            assert!(content.is_empty());
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].name, "web_search");
            assert_eq!(tool_calls[0].id, "call-123");
        } else {
            panic!("expected ModelResponse");
        }
    }

    #[tokio::test]
    async fn emits_delivery_request_on_error() {
        let (emitter, mut rx) = make_emitter();
        let invoker = ModelInvoker::new(
            Arc::new(FailingModelClient),
            Arc::new(MockTools),
        );

        let event = SpineEvent::ModelRequest {
            source: "test".into(),
            id: "req-3".into(),
            chat_id: "chat-3".into(),
            sender: "user".into(),
            content: "Hello".into(),
            system_prompt: None,
            metadata: json!({}),
        };

        invoker.handle(&event, &emitter).await;

        let response = rx.recv().await.unwrap();
        assert_eq!(response.event_type(), "delivery_request");
        if let SpineEvent::DeliveryRequest { content, .. } = response {
            assert!(content.contains("Model error"));
            assert!(content.contains("connection timeout"));
        } else {
            panic!("expected DeliveryRequest");
        }
    }

    #[tokio::test]
    async fn includes_system_prompt() {
        let (emitter, _rx) = make_emitter();
        let client = Arc::new(CapturingModelClient::new());
        let invoker = ModelInvoker::with_system_prompt(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::new(EmptyTools),
            "You are a helpful assistant.",
        );

        let event = SpineEvent::ModelRequest {
            source: "test".into(),
            id: "req-4".into(),
            chat_id: "chat-4".into(),
            sender: "user".into(),
            content: "Hello".into(),
            system_prompt: None,
            metadata: json!({}),
        };

        invoker.handle(&event, &emitter).await;

        let msgs = client.last_messages().await;
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, "You are a helpful assistant.");
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "Hello");
    }

    #[tokio::test]
    async fn event_system_prompt_overrides_default() {
        let (emitter, _rx) = make_emitter();
        let client = Arc::new(CapturingModelClient::new());
        let invoker = ModelInvoker::with_system_prompt(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::new(EmptyTools),
            "Default prompt",
        );

        let event = SpineEvent::ModelRequest {
            source: "test".into(),
            id: "req-5".into(),
            chat_id: "chat-5".into(),
            sender: "user".into(),
            content: "Hi".into(),
            system_prompt: Some("Override prompt".into()),
            metadata: json!({}),
        };

        invoker.handle(&event, &emitter).await;

        let msgs = client.last_messages().await;
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, "Override prompt");
    }

    #[tokio::test]
    async fn builds_messages_from_conversation_history() {
        let (emitter, _rx) = make_emitter();
        let client = Arc::new(CapturingModelClient::new());
        let invoker = ModelInvoker::new(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::new(EmptyTools),
        );

        let event = SpineEvent::ModelRequest {
            source: "test".into(),
            id: "req-6".into(),
            chat_id: "chat-6".into(),
            sender: "system".into(),
            content: "[tool:web_search] Results for: rust".into(),
            system_prompt: None,
            metadata: json!({
                "source": "tool_executor",
                "conversation_history": [
                    {"role": "assistant", "content": "Let me search"},
                    {"role": "tool", "content": "Results for: rust", "tool_call_id": "tc-1", "tool_name": "web_search"}
                ]
            }),
        };

        invoker.handle(&event, &emitter).await;

        let msgs = client.last_messages().await;
        // Should have: assistant (from history) + tool (from history) + user (tool results summary)
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "assistant");
        assert_eq!(msgs[0].content, "Let me search");
        assert_eq!(msgs[1].role, "tool");
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("tc-1"));
        assert_eq!(msgs[2].role, "user");
        assert!(msgs[2].content.contains("Tool results:"));
    }

    #[tokio::test]
    async fn ignores_non_model_request_events() {
        let (emitter, mut rx) = make_emitter();
        let invoker = ModelInvoker::new(
            Arc::new(TextModelClient::new("should not appear")),
            Arc::new(MockTools),
        );

        let event = SpineEvent::Inbound {
            id: "in-1".into(),
            source: "test".into(),
            chat_id: "chat-7".into(),
            sender: "user".into(),
            content: "hello".into(),
            metadata: json!({}),
        };

        invoker.handle(&event, &emitter).await;

        // No events should be emitted
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "should timeout — no events emitted");
    }
}
