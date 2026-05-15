//! Async procedure executor — runs compiled `.px` procedures with async action handlers.
//!
//! This is the production-ready executor that supports real tool calls (shell commands,
//! HTTP requests, model invocations) which are inherently asynchronous.
//!
//! # Architecture
//!
//! The async executor mirrors the synchronous [`super::executor`] but uses
//! [`AsyncActionHandler`] which returns futures. This allows procedures to:
//!
//! - Invoke shell commands and wait for output
//! - Make HTTP/API calls
//! - Call language models
//! - Execute MCP tool calls
//!
//! The executor supports optional per-step timeouts via the `timeout_ms` field
//! on call steps.
//!
//! # Example
//!
//! ```rust,ignore
//! use pares_agens_praxis::px::async_executor::{AsyncActionHandler, execute_async};
//!
//! struct MyHandler;
//!
//! #[async_trait::async_trait]
//! impl AsyncActionHandler for MyHandler {
//!     async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
//!         // invoke real tools here
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::time::timeout;

use super::executor::{
    default_evaluate_condition, ExecutionError, ExecutionResult, StepResult,
};

// ── Async Action Handler Trait ────────────────────────────────────────────────

/// Async trait for handling procedure step calls.
///
/// This is the production integration point. Implementors provide async
/// side-effects (tool invocations, API calls, model calls) that procedure
/// steps reference by name.
#[async_trait]
pub trait AsyncActionHandler: Send + Sync {
    /// Execute a named action with the given parameters asynchronously.
    ///
    /// Returns a JSON value representing the result, which may be bound to
    /// an output variable for subsequent steps.
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError>;

    /// Evaluate a condition expression against the current execution context.
    ///
    /// Default implementation uses the synchronous evaluator. Override for
    /// async condition evaluation (e.g., checking external state).
    fn evaluate_condition(&self, expr: &str, vars: &HashMap<String, Value>) -> bool {
        default_evaluate_condition(expr, vars)
    }

    /// Called before each step executes. Useful for logging, tracing, or
    /// implementing step-level hooks.
    async fn on_step_start(&self, _step_index: usize, _kind: &str) {}

    /// Called after each step completes. Receives the result for inspection.
    async fn on_step_complete(&self, _step_index: usize, _result: &StepResult) {}
}

/// Default step timeout (30 seconds). Individual steps can override via `timeout_ms`.
const DEFAULT_STEP_TIMEOUT_MS: u64 = 30_000;

/// Maximum loop iterations to prevent infinite loops in procedures.
const MAX_LOOP_ITERATIONS: usize = 10_000;

// ── Async Executor ────────────────────────────────────────────────────────────

/// Execute a compiled procedure record asynchronously.
///
/// This is the main entry point for running procedures with real (async) tools.
pub async fn execute_async(
    record_data: &Value,
    handler: &dyn AsyncActionHandler,
) -> Result<ExecutionResult, ExecutionError> {
    execute_async_with_vars(record_data, handler, HashMap::new()).await
}

/// Execute a compiled procedure record asynchronously with pre-seeded variables.
pub async fn execute_async_with_vars(
    record_data: &Value,
    handler: &dyn AsyncActionHandler,
    initial_vars: HashMap<String, Value>,
) -> Result<ExecutionResult, ExecutionError> {
    let procedure_name = record_data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let steps = record_data
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ExecutionError::InvalidStructure("missing or non-array 'steps' field".into())
        })?;

    let mut vars = initial_vars;
    let mut step_results = Vec::new();

    for (index, step) in steps.iter().enumerate() {
        let kind = step
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        handler.on_step_start(index, kind).await;
        let result = execute_step_async(step, index, &mut vars, handler).await?;
        handler.on_step_complete(index, &result).await;
        step_results.push(result);
    }

    Ok(ExecutionResult {
        procedure_name,
        step_results,
        variables: vars,
        success: true,
        error: None,
    })
}

