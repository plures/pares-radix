//! Model invoker procedure — calls the LLM and emits ModelResponse.
//!
//! Integrates with the `ModelClient` trait for real model calls.
//! Builds conversation context from event metadata (tool results, history)
//! and passes available tool definitions so the model can make tool calls.
//!
//! Streaming: When a `stream_tx` sender is configured, uses `complete_stream()`
//! to emit `StreamDelta` tokens in real-time. Channel handlers (Telegram, etc.)
//! subscribe to this sender for progressive message editing.

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{debug, error, info};

use crate::model::{ChatMessage, ChatOptions, ModelClient, StreamDelta, ToolDispatcher};
use crate::spine::conversation::ConversationStore;
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::task_manager::TaskManager;

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
    /// Broadcast sender for streaming deltas to channel handlers.
    /// When set, uses `complete_stream()` for real-time token delivery.
    stream_tx: Option<broadcast::Sender<StreamDelta>>,
    /// Optional durable task manager. When set, the open task list is injected
    /// into the model context each turn so the agent always sees its persisted
    /// obligations (fixes conversational task/commitment amnesia — the tasks
    /// live in Sled but were never surfaced into the prompt).
    task_manager: Option<Arc<TaskManager>>,
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
            stream_tx: None,
            task_manager: None,
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
            stream_tx: None,
            task_manager: None,
        }
    }

    /// Attach the durable [`TaskManager`] so persisted open tasks are injected
    /// into the model context each turn.
    pub fn with_task_manager(mut self, task_manager: Arc<TaskManager>) -> Self {
        self.task_manager = Some(task_manager);
        self
    }

    /// Render a compact grounding block of the agent's persisted open tasks for
    /// the given chat. Returns `None` when there are no open tasks so we never
    /// inject an empty/noise block.
    ///
    /// Combines chat-scoped open tasks with globally open tasks (deduped by id)
    /// so obligations survive conversation-history trimming and process restarts.
    fn render_open_tasks_block(&self, chat_id: &str) -> Option<String> {
        let manager = self.task_manager.as_ref()?;

        // Chat-scoped open tasks first, then any other globally-open tasks.
        let mut tasks = manager.tasks_for_chat(chat_id, false);
        let mut seen: std::collections::HashSet<String> =
            tasks.iter().map(|t| t.id.clone()).collect();
        for t in manager.open_tasks() {
            if seen.insert(t.id.clone()) {
                tasks.push(t);
            }
        }

        if tasks.is_empty() {
            return None;
        }

        // Highest priority first (priority 1 = highest), then most recent.
        tasks.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(b.created_at.cmp(&a.created_at))
        });

        let mut block = String::from(
            "## Your open tasks/commitments (durable, from the task store — treat as authoritative)\n",
        );
        for t in tasks.iter().take(25) {
            block.push_str(&format!(
                "- [{:?}] (p{}) {}\n",
                t.status, t.priority, t.description
            ));
        }
        block.push_str(
            "\nThese are your actual tracked obligations regardless of chat history length. \
            When asked what your tasks/commitments are, answer from this list.",
        );
        Some(block)
    }

    /// Attach a conversation store for multi-turn history.
    pub fn with_conversation_store(mut self, store: Arc<dyn ConversationStore>) -> Self {
        self.conversation_store = Some(store);
        self
    }

    /// Attach a broadcast sender for streaming deltas.
    /// Channel handlers subscribe to this to receive real-time tokens.
    pub fn with_stream_sender(mut self, tx: broadcast::Sender<StreamDelta>) -> Self {
        self.stream_tx = Some(tx);
        self
    }

    /// Map a model tier string to a specific model name.
    ///
    /// Returns `None` for "standard" (use client's default) or unknown tiers.
    /// This allows the .px routing decision to influence which model handles
    /// a request without hardcoding model names in the .px procedures.
    fn tier_to_model(tier: &str) -> Option<String> {
        match tier {
            "fast" => Some("qwen2.5:3b".to_string()),
            "standard" => None, // use default
            "premium" => Some("qwen2.5:14b".to_string()),
            _ => None,
        }
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
        chat_id: &str,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System prompt
        if let Some(sp) = system_prompt.or(self.default_system_prompt.as_deref()) {
            messages.push(ChatMessage::system(sp));
        }

        // Durable task grounding: inject the persisted open task list so the
        // agent always sees its obligations, independent of trimmed history.
        if let Some(task_block) = self.render_open_tasks_block(chat_id) {
            messages.push(ChatMessage::system(task_block));
        }

        // Prior conversation history from ConversationStore (multi-turn context)
        if !prior_history.is_empty() {
            messages.extend(prior_history.iter().cloned());
        }

        // If this is a follow-up from tool_executor, include conversation history
        if let Some(history) = metadata
            .get("conversation_history")
            .and_then(|h| h.as_array())
        {
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
                messages.push(ChatMessage::user(format!("Tool results:\n\n{}", content)));
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
        let messages = self.build_messages(
            content,
            system_prompt.as_deref(),
            metadata,
            &prior_history,
            chat_id,
        );

        if messages.is_empty() || (messages.len() == 1 && messages[0].role == "system") {
            error!(event_id = %id, "model_invoker: no user content to send to model");
            return;
        }

        // Determine model tier from .px routing metadata (if present)
        let model_tier = metadata
            .get("model_tier")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        let routed_by_px = metadata.get("routed_by").and_then(|v| v.as_str()) == Some("px");

        if routed_by_px {
            debug!(
                event_id = %id,
                tier = %model_tier,
                reason = metadata.get("route_reason").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "model_invoker: using .px-routed model tier"
            );
        }

        // Get available tools
        let tool_defs = self.tool_dispatcher.available_tools().await;

        // Build options with tier-based model selection
        let options = ChatOptions {
            model: Self::tier_to_model(model_tier),
            ..ChatOptions::default()
        };

        // Call the model — use streaming when a broadcast sender is configured
        let result = if let Some(broadcast_tx) = &self.stream_tx {
            // Streaming path: bridge mpsc (what ModelClient expects) to broadcast (what channels subscribe to)
            debug!(event_id = %id, "model_invoker: using streaming completion");
            let (mpsc_tx, mut mpsc_rx) = tokio::sync::mpsc::unbounded_channel::<StreamDelta>();
            let broadcast_tx_clone = broadcast_tx.clone();

            // Forward deltas from mpsc to broadcast in background
            tokio::spawn(async move {
                while let Some(delta) = mpsc_rx.recv().await {
                    let _ = broadcast_tx_clone.send(delta);
                }
            });

            self.model_client
                .complete_stream(&messages, &tool_defs, &options, mpsc_tx)
                .await
        } else {
            // Non-streaming path: full completion returned at once
            self.model_client
                .complete(&messages, &tool_defs, &options)
                .await
        };

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
                        metadata: metadata.clone(),
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
        let invoker = ModelInvoker::new(Arc::new(ToolCallingModelClient), Arc::new(MockTools));

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
        let invoker = ModelInvoker::new(Arc::new(FailingModelClient), Arc::new(MockTools));

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
        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "should timeout — no events emitted");
    }

    #[test]
    fn tier_to_model_maps_correctly() {
        assert_eq!(
            ModelInvoker::tier_to_model("fast"),
            Some("qwen2.5:3b".to_string())
        );
        assert_eq!(ModelInvoker::tier_to_model("standard"), None);
        assert_eq!(
            ModelInvoker::tier_to_model("premium"),
            Some("qwen2.5:14b".to_string())
        );
        assert_eq!(ModelInvoker::tier_to_model("unknown"), None);
    }

    #[tokio::test]
    async fn respects_px_model_tier_in_metadata() {
        // Use a model client that captures the model override
        struct CapturingClient {
            called_with_model: std::sync::Arc<tokio::sync::Mutex<Option<Option<String>>>>,
        }

        #[async_trait]
        impl ModelClient for CapturingClient {
            async fn complete(
                &self,
                _messages: &[ChatMessage],
                _tools: &[ToolDefinition],
                options: &ChatOptions,
            ) -> Result<ModelCompletion, String> {
                *self.called_with_model.lock().await = Some(options.model.clone());
                Ok(ModelCompletion {
                    content: Some("ok".into()),
                    model: Some("test".into()),
                    tool_calls: vec![],
                    logprobs: None,
                })
            }
        }

        let captured = std::sync::Arc::new(tokio::sync::Mutex::new(None));
        let client: Arc<dyn ModelClient> = Arc::new(CapturingClient {
            called_with_model: captured.clone(),
        });
        let dispatcher: Arc<dyn ToolDispatcher> = Arc::new(EmptyTools);

        let invoker = ModelInvoker::new(client, dispatcher);
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

        // Simulate a .px-routed event with premium tier
        let event = SpineEvent::ModelRequest {
            id: "tier-test".into(),
            source: "telegram".into(),
            chat_id: "test".into(),
            sender: "user".into(),
            content: "complex question".into(),
            system_prompt: None,
            metadata: json!({
                "model_tier": "premium",
                "routed_by": "px",
                "route_reason": "high complexity"
            }),
        };

        invoker.handle(&event, &emitter).await;

        // Verify model override was passed
        let model_used = captured.lock().await.take().unwrap();
        assert_eq!(model_used, Some("qwen2.5:14b".to_string()));

        // Verify response was emitted
        let emitted = rx.recv().await.unwrap();
        assert_eq!(emitted.event_type(), "model_response");
    }

    #[test]
    fn injects_persisted_open_tasks_into_messages() {
        use crate::task_manager::TaskManager;
        use pluresdb::{CrdtStore, MemoryStorage};

        let storage: Arc<dyn pluresdb::StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        let manager = Arc::new(TaskManager::new(Arc::new(store)));
        manager.create_task("Ship the release binary", "chat-inject", vec![]);

        let invoker = ModelInvoker::new(Arc::new(TextModelClient::new("ok")), Arc::new(MockTools))
            .with_task_manager(Arc::clone(&manager));

        let messages = invoker.build_messages(
            "what are my tasks?",
            Some("base system prompt"),
            &json!({}),
            &[],
            "chat-inject",
        );

        // Base system prompt + injected task grounding + user message.
        let injected = messages
            .iter()
            .any(|m| m.role == "system" && m.content.contains("Ship the release binary"));
        assert!(
            injected,
            "expected persisted open task injected into system context"
        );
        let has_header = messages
            .iter()
            .any(|m| m.content.contains("Your open tasks/commitments"));
        assert!(has_header, "expected task grounding header");
    }

    #[test]
    fn no_task_block_when_no_open_tasks() {
        use crate::task_manager::TaskManager;
        use pluresdb::{CrdtStore, MemoryStorage};

        let storage: Arc<dyn pluresdb::StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        let manager = Arc::new(TaskManager::new(Arc::new(store)));

        let invoker = ModelInvoker::new(Arc::new(TextModelClient::new("ok")), Arc::new(MockTools))
            .with_task_manager(manager);

        let messages = invoker.build_messages("hi", Some("sys"), &json!({}), &[], "empty-chat");
        assert!(
            !messages
                .iter()
                .any(|m| m.content.contains("open tasks/commitments")),
            "no task block should be injected when there are no open tasks"
        );
    }

    /// Defect E, C-NOSTUB-001 / C-TEST-002: prove a persisted open task is
    /// injected into the model grounding after a FRESH on-disk store handle
    /// (simulates a process restart — not an in-memory cache). Uses real
    /// SledStorage on disk, drops the writer handle, reopens fresh, and
    /// verifies the task text reaches build_messages.
    #[test]
    fn injects_open_tasks_after_fresh_process_reload() {
        use crate::task_manager::TaskManager;
        use pluresdb::{CrdtStore, SledStorage, StorageEngine};

        let dir = std::env::temp_dir().join(format!("radix-e-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // --- Process 1: create + persist a task, then drop everything ---
        {
            let storage: Arc<dyn StorageEngine> =
                Arc::new(SledStorage::open(&dir).expect("open sled (write)"));
            let store = CrdtStore::default().with_persistence(storage);
            let manager = Arc::new(TaskManager::new(Arc::new(store)));
            manager.create_task("Finish the deploy verify", "chat-reload", vec![]);
            assert_eq!(manager.open_tasks().len(), 1);
        } // writer handle dropped — sled flushed to disk

        // --- Process 2: fresh handle to the SAME on-disk store ---
        let storage2: Arc<dyn StorageEngine> =
            Arc::new(SledStorage::open(&dir).expect("reopen sled (read)"));
        let store2 = CrdtStore::default().with_persistence(storage2);
        let manager2 = Arc::new(TaskManager::new(Arc::new(store2)));
        assert_eq!(
            manager2.open_tasks().len(),
            1,
            "persisted task must survive a fresh store handle (process reload)"
        );

        let invoker = ModelInvoker::new(Arc::new(TextModelClient::new("ok")), Arc::new(MockTools))
            .with_task_manager(Arc::clone(&manager2));
        let messages = invoker.build_messages(
            "what am I working on?",
            Some("base system prompt"),
            &json!({}),
            &[],
            "chat-reload",
        );
        assert!(
            messages
                .iter()
                .any(|m| m.role == "system" && m.content.contains("Finish the deploy verify")),
            "reloaded persisted task must be injected into model grounding after restart"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
