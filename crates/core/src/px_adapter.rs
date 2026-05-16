//! Bridge between compiled `.px` procedures and the core [`Procedure`] trait.
//!
//! This module provides [`PxProcedureAdapter`], which wraps a compiled `.px`
//! procedure record (from `pares-radix-praxis`) and implements the core
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
//! the synchronous [`pares_radix_praxis::px::executor::ActionHandler`] into
//! the async world of the core event loop. This is the integration point
//! where tool calls, model invocations, and state mutations are wired in.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::event::Event;
use crate::procedure::Procedure;
use pares_radix_praxis::px::executor::{self, ActionHandler, ExecutionError, ExecutionResult};

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
    vars.insert(
        "event_kind".to_string(),
        Value::String(event.kind().to_string()),
    );

    match event {
        Event::Message {
            id,
            channel,
            sender,
            content,
        } => {
            vars.insert("message_id".to_string(), Value::String(id.clone()));
            vars.insert("channel".to_string(), Value::String(channel.clone()));
            vars.insert("sender".to_string(), Value::String(sender.clone()));
            vars.insert("content".to_string(), Value::String(content.clone()));
        }
        Event::Timer {
            id,
            name,
            recurring,
        } => {
            vars.insert("timer_id".to_string(), Value::String(id.clone()));
            vars.insert("timer_name".to_string(), Value::String(name.clone()));
            vars.insert("recurring".to_string(), Value::Bool(*recurring));
        }
        Event::StateChange {
            key,
            old_value,
            new_value,
        } => {
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
/// ```ignore
/// use pares_agens_core::px_adapter::{PxProcedureAdapter, AsyncActionHandler};
/// use pares_radix_praxis::px::{parse, compiler::compile};
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
    pub fn from_compiled(data: Value, handler: Arc<dyn AsyncActionHandler>) -> Option<Self> {
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

    /// Generate a [`ToolDefinition`] so the model can discover and call this
    /// procedure as a tool.
    ///
    /// The tool name is the procedure's trigger kind (which becomes the tool
    /// name in the dispatch path). The description is extracted from the
    /// compiled record's `description` field if present, otherwise generated
    /// from the procedure name.
    ///
    /// Parameters are derived from the procedure's first `call` step params
    /// (if any have `$`-prefixed values indicating expected inputs), or
    /// default to accepting a free-form JSON object.
    pub fn tool_definition(&self) -> crate::model::ToolDefinition {
        let description = self
            .compiled
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Execute the {} procedure", self.name));

        // Extract parameter hints from steps that reference $variables
        let params = self.infer_parameters();

        crate::model::ToolDefinition {
            name: self.trigger_kind.clone(),
            description,
            parameters: params,
        }
    }

    /// Infer a JSON Schema for the procedure's input parameters by scanning
    /// steps for `$variable` references that aren't produced by earlier steps.
    fn infer_parameters(&self) -> Value {
        use serde_json::json;

        let steps = match self.compiled.get("steps").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => return json!({"type": "object", "properties": {}}),
        };

        // Collect all $var references in params and all output_var bindings
        let mut inputs: Vec<String> = Vec::new();
        let mut outputs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for step in steps {
            // Track outputs
            if let Some(out) = step.get("output_var").and_then(|v| v.as_str()) {
                outputs.insert(out.to_string());
            }
            // Track $var references in params
            if let Some(params) = step.get("params") {
                collect_var_refs(params, &mut inputs);
            }
        }

        // Input parameters are $vars that aren't produced by earlier steps
        // (and aren't built-in event vars)
        let builtin_vars: std::collections::HashSet<&str> = [
            "event_kind",
            "channel",
            "sender",
            "content",
            "message_id",
            "timer_id",
            "timer_name",
            "recurring",
            "key",
            "old_value",
            "new_value",
        ]
        .into_iter()
        .collect();

        let mut properties = serde_json::Map::new();
        for var in &inputs {
            if !outputs.contains(var) && !builtin_vars.contains(var.as_str()) {
                properties.insert(
                    var.clone(),
                    json!({"type": "string", "description": format!("Input: {}", var)}),
                );
            }
        }

        json!({
            "type": "object",
            "properties": properties
        })
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
/// Recursively collect `$variable` references from a JSON value.
fn collect_var_refs(value: &Value, refs: &mut Vec<String>) {
    match value {
        Value::String(s) if s.starts_with('$') => {
            let var_name = s[1..].to_string();
            if !refs.contains(&var_name) {
                refs.push(var_name);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_var_refs(v, refs);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_var_refs(v, refs);
            }
        }
        _ => {}
    }
}

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

// ── Runtime Action Handler ────────────────────────────────────────────────────

/// An [`AsyncActionHandler`] that bridges `.px` procedure step calls to the
/// core [`ToolDispatcher`] interface.
///
/// This is the production integration point: when a `.px` procedure step calls
/// an action like `classify_intent {...}`, the `ToolDispatchActionHandler`
/// routes it through the same tool dispatch pipeline that model-initiated tool
/// calls use (governance, tracing, procedure lookup).
///
/// # Lazy Initialization
///
/// Due to circular dependencies (procedures need the dispatcher, dispatcher
/// needs the registry, registry holds the procedures), this handler supports
/// lazy initialization. Create it with [`new_lazy`], register `.px` procedures,
/// then call [`set_dispatcher`] once the tool dispatcher is available.
///
/// # Usage
///
/// ```ignore
/// use pares_agens_core::px_adapter::ToolDispatchActionHandler;
/// use pares_agens_core::model::ToolDispatcher;
///
/// // Phase 1: create lazy handler, load procedures
/// let handler = Arc::new(ToolDispatchActionHandler::new_lazy());
/// let procedures = load_px_procedures(source, handler.clone())?;
/// // ... register procedures in registry ...
///
/// // Phase 2: set dispatcher after registry is finalized
/// handler.set_dispatcher(tool_dispatcher);
/// ```
pub struct ToolDispatchActionHandler {
    dispatcher: std::sync::RwLock<Option<Arc<dyn crate::model::ToolDispatcher>>>,
}

impl ToolDispatchActionHandler {
    /// Create a handler backed by the given tool dispatcher.
    pub fn new(dispatcher: Arc<dyn crate::model::ToolDispatcher>) -> Self {
        Self {
            dispatcher: std::sync::RwLock::new(Some(dispatcher)),
        }
    }

    /// Create a handler without a dispatcher (for two-phase initialization).
    pub fn new_lazy() -> Self {
        Self {
            dispatcher: std::sync::RwLock::new(None),
        }
    }

    /// Set the dispatcher after construction. Call this once the tool
    /// dispatcher is available.
    pub fn set_dispatcher(&self, dispatcher: Arc<dyn crate::model::ToolDispatcher>) {
        let mut guard = self.dispatcher.write().unwrap();
        *guard = Some(dispatcher);
    }
}

#[async_trait]
impl AsyncActionHandler for ToolDispatchActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        let dispatcher = {
            let guard = self.dispatcher.read().unwrap();
            guard.clone()
        };

        let dispatcher = dispatcher.ok_or_else(|| ExecutionError::ActionFailed {
            action: name.to_string(),
            message: "tool dispatcher not yet initialized".to_string(),
        })?;

        let result_str = dispatcher.call_tool(name, params.clone()).await;

        // Try to parse the result as JSON; if it fails, wrap as a string value.
        match serde_json::from_str::<Value>(&result_str) {
            Ok(val) => Ok(val),
            Err(_) => Ok(Value::String(result_str)),
        }
    }
}

// ── Directory Loader ─────────────────────────────────────────────────────────

/// Load all `.px` files from a directory and return procedure adapters.
///
/// Walks the directory recursively, parses each `.px` file, and wraps
/// procedure records in [`PxProcedureAdapter`]s. Non-procedure records
/// (facts, rules) are silently skipped.
///
/// Returns the successfully loaded adapters and logs warnings for any
/// files that fail to parse.
pub fn load_px_directory(
    dir: &std::path::Path,
    handler: Arc<dyn AsyncActionHandler>,
) -> Vec<PxProcedureAdapter> {
    let mut adapters = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "failed to read .px directory");
            return adapters;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Recurse into subdirectories
            adapters.extend(load_px_directory(&path, handler.clone()));
        } else if path.extension().is_some_and(|ext| ext == "px") {
            match std::fs::read_to_string(&path) {
                Ok(source) => match load_px_procedures(&source, handler.clone()) {
                    Ok(loaded) => {
                        tracing::info!(
                            path = %path.display(),
                            count = loaded.len(),
                            "loaded .px procedures"
                        );
                        adapters.extend(loaded);
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "failed to compile .px file");
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to read .px file");
                }
            }
        }
    }

    adapters
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
    let doc = pares_radix_praxis::px::parse(source).map_err(|e| format!("parse error: {e}"))?;

    let records = pares_radix_praxis::px::compiler::compile(&doc);

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

        let handler: Arc<dyn AsyncActionHandler> =
            Arc::new(TestHandler::new().with_result("say_hello", json!("hello world")));
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

        let handler: Arc<dyn AsyncActionHandler> =
            Arc::new(TestHandler::new().with_result("do_check", json!("green")));
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
        let handler: Arc<dyn AsyncActionHandler> =
            Arc::new(TestHandler::new().with_result("check_health", json!("ok")));

        let adapters = load_px_procedures(source, handler).unwrap();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].name(), "health_check");
        assert_eq!(adapters[0].handles(), "manual");
    }

    #[test]
    fn tool_definition_generates_from_procedure() {
        let data = json!({
            "type": "procedure",
            "name": "summarize",
            "trigger": { "kind": "summarize" },
            "description": "Summarize the given text",
            "steps": [
                { "kind": "call", "name": "llm_complete", "params": { "text": "$input_text" }, "output_var": "summary" }
            ]
        });

        let handler: Arc<dyn AsyncActionHandler> =
            Arc::new(TestHandler::new().with_result("llm_complete", json!("done")));
        let adapter = PxProcedureAdapter::from_compiled(data, handler).unwrap();
        let tool_def = adapter.tool_definition();

        assert_eq!(tool_def.name, "summarize");
        assert_eq!(tool_def.description, "Summarize the given text");
        // input_text should be inferred as a parameter (not an output var)
        assert!(tool_def.parameters["properties"]["input_text"].is_object());
    }

    #[test]
    fn tool_definition_excludes_output_vars_and_builtins() {
        let data = json!({
            "type": "procedure",
            "name": "process_msg",
            "trigger": { "kind": "process_msg" },
            "steps": [
                { "kind": "call", "name": "classify", "params": { "msg": "$content" }, "output_var": "intent" },
                { "kind": "call", "name": "route", "params": { "intent": "$intent", "custom": "$user_pref" } }
            ]
        });

        let handler: Arc<dyn AsyncActionHandler> = Arc::new(
            TestHandler::new()
                .with_result("classify", json!("greeting"))
                .with_result("route", json!("routed")),
        );
        let adapter = PxProcedureAdapter::from_compiled(data, handler).unwrap();
        let tool_def = adapter.tool_definition();

        // $content is a builtin (from event), $intent is an output var — neither should appear
        assert!(tool_def.parameters["properties"].get("content").is_none());
        assert!(tool_def.parameters["properties"].get("intent").is_none());
        // $user_pref is a genuine input parameter
        assert!(tool_def.parameters["properties"]["user_pref"].is_object());
    }

    #[test]
    fn collect_var_refs_finds_nested_refs() {
        let value = json!({
            "top": "$alpha",
            "nested": { "deep": "$beta" },
            "arr": ["$gamma", "literal"],
            "plain": "no ref here"
        });

        let mut refs = Vec::new();
        collect_var_refs(&value, &mut refs);
        assert!(refs.contains(&"alpha".to_string()));
        assert!(refs.contains(&"beta".to_string()));
        assert!(refs.contains(&"gamma".to_string()));
        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn load_px_directory_handles_missing_dir() {
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(TestHandler::new());
        let adapters = load_px_directory(std::path::Path::new("/nonexistent/path"), handler);
        assert!(adapters.is_empty());
    }

    #[tokio::test]
    async fn tool_dispatch_handler_lazy_init() {
        use crate::model::{ToolDefinition, ToolDispatcher};

        /// Mock dispatcher for testing.
        struct MockDispatcher;

        #[async_trait]
        impl ToolDispatcher for MockDispatcher {
            async fn available_tools(&self) -> Vec<ToolDefinition> {
                vec![]
            }
            async fn call_tool(&self, _name: &str, _args: Value) -> String {
                r#"{"status": "ok"}"#.to_string()
            }
        }

        let handler = Arc::new(ToolDispatchActionHandler::new_lazy());

        // Before setting dispatcher, calls should fail
        let result = handler.call("test", &json!({})).await;
        assert!(result.is_err());

        // After setting dispatcher, calls should succeed
        handler.set_dispatcher(Arc::new(MockDispatcher));
        let result = handler.call("test", &json!({})).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), json!({"status": "ok"}));
    }
}