/// Execute a single step asynchronously.
fn execute_step_async<'a>(
    step: &'a Value,
    index: usize,
    vars: &'a mut HashMap<String, Value>,
    handler: &'a dyn AsyncActionHandler,
) -> Pin<Box<dyn Future<Output = Result<StepResult, ExecutionError>> + Send + 'a>> {
    Box::pin(async move {
        let kind = step
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::InvalidStructure("step missing 'kind'".into()))?;

        match kind {
            "call" => execute_call_async(step, index, vars, handler).await,
            "match" => execute_match_async(step, index, vars, handler),
            "when" => execute_when_async(step, index, vars, handler).await,
            "loop" => execute_loop_async(step, index, vars, handler).await,
            "emit" => execute_emit_async(step, index, vars),
            "try" => execute_try_async(step, index, vars, handler).await,
            other => Err(ExecutionError::InvalidStructure(format!(
                "unknown step kind: {other}"
            ))),
        }
    })
}

/// Execute a `call` step asynchronously with optional timeout.
async fn execute_call_async(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn AsyncActionHandler,
) -> Result<StepResult, ExecutionError> {
    let name = step
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::InvalidStructure("call step missing 'name'".into()))?;

    let params = step.get("params").cloned().unwrap_or(Value::Null);
    let resolved_params = resolve_vars(&params, vars);

    // Check for step-level timeout
    let timeout_ms = step
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_STEP_TIMEOUT_MS);

    let output = timeout(
        Duration::from_millis(timeout_ms),
        handler.call(name, &resolved_params),
    )
    .await
    .map_err(|_| ExecutionError::ActionFailed {
        action: name.to_string(),
        message: format!("timed out after {timeout_ms}ms"),
    })??;

    // Bind output to variable if specified
    if let Some(output_var) = step.get("output_var").and_then(|v| v.as_str()) {
        if !output_var.is_empty() {
            vars.insert(output_var.to_string(), output.clone());
        }
    }

    Ok(StepResult {
        index,
        kind: "call".into(),
        output: Some(output),
        skipped: false,
    })
}

/// Execute a `match` step (synchronous — condition evaluation is sync).
fn execute_match_async(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn AsyncActionHandler,
) -> Result<StepResult, ExecutionError> {
    let arms = step
        .get("arms")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::InvalidStructure("match step missing 'arms'".into()))?;

    for arm in arms {
        let condition = arm
            .get("condition")
            .and_then(|v| v.as_str())
            .unwrap_or("true");

        if handler.evaluate_condition(condition, vars) {
            let result_val = arm.get("result").cloned().unwrap_or(Value::Null);
            return Ok(StepResult {
                index,
                kind: "match".into(),
                output: Some(result_val),
                skipped: false,
            });
        }
    }

    Ok(StepResult {
        index,
        kind: "match".into(),
        output: None,
        skipped: true,
    })
}

/// Execute a `when` step asynchronously.
async fn execute_when_async(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn AsyncActionHandler,
) -> Result<StepResult, ExecutionError> {
    let condition = step
        .get("condition")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::InvalidStructure("when step missing 'condition'".into()))?;

    if !handler.evaluate_condition(condition, vars) {
        return Ok(StepResult {
            index,
            kind: "when".into(),
            output: None,
            skipped: true,
        });
    }

    let nested_steps = step
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::InvalidStructure("when step missing 'steps'".into()))?;

    let mut last_output = None;
    for (i, nested) in nested_steps.iter().enumerate() {
        let result = execute_step_async(nested, i, vars, handler).await?;
        last_output = result.output;
    }

    Ok(StepResult {
        index,
        kind: "when".into(),
        output: last_output,
        skipped: false,
    })
}

