//! Tool executor procedure — executes tool calls from model responses.
//!
//! When the model responds with tool calls instead of (or in addition to)
//! text content, this procedure:
//! 1. Executes each tool call via the ToolDispatcher trait
//! 2. Emits ToolResult events for each completed call
//! 3. Emits a new ModelRequest with the tool results so the model can
//!    continue the conversation
//!
//! Safety: A per-chat iteration counter prevents infinite tool loops.
//! Conversation history is threaded through metadata for full context.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::model::{ChatMessage, ToolDispatcher};
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Default maximum tool-loop iterations per chat before aborting.
const DEFAULT_MAX_ITERATIONS: usize = 25;

/// Per-chat state tracking for tool loop safety and context accumulation.
#[derive(Debug, Clone, Default)]
struct ChatLoopState {
    /// How many tool-loop iterations have occurred in the current turn.
    iterations: usize,
    /// Accumulated conversation messages for this turn.
    history: Vec<ChatMessage>,
}

/// Executes tool calls from model responses and feeds results back
/// into the pipeline as a new ModelRequest.
///
/// Tracks per-chat iteration count to prevent infinite loops, and
/// accumulates conversation history for full model context.
pub struct ToolExecutor {
    dispatcher: Arc<dyn ToolDispatcher>,
    max_iterations: usize,
    /// Per-chat loop state. Keyed by chat_id.
    chat_states: Mutex<HashMap<String, ChatLoopState>>,
}

impl ToolExecutor {
    /// Create a new ToolExecutor with the given dispatcher and default max iterations.
    pub fn new(dispatcher: Arc<dyn ToolDispatcher>) -> Self {
        Self {
            dispatcher,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            chat_states: Mutex::new(HashMap::new()),
        }
    }

    /// Create a ToolExecutor with a custom max iterations limit.
    pub fn with_max_iterations(dispatcher: Arc<dyn ToolDispatcher>, max_iterations: usize) -> Self {
        Self {
            dispatcher,
            max_iterations,
            chat_states: Mutex::new(HashMap::new()),
        }
    }

