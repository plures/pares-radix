#![cfg(feature = "spine")]
//! Spine pipeline integration test — full Inbound → Model → ToolExec → Delivery flow.
//!
//! Tests the complete agentic loop:
//! 1. Inbound event from channel
//! 2. InboundRouter → ModelRequest
//! 3. ModelInvoker → ModelResponse (with tool calls on first pass)
//! 4. ToolExecutor → executes tools → ModelRequest (with results)
//! 5. ModelInvoker → ModelResponse (text on second pass)
//! 6. ResponseRouter → DeliveryRequest
//! 7. Channel subscriber receives delivery

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use async_trait::async_trait;
use serde_json::Value;
use tokio::time::Duration;

use pares_agens_core::model::{ToolCall, ToolDefinition, ToolDispatcher};
use pares_agens_core::spine::{
    event::SpineEvent,
    pipeline::{Pipeline, PipelineEmitter, SpineProcedure},
    procedures::{
        inbound_router::InboundRouter,
        response_router::ResponseRouter,
        tool_executor::ToolExecutor,
    },
};

// ─── Mock Model Invoker (stateful: tool calls on first call, text on second) ─

/// A model invoker that returns tool calls on the first invocation per chat,
/// then returns a text response on subsequent invocations (simulating tool
/// result processing).
struct StatefulModelInvoker {
    call_count: AtomicUsize,
}

impl StatefulModelInvoker {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl SpineProcedure for StatefulModelInvoker {
    fn name(&self) -> &str {
        "stateful_model_invoker"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["model_request"])
    }

    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
        let SpineEvent::ModelRequest {
            chat_id, content, ..
        } = event
        else {
            return;
        };

        let call_num = self.call_count.fetch_add(1, Ordering::SeqCst);

        if call_num == 0 {
            // First call: model wants to use a tool
            emitter
                .emit(SpineEvent::ModelResponse {
                    id: SpineEvent::new_id(),
                    chat_id: chat_id.clone(),
                    content: String::new(),
                    model: "test-model".into(),
                    tool_calls: vec![ToolCall {
                        id: "call-001".into(),
                        name: "web_search".into(),
                        arguments: serde_json::json!({"query": "latest rust news"}),
                    }],
                    metadata: serde_json::json!({}),
                })
                .await;
        } else {
            // Second call: model produces final text response
            emitter
                .emit(SpineEvent::ModelResponse {
                    id: SpineEvent::new_id(),
                    chat_id: chat_id.clone(),
                    content: format!("Based on tool results: {}", content),
                    model: "test-model".into(),
                    tool_calls: vec![],
                    metadata: serde_json::json!({}),
                })
                .await;
        }
    }
}

// ─── Mock Tool Dispatcher ────────────────────────────────────────────────────

struct MockTools;

#[async_trait]
impl ToolDispatcher for MockTools {
    async fn available_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "web_search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({}),
        }]
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> String {
        match name {
            "web_search" => {
                let query = arguments["query"].as_str().unwrap_or("?");
                format!("Top result for '{}': Rust 2026 edition released!", query)
            }
            _ => format!("error: unknown tool {}", name),
        }
    }
}

// ─── Integration Test ────────────────────────────────────────────────────────

#[tokio::test]
async fn full_spine_pipeline_with_tool_calling() {
    // Build the pipeline
    let (pipeline, rx) = Pipeline::new(64);

    // Register all procedures
    pipeline.register(Arc::new(InboundRouter)).await;
    pipeline
        .register(Arc::new(StatefulModelInvoker::new()))
        .await;
    pipeline
        .register(Arc::new(ToolExecutor::new(Arc::new(MockTools))))
        .await;
    pipeline.register(Arc::new(ResponseRouter)).await;

    // Subscribe to deliveries (simulates a channel adapter)
    let mut delivery_rx = pipeline.subscribe_deliveries();
    let emitter = pipeline.emitter();

    // Start the pipeline event loop
    let pipeline_clone = Arc::clone(&pipeline);
    let handle = tokio::spawn(async move {
        pipeline_clone.run(rx).await;
    });

    // Simulate an inbound message
    emitter
        .emit(SpineEvent::Inbound {
            id: SpineEvent::new_id(),
            source: "telegram".into(),
            chat_id: "user-123".into(),
            sender: "kbristol".into(),
            content: "What's new in Rust?".into(),
            metadata: serde_json::json!({}),
        })
        .await;

    // Wait for the final delivery (should go through the full loop)
    let delivered = tokio::time::timeout(Duration::from_secs(5), delivery_rx.recv())
        .await
        .expect("timeout waiting for delivery")
        .expect("delivery channel error");

    // Verify the delivery contains tool results processed by the model
    if let SpineEvent::DeliveryRequest {
        channel,
        chat_id,
        content,
        ..
    } = delivered
    {
        assert_eq!(channel, "telegram");
        assert_eq!(chat_id, "user-123");
        assert!(
            content.contains("Based on tool results"),
            "expected model to incorporate tool results, got: {}",
            content
        );
        assert!(
            content.contains("web_search"),
            "expected tool name in content, got: {}",
            content
        );
        assert!(
            content.contains("Rust 2026"),
            "expected tool result content, got: {}",
            content
        );
    } else {
        panic!(
            "expected DeliveryRequest, got: {:?}",
            delivered.event_type()
        );
    }

    handle.abort();
}

#[tokio::test]
async fn pipeline_direct_response_skips_tool_executor() {
    // Test that when the model responds with text only (no tool calls),
    // the response goes straight to delivery without tool execution.

    /// A model that always responds with text, no tools.
    struct DirectModelInvoker;

    #[async_trait]
    impl SpineProcedure for DirectModelInvoker {
        fn name(&self) -> &str {
            "direct_model"
        }
        fn handles(&self) -> Option<Vec<&'static str>> {
            Some(vec!["model_request"])
        }
        async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
            let SpineEvent::ModelRequest { chat_id, .. } = event else {
                return;
            };
            emitter
                .emit(SpineEvent::ModelResponse {
                    id: SpineEvent::new_id(),
                    chat_id: chat_id.clone(),
                    content: "Direct answer, no tools needed.".into(),
                    model: "test".into(),
                    tool_calls: vec![],
                    metadata: serde_json::json!({}),
                })
                .await;
        }
    }

    let (pipeline, rx) = Pipeline::new(32);
    pipeline.register(Arc::new(InboundRouter)).await;
    pipeline.register(Arc::new(DirectModelInvoker)).await;
    pipeline
        .register(Arc::new(ToolExecutor::new(Arc::new(MockTools))))
        .await;
    pipeline.register(Arc::new(ResponseRouter)).await;

    let mut delivery_rx = pipeline.subscribe_deliveries();
    let emitter = pipeline.emitter();

    let pipeline_clone = Arc::clone(&pipeline);
    let handle = tokio::spawn(async move {
        pipeline_clone.run(rx).await;
    });

    emitter
        .emit(SpineEvent::Inbound {
            id: SpineEvent::new_id(),
            source: "test".into(),
            chat_id: "chat-1".into(),
            sender: "user".into(),
            content: "Hello".into(),
            metadata: serde_json::json!({}),
        })
        .await;

    let delivered = tokio::time::timeout(Duration::from_secs(2), delivery_rx.recv())
        .await
        .expect("timeout")
        .expect("recv error");

    if let SpineEvent::DeliveryRequest { content, .. } = delivered {
        assert_eq!(content, "Direct answer, no tools needed.");
    } else {
        panic!("expected DeliveryRequest");
    }

    handle.abort();
}
