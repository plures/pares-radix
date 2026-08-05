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
use tracing::{debug, error, info, warn};

use crate::model::{
    ChatMessage, ChatOptions, ModelClient, ModelClientError, StreamDelta, ToolDispatcher,
};
#[cfg(test)]
use crate::model::TransportFailure;
use crate::spine::conversation::ConversationStore;
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::task_manager::TaskManager;
use serde_json::Value;

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
    /// Delegates to the shared [`crate::task_manager::render_open_tasks_block`]
    /// so the Rust SpineProcedure path and the live reactive `.px` path
    /// (`read_open_tasks_block` action) render an identical block — one
    /// implementation, no duplication (ADR-0010).
    fn render_open_tasks_block(&self, chat_id: &str) -> Option<String> {
        let manager = self.task_manager.as_ref()?;
        crate::task_manager::render_open_tasks_block(manager, chat_id)
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
            sender,
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

        let request_context = serde_json::json!({
            "event_id": id,
            "source": source,
            "chat_id": chat_id,
            "sender": sender,
            "metadata": metadata,
            "message_count": messages.len(),
            "tool_count": tool_defs.len(),
        });
        let result = self
            .complete_with_fallback(
                id,
                chat_id,
                &messages,
                &tool_defs,
                options,
                &request_context,
            )
            .await;

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

impl ModelInvoker {
    /// Maximum total model-call attempts for a single request, including the
    /// initial call. Bounds fallback retries so a misbehaving `.px` selection
    /// (or a selector that keeps returning fresh-looking but ultimately
    /// unusable models) cannot loop forever.
    const MAX_FALLBACK_ATTEMPTS: usize = 4;

    /// Call the model client, and on a fallback-eligible failure, ask the
    /// `select_fallback_model` `.px` procedure (via the existing
    /// [`ToolDispatcher`] seam) which model to retry with. Loops up to
    /// [`Self::MAX_FALLBACK_ATTEMPTS`] total attempts.
    ///
    /// Fallback *selection* is owned entirely by praxis — this method never
    /// hardcodes or infers a replacement model itself; it only orchestrates
    /// the retry loop and enforces the already-tried/attempt-cap safety net
    /// (Option B in `docs/design/copilot-fallback-px-wiring.md`).
    async fn complete_with_fallback(
        &self,
        event_id: &str,
        chat_id: &str,
        messages: &[ChatMessage],
        tool_defs: &[crate::model::ToolDefinition],
        mut options: ChatOptions,
        request_context: &Value,
    ) -> Result<crate::model::ModelCompletion, ModelClientError> {
        let mut already_tried: Vec<String> = Vec::new();
        if let Some(model) = &options.model {
            already_tried.push(model.clone());
        }

        for attempt in 0..Self::MAX_FALLBACK_ATTEMPTS {
            let call_result = if let Some(broadcast_tx) = &self.stream_tx {
                let (mpsc_tx, mut mpsc_rx) = tokio::sync::mpsc::unbounded_channel::<StreamDelta>();
                let broadcast_tx_clone = broadcast_tx.clone();
                tokio::spawn(async move {
                    while let Some(delta) = mpsc_rx.recv().await {
                        let _ = broadcast_tx_clone.send(delta);
                    }
                });
                self.model_client
                    .complete_stream(messages, tool_defs, &options, mpsc_tx)
                    .await
            } else {
                self.model_client
                    .complete(messages, tool_defs, &options)
                    .await
            };

            let needs_fallback_ctx = match call_result {
                Ok(completion) => return Ok(completion),
                Err(ModelClientError::NeedsFallback(ctx)) => ctx,
                Err(other) => return Err(other),
            };

            for model in &needs_fallback_ctx.already_tried {
                if !already_tried.contains(model) {
                    already_tried.push(model.clone());
                }
            }
            if !already_tried.contains(&needs_fallback_ctx.failed_model) {
                already_tried.push(needs_fallback_ctx.failed_model.clone());
            }

            if attempt + 1 >= Self::MAX_FALLBACK_ATTEMPTS {
                return Err(ModelClientError::ProviderFailure {
                    status: Some(needs_fallback_ctx.error_status),
                    model: needs_fallback_ctx.failed_model.clone(),
                    message: format!(
                        "fallback exhausted: hit max attempts ({})",
                        Self::MAX_FALLBACK_ATTEMPTS
                    ),
                });
            }

            debug!(
                event_id = %event_id,
                chat_id = %chat_id,
                attempt,
                failed_model = %needs_fallback_ctx.failed_model,
                already_tried = ?already_tried,
                "model_invoker: model call needs fallback, consulting praxis selection"
            );

            // Praxis owns the decision; the invoker only asks and applies it.
            let selector_args = serde_json::json!({
                "failed_model": needs_fallback_ctx.failed_model,
                "already_tried": already_tried,
                "error_status": needs_fallback_ctx.error_status,
                "task_context": {
                    "provider_context": needs_fallback_ctx.task_context,
                    "request": request_context,
                },
            });
            let raw = self
                .tool_dispatcher
                .call_tool("select_fallback_model", selector_args)
                .await;

            let decision: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => {
                    let raw_preview: String = raw.chars().take(500).collect();
                    // Non-JSON / unparseable response from the selector is treated
                    // as "no candidate" — fail closed rather than guess.
                    return Err(ModelClientError::ProviderFailure {
                        status: Some(needs_fallback_ctx.error_status),
                        model: needs_fallback_ctx.failed_model.clone(),
                        message: format!(
                            "fallback selection returned an unparseable response (preview): {raw_preview}"
                        ),
                    });
                }
            };

            let candidate = decision
                .get("model")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let candidate = match candidate {
                Some(c) if !already_tried.contains(&c) => c,
                Some(c) => {
                    warn!(
                        event_id = %event_id,
                        chat_id = %chat_id,
                        candidate = %c,
                        "model_invoker: fallback selector returned an already-tried model, stopping"
                    );
                    return Err(ModelClientError::ProviderFailure {
                        status: Some(needs_fallback_ctx.error_status),
                        model: needs_fallback_ctx.failed_model.clone(),
                        message: format!(
                            "fallback exhausted: selector re-suggested already-tried model '{c}'"
                        ),
                    });
                }
                None => {
                    warn!(
                        event_id = %event_id,
                        chat_id = %chat_id,
                        "model_invoker: fallback selection exhausted, no further candidate"
                    );
                    return Err(ModelClientError::ProviderFailure {
                        status: Some(needs_fallback_ctx.error_status),
                        model: needs_fallback_ctx.failed_model.clone(),
                        message: "fallback exhausted: no candidate model available".to_string(),
                    });
                }
            };

            info!(
                event_id = %event_id,
                chat_id = %chat_id,
                candidate = %candidate,
                attempt,
                "model_invoker: retrying with praxis-selected fallback model"
            );
            already_tried.push(candidate.clone());
            options.model = Some(candidate);
        }

        Err(ModelClientError::ProviderFailure {
            status: None,
            model: options.model.unwrap_or_else(|| "unknown".into()),
            message: format!(
                "fallback exhausted: hit max attempts ({})",
                Self::MAX_FALLBACK_ATTEMPTS
            ),
        })
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
        ) -> Result<ModelCompletion, ModelClientError> {
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
        ) -> Result<ModelCompletion, ModelClientError> {
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
        ) -> Result<ModelCompletion, ModelClientError> {
            Err(ModelClientError::Transport(TransportFailure::message("connection timeout")))
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
        ) -> Result<ModelCompletion, ModelClientError> {
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

    /// Scripted client for fallback orchestration tests. It records the model
    /// supplied on each call so tests prove the invoker, rather than the
    /// provider client, applies the Praxis-selected override.
    struct ScriptedModelClient {
        responses: tokio::sync::Mutex<
            std::collections::VecDeque<Result<ModelCompletion, ModelClientError>>,
        >,
        models: tokio::sync::Mutex<Vec<Option<String>>>,
    }

    impl ScriptedModelClient {
        fn new(responses: Vec<Result<ModelCompletion, ModelClientError>>) -> Self {
            Self {
                responses: tokio::sync::Mutex::new(responses.into()),
                models: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ModelClient for ScriptedModelClient {
        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            options: &ChatOptions,
        ) -> Result<ModelCompletion, ModelClientError> {
            self.models.lock().await.push(options.model.clone());
            self.responses
                .lock()
                .await
                .pop_front()
                .expect("scripted model client received an unexpected extra call")
        }
    }

    struct FallbackTools {
        response: String,
        calls: tokio::sync::Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl FallbackTools {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
                calls: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ToolDispatcher for FallbackTools {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> String {
            self.calls.lock().await.push((name.to_owned(), arguments));
            self.response.clone()
        }
    }

    struct SequencedFallbackTools {
        responses: tokio::sync::Mutex<std::collections::VecDeque<String>>,
        calls: tokio::sync::Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl SequencedFallbackTools {
        fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: tokio::sync::Mutex::new(
                    responses.into_iter().map(str::to_owned).collect(),
                ),
                calls: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ToolDispatcher for SequencedFallbackTools {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> String {
            self.calls.lock().await.push((name.to_owned(), arguments));
            self.responses
                .lock()
                .await
                .pop_front()
                .expect("selector received an unexpected extra call")
        }
    }

    fn fallback_needed(model: &str) -> ModelClientError {
        ModelClientError::NeedsFallback(crate::model::FallbackRequestContext {
            failed_model: model.to_owned(),
            already_tried: vec![model.to_owned()],
            error_status: 400,
            task_context: json!({"task_kind": "chat"}),
        })
    }

    fn fallback_test_event() -> SpineEvent {
        SpineEvent::ModelRequest {
            source: "test".into(),
            id: "fallback-request".into(),
            chat_id: "fallback-chat".into(),
            sender: "user".into(),
            content: "please handle this".into(),
            system_prompt: None,
            metadata: json!({"model_tier": "standard", "route_reason": "test"}),
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
    async fn retries_with_praxis_selected_fallback_model() {
        let (emitter, mut rx) = make_emitter();
        let client = Arc::new(ScriptedModelClient::new(vec![
            Err(fallback_needed("primary-model")),
            Ok(ModelCompletion {
                content: Some("fallback succeeded".into()),
                tool_calls: vec![],
                logprobs: None,
                model: Some("fallback-model".into()),
            }),
        ]));
        let tools = Arc::new(FallbackTools::new(
            r#"{"model":"fallback-model","reason":"live candidate","exhausted":false}"#,
        ));
        let invoker = ModelInvoker::new(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::clone(&tools) as Arc<dyn ToolDispatcher>,
        );

        invoker.handle(&fallback_test_event(), &emitter).await;

        match rx.recv().await.expect("model response") {
            SpineEvent::ModelResponse { content, model, .. } => {
                assert_eq!(content, "fallback succeeded");
                assert_eq!(model, "fallback-model");
            }
            other => panic!("expected ModelResponse, got {other:?}"),
        }
        assert_eq!(
            *client.models.lock().await,
            vec![None, Some("fallback-model".into())]
        );
        let calls = tools.calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "select_fallback_model");
        assert_eq!(calls[0].1["failed_model"], "primary-model");
        assert_eq!(calls[0].1["already_tried"], json!(["primary-model"]));
        assert_eq!(
            calls[0].1["task_context"]["request"]["chat_id"],
            "fallback-chat"
        );
    }

    #[tokio::test]
    async fn fallback_attempts_are_hard_capped() {
        let (emitter, mut rx) = make_emitter();
        let client = Arc::new(ScriptedModelClient::new(vec![
            Err(fallback_needed("primary-model")),
            Err(fallback_needed("fallback-1")),
            Err(fallback_needed("fallback-2")),
            Err(fallback_needed("fallback-3")),
        ]));
        let tools = Arc::new(SequencedFallbackTools::new(vec![
            r#"{"model":"fallback-1"}"#,
            r#"{"model":"fallback-2"}"#,
            r#"{"model":"fallback-3"}"#,
        ]));
        let invoker = ModelInvoker::new(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::clone(&tools) as Arc<dyn ToolDispatcher>,
        );

        invoker.handle(&fallback_test_event(), &emitter).await;

        match rx.recv().await.expect("terminal error") {
            SpineEvent::DeliveryRequest { content, .. } => {
                assert!(content.contains("hit max attempts (4)"));
            }
            other => panic!("expected DeliveryRequest, got {other:?}"),
        }
        assert_eq!(
            client.models.lock().await.len(),
            ModelInvoker::MAX_FALLBACK_ATTEMPTS
        );
        assert_eq!(
            tools.calls.lock().await.len(),
            ModelInvoker::MAX_FALLBACK_ATTEMPTS - 1
        );
    }

    #[tokio::test]
    async fn rejects_praxis_selector_that_repeats_a_tried_model() {
        let (emitter, mut rx) = make_emitter();
        let client = Arc::new(ScriptedModelClient::new(vec![Err(fallback_needed(
            "primary-model",
        ))]));
        let tools = Arc::new(FallbackTools::new(r#"{"model":"primary-model"}"#));
        let invoker = ModelInvoker::new(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::clone(&tools) as Arc<dyn ToolDispatcher>,
        );

        invoker.handle(&fallback_test_event(), &emitter).await;

        match rx.recv().await.expect("terminal error") {
            SpineEvent::DeliveryRequest { content, .. } => {
                assert!(content.contains("fallback exhausted"));
                assert!(content.contains("already-tried"));
            }
            other => panic!("expected DeliveryRequest, got {other:?}"),
        }
        assert_eq!(client.models.lock().await.len(), 1);
        assert_eq!(tools.calls.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn fails_closed_when_praxis_selector_returns_invalid_payload() {
        let (emitter, mut rx) = make_emitter();
        let client = Arc::new(ScriptedModelClient::new(vec![Err(fallback_needed(
            "primary-model",
        ))]));
        let tools = Arc::new(FallbackTools::new("Tool error: selector unavailable"));
        let invoker = ModelInvoker::new(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::clone(&tools) as Arc<dyn ToolDispatcher>,
        );

        invoker.handle(&fallback_test_event(), &emitter).await;

        match rx.recv().await.expect("terminal error") {
            SpineEvent::DeliveryRequest { content, .. } => {
                assert!(content.contains("unparseable response"));
            }
            other => panic!("expected DeliveryRequest, got {other:?}"),
        }
        assert_eq!(client.models.lock().await.len(), 1);
        assert_eq!(tools.calls.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn does_not_invoke_selector_when_model_request_is_cancelled() {
        let (emitter, mut rx) = make_emitter();
        let client = Arc::new(ScriptedModelClient::new(vec![Err(
            ModelClientError::Cancelled,
        )]));
        let tools = Arc::new(FallbackTools::new(r#"{"model":"fallback-model"}"#));
        let invoker = ModelInvoker::new(
            Arc::clone(&client) as Arc<dyn ModelClient>,
            Arc::clone(&tools) as Arc<dyn ToolDispatcher>,
        );

        invoker.handle(&fallback_test_event(), &emitter).await;

        match rx.recv().await.expect("terminal cancellation") {
            SpineEvent::DeliveryRequest { content, .. } => {
                assert!(content.contains("model request cancelled"));
            }
            other => panic!("expected DeliveryRequest, got {other:?}"),
        }
        assert!(tools.calls.lock().await.is_empty());
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
            ) -> Result<ModelCompletion, ModelClientError> {
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
