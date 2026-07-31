use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

/// A single delta event emitted during streaming completion.
#[derive(Debug, Clone)]
pub enum StreamDelta {
    /// A text content chunk from the model.
    Content(String),
    /// A tool call being assembled (streamed incrementally).
    ToolCallStart {
        /// Index of the tool call in the response.
        index: usize,
        /// Unique ID for this tool call.
        id: String,
        /// Tool name.
        name: String,
    },
    /// Additional JSON argument fragment for a tool call.
    ToolCallDelta {
        /// Index of the tool call in the response.
        index: usize,
        /// Partial arguments JSON.
        arguments: String,
    },
    /// Stream has completed.
    Done,
}

/// A channel sender for streaming deltas to the consumer.
pub type StreamSender = mpsc::UnboundedSender<StreamDelta>;

// ── Chat message types ───────────────────────────────────────────────────────

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// `"system"`, `"user"`, `"assistant"`, or `"tool"`.
    pub role: String,
    /// The message text content.
    pub content: String,
    /// For `"tool"` role messages — the tool call ID this result belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls requested by the model (present on `"assistant"` messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    /// Create a system message (role `"system"`).
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create a user message (role `"user"`).
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message (role `"assistant"`).
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message with tool calls (role `"assistant"`).
    ///
    /// Use this when the model responded with both content and tool call
    /// requests, or with tool calls only (pass an empty string for content).
    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<ToolCall>,
    ) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        }
    }

    /// Create a tool-result message (role `"tool"`) correlating with
    /// `tool_call_id`.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }

    /// Get the role as a string slice.
    pub fn role_str(&self) -> &str {
        &self.role
    }

    /// Get a truncated preview of the content, up to `max_chars` characters.
    pub fn content_preview(&self, max_chars: usize) -> &str {
        if self.content.len() <= max_chars {
            &self.content
        } else {
            // Find a safe char boundary to truncate at
            let mut end = max_chars;
            while end > 0 && !self.content.is_char_boundary(end) {
                end -= 1;
            }
            &self.content[..end]
        }
    }
}

// ── Tool types ───────────────────────────────────────────────────────────────

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for correlating with the tool result.
    pub id: String,
    /// Name of the tool to call.
    pub name: String,
    /// JSON arguments for the tool.
    pub arguments: Value,
}

/// A tool definition provided to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (must be unique across all tools).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters: Value,
}

// ── Model completion ─────────────────────────────────────────────────────────

/// The result of a model completion request.
#[derive(Debug, Clone)]
pub struct ModelCompletion {
    /// Text content (present when the model produced a direct response).
    pub content: Option<String>,
    /// Tool calls requested by the model (empty when the model responded
    /// directly with text).
    pub tool_calls: Vec<ToolCall>,
    /// Log probabilities for each generated token (when supported).
    pub logprobs: Option<Vec<f64>>,
    /// Model identifier returned by the provider (e.g. "gpt-4o", "claude-sonnet-4-20250514").
    pub model: Option<String>,
}

/// Optional settings for a chat completion.
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    /// Sampling temperature (0.0–2.0). `None` uses the provider default.
    pub temperature: Option<f64>,
    /// Request token logprobs when supported by the provider.
    pub logprobs: bool,
    /// Model override for this specific request. `None` uses the client's default.
    pub model: Option<String>,
}

/// Context describing a failed model call that is eligible for a fallback
/// model to be selected by the `.px` `select_fallback_model` procedure.
///
/// This is a data record, not a decision — the *choice* of which fallback
/// model to try next is owned by praxis (see
/// `praxis/procedures/model-fallback-selection.px`), not by the model client
/// itself. The client only reports "I failed and this looks fallback-eligible";
/// the orchestrator (`ModelInvoker`) is responsible for calling the procedure
/// and re-issuing the request with the selected model.
#[derive(Debug, Clone)]
pub struct FallbackRequestContext {
    /// The model that just failed.
    pub failed_model: String,
    /// Every model already attempted in this request chain (including
    /// `failed_model`), so the selector never loops back to one.
    pub already_tried: Vec<String>,
    /// The HTTP status code that triggered the fallback-eligible failure.
    pub error_status: u16,
    /// Arbitrary task/request context the selection procedure may use to
    /// pick an appropriate replacement model (e.g. tier, tool usage).
    pub task_context: serde_json::Value,
}

/// Typed error returned by [`ModelClient::complete`] / [`ModelClient::complete_stream`].
///
/// Replaces the previous `Result<_, String>` contract so that fallback-eligible
/// failures can be distinguished from terminal ones without string matching,
/// and so that model clients no longer own fallback *selection* logic
/// themselves (see `docs/design/copilot-fallback-px-wiring.md`, Option B).
#[derive(Debug, Clone)]
pub enum ModelClientError {
    /// The call failed with a fallback-eligible error (e.g. a 4xx the
    /// provider returned for an unsupported/unavailable model). The caller
    /// (orchestrator) should select a fallback model via praxis and retry.
    NeedsFallback(FallbackRequestContext),
    /// The provider returned a terminal failure that is not fallback-eligible
    /// (e.g. exhausted retries, malformed response, non-fallback-eligible
    /// status code).
    ProviderFailure {
        /// HTTP status code, when the failure originated from an HTTP response.
        status: Option<u16>,
        /// The model that was in use when the failure occurred.
        model: String,
        /// Human-readable failure detail.
        message: String,
    },
    /// The request was cancelled; no fallback should be attempted.
    Cancelled,
    /// A transport-level failure (connection, timeout, serialization, etc.)
    /// that is not itself fallback-eligible.
    Transport(String),
}