/// Execute a `loop` step asynchronously.
async fn execute_loop_async(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn AsyncActionHandler,
) -> Result<StepResult, ExecutionError> {
    let nested_steps = step
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::InvalidStructure("loop step missing 'steps'".into()))?;

    let item_var = step
        .get("as")
        .and_then(|v| v.as_str())
        .unwrap_or("item");

    let output_var = step.get("output_var").and_then(|v| v.as_str());

    // Determine iteration source
    let iterations: Vec<Value> = if let Some(over_ref) = step.get("over").and_then(|v| v.as_str()) {
        let var_name = over_ref.strip_prefix('$').unwrap_or(over_ref);
        match vars.get(var_name) {
            Some(Value::Array(arr)) => arr.clone(),
            Some(other) => vec![other.clone()],
            None => {
                return Ok(StepResult {
                    index,
                    kind: "loop".into(),
                    output: None,
                    skipped: true,
                })
            }
        }
    } else if let Some(times) = step.get("times").and_then(|v| v.as_u64()) {
        (0..times).map(|i| Value::Number(i.into())).collect()
    } else {
        return Err(ExecutionError::InvalidStructure(
            "loop step requires 'over' or 'times'".into(),
        ));
    };

    // Guard against infinite loops
    if iterations.len() > MAX_LOOP_ITERATIONS {
        return Err(ExecutionError::ActionFailed {
            action: "loop".into(),
            message: format!(
                "loop iteration count {} exceeds maximum {}",
                iterations.len(),
                MAX_LOOP_ITERATIONS
            ),
        });
    }

    let mut results: Vec<Value> = Vec::new();

    for (iter_index, item) in iterations.into_iter().enumerate() {
        vars.insert(item_var.to_string(), item);
        vars.insert("index".to_string(), Value::Number(iter_index.into()));

        for nested in nested_steps {
            let result = execute_step_async(nested, iter_index, vars, handler).await?;
            if let Some(output) = &result.output {
                results.push(output.clone());
            }
        }
    }

    // Clean up loop variables
    vars.remove(item_var);
    vars.remove("index");

    let output = Value::Array(results);

    if let Some(out_var) = output_var {
        if !out_var.is_empty() {
            vars.insert(out_var.to_string(), output.clone());
        }
    }

    Ok(StepResult {
        index,
        kind: "loop".into(),
        output: Some(output),
        skipped: false,
    })
}

/// Execute an `emit` step (synchronous — just appends to variables).
fn execute_emit_async(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
) -> Result<StepResult, ExecutionError> {
    let event_data = step
        .get("event")
        .cloned()
        .ok_or_else(|| ExecutionError::InvalidStructure("emit step missing 'event'".into()))?;

    let resolved = resolve_vars(&event_data, vars);

    let emit_arr = vars
        .entry("emit".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));

    if let Value::Array(arr) = emit_arr {
        arr.push(resolved.clone());
    }

    Ok(StepResult {
        index,
        kind: "emit".into(),
        output: Some(resolved),
        skipped: false,
    })
}

/// Execute a `try` step asynchronously with error recovery.
async fn execute_try_async(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn AsyncActionHandler,
) -> Result<StepResult, ExecutionError> {
    let try_steps = step
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::InvalidStructure("try step missing 'steps'".into()))?;

    let catch_steps = step.get("catch").and_then(|v| v.as_array());

    for (i, nested) in try_steps.iter().enumerate() {
        match execute_step_async(nested, i, vars, handler).await {
            Ok(_result) => { /* continue */ }
            Err(err) => {
                vars.insert("error".to_string(), Value::String(err.to_string()));

                if let Some(catch) = catch_steps {
                    let mut last_output = None;
                    for (j, catch_step) in catch.iter().enumerate() {
                        let result = execute_step_async(catch_step, j, vars, handler).await?;
                        last_output = result.output;
                    }
                    return Ok(StepResult {
                        index,
                        kind: "try".into(),
                        output: last_output,
                        skipped: false,
                    });
                }

                return Ok(StepResult {
                    index,
                    kind: "try".into(),
                    output: Some(Value::String(err.to_string())),
                    skipped: false,
                });
            }
        }
    }

    vars.remove("error");
    Ok(StepResult {
        index,
        kind: "try".into(),
        output: None,
        skipped: false,
    })
}

// ── Variable Resolution ───────────────────────────────────────────────────────

