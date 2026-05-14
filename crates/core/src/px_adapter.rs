//! Bridge between compiled `.px` procedures and the core [`Procedure`] trait.
//!
//! This module provides [`PxProcedureAdapter`], which wraps a compiled `.px`
//! procedure record (from `pares-agens-praxis`) and implements the core
//! [`Procedure`] trait so that `.px` procedures can be registered in the
//! [`ProcedureRegistry`] and dispatched through the normal event system.
//!
//! # Architecture
//!
//! ```text
//! .px source → parser → compiler → CompiledRecord
//!                                       ↓
//!                              PxProcedureAdapter
//!                                       ↓
//!                              ProcedureRegistry.register()
//!                                       ↓
//!                              Event dispatch loop
//! ```
//!
//! # Action Handler Integration
//!
//! The adapter requires an [`AsyncActionHandler`] implementation that bridges
//! the synchronous [`pares_agens_praxis::px::executor::ActionHandler`] into
//! the async world of the core event loop. This is the integration point
//! where tool calls, model invocations, and state mutations are wired in.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::event::Event;
use crate::procedure::Procedure;
use pares_agens_praxis::px::executor::{self, ActionHandler, ExecutionError, ExecutionResult};

// ── Async Action Handler ─────────────────────────────────────────────────────

/// Async version of the praxis [`ActionHandler`] trait.
///
/// Implementors provide the actual side-effects (tool invocations, model calls,
/// state mutations) that procedure steps reference by name. This trait is
/// async-native, unlike the underlying executor's synchronous trait.
#[async_trait]
pub trait AsyncActionHandler: Send + Sync {
    /// Execute a named action with the given parameters.
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError>;

    /// Evaluate a condition expression against the current execution context.
    fn evaluate_condition(&self, expr: &str, vars: &HashMap<String, Value>) -> bool {
        // Default: delegate to the executor's built-in evaluator
        executor::default_evaluate_condition(expr, vars)
    }

    /// Convert an [`Event`] into initial variable bindings for procedure execution.
    ///
    /// The default implementation extracts common fields from the event into
    /// a flat variable map that `.px` steps can reference via `$var` syntax.
    fn event_to_vars(&self, event: &Event) -> HashMap<String, Value> {
        default_event_to_vars(event)
    }
}

/// Default event-to-variables extraction.
///
/// Maps common event fields into procedure variable bindings:
/// - `$event_kind` — the event kind string
/// - `$channel` — channel name (for Message events)
/// - `$sender` — sender identifier (for Message events)
/// - `$content` — message content (for Message events)
/// - `$timer_name` — timer name (for Timer events)
/// - `$key` — state key (for StateChange events)
/// - `$new_value` — new value (for StateChange events)
pub fn default_event_to_vars(event: &Event) -> HashMap<String, Value> {
    let mut vars = HashMap::new();
    vars.insert("event_kind".to_string(), Value::String(event.kind().to_string()));

    match event {
        Event::Message { id, channel, sender, content } => {
            vars.insert("message_id".to_string(), Value::String(id.clone()));
            vars.insert("channel".to_string(), Value::String(channel.clone()));
            vars.insert("sender".to_string(), Value::String(sender.clone()));
            vars.insert("content".to_string(), Value::String(content.clone()));
        }
        Event::Timer { id, name, recurring } => {
            vars.insert("timer_id".to_string(), Value::String(id.clone()));
            vars.insert("timer_name".to_string(), Value::String(name.clone()));
            vars.insert("recurring".to_string(), Value::Bool(*recurring));
        }
        Event::StateChange { key, old_value, new_value } => {
            vars.insert("key".to_string(), Value::String(key.clone()));
            if let Some(old) = old_value {
                vars.insert("old_value".to_string(), old.clone());
            }
            vars.insert("new_value".to_string(), new_value.clone());
        }
        Event::ModelResponse { .. } => {
            // ModelResponse fields are opaque; pass the event kind only
        }
        Event::ToolResult { .. } => {
            // ToolResult fields are opaque; pass the event kind only
        }
        _ => {}
    }

    vars
}

// ── Blocking Action Handler Wrapper ──────────────────────────────────────────

/// Wraps an [`AsyncActionHandler`] for use with the synchronous executor.
///
/// Uses `tokio::runtime::Handle::block_on` to bridge async calls into the
/// synchronous [`ActionHandler`] trait. This is safe because the executor
/// runs on a blocking task (see [`PxProcedureAdapter::execute`]).
struct BlockingHandlerWrapper {
    inner: Arc<dyn AsyncActionHandler>,
    rt: tokio::runtime::Handle,
}

impl ActionHandler for BlockingHandlerWrapper {
    fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        self.rt.block_on(self.inner.call(name, params))
    }

    fn evaluate_condition(&self, expr: &str, vars: &HashMap<String, Value>) -> bool {
        self.inner.evaluate_condition(expr, vars)
    }
}

