//! Procedure executor — runs compiled `.px` procedures step by step.
//!
//! The executor takes a compiled procedure (as a `serde_json::Value` record
//! from the compiler) and walks its steps, resolving calls through a
//! pluggable [`ActionHandler`] trait, evaluating `when` guards, and
//! matching on conditions.
//!
//! # Architecture
//!
//! ```text
//! PxDocument ──► compiler ──► CompiledRecord (JSON) ──► Executor
//!                                                          │
//!                                                    ActionHandler
//!                                                    (pluggable)
//! ```
//!
//! The executor is intentionally model-agnostic: it doesn't know about LLMs,
//! HTTP, or any specific runtime. The [`ActionHandler`] trait is the
//! integration point where the host system (pares-agens core, MCP server,
//! etc.) provides concrete implementations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Action Handler Trait ──────────────────────────────────────────────────────

/// Trait for handling procedure step calls.
///
/// Implementors provide the actual side-effects (API calls, tool invocations,
/// state mutations) that procedure steps reference by name.
pub trait ActionHandler: Send + Sync {
    /// Execute a named action with the given parameters.
    ///
    /// Returns a JSON value representing the result, which may be bound to
    /// an output variable for subsequent steps.
    fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError>;

    /// Evaluate a condition expression against the current execution context.
    ///
    /// Returns `true` if the condition is satisfied. The default implementation
    /// does simple equality checks against the variable bindings passed in
    /// `vars`. Override for richer expression evaluation.
    fn evaluate_condition(&self, expr: &str, vars: &HashMap<String, Value>) -> bool {
        default_evaluate_condition(expr, vars)
    }
}

// ── Execution Types ───────────────────────────────────────────────────────────

/// The result of executing a procedure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Name of the procedure that was executed.
    pub procedure_name: String,
    /// Results of each step, in order.
    pub step_results: Vec<StepResult>,
    /// Final variable bindings after execution.
    pub variables: HashMap<String, Value>,
    /// Whether the procedure completed successfully.
    pub success: bool,
    /// Error message if the procedure failed.
    pub error: Option<String>,
}

/// The result of executing a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Which step index this corresponds to.
    pub index: usize,
    /// The kind of step that was executed.
    pub kind: String,
    /// The output value (if any).
    pub output: Option<Value>,
    /// Whether this step was skipped (e.g., `when` guard failed).
    pub skipped: bool,
}

/// Errors that can occur during procedure execution.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExecutionError {
    /// A called action is not registered in the handler.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// A called action failed.
    #[error("action '{action}' failed: {message}")]
    ActionFailed { action: String, message: String },

    /// The procedure record has an invalid structure.
    #[error("invalid procedure structure: {0}")]
    InvalidStructure(String),

    /// A variable referenced in a step was not bound.
    #[error("unbound variable: {0}")]
    UnboundVariable(String),

    /// A match step had no matching arm.
    #[error("no matching arm in match step")]
    NoMatchingArm,
}

// ── Executor ──────────────────────────────────────────────────────────────────

/// Executes a compiled procedure record.
///
/// The `record_data` parameter is the `data` field of a `CompiledRecord`
/// with `type: "procedure"`.
pub fn execute(
    record_data: &Value,
    handler: &dyn ActionHandler,
) -> Result<ExecutionResult, ExecutionError> {
    execute_with_vars(record_data, handler, HashMap::new())
}

/// Executes a compiled procedure record with pre-seeded variables.
///
/// Use this when the procedure is triggered with parameters that should
/// be available as variable bindings during execution.
pub fn execute_with_vars(
    record_data: &Value,
    handler: &dyn ActionHandler,
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
        let result = execute_step(step, index, &mut vars, handler)?;
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

/// Execute a single step within a procedure.
fn execute_step(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
) -> Result<StepResult, ExecutionError> {
    let kind = step
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::InvalidStructure("step missing 'kind'".into()))?;

    match kind {
        "call" => execute_call(step, index, vars, handler),
        "match" => execute_match(step, index, vars, handler),
        "when" => execute_when(step, index, vars, handler),
        "loop" => execute_loop(step, index, vars, handler),
        "emit" => execute_emit(step, index, vars, handler),
        "try" => execute_try(step, index, vars, handler),
        other => Err(ExecutionError::InvalidStructure(format!(
            "unknown step kind: {other}"
        ))),
    }
}

