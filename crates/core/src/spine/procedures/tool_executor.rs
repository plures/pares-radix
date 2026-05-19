//! Tool executor procedure — executes tool calls from model responses.
//!
//! When the model responds with tool calls instead of (or in addition to)
//! text content, this procedure:
//! 1. Executes each tool call via the ToolDispatcher trait
//! 2. Emits ToolResult events for each completed call
//! 3. Emits a new ModelRequest with the tool results so the model can
//!    continue the conversation

use std::sync::Arc;

use tracing::{debug, info};

use crate::model::ToolDispatcher;
use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};

/// Executes tool calls from model responses and feeds results back
/// into the pipeline as a new ModelRequest.
pub struct ToolExecutor {
    dispatcher: Arc<dyn ToolDispatcher>,
}

impl ToolExecutor {
    /// Create a new ToolExecutor with the given dispatcher.
    pub fn new(dispatcher: Arc<dyn ToolDispatcher>) -> Self {
        Self { dispatcher }
    }
}

#[async_trait::async_trait]
impl SpineProcedure for ToolExecutor {
    fn name(&self) -> &str {
        "tool_executor"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["model_response"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::ModelResponse {
            id,
            chat_id,
            tool_calls,
            ..
        } = event
        else {
            return;
        };

        // Only act when there are tool calls to execute
        if tool_calls.is_empty() {
            return;
        }

        info!(
            event_id = %id,
            tool_count = tool_calls.len(),
            "tool_executor: executing {} tool call(s)",
            tool_calls.len()
        );

        // Execute each tool call and collect results
        let mut results: Vec<String> = Vec::with_capacity(tool_calls.len());

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

            results.push(format!("[tool:{}] {}", tc.name, result));
        }

        // Emit a new ModelRequest with tool results so the model can continue
        let tool_results_content = results.join("\n\n");

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
                "web_search" => format!("Results for: {}", arguments["query"].as_str().unwrap_or("?")),
                "read" => "file contents here".into(),
                _ => format!("unknown tool: {}", name),
            }
        }
    }

    #[tokio::test]
    async fn executes_tool_calls_and_emits_model_request() {
        let (tx, mut rx) = mpsc::channel(32);
        let emitter = PipelineEmitter { tx };

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
        } else {
            panic!("expected ModelRequest");
        }
    }

    #[tokio::test]
    async fn skips_when_no_tool_calls() {
        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };

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
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx.recv(),
        )
        .await;
        assert!(result.is_err(), "should timeout — no events emitted");
    }

    #[tokio::test]
    async fn handles_multiple_tool_calls() {
        let (tx, mut rx) = mpsc::channel(32);
        let emitter = PipelineEmitter { tx };

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
}
