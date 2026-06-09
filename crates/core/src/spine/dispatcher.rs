//! Spine tool dispatcher — bridges the ProcedureRegistry into the ToolDispatcher trait.
//!
//! This allows the spine pipeline's `ModelInvoker` and `ToolExecutor` to dispatch
//! tool calls through the same `ProcedureRegistry` used by the agent in serve mode,
//! unifying tool execution across both code paths.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::event::Event;
use crate::model::{ToolDefinition, ToolDispatcher};
use crate::procedure::ProcedureRegistry;

/// A `ToolDispatcher` implementation backed by a `ProcedureRegistry`.
///
/// Tool calls are routed to the first procedure whose `handles()` matches
/// the tool name, with arguments passed as the event content.
///
/// Tool definitions can be provided explicitly at construction time, or
/// registered dynamically via `add_tool_definition`.
pub struct SpineProcedureDispatcher {
    /// The procedure registry containing all registered tools.
    registry: Arc<RwLock<ProcedureRegistry>>,
    /// Explicit tool definitions exposed to the model.
    tool_definitions: RwLock<Vec<ToolDefinition>>,
}

impl SpineProcedureDispatcher {
    /// Create a new dispatcher backed by the given procedure registry.
    ///
    /// Starts with no tool definitions — call [`Self::with_tools`] or
    /// [`Self::add_tool_definition`] to expose tools to the model.
    pub fn new(registry: Arc<RwLock<ProcedureRegistry>>) -> Self {
        Self {
            registry,
            tool_definitions: RwLock::new(Vec::new()),
        }
    }

    /// Create a dispatcher with an initial set of tool definitions.
    pub fn with_tools(
        registry: Arc<RwLock<ProcedureRegistry>>,
        tools: Vec<ToolDefinition>,
    ) -> Self {
        Self {
            registry,
            tool_definitions: RwLock::new(tools),
        }
    }

    /// Add a tool definition dynamically.
    pub async fn add_tool_definition(&self, tool: ToolDefinition) {
        self.tool_definitions.write().await.push(tool);
    }

    /// Replace all tool definitions.
    pub async fn set_tool_definitions(&self, tools: Vec<ToolDefinition>) {
        *self.tool_definitions.write().await = tools;
    }
}