/// Execute a `call` step: invoke an action and optionally bind the result.
fn execute_call(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
) -> Result<StepResult, ExecutionError> {
    let name = step
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::InvalidStructure("call step missing 'name'".into()))?;

    let params = step.get("params").cloned().unwrap_or(Value::Null);

    // Resolve variable references in params
    let resolved_params = resolve_vars(&params, vars);

    let output = handler.call(name, &resolved_params)?;

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

/// Execute a `match` step: find the first arm whose condition is true.
fn execute_match(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
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

    // No arm matched — this is not necessarily an error for match steps.
    // Return a skipped result rather than failing hard.
    Ok(StepResult {
        index,
        kind: "match".into(),
        output: None,
        skipped: true,
    })
}

/// Execute a `when` step: run nested steps only if the condition holds.
fn execute_when(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
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

    // Condition met — execute nested steps
    let nested_steps = step
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::InvalidStructure("when step missing 'steps'".into()))?;

    let mut nested_results = Vec::new();
    for (i, nested) in nested_steps.iter().enumerate() {
        let result = execute_step(nested, i, vars, handler)?;
        nested_results.push(result);
    }

    // Return the last nested result as the when step's output
    let last_output = nested_results.last().and_then(|r| r.output.clone());

    Ok(StepResult {
        index,
        kind: "when".into(),
        output: last_output,
        skipped: false,
    })
}

/// Execute a `loop` step: iterate over an array or repeat N times.
///
/// Supports two modes:
/// - `over`: a `$variable` reference to an array; iterates each element
/// - `times`: a number; repeats nested steps that many times
///
/// The current item is bound to `$item` (configurable via `as` field),
/// and the index is bound to `$index`.
fn execute_loop(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
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
        // Resolve variable reference
        let var_name = over_ref.strip_prefix('$').unwrap_or(over_ref);
        match vars.get(var_name) {
            Some(Value::Array(arr)) => arr.clone(),
            Some(other) => vec![other.clone()], // single-item iteration
            None => return Ok(StepResult { index, kind: "loop".into(), output: None, skipped: true }),
        }
    } else if let Some(times) = step.get("times").and_then(|v| v.as_u64()) {
        (0..times).map(|i| Value::Number(i.into())).collect()
    } else {
        return Err(ExecutionError::InvalidStructure(
            "loop step requires 'over' or 'times'".into(),
        ));
    };

    let mut results: Vec<Value> = Vec::new();

    for (iter_index, item) in iterations.into_iter().enumerate() {
        vars.insert(item_var.to_string(), item);
        vars.insert("index".to_string(), Value::Number(iter_index.into()));

        for nested in nested_steps {
            let result = execute_step(nested, iter_index, vars, handler)?;
            if let Some(output) = &result.output {
                results.push(output.clone());
            }
        }
    }

    // Clean up loop variables
    vars.remove(item_var);
    vars.remove("index");

    let output = Value::Array(results);

    // Bind collected results if output_var specified
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