// ── PxProcedureAdapter ───────────────────────────────────────────────────────

/// Adapter that wraps a compiled `.px` procedure as a core [`Procedure`].
///
/// # Usage
///
/// ```no_run
/// use pares_agens_core::px_adapter::{PxProcedureAdapter, AsyncActionHandler};
/// use pares_agens_praxis::px::{parse, compiler::compile};
///
/// let source = r#"procedure on_message:
///   trigger: message
///   classify_intent {content: $content} -> $intent
/// "#;
///
/// let doc = parse(source).unwrap();
/// let records = compile(&doc);
///
/// // Find procedure records
/// for record in records.iter().filter(|r| r.key.starts_with("px:procedure/")) {
///     let adapter = PxProcedureAdapter::from_compiled(
///         record.data.clone(),
///         handler.clone(), // Arc<dyn AsyncActionHandler>
///     ).unwrap();
///     registry.register(Box::new(adapter));
/// }
/// ```
pub struct PxProcedureAdapter {
    /// Procedure name (from compiled record).
    name: String,
    /// Event kind this procedure handles (from trigger.kind).
    trigger_kind: String,
    /// The compiled procedure data (JSON).
    compiled: Value,
    /// The async action handler that provides side-effects.
    handler: Arc<dyn AsyncActionHandler>,
}

impl PxProcedureAdapter {
    /// Create an adapter from a compiled procedure record.
    ///
    /// Returns `None` if the record is not a valid procedure or lacks a trigger.
    pub fn from_compiled(
        data: Value,
        handler: Arc<dyn AsyncActionHandler>,
    ) -> Option<Self> {
        let record_type = data.get("type")?.as_str()?;
        if record_type != "procedure" {
            return None;
        }

        let name = data.get("name")?.as_str()?.to_string();

        // Extract trigger kind; default to "manual" if no trigger specified
        let trigger_kind = data
            .get("trigger")
            .and_then(|t| t.get("kind"))
            .and_then(|k| k.as_str())
            .unwrap_or("manual")
            .to_string();

        Some(Self {
            name,
            trigger_kind,
            compiled: data,
            handler,
        })
    }

    /// The compiled procedure data (for introspection/debugging).
    pub fn compiled_data(&self) -> &Value {
        &self.compiled
    }

    /// Execute the procedure with explicit initial variables.
    ///
    /// This is useful for testing or when the caller wants to provide
    /// custom variable bindings beyond what `event_to_vars` produces.
    pub async fn execute_with_vars(
        &self,
        vars: HashMap<String, Value>,
    ) -> Result<ExecutionResult, ExecutionError> {
        let handler = self.handler.clone();
        let compiled = self.compiled.clone();

        // Run the synchronous executor on a blocking task to avoid
        // blocking the async runtime.
        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            let wrapper = BlockingHandlerWrapper { inner: handler, rt };
            executor::execute_with_vars(&compiled, &wrapper, vars)
        })
        .await
        .map_err(|e| ExecutionError::ActionFailed {
            action: "spawn_blocking".to_string(),
            message: e.to_string(),
        })?;

        result
    }
}

#[async_trait]
impl Procedure for PxProcedureAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn handles(&self) -> &str {
        &self.trigger_kind
    }

    async fn execute(&self, event: &Event) -> Vec<Event> {
        let vars = self.handler.event_to_vars(event);

        match self.execute_with_vars(vars).await {
            Ok(result) => {
                if result.success {
                    // Procedures can emit events by returning them as step outputs.
                    // Convention: steps that produce events store them in a
                    // variable named `$emit` as a JSON array of event objects.
                    if let Some(emit_val) = result.variables.get("emit") {
                        if let Some(events) = parse_emitted_events(emit_val) {
                            return events;
                        }
                    }
                    vec![]
                } else {
                    // Log execution failure but don't crash the event loop.
                    // In production this would go through the telemetry system.
                    tracing::warn!(
                        procedure = %self.name,
                        error = ?result.error,
                        "px procedure execution failed"
                    );
                    vec![]
                }
            }
            Err(err) => {
                tracing::error!(
                    procedure = %self.name,
                    error = %err,
                    "px procedure executor error"
                );
                vec![]
            }
        }
    }
}

/// Attempt to parse emitted events from a procedure's `$emit` variable.
///
/// Events are expected as a JSON array of objects with a `"type"` field
/// matching the [`Event`] enum variants.
fn parse_emitted_events(value: &Value) -> Option<Vec<Event>> {
    let arr = value.as_array()?;
    let mut events = Vec::new();
    for item in arr {
        if let Ok(event) = serde_json::from_value::<Event>(item.clone()) {
            events.push(event);
        }
    }
    Some(events)
}

// ── Loader Utility ───────────────────────────────────────────────────────────