/// Resolve variable references (`$var_name`) in a JSON value tree.
fn resolve_vars(value: &Value, vars: &HashMap<String, Value>) -> Value {
    match value {
        Value::String(s) if s.starts_with('$') => {
            let var_name = &s[1..];
            vars.get(var_name).cloned().unwrap_or_else(|| value.clone())
        }
        Value::Object(map) => {
            let resolved: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), resolve_vars(v, vars)))
                .collect();
            Value::Object(resolved)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_vars(v, vars)).collect()),
        other => other.clone(),
    }
}

// ── Adapter: Wrap sync handler as async ───────────────────────────────────────

use super::executor::ActionHandler;

/// Wraps a synchronous [`ActionHandler`] into an [`AsyncActionHandler`].
///
/// Useful for testing or when all actions are CPU-bound.
pub struct SyncAdapter<H: ActionHandler>(pub H);

#[async_trait]
impl<H: ActionHandler + 'static> AsyncActionHandler for SyncAdapter<H> {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        self.0.call(name, params)
    }

    fn evaluate_condition(&self, expr: &str, vars: &HashMap<String, Value>) -> bool {
        self.0.evaluate_condition(expr, vars)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Mock async handler for testing.
    struct MockAsyncHandler {
        results: HashMap<String, Value>,
    }

    impl MockAsyncHandler {
        fn new() -> Self {
            Self {
                results: HashMap::new(),
            }
        }

        fn with_result(mut self, name: &str, value: Value) -> Self {
            self.results.insert(name.to_string(), value);
            self
        }
    }

    #[async_trait]
    impl AsyncActionHandler for MockAsyncHandler {
        async fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            self.results
                .get(name)
                .cloned()
                .ok_or_else(|| ExecutionError::UnknownAction(name.to_string()))
        }
    }

    #[tokio::test]
    async fn execute_simple_procedure() {
        let handler = MockAsyncHandler::new()
            .with_result("greet", json!("hello"));

        let procedure = json!({
            "type": "procedure",
            "name": "test_proc",
            "steps": [
                { "kind": "call", "name": "greet", "params": {} , "output_var": "result" }
            ]
        });

        let result = execute_async(&procedure, &handler).await.unwrap();
        assert!(result.success);
        assert_eq!(result.procedure_name, "test_proc");
        assert_eq!(result.variables.get("result"), Some(&json!("hello")));
    }

    #[tokio::test]
    async fn execute_with_timeout() {
        struct SlowHandler;

        #[async_trait]
        impl AsyncActionHandler for SlowHandler {
            async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(json!("too late"))
            }
        }

        let procedure = json!({
            "type": "procedure",
            "name": "timeout_test",
            "steps": [
                { "kind": "call", "name": "slow_action", "params": {}, "timeout_ms": 50 }
            ]
        });

        let result = execute_async(&procedure, &SlowHandler).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutionError::ActionFailed { action, message } => {
                assert_eq!(action, "slow_action");
                assert!(message.contains("timed out"));
            }
            other => panic!("expected ActionFailed, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn execute_loop_with_async_calls() {
        let handler = MockAsyncHandler::new()
            .with_result("get_items", json!(["a", "b", "c"]))
            .with_result("process", json!("done"));

        let procedure = json!({
            "type": "procedure",
            "name": "loop_test",
            "steps": [
                { "kind": "call", "name": "get_items", "params": {}, "output_var": "items" },
                {
                    "kind": "loop",
                    "over": "$items",
                    "as": "item",
                    "output_var": "results",
                    "steps": [
                        { "kind": "call", "name": "process", "params": { "val": "$item" } }
                    ]
                }
            ]
        });

        let result = execute_async(&procedure, &handler).await.unwrap();
        assert!(result.success);
        assert_eq!(
            result.variables.get("results"),
            Some(&json!(["done", "done", "done"]))
        );
    }

    #[tokio::test]
    async fn execute_try_catch_async() {
        let handler = MockAsyncHandler::new()
            .with_result("fallback", json!("recovered"));

        let procedure = json!({
            "type": "procedure",
            "name": "try_test",
            "steps": [
                {
                    "kind": "try",
                    "steps": [
                        { "kind": "call", "name": "nonexistent", "params": {} }
                    ],
                    "catch": [
                        { "kind": "call", "name": "fallback", "params": {} }
                    ]
                }
            ]
        });

        let result = execute_async(&procedure, &handler).await.unwrap();
        assert!(result.success);
        assert_eq!(result.step_results[0].output, Some(json!("recovered")));
    }

    #[tokio::test]
    async fn execute_when_condition() {
        let handler = MockAsyncHandler::new()
            .with_result("action_a", json!("a_result"));

        let procedure = json!({
            "type": "procedure",
            "name": "when_test",
            "steps": [
                {
                    "kind": "when",
                    "condition": "mode == fast",
                    "steps": [
                        { "kind": "call", "name": "action_a", "params": {} }
                    ]
                }
            ]
        });

        // Without the variable set — should skip
        let result = execute_async(&procedure, &handler).await.unwrap();
        assert!(result.step_results[0].skipped);

        // With the variable set — should execute
        let mut vars = HashMap::new();
        vars.insert("mode".to_string(), json!("fast"));
        let result = execute_async_with_vars(&procedure, &handler, vars).await.unwrap();
        assert!(!result.step_results[0].skipped);
        assert_eq!(result.step_results[0].output, Some(json!("a_result")));
    }

    #[tokio::test]
    async fn sync_adapter_works() {
        use super::super::executor::ActionHandler;

        struct SyncHandler;
        impl ActionHandler for SyncHandler {
            fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                match name {
                    "ping" => Ok(json!("pong")),
                    _ => Err(ExecutionError::UnknownAction(name.to_string())),
                }
            }
        }

        let handler = SyncAdapter(SyncHandler);

        let procedure = json!({
            "type": "procedure",
            "name": "adapter_test",
            "steps": [
                { "kind": "call", "name": "ping", "params": {}, "output_var": "reply" }
            ]
        });

        let result = execute_async(&procedure, &handler).await.unwrap();
        assert!(result.success);
        assert_eq!(result.variables.get("reply"), Some(&json!("pong")));
    }

    #[tokio::test]
    async fn loop_guard_prevents_excessive_iterations() {
        let handler = MockAsyncHandler::new()
            .with_result("noop", json!(null));

        let procedure = json!({
            "type": "procedure",
            "name": "bomb",
            "steps": [
                {
                    "kind": "loop",
                    "times": 100_001,
                    "as": "i",
                    "steps": [
                        { "kind": "call", "name": "noop", "params": {} }
                    ]
                }
            ]
        });

        let result = execute_async(&procedure, &handler).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutionError::ActionFailed { message, .. } => {
                assert!(message.contains("exceeds maximum"));
            }
            other => panic!("expected ActionFailed, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn step_hooks_called() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct HookHandler {
            start_count: AtomicUsize,
            complete_count: AtomicUsize,
        }

        #[async_trait]
        impl AsyncActionHandler for HookHandler {
            async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                Ok(json!("ok"))
            }

            async fn on_step_start(&self, _index: usize, _kind: &str) {
                self.start_count.fetch_add(1, Ordering::SeqCst);
            }

            async fn on_step_complete(&self, _index: usize, _result: &StepResult) {
                self.complete_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let handler = HookHandler {
            start_count: AtomicUsize::new(0),
            complete_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "hooks_test",
            "steps": [
                { "kind": "call", "name": "a", "params": {} },
                { "kind": "call", "name": "b", "params": {} },
                { "kind": "call", "name": "c", "params": {} }
            ]
        });

        let result = execute_async(&procedure, &handler).await.unwrap();
        assert!(result.success);
        assert_eq!(handler.start_count.load(Ordering::SeqCst), 3);
        assert_eq!(handler.complete_count.load(Ordering::SeqCst), 3);
    }
}