/// Execute an `emit` step: produce events for the event loop.
///
/// The emitted value is appended to the `$emit` variable (an array).
/// The adapter reads this variable after execution to dispatch events.
fn execute_emit(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    _handler: &dyn ActionHandler,
) -> Result<StepResult, ExecutionError> {
    let event_data = step
        .get("event")
        .cloned()
        .ok_or_else(|| ExecutionError::InvalidStructure("emit step missing 'event'".into()))?;

    // Resolve variable references in the event data
    let resolved = resolve_vars(&event_data, vars);

    // Append to $emit array
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

/// Execute a `try` step: run nested steps with error recovery.
///
/// If any nested step fails, the `catch` steps are executed instead.
/// The error is bound to `$error` for use in catch steps.
fn execute_try(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
) -> Result<StepResult, ExecutionError> {
    let try_steps = step
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::InvalidStructure("try step missing 'steps'".into()))?;

    let catch_steps = step
        .get("catch")
        .and_then(|v| v.as_array());

    // Attempt the try block
    for (i, nested) in try_steps.iter().enumerate() {
        match execute_step(nested, i, vars, handler) {
            Ok(_result) => { /* continue */ }
            Err(err) => {
                // Bind the error for catch steps
                vars.insert("error".to_string(), Value::String(err.to_string()));

                // Execute catch steps if they exist
                if let Some(catch) = catch_steps {
                    let mut last_output = None;
                    for (j, catch_step) in catch.iter().enumerate() {
                        let result = execute_step(catch_step, j, vars, handler)?;
                        last_output = result.output;
                    }
                    return Ok(StepResult {
                        index,
                        kind: "try".into(),
                        output: last_output,
                        skipped: false,
                    });
                }

                // No catch block — return error info but don't propagate
                return Ok(StepResult {
                    index,
                    kind: "try".into(),
                    output: Some(Value::String(err.to_string())),
                    skipped: false,
                });
            }
        }
    }

    // All try steps succeeded
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
///
/// Strings starting with `$` are looked up in the variables map. If not
/// found, the original string is preserved (no error — allows literal `$`
/// in params for forward-compatible use).
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
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| resolve_vars(v, vars)).collect())
        }
        other => other.clone(),
    }
}

// ── Default Condition Evaluator ───────────────────────────────────────────────

/// Evaluate a condition expression against variable bindings.
///
/// Supports:
/// - `true`, `false`, `_`, `default`, `else` — literals
/// - `var == value` — equality checks against bound variables
/// - `var != value` — inequality checks
/// - Dotted access (`result.status == ok`)
/// - Bare truthiness checks
pub fn default_evaluate_condition(expr: &str, vars: &HashMap<String, Value>) -> bool {
    let expr = expr.trim();

    match expr {
        "true" | "_" | "default" | "else" => return true,
        "false" => return false,
        _ => {}
    }

    // Try == comparison
    if let Some((lhs, rhs)) = expr.split_once("==") {
        let lhs = lhs.trim();
        let rhs = rhs.trim().trim_matches('"');

        if let Some(val) = vars.get(lhs) {
            return match val {
                Value::String(s) => s == rhs,
                Value::Number(n) => n.to_string() == rhs,
                Value::Bool(b) => b.to_string() == rhs,
                Value::Null => rhs == "null",
                _ => false,
            };
        }
        // Also check dotted access (e.g., "result.status")
        if let Some(val) = resolve_dotted(lhs, vars) {
            return match &val {
                Value::String(s) => s.as_str() == rhs,
                Value::Number(n) => n.to_string() == rhs,
                Value::Bool(b) => b.to_string() == rhs,
                Value::Null => rhs == "null",
                _ => false,
            };
        }
        return false;
    }

    // Try != comparison
    if let Some((lhs, rhs)) = expr.split_once("!=") {
        let lhs = lhs.trim();
        let rhs = rhs.trim().trim_matches('"');

        if let Some(val) = vars.get(lhs) {
            return match val {
                Value::String(s) => s != rhs,
                Value::Number(n) => n.to_string() != rhs,
                Value::Bool(b) => b.to_string() != rhs,
                Value::Null => rhs != "null",
                _ => true,
            };
        }
        if let Some(val) = resolve_dotted(lhs, vars) {
            return match &val {
                Value::String(s) => s.as_str() != rhs,
                Value::Number(n) => n.to_string() != rhs,
                Value::Bool(b) => b.to_string() != rhs,
                Value::Null => rhs != "null",
                _ => true,
            };
        }
        return true; // unbound var != anything is true
    }

    // Bare variable name — truthy check
    if let Some(val) = vars.get(expr) {
        return match val {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
            _ => true,
        };
    }

    false
}