#[async_trait]
impl ToolDispatcher for SpineProcedureDispatcher {
    async fn available_tools(&self) -> Vec<ToolDefinition> {
        self.tool_definitions.read().await.clone()
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> String {
        let registry = self.registry.read().await;

        // Find the first procedure that handles this tool name
        let handler = match registry.matching(name).next() {
            Some(h) => h,
            None => {
                warn!(
                    tool = name,
                    "spine dispatcher: no procedure registered for tool"
                );
                return format!("Error: no procedure registered for tool '{name}'");
            }
        };

        debug!(
            tool = name,
            "spine dispatcher: executing tool via procedure"
        );

        // Create a tool-invocation event with arguments as content
        let event = Event::Message {
            id: Uuid::new_v4().to_string(),
            channel: "tool".into(),
            sender: "model".into(),
            content: arguments.to_string(),
        };

        // Execute and collect results
        let results = handler.execute(&event).await;

        for result in results {
            if let Event::ToolResult {
                content, is_error, ..
            } = result
            {
                if is_error {
                    return format!("Tool error: {content}");
                }
                return content;
            }
        }

        // No ToolResult event returned — procedure may have emitted nothing
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::procedure::Procedure;
    use async_trait::async_trait;
    use serde_json::json;

    /// A test procedure that echoes back its arguments.
    struct EchoProcedure;

    #[async_trait]
    impl Procedure for EchoProcedure {
        fn name(&self) -> &str {
            "echo"
        }

        fn handles(&self) -> &str {
            "echo"
        }

        async fn execute(&self, event: &Event) -> Vec<Event> {
            let content = match event {
                Event::Message { content, .. } => content.clone(),
                _ => "unexpected event".into(),
            };
            vec![Event::ToolResult {
                tool_call_id: "test".into(),
                tool_name: "echo".into(),
                content: format!("echoed: {content}"),
                is_error: false,
            }]
        }
    }

    /// A test procedure that always returns an error.
    struct FailingProcedure;

    #[async_trait]
    impl Procedure for FailingProcedure {
        fn name(&self) -> &str {
            "failing"
        }

        fn handles(&self) -> &str {
            "failing_tool"
        }

        async fn execute(&self, _event: &Event) -> Vec<Event> {
            vec![Event::ToolResult {
                tool_call_id: "test".into(),
                tool_name: "failing_tool".into(),
                content: "something went wrong".into(),
                is_error: true,
            }]
        }
    }

    /// A test procedure that returns no events.
    struct SilentProcedure;

    #[async_trait]
    impl Procedure for SilentProcedure {
        fn name(&self) -> &str {
            "silent"
        }

        fn handles(&self) -> &str {
            "silent_tool"
        }

        async fn execute(&self, _event: &Event) -> Vec<Event> {
            vec![]
        }
    }

    fn make_registry_with_echo() -> Arc<RwLock<ProcedureRegistry>> {
        let mut registry = ProcedureRegistry::new();
        registry.register(Box::new(EchoProcedure));
        registry.register(Box::new(FailingProcedure));
        registry.register(Box::new(SilentProcedure));
        Arc::new(RwLock::new(registry))
    }

    #[tokio::test]
    async fn dispatches_to_matching_procedure() {
        let registry = make_registry_with_echo();
        let dispatcher = SpineProcedureDispatcher::new(registry);

        let result = dispatcher.call_tool("echo", json!({"msg": "hello"})).await;
        assert!(result.contains("echoed:"));
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn returns_error_for_unknown_tool() {
        let registry = make_registry_with_echo();
        let dispatcher = SpineProcedureDispatcher::new(registry);

        let result = dispatcher.call_tool("nonexistent", json!({})).await;
        assert!(result.contains("no procedure registered"));
        assert!(result.contains("nonexistent"));
    }

    #[tokio::test]
    async fn returns_tool_error_when_procedure_fails() {
        let registry = make_registry_with_echo();
        let dispatcher = SpineProcedureDispatcher::new(registry);

        let result = dispatcher.call_tool("failing_tool", json!({})).await;
        assert!(result.contains("Tool error:"));
        assert!(result.contains("something went wrong"));
    }

    #[tokio::test]
    async fn returns_empty_string_when_procedure_emits_nothing() {
        let registry = make_registry_with_echo();
        let dispatcher = SpineProcedureDispatcher::new(registry);

        let result = dispatcher.call_tool("silent_tool", json!({})).await;
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn available_tools_returns_registered_definitions() {
        let registry = make_registry_with_echo();
        let tools = vec![
            ToolDefinition {
                name: "echo".into(),
                description: "Echo input back".into(),
                parameters: json!({"type": "object"}),
            },
            ToolDefinition {
                name: "read_file".into(),
                description: "Read a file".into(),
                parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            },
        ];
        let dispatcher = SpineProcedureDispatcher::with_tools(registry, tools);

        let available = dispatcher.available_tools().await;
        assert_eq!(available.len(), 2);
        assert_eq!(available[0].name, "echo");
        assert_eq!(available[1].name, "read_file");
    }

    #[tokio::test]
    async fn add_tool_definition_dynamically() {
        let registry = make_registry_with_echo();
        let dispatcher = SpineProcedureDispatcher::new(registry);

        assert_eq!(dispatcher.available_tools().await.len(), 0);

        dispatcher
            .add_tool_definition(ToolDefinition {
                name: "new_tool".into(),
                description: "A dynamically added tool".into(),
                parameters: json!({"type": "object"}),
            })
            .await;

        let tools = dispatcher.available_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "new_tool");
    }

    #[tokio::test]
    async fn set_tool_definitions_replaces_all() {
        let registry = make_registry_with_echo();
        let initial = vec![ToolDefinition {
            name: "old".into(),
            description: "Old tool".into(),
            parameters: json!({}),
        }];
        let dispatcher = SpineProcedureDispatcher::with_tools(registry, initial);

        assert_eq!(dispatcher.available_tools().await.len(), 1);

        dispatcher
            .set_tool_definitions(vec![
                ToolDefinition {
                    name: "new_a".into(),
                    description: "A".into(),
                    parameters: json!({}),
                },
                ToolDefinition {
                    name: "new_b".into(),
                    description: "B".into(),
                    parameters: json!({}),
                },
            ])
            .await;

        let tools = dispatcher.available_tools().await;
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "new_a");
        assert_eq!(tools[1].name, "new_b");
    }
}