impl std::fmt::Display for ModelClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelClientError::NeedsFallback(ctx) => write!(
                f,
                "model '{}' failed with fallback-eligible error (status {}); already tried: {:?}",
                ctx.failed_model, ctx.error_status, ctx.already_tried
            ),
            ModelClientError::ProviderFailure {
                status,
                model,
                message,
            } => write!(
                f,
                "model '{model}' provider failure (status {status:?}): {message}"
            ),
            ModelClientError::Cancelled => write!(f, "model request cancelled"),
            ModelClientError::Transport(msg) => write!(f, "transport error: {msg}"),
        }
    }
}

impl std::error::Error for ModelClientError {}

// ── Traits ───────────────────────────────────────────────────────────────────

/// Abstraction over an OpenAI-compatible language model endpoint.
///
/// In production this will call the Docker Model Runner or a remote API.
/// Tests use mock implementations.
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// Request a chat completion.
    ///
    /// Returns `None` content when the model makes tool calls instead of
    /// producing a direct text response.
    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        options: &ChatOptions,
    ) -> Result<ModelCompletion, ModelClientError>;

    /// Request a streaming chat completion.
    ///
    /// Sends [`StreamDelta`] events to `tx` as tokens arrive, then returns the
    /// final assembled [`ModelCompletion`]. The default implementation calls
    /// [`Self::complete`] and emits the full content as a single delta + Done.
    ///
    /// Implementors should override this for true token-by-token streaming.
    async fn complete_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        options: &ChatOptions,
        tx: StreamSender,
    ) -> Result<ModelCompletion, ModelClientError> {
        let result = self.complete(messages, tools, options).await?;
        if let Some(content) = &result.content {
            let _ = tx.send(StreamDelta::Content(content.clone()));
        }
        let _ = tx.send(StreamDelta::Done);
        Ok(result)
    }

    /// Returns the model's maximum context window in tokens, if known.
    /// Used for context-aware model routing: requests exceeding this window
    /// should be routed to a model with a larger context.
    fn context_window(&self) -> Option<u64> {
        None
    }

    /// Returns the model identifier this client is currently using.
    fn model_id(&self) -> Option<String> {
        None
    }
}

/// Abstraction over the MCP tool dispatcher.
///
/// In production this will be backed by the MCP client crate.  Tests use mock
/// implementations.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    /// List all available tool definitions.
    async fn available_tools(&self) -> Vec<ToolDefinition>;

    /// Invoke a tool by name and return its result as a string.
    async fn call_tool(&self, name: &str, arguments: Value) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoClient;

    #[async_trait]
    impl ModelClient for EchoClient {
        async fn complete(
            &self,
            messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<ModelCompletion, ModelClientError> {
            let content = messages.last().map(|m| m.content.clone());
            Ok(ModelCompletion {
                content,
                tool_calls: vec![],
                logprobs: None,
                model: Some("echo".into()),
            })
        }
    }

    #[tokio::test]
    async fn default_complete_stream_emits_content_and_done() {
        let client = EchoClient;
        let (tx, mut rx) = mpsc::unbounded_channel();

        let result = client
            .complete_stream(
                &[ChatMessage::user("hello")],
                &[],
                &ChatOptions::default(),
                tx,
            )
            .await
            .unwrap();

        assert_eq!(result.content.as_deref(), Some("hello"));

        // Should receive Content then Done.
        let delta1 = rx.recv().await.unwrap();
        assert!(matches!(delta1, StreamDelta::Content(ref s) if s == "hello"));
        let delta2 = rx.recv().await.unwrap();
        assert!(matches!(delta2, StreamDelta::Done));
        // Channel should be empty.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn default_complete_stream_skips_content_when_none() {
        struct ToolOnlyClient;

        #[async_trait]
        impl ModelClient for ToolOnlyClient {
            async fn complete(
                &self,
                _messages: &[ChatMessage],
                _tools: &[ToolDefinition],
                _options: &ChatOptions,
            ) -> Result<ModelCompletion, ModelClientError> {
                Ok(ModelCompletion {
                    content: None,
                    tool_calls: vec![ToolCall {
                        id: "tc1".into(),
                        name: "foo".into(),
                        arguments: serde_json::json!({}),
                    }],
                    logprobs: None,
                    model: None,
                })
            }
        }

        let client = ToolOnlyClient;
        let (tx, mut rx) = mpsc::unbounded_channel();

        let result = client
            .complete_stream(
                &[ChatMessage::user("call tool")],
                &[],
                &ChatOptions::default(),
                tx,
            )
            .await
            .unwrap();

        assert!(result.content.is_none());
        assert_eq!(result.tool_calls.len(), 1);

        // Only Done should be emitted (no content delta).
        let delta = rx.recv().await.unwrap();
        assert!(matches!(delta, StreamDelta::Done));
        assert!(rx.try_recv().is_err());
    }
}