    /// Reset the loop state for a chat (called when a new user turn begins).
    pub async fn reset_chat(&self, chat_id: &str) {
        let mut states = self.chat_states.lock().await;
        states.remove(chat_id);
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ToolExecutor {
    fn name(&self) -> &str {
        "tool_executor"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["model_response", "inbound"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        // On inbound messages, reset the loop state for that chat
        if let SpineEvent::Inbound { chat_id, .. } = event {
            self.reset_chat(chat_id).await;
            return;
        }

        let SpineEvent::ModelResponse {
            id,
            chat_id,
            content,
            tool_calls,
            ..
        } = event
        else {
            return;
        };

        // Only act when there are tool calls to execute
        if tool_calls.is_empty() {
            // Clean up state — this turn is done
            self.reset_chat(chat_id).await;
            return;
        }

        // Check iteration limit
        let mut states = self.chat_states.lock().await;
        let state = states.entry(chat_id.clone()).or_default();
        state.iterations += 1;

        if state.iterations > self.max_iterations {
            error!(
                chat_id = %chat_id,
                iterations = state.iterations,
                max = self.max_iterations,
                "tool_executor: max iterations exceeded, aborting tool loop"
            );

            // Emit a delivery request with an error message
            emitter
                .emit(SpineEvent::DeliveryRequest {
                    id: SpineEvent::new_id(),
                    channel: "system".into(),
                    chat_id: chat_id.clone(),
                    content: format!(
                        "⚠️ Tool loop aborted: exceeded maximum of {} iterations. \
                         The model may be stuck in a loop. Please try rephrasing your request.",
                        self.max_iterations
                    ),
                    metadata: serde_json::json!({
                        "source": "tool_executor",
                        "reason": "max_iterations_exceeded",
                        "iterations": state.iterations,
                    }),
                })
                .await;

            // Clean up state
            drop(states);
            self.reset_chat(chat_id).await;
            return;
        }

        // Record the assistant's response (with tool calls) in history.
        // Always record the assistant message when tool_calls are present,
        // even if content is empty — the model needs to see which calls it made.
        {
            let mut msg = ChatMessage::assistant(content.clone());
            msg.tool_calls = Some(tool_calls.clone());
            state.history.push(msg);
        }

        // Drop the lock before doing async tool calls
        let iteration = state.iterations;
        let mut history_snapshot = state.history.clone();
        drop(states);

        info!(
            event_id = %id,
            tool_count = tool_calls.len(),
            iteration = iteration,
            "tool_executor: executing {} tool call(s) (iteration {}/{})",
            tool_calls.len(), iteration, self.max_iterations
        );

        // Execute each tool call and collect results
        let mut tool_results: Vec<ChatMessage> = Vec::with_capacity(tool_calls.len());

        for tc in tool_calls {
            debug!(
                tool_name = %tc.name,
                tool_call_id = %tc.id,
                "tool_executor: calling tool"
            );

            let result = self.dispatcher.call_tool(&tc.name, tc.arguments.clone()).await;

            // Emit a ToolResult event for observability
            emitter
                .emit(SpineEvent::ToolResult {
                    id: SpineEvent::new_id(),
                    chat_id: chat_id.clone(),
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    content: result.clone(),
                    metadata: serde_json::json!({}),
                })
                .await;

            tool_results.push(ChatMessage::tool_result(tc.id.clone(), result));
        }

        // Append tool results to history
        history_snapshot.extend(tool_results.clone());

        // Update the stored state with new history
        {
            let mut states = self.chat_states.lock().await;
            if let Some(state) = states.get_mut(chat_id) {
                state.history = history_snapshot.clone();
            }
        }

        // Build content for the model request (flattened tool results for legacy consumers)
        let tool_results_content = tool_results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let tool_name = tool_calls.get(i)
                    .map(|tc| tc.name.as_str())
                    .unwrap_or("unknown");
                format!("[tool:{}] {}", tool_name, r.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        // Emit a new ModelRequest with tool results and full conversation history
        emitter
            .emit(SpineEvent::ModelRequest {
                id: SpineEvent::new_id(),
                chat_id: chat_id.clone(),
                sender: "system".into(),
                content: tool_results_content,
                system_prompt: None,
                metadata: serde_json::json!({
                    "source": "tool_executor",
                    "parent_event": id,
                    "iteration": iteration,
                    "conversation_history": history_snapshot,
                }),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ToolCall, ToolDefinition};
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio::sync::mpsc;

    /// A mock dispatcher that returns predictable results.
    struct MockDispatcher;

    #[async_trait]
    impl ToolDispatcher for MockDispatcher {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        async fn call_tool(&self, name: &str, arguments: Value) -> String {
            match name {
                "web_search" => {
                    format!("Results for: {}", arguments["query"].as_str().unwrap_or("?"))
                }
                "read" => "file contents here".into(),
                _ => format!("unknown tool: {}", name),
            }
        }
    }

    /// A mock dispatcher that always requests more tool calls (for loop testing).
    struct InfiniteLoopDispatcher;

    #[async_trait]
    impl ToolDispatcher for InfiniteLoopDispatcher {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        async fn call_tool(&self, _name: &str, _arguments: Value) -> String {
            "need more data".into()
        }
    }

    fn make_emitter() -> (PipelineEmitter, mpsc::Receiver<SpineEvent>) {
        let (tx, rx) = mpsc::channel(64);
        (PipelineEmitter { tx }, rx)
    }

    #[tokio::test]
    async fn executes_tool_calls_and_emits_model_request() {
        let (emitter, mut rx) = make_emitter();
        let executor = ToolExecutor::new(Arc::new(MockDispatcher));

        let event = SpineEvent::ModelResponse {
            id: "resp-1".into(),
            chat_id: "chat-42".into(),
            content: String::new(),
            model: "gpt-4".into(),
            tool_calls: vec![ToolCall {
                id: "tc-1".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "rust async"}),
            }],
            metadata: serde_json::json!({}),
        };

        executor.handle(&event, &emitter).await;

        // First emitted: ToolResult
        let tool_result = rx.recv().await.unwrap();
        assert_eq!(tool_result.event_type(), "tool_result");
        if let SpineEvent::ToolResult {
            tool_call_id,
            tool_name,
            content,
            chat_id,
            ..
        } = tool_result
        {
            assert_eq!(tool_call_id, "tc-1");
            assert_eq!(tool_name, "web_search");
            assert_eq!(chat_id, "chat-42");
            assert!(content.contains("rust async"));
        } else {
            panic!("expected ToolResult");
        }

        // Second emitted: ModelRequest (with tool results)
        let model_req = rx.recv().await.unwrap();
        assert_eq!(model_req.event_type(), "model_request");
        if let SpineEvent::ModelRequest {
            chat_id,
            content,
            sender,
            metadata,
            ..
        } = model_req
        {
            assert_eq!(chat_id, "chat-42");
            assert_eq!(sender, "system");
            assert!(content.contains("[tool:web_search]"));
            assert!(content.contains("rust async"));
            assert_eq!(metadata["source"], "tool_executor");
            assert_eq!(metadata["iteration"], 1);
            // Conversation history should be present
            assert!(metadata["conversation_history"].is_array());
        } else {
            panic!("expected ModelRequest");
        }
    }

    #[tokio::test]
    async fn skips_when_no_tool_calls() {
        let (emitter, mut rx) = make_emitter();
        let executor = ToolExecutor::new(Arc::new(MockDispatcher));

        let event = SpineEvent::ModelResponse {
            id: "resp-2".into(),
            chat_id: "chat-42".into(),
            content: "Just a text response".into(),
            model: "gpt-4".into(),
            tool_calls: vec![],
            metadata: serde_json::json!({}),
        };

        executor.handle(&event, &emitter).await;

        // No events emitted
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_err(), "should timeout — no events emitted");
    }

    #[tokio::test]
    async fn handles_multiple_tool_calls() {
        let (emitter, mut rx) = make_emitter();
        let executor = ToolExecutor::new(Arc::new(MockDispatcher));

        let event = SpineEvent::ModelResponse {
            id: "resp-3".into(),
            chat_id: "chat-99".into(),
            content: String::new(),
            model: "gpt-4".into(),
            tool_calls: vec![
                ToolCall {
                    id: "tc-a".into(),
                    name: "web_search".into(),
                    arguments: serde_json::json!({"query": "foo"}),
                },
                ToolCall {
                    id: "tc-b".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/tmp/x.md"}),
                },
            ],
            metadata: serde_json::json!({}),
        };

        executor.handle(&event, &emitter).await;

        // 2 ToolResult events + 1 ModelRequest = 3 events
        let r1 = rx.recv().await.unwrap();
        assert_eq!(r1.event_type(), "tool_result");

        let r2 = rx.recv().await.unwrap();
        assert_eq!(r2.event_type(), "tool_result");

        let req = rx.recv().await.unwrap();
        assert_eq!(req.event_type(), "model_request");
        if let SpineEvent::ModelRequest { content, .. } = req {
            assert!(content.contains("[tool:web_search]"));
            assert!(content.contains("[tool:read]"));
            assert!(content.contains("file contents here"));
        } else {
            panic!("expected ModelRequest");
        }
    }

    #[tokio::test]
    async fn max_iterations_guard_aborts_loop() {
        let (emitter, mut rx) = make_emitter();
        // Set max iterations to 3 for easy testing
        let executor =
            ToolExecutor::with_max_iterations(Arc::new(InfiniteLoopDispatcher), 3);

        let chat_id = "chat-loop".to_string();

        // Simulate 3 iterations (all should succeed)
        for i in 1..=3 {
            let event = SpineEvent::ModelResponse {
                id: format!("resp-{}", i),
                chat_id: chat_id.clone(),
                content: String::new(),
                model: "gpt-4".into(),
                tool_calls: vec![ToolCall {
                    id: format!("tc-{}", i),
                    name: "web_search".into(),
                    arguments: serde_json::json!({"query": "loop"}),
                }],
                metadata: serde_json::json!({}),
            };
            executor.handle(&event, &emitter).await;
        }

        // Drain the 3 successful iterations (each = 1 ToolResult + 1 ModelRequest)
        for _ in 0..6 {
            rx.recv().await.unwrap();
        }

        // 4th iteration should be blocked
        let event = SpineEvent::ModelResponse {
            id: "resp-4".into(),
            chat_id: chat_id.clone(),
            content: String::new(),
            model: "gpt-4".into(),
            tool_calls: vec![ToolCall {
                id: "tc-4".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "loop"}),
            }],
            metadata: serde_json::json!({}),
        };
        executor.handle(&event, &emitter).await;

        // Should get a DeliveryRequest with error message
        let abort_event = rx.recv().await.unwrap();
        assert_eq!(abort_event.event_type(), "delivery_request");
        if let SpineEvent::DeliveryRequest {
            content, metadata, ..
        } = abort_event
        {
            assert!(content.contains("Tool loop aborted"));
            assert!(content.contains("3 iterations"));
            assert_eq!(metadata["reason"], "max_iterations_exceeded");
        } else {
            panic!("expected DeliveryRequest abort message");
        }
    }

    #[tokio::test]
    async fn inbound_resets_iteration_counter() {
        let (emitter, mut rx) = make_emitter();
        let executor =
            ToolExecutor::with_max_iterations(Arc::new(InfiniteLoopDispatcher), 2);

        let chat_id = "chat-reset".to_string();

        // Use 2 iterations (the max)
        for i in 1..=2 {
            let event = SpineEvent::ModelResponse {
                id: format!("resp-{}", i),
                chat_id: chat_id.clone(),
                content: String::new(),
                model: "gpt-4".into(),
                tool_calls: vec![ToolCall {
                    id: format!("tc-{}", i),
                    name: "web_search".into(),
                    arguments: serde_json::json!({"query": "test"}),
                }],
                metadata: serde_json::json!({}),
            };
            executor.handle(&event, &emitter).await;
        }

        // Drain events (2 iterations × 2 events = 4)
        for _ in 0..4 {
            rx.recv().await.unwrap();
        }

        // Simulate a new inbound message — should reset counter
        let inbound = SpineEvent::Inbound {
            id: SpineEvent::new_id(),
            source: "test".into(),
            chat_id: chat_id.clone(),
            sender: "user".into(),
            content: "new question".into(),
            metadata: serde_json::json!({}),
        };
        executor.handle(&inbound, &emitter).await;

        // Now we should be able to do 2 more iterations without hitting the guard
        let event = SpineEvent::ModelResponse {
            id: "resp-after-reset".into(),
            chat_id: chat_id.clone(),
            content: String::new(),
            model: "gpt-4".into(),
            tool_calls: vec![ToolCall {
                id: "tc-after".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "works"}),
            }],
            metadata: serde_json::json!({}),
        };
        executor.handle(&event, &emitter).await;