/// Load all `.px` procedures from source text and return adapters.
///
/// This is a convenience function for loading `.px` files at startup.
/// It parses the source, compiles it, and wraps each procedure record
/// in a [`PxProcedureAdapter`].
pub fn load_px_procedures(
    source: &str,
    handler: Arc<dyn AsyncActionHandler>,
) -> Result<Vec<PxProcedureAdapter>, String> {
    let doc = pares_agens_praxis::px::parse(source)
        .map_err(|e| format!("parse error: {e}"))?;

    let records = pares_agens_praxis::px::compiler::compile(&doc);

    let adapters: Vec<PxProcedureAdapter> = records
        .into_iter()
        .filter(|r| r.key.starts_with("px:procedure/"))
        .filter_map(|r| PxProcedureAdapter::from_compiled(r.data, handler.clone()))
        .collect();

    Ok(adapters)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A test handler that returns predefined results for known actions.
    struct TestHandler {
        results: HashMap<String, Value>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                results: HashMap::new(),
            }
        }

        fn with_result(mut self, action: &str, result: Value) -> Self {
            self.results.insert(action.to_string(), result);
            self
        }
    }

    #[async_trait]
    impl AsyncActionHandler for TestHandler {
        async fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            self.results
                .get(name)
                .cloned()
                .ok_or_else(|| ExecutionError::UnknownAction(name.to_string()))
        }
    }

    #[test]
    fn from_compiled_extracts_name_and_trigger() {
        let data = json!({
            "type": "procedure",
            "name": "on_message",
            "trigger": { "kind": "message" },
            "steps": []
        });

        let handler: Arc<dyn AsyncActionHandler> = Arc::new(TestHandler::new());
        let adapter = PxProcedureAdapter::from_compiled(data, handler).unwrap();

        assert_eq!(adapter.name(), "on_message");
        assert_eq!(adapter.handles(), "message");
    }

    #[test]
    fn from_compiled_defaults_trigger_to_manual() {
        let data = json!({
            "type": "procedure",
            "name": "deploy",
            "steps": []
        });

        let handler: Arc<dyn AsyncActionHandler> = Arc::new(TestHandler::new());
        let adapter = PxProcedureAdapter::from_compiled(data, handler).unwrap();

        assert_eq!(adapter.handles(), "manual");
    }

    #[test]
    fn from_compiled_rejects_non_procedure() {
        let data = json!({
            "type": "rule",
            "name": "some_rule"
        });

        let handler: Arc<dyn AsyncActionHandler> = Arc::new(TestHandler::new());
        assert!(PxProcedureAdapter::from_compiled(data, handler).is_none());
    }

    #[tokio::test]
    async fn execute_runs_procedure_steps() {
        let data = json!({
            "type": "procedure",
            "name": "greet",
            "trigger": { "kind": "message" },
            "steps": [
                { "kind": "call", "name": "say_hello", "params": {}, "output_var": "greeting" }
            ]
        });

        let handler: Arc<dyn AsyncActionHandler> = Arc::new(
            TestHandler::new().with_result("say_hello", json!("hello world")),
        );
        let adapter = PxProcedureAdapter::from_compiled(data, handler).unwrap();

        let event = Event::Message {
            id: "msg-1".to_string(),
            channel: "test".to_string(),
            sender: "user".to_string(),
            content: "hi".to_string(),
        };

        let emitted = adapter.execute(&event).await;
        // No explicit $emit, so no events emitted
        assert!(emitted.is_empty());
    }

    #[tokio::test]
    async fn execute_with_vars_returns_result() {
        let data = json!({
            "type": "procedure",
            "name": "check",
            "trigger": { "kind": "manual" },
            "steps": [
                { "kind": "call", "name": "do_check", "params": {}, "output_var": "status" }
            ]
        });

        let handler: Arc<dyn AsyncActionHandler> = Arc::new(
            TestHandler::new().with_result("do_check", json!("green")),
        );
        let adapter = PxProcedureAdapter::from_compiled(data, handler).unwrap();

        let vars = HashMap::new();
        let result = adapter.execute_with_vars(vars).await.unwrap();
        assert!(result.success);
        assert_eq!(result.variables.get("status"), Some(&json!("green")));
    }

    #[test]
    fn default_event_to_vars_extracts_message_fields() {
        let event = Event::Message {
            id: "m1".to_string(),
            channel: "telegram".to_string(),
            sender: "alice".to_string(),
            content: "hello".to_string(),
        };

        let vars = default_event_to_vars(&event);
        assert_eq!(vars["event_kind"], json!("message"));
        assert_eq!(vars["channel"], json!("telegram"));
        assert_eq!(vars["sender"], json!("alice"));
        assert_eq!(vars["content"], json!("hello"));
    }

    #[test]
    fn load_px_procedures_parses_and_wraps() {
        let source = "procedure health_check:\n  trigger: manual\n  check_health {} -> $status\n";
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(
            TestHandler::new().with_result("check_health", json!("ok")),
        );

        let adapters = load_px_procedures(source, handler).unwrap();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].name(), "health_check");
        assert_eq!(adapters[0].handles(), "manual");
    }
}