/// Resolve dotted variable access (e.g., "result.status" looks up vars["result"]["status"]).
fn resolve_dotted(path: &str, vars: &HashMap<String, Value>) -> Option<Value> {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }
    let root = vars.get(parts[0])?;
    root.get(parts[1]).cloned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A test handler that records calls and returns configurable results.
    struct MockHandler {
        results: HashMap<String, Value>,
    }

    impl MockHandler {
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

    impl ActionHandler for MockHandler {
        fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            self.results.get(name).cloned().ok_or_else(|| {
                ExecutionError::UnknownAction(name.to_string())
            })
        }
    }

    #[test]
    fn execute_simple_call() {
        let handler = MockHandler::new().with_result("greet", json!("hello"));

        let procedure = json!({
            "type": "procedure",
            "name": "test_proc",
            "steps": [
                { "kind": "call", "name": "greet", "params": {}, "output_var": "greeting" }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.procedure_name, "test_proc");
        assert_eq!(result.step_results.len(), 1);
        assert_eq!(result.step_results[0].output, Some(json!("hello")));
        assert_eq!(result.variables.get("greeting"), Some(&json!("hello")));
    }

    #[test]
    fn execute_call_chain_with_var_passing() {
        let handler = MockHandler::new()
            .with_result("fetch_data", json!({"status": "ok", "count": 42}))
            .with_result("process", json!("done"));

        let procedure = json!({
            "type": "procedure",
            "name": "chain",
            "steps": [
                { "kind": "call", "name": "fetch_data", "params": {}, "output_var": "data" },
                { "kind": "call", "name": "process", "params": { "input": "$data" }, "output_var": "result" }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.variables.get("data"), Some(&json!({"status": "ok", "count": 42})));
        assert_eq!(result.variables.get("result"), Some(&json!("done")));
    }

    #[test]
    fn execute_when_condition_true() {
        let handler = MockHandler::new().with_result("notify", json!("notified"));

        let procedure = json!({
            "type": "procedure",
            "name": "conditional",
            "steps": [
                {
                    "kind": "when",
                    "condition": "should_notify",
                    "steps": [
                        { "kind": "call", "name": "notify", "params": {} }
                    ]
                }
            ]
        });

        let vars = HashMap::from([("should_notify".to_string(), json!(true))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        assert!(!result.step_results[0].skipped);
        assert_eq!(result.step_results[0].output, Some(json!("notified")));
    }

    #[test]
    fn execute_when_condition_false_skips() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "conditional",
            "steps": [
                {
                    "kind": "when",
                    "condition": "should_notify",
                    "steps": [
                        { "kind": "call", "name": "notify", "params": {} }
                    ]
                }
            ]
        });

        let vars = HashMap::from([("should_notify".to_string(), json!(false))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        assert!(result.step_results[0].skipped);
    }

    #[test]
    fn execute_match_selects_first_true_arm() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "matcher",
            "steps": [
                {
                    "kind": "match",
                    "arms": [
                        { "condition": "status == error", "result": "handle_error" },
                        { "condition": "status == ok", "result": "handle_ok" },
                        { "condition": "default", "result": "handle_default" }
                    ]
                }
            ]
        });

        let vars = HashMap::from([("status".to_string(), json!("ok"))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        assert_eq!(result.step_results[0].output, Some(json!("handle_ok")));
        assert!(!result.step_results[0].skipped);
    }

    #[test]
    fn execute_match_falls_through_to_default() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "matcher",
            "steps": [
                {
                    "kind": "match",
                    "arms": [
                        { "condition": "status == error", "result": "handle_error" },
                        { "condition": "default", "result": "fallback" }
                    ]
                }
            ]
        });

        let vars = HashMap::from([("status".to_string(), json!("ok"))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        assert_eq!(result.step_results[0].output, Some(json!("fallback")));
    }

    #[test]
    fn execute_match_no_match_skips() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "matcher",
            "steps": [
                {
                    "kind": "match",
                    "arms": [
                        { "condition": "status == error", "result": "handle_error" }
                    ]
                }
            ]
        });

        let vars = HashMap::from([("status".to_string(), json!("ok"))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        assert!(result.step_results[0].skipped);
    }

    #[test]
    fn execute_unknown_action_errors() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "bad",
            "steps": [
                { "kind": "call", "name": "nonexistent", "params": {} }
            ]
        });

        let result = execute(&procedure, &handler);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ExecutionError::UnknownAction(_)));
    }

    #[test]
    fn resolve_vars_in_params() {
        let vars = HashMap::from([
            ("name".to_string(), json!("world")),
            ("count".to_string(), json!(42)),
        ]);

        let params = json!({
            "greeting": "$name",
            "times": "$count",
            "literal": "hello",
            "nested": { "ref": "$name" }
        });

        let resolved = resolve_vars(&params, &vars);
        assert_eq!(resolved["greeting"], json!("world"));
        assert_eq!(resolved["times"], json!(42));
        assert_eq!(resolved["literal"], json!("hello"));
        assert_eq!(resolved["nested"]["ref"], json!("world"));
    }

    #[test]
    fn default_condition_evaluator() {
        let vars = HashMap::from([
            ("status".to_string(), json!("ok")),
            ("count".to_string(), json!(5)),
            ("flag".to_string(), json!(true)),
        ]);

        assert!(default_evaluate_condition("true", &vars));
        assert!(!default_evaluate_condition("false", &vars));
        assert!(default_evaluate_condition("_", &vars));
        assert!(default_evaluate_condition("default", &vars));
        assert!(default_evaluate_condition("status == ok", &vars));
        assert!(!default_evaluate_condition("status == error", &vars));
        assert!(default_evaluate_condition("status != error", &vars));
        assert!(default_evaluate_condition("count == 5", &vars));
        assert!(default_evaluate_condition("flag", &vars));
        assert!(!default_evaluate_condition("nonexistent", &vars));
    }

    #[test]
    fn dotted_access_in_conditions() {
        let vars = HashMap::from([(
            "result".to_string(),
            json!({"status": "green", "count": 3}),
        )]);

        assert!(default_evaluate_condition("result.status == green", &vars));
        assert!(!default_evaluate_condition("result.status == red", &vars));
        assert!(default_evaluate_condition("result.count == 3", &vars));
    }

    #[test]
    fn end_to_end_compiled_procedure() {
        // Test the executor against compiler output format directly.
        // This validates that the executor correctly handles the JSON
        // structure that the compiler produces.
        let proc_data = json!({
            "type": "procedure",
            "name": "deploy_check",
            "trigger": { "kind": "manual" },
            "steps": [
                { "kind": "call", "name": "check_window", "params": {}, "output_var": "window_status" },
                {
                    "kind": "match",
                    "arms": [
                        { "condition": "window_status == blocked", "result": "abort" },
                        { "condition": "_", "result": "proceed" }
                    ]
                }
            ]
        });

        let handler = MockHandler::new()
            .with_result("check_window", json!("open"));

        let result = execute(&proc_data, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.procedure_name, "deploy_check");
        assert_eq!(result.variables.get("window_status"), Some(&json!("open")));
        // "open" != "blocked", so match falls to default "_"
        assert_eq!(result.step_results[1].output, Some(json!("proceed")));
    }

    #[test]
    fn execute_loop_over_array() {
        let handler = MockHandler::new()
            .with_result("process_item", json!("processed"));

        let procedure = json!({
            "type": "procedure",
            "name": "batch",
            "steps": [
                {
                    "kind": "loop",
                    "over": "items",
                    "as": "item",
                    "steps": [
                        { "kind": "call", "name": "process_item", "params": { "val": "$item" } }
                    ],
                    "output_var": "results"
                }
            ]
        });

        let vars = HashMap::from([("items".to_string(), json!(["a", "b", "c"]))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        assert!(result.success);
        assert_eq!(
            result.variables.get("results"),
            Some(&json!(["processed", "processed", "processed"]))
        );
    }

    #[test]
    fn execute_loop_times() {
        let handler = MockHandler::new()
            .with_result("tick", json!("tock"));

        let procedure = json!({
            "type": "procedure",
            "name": "repeat",
            "steps": [
                {
                    "kind": "loop",
                    "times": 3,
                    "steps": [
                        { "kind": "call", "name": "tick", "params": {} }
                    ],
                    "output_var": "ticks"
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.variables.get("ticks"), Some(&json!(["tock", "tock", "tock"])));
    }

    #[test]
    fn execute_loop_missing_var_skips() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "skip",
            "steps": [
                {
                    "kind": "loop",
                    "over": "nonexistent",
                    "steps": [
                        { "kind": "call", "name": "noop", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.step_results[0].skipped);
    }

    #[test]
    fn execute_emit_appends_to_emit_var() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "emitter",
            "steps": [
                {
                    "kind": "emit",
                    "event": { "type": "timer", "id": "t1", "name": "check", "recurring": false }
                },
                {
                    "kind": "emit",
                    "event": { "type": "timer", "id": "t2", "name": "backup", "recurring": true }
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        let emit = result.variables.get("emit").unwrap();
        let arr = emit.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "check");
        assert_eq!(arr[1]["name"], "backup");
    }

    #[test]
    fn execute_emit_resolves_vars() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "emitter",
            "steps": [
                {
                    "kind": "emit",
                    "event": { "type": "state_change", "key": "$target_key", "new_value": "done" }
                }
            ]
        });

        let vars = HashMap::from([("target_key".to_string(), json!("build_status"))]);
        let result = execute_with_vars(&procedure, &handler, vars).unwrap();
        let emit = result.variables.get("emit").unwrap();
        assert_eq!(emit[0]["key"], "build_status");
    }

    #[test]
    fn execute_try_catches_error() {
        let handler = MockHandler::new()
            .with_result("fallback", json!("recovered"));

        let procedure = json!({
            "type": "procedure",
            "name": "resilient",
            "steps": [
                {
                    "kind": "try",
                    "steps": [
                        { "kind": "call", "name": "failing_action", "params": {} }
                    ],
                    "catch": [
                        { "kind": "call", "name": "fallback", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        // The error was caught and fallback ran
        assert!(!result.step_results[0].skipped);
        assert_eq!(result.step_results[0].output, Some(json!("recovered")));
        // Error variable should be set
        assert!(result.variables.get("error").is_some());
    }

    #[test]
    fn execute_try_no_error_clears_error_var() {
        let handler = MockHandler::new()
            .with_result("safe_action", json!("ok"));

        let procedure = json!({
            "type": "procedure",
            "name": "safe",
            "steps": [
                {
                    "kind": "try",
                    "steps": [
                        { "kind": "call", "name": "safe_action", "params": {} }
                    ],
                    "catch": [
                        { "kind": "call", "name": "fallback", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        assert!(result.variables.get("error").is_none());
    }

    #[test]
    fn execute_try_without_catch_still_succeeds() {
        let handler = MockHandler::new();

        let procedure = json!({
            "type": "procedure",
            "name": "no_catch",
            "steps": [
                {
                    "kind": "try",
                    "steps": [
                        { "kind": "call", "name": "failing_action", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        // Error info in output but procedure continues
        assert!(result.step_results[0].output.is_some());
    }

    #[test]
    fn end_to_end_parse_compile_execute() {
        // Full pipeline: parse .px source → compile → execute
        use crate::px::{parse, compiler::compile};

        // Use valid .px grammar syntax
        let source = "procedure greet_user:\n  trigger: manual\n  say_hello {} -> $greeting\n";

        let doc = parse(source).expect("parse failed");
        let records = compile(&doc);
        assert_eq!(records.len(), 1);

        let handler = MockHandler::new()
            .with_result("say_hello", json!("hello world"));

        let result = execute(&records[0].data, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.procedure_name, "greet_user");
        assert_eq!(result.variables.get("greeting"), Some(&json!("hello world")));
    }
}