        // Should succeed (ToolResult + ModelRequest, not an abort)
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.event_type(), "tool_result");
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.event_type(), "model_request");
    }

    #[tokio::test]
    async fn conversation_history_accumulates_across_iterations() {
        let (emitter, mut rx) = make_emitter();
        let executor = ToolExecutor::new(Arc::new(MockDispatcher));
        let chat_id = "chat-history".to_string();

        // First iteration
        let event1 = SpineEvent::ModelResponse {
            id: "resp-h1".into(),
            chat_id: chat_id.clone(),
            content: "Let me search for that".into(),
            model: "gpt-4".into(),
            tool_calls: vec![ToolCall {
                id: "tc-h1".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "first"}),
            }],
            metadata: serde_json::json!({}),
        };
        executor.handle(&event1, &emitter).await;

        // Drain first iteration events
        rx.recv().await.unwrap(); // ToolResult
        let req1 = rx.recv().await.unwrap(); // ModelRequest

        if let SpineEvent::ModelRequest { metadata, .. } = &req1 {
            let history = metadata["conversation_history"].as_array().unwrap();
            // Should have: 1 assistant message + 1 tool result = 2
            assert_eq!(history.len(), 2);
            assert_eq!(history[0]["role"], "assistant");
            assert_eq!(history[1]["role"], "tool");
        } else {
            panic!("expected ModelRequest");
        }

        // Second iteration
        let event2 = SpineEvent::ModelResponse {
            id: "resp-h2".into(),
            chat_id: chat_id.clone(),
            content: "Now let me read a file".into(),
            model: "gpt-4".into(),
            tool_calls: vec![ToolCall {
                id: "tc-h2".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "/tmp/test.md"}),
            }],
            metadata: serde_json::json!({}),
        };
        executor.handle(&event2, &emitter).await;

        // Drain second iteration
        rx.recv().await.unwrap(); // ToolResult
        let req2 = rx.recv().await.unwrap(); // ModelRequest

        if let SpineEvent::ModelRequest { metadata, .. } = &req2 {
            let history = metadata["conversation_history"].as_array().unwrap();
            // Should have accumulated: 2 from first + 1 assistant + 1 tool = 4
            assert_eq!(history.len(), 4);
            assert_eq!(history[0]["role"], "assistant");
            assert_eq!(history[1]["role"], "tool");
            assert_eq!(history[2]["role"], "assistant");
            assert_eq!(history[3]["role"], "tool");
            assert_eq!(history[3]["tool_call_id"], "tc-h2");
        } else {
            panic!("expected ModelRequest");
        }
    }

    #[tokio::test]
    async fn different_chats_have_independent_state() {
        let (emitter, mut rx) = make_emitter();
        let executor =
            ToolExecutor::with_max_iterations(Arc::new(InfiniteLoopDispatcher), 2);

        // Fill up chat-A to max
        for i in 1..=2 {
            let event = SpineEvent::ModelResponse {
                id: format!("a-{}", i),
                chat_id: "chat-A".into(),
                content: String::new(),
                model: "gpt-4".into(),
                tool_calls: vec![ToolCall {
                    id: format!("tc-a-{}", i),
                    name: "web_search".into(),
                    arguments: serde_json::json!({}),
                }],
                metadata: serde_json::json!({}),
            };
            executor.handle(&event, &emitter).await;
        }

        // Drain chat-A events (2 × 2 = 4)
        for _ in 0..4 {
            rx.recv().await.unwrap();
        }

        // chat-B should still work fine (independent counter)
        let event_b = SpineEvent::ModelResponse {
            id: "b-1".into(),
            chat_id: "chat-B".into(),
            content: String::new(),
            model: "gpt-4".into(),
            tool_calls: vec![ToolCall {
                id: "tc-b-1".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({}),
            }],
            metadata: serde_json::json!({}),
        };
        executor.handle(&event_b, &emitter).await;

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.event_type(), "tool_result"); // Not an abort
    }

    #[tokio::test]
    async fn history_includes_structured_tool_calls_on_assistant_messages() {
        let (emitter, mut rx) = make_emitter();
        let executor = ToolExecutor::new(Arc::new(MockDispatcher));
        let chat_id = "chat-structured".to_string();

        let event = SpineEvent::ModelResponse {
            id: "resp-s1".into(),
            chat_id: chat_id.clone(),
            content: "I'll search for that".into(),
            model: "gpt-4".into(),
            tool_calls: vec![
                ToolCall {
                    id: "tc-s1".into(),
                    name: "web_search".into(),
                    arguments: serde_json::json!({"query": "structured test"}),
                },
                ToolCall {
                    id: "tc-s2".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/tmp/f.md"}),
                },
            ],
            metadata: serde_json::json!({}),
        };
        executor.handle(&event, &emitter).await;

        // Drain ToolResult events
        rx.recv().await.unwrap();
        rx.recv().await.unwrap();

        // Get the ModelRequest
        let req = rx.recv().await.unwrap();
        if let SpineEvent::ModelRequest { metadata, .. } = &req {
            let history = metadata["conversation_history"].as_array().unwrap();

            // First message: assistant with tool_calls array
            let assistant_msg = &history[0];
            assert_eq!(assistant_msg["role"], "assistant");
            assert_eq!(assistant_msg["content"], "I'll search for that");
            let tool_calls = assistant_msg["tool_calls"].as_array().unwrap();
            assert_eq!(tool_calls.len(), 2);
            assert_eq!(tool_calls[0]["id"], "tc-s1");
            assert_eq!(tool_calls[0]["name"], "web_search");
            assert_eq!(tool_calls[1]["id"], "tc-s2");
            assert_eq!(tool_calls[1]["name"], "read");

            // Tool results follow with proper tool_call_id
            assert_eq!(history[1]["role"], "tool");
            assert_eq!(history[1]["tool_call_id"], "tc-s1");
            assert_eq!(history[2]["role"], "tool");
            assert_eq!(history[2]["tool_call_id"], "tc-s2");
        } else {
            panic!("expected ModelRequest");
        }
    }
}
