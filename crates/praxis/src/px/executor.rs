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
//! integration point where the host system (pares-radix core, MCP server,
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
        "parallel" => execute_parallel(step, index, vars, handler),
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

    let item_var = step.get("as").and_then(|v| v.as_str()).unwrap_or("item");

    let output_var = step.get("output_var").and_then(|v| v.as_str());

    // Determine iteration source
    let iterations: Vec<Value> = if let Some(over_ref) = step.get("over").and_then(|v| v.as_str()) {
        // Resolve variable reference
        let var_name = over_ref.strip_prefix('$').unwrap_or(over_ref);
        match vars.get(var_name) {
            Some(Value::Array(arr)) => arr.clone(),
            Some(other) => vec![other.clone()], // single-item iteration
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

    let catch_steps = step.get("catch").and_then(|v| v.as_array());

    // Retry configuration: retry=N means up to N additional attempts after the first.
    let max_retries = step
        .get("retry")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let mut last_err: Option<ExecutionError> = None;

    for attempt in 0..=max_retries {
        vars.insert("retry_count".to_string(), Value::Number(attempt.into()));

        // Attempt the try block
        let mut last_output = None;
        let mut failed = false;

        for (i, nested) in try_steps.iter().enumerate() {
            match execute_step(nested, i, vars, handler) {
                Ok(result) => {
                    last_output = result.output;
                }
                Err(err) => {
                    last_err = Some(err);
                    failed = true;
                    break;
                }
            }
        }

        if !failed {
            // All try steps succeeded
            vars.remove("error");
            vars.remove("retry_count");
            return Ok(StepResult {
                index,
                kind: "try".into(),
                output: last_output,
                skipped: false,
            });
        }

        // If we have retries left, continue the loop
        if attempt < max_retries {
            continue;
        }

        // All retries exhausted — run catch or return error
        let err = last_err.take().unwrap();
        vars.insert("error".to_string(), Value::String(err.to_string()));

        if let Some(catch) = catch_steps {
            let mut catch_output = None;
            for (j, catch_step) in catch.iter().enumerate() {
                let result = execute_step(catch_step, j, vars, handler)?;
                catch_output = result.output;
            }
            vars.remove("retry_count");
            return Ok(StepResult {
                index,
                kind: "try".into(),
                output: catch_output,
                skipped: false,
            });
        }

        vars.remove("retry_count");
        return Ok(StepResult {
            index,
            kind: "try".into(),
            output: Some(Value::String(err.to_string())),
            skipped: false,
        });
    }

    // Unreachable but satisfies the compiler
    vars.remove("retry_count");
    Ok(StepResult {
        index,
        kind: "try".into(),
        output: None,
        skipped: false,
    })
}

/// Execute a `parallel` step: run named branches.
///
/// In the synchronous executor, branches are executed sequentially (no true
/// parallelism). Each branch gets its own copy of the variables, and the
/// results are collected into a map keyed by branch name.
///
/// The async executor provides true concurrent execution via `tokio::join!`.
fn execute_parallel(
    step: &Value,
    index: usize,
    vars: &mut HashMap<String, Value>,
    handler: &dyn ActionHandler,
) -> Result<StepResult, ExecutionError> {
    let branches = step
        .get("branches")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ExecutionError::InvalidStructure("parallel step missing 'branches'".into())
        })?;

    let output_var = step.get("output_var").and_then(|v| v.as_str());

    let mut results_map = serde_json::Map::new();

    for branch in branches {
        let branch_name = branch
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ExecutionError::InvalidStructure("parallel branch missing 'name'".into())
            })?;

        let branch_steps = branch
            .get("steps")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ExecutionError::InvalidStructure("parallel branch missing 'steps'".into())
            })?;

        // Each branch gets a snapshot of vars (isolation)
        let mut branch_vars = vars.clone();
        let mut last_output = Value::Null;

        for (i, nested) in branch_steps.iter().enumerate() {
            let result = execute_step(nested, i, &mut branch_vars, handler)?;
            if let Some(output) = result.output {
                last_output = output;
            }
        }

        results_map.insert(branch_name.to_string(), last_output);
    }

    let output = Value::Object(results_map);

    if let Some(out_var) = output_var {
        if !out_var.is_empty() {
            vars.insert(out_var.to_string(), output.clone());
        }
    }

    Ok(StepResult {
        index,
        kind: "parallel".into(),
        output: Some(output),
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
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_vars(v, vars)).collect()),
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
    eval_or(expr, vars)
}

// ── Expression Parser (recursive descent) ─────────────────────────────────────
//
// Grammar (lowest to highest precedence):
//   or_expr   := and_expr ( "||" and_expr )*
//   and_expr  := unary ( "&&" unary )*
//   unary     := "!" unary | atom
//   atom      := "(" or_expr ")" | comparison | literal | truthy_var
//   comparison := var ("==" | "!=" | ">=" | "<=" | ">" | "<") value
//
// We split on logical operators first (outside parentheses), then evaluate atoms.

/// Evaluate an OR expression: `a || b || c`
fn eval_or(expr: &str, vars: &HashMap<String, Value>) -> bool {
    let parts = split_logical(expr, "||");
    if parts.len() > 1 {
        return parts.iter().any(|part| eval_and(part.trim(), vars));
    }
    eval_and(expr, vars)
}

/// Evaluate an AND expression: `a && b && c`
fn eval_and(expr: &str, vars: &HashMap<String, Value>) -> bool {
    let parts = split_logical(expr, "&&");
    if parts.len() > 1 {
        return parts.iter().all(|part| eval_unary(part.trim(), vars));
    }
    eval_unary(expr, vars)
}

/// Evaluate a unary expression: `!expr` or just `atom`
fn eval_unary(expr: &str, vars: &HashMap<String, Value>) -> bool {
    let expr = expr.trim();
    if let Some(rest) = expr.strip_prefix('!') {
        let rest = rest.trim();
        // Handle `!(...)` or `!var`
        return !eval_unary(rest, vars);
    }
    eval_atom(expr, vars)
}

/// Evaluate an atom: parenthesized expression, comparison, literal, or truthy variable.
fn eval_atom(expr: &str, vars: &HashMap<String, Value>) -> bool {
    let expr = expr.trim();

    // Parenthesized expression
    if expr.starts_with('(') && matching_close_paren(expr) == Some(expr.len() - 1) {
        return eval_or(&expr[1..expr.len() - 1], vars);
    }

    // Literals
    match expr {
        "true" | "_" | "default" | "else" => return true,
        "false" => return false,
        _ => {}
    }

    // Try comparison operators (order matters: >= before >, <= before <, == and != before others)
    // == comparison
    if let Some((lhs, rhs)) = split_comparison(expr, "==") {
        return compare_eq(lhs, rhs, vars);
    }
    // != comparison
    if let Some((lhs, rhs)) = split_comparison(expr, "!=") {
        return !compare_eq(lhs, rhs, vars);
    }
    // >= comparison
    if let Some((lhs, rhs)) = split_comparison(expr, ">=") {
        return compare_ord(lhs, rhs, vars, |a, b| a >= b);
    }
    // <= comparison
    if let Some((lhs, rhs)) = split_comparison(expr, "<=") {
        return compare_ord(lhs, rhs, vars, |a, b| a <= b);
    }
    // > comparison
    if let Some((lhs, rhs)) = split_comparison(expr, ">") {
        return compare_ord(lhs, rhs, vars, |a, b| a > b);
    }
    // < comparison
    if let Some((lhs, rhs)) = split_comparison(expr, "<") {
        return compare_ord(lhs, rhs, vars, |a, b| a < b);
    }

    // `contains` operator: `list contains "value"` or `str contains "sub"`
    if let Some((lhs, rhs)) = split_contains(expr) {
        return eval_contains(lhs, rhs, vars);
    }

    // `in` operator: `"value" in list`
    if let Some((item, collection)) = split_in(expr) {
        return eval_contains(collection, item, vars);
    }

    // Bare variable name — truthy check
    if let Some(val) = resolve_var(expr, vars) {
        return is_truthy(&val);
    }

    false
}

/// Split an expression on a logical operator (`&&` or `||`), respecting parentheses.
fn split_logical<'a>(expr: &'a str, op: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut last = 0;
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    let op_len = op_bytes.len();

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'"' => {
                // Skip quoted strings
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ if depth == 0 && i + op_len <= bytes.len() && &bytes[i..i + op_len] == op_bytes => {
                parts.push(&expr[last..i]);
                i += op_len;
                last = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&expr[last..]);
    parts
}

/// Find the index of the matching close parenthesis for a leading `(`.
fn matching_close_paren(expr: &str) -> Option<usize> {
    if !expr.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    for (i, ch) in expr.chars().enumerate() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a simple comparison expression on an operator, ensuring we don't confuse
/// `>=` with `>` followed by `=`.
fn split_comparison<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    // For multi-char ops, find the first occurrence outside parens
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    let op_len = op_bytes.len();
    let mut depth = 0i32;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ if depth == 0 && i + op_len <= bytes.len() && &bytes[i..i + op_len] == op_bytes => {
                // For single-char ops (> or <), ensure they're not part of >= or <=
                if op_len == 1 && i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    i += 1;
                    continue;
                }
                let lhs = expr[..i].trim();
                let rhs = expr[i + op_len..].trim().trim_matches('"');
                if !lhs.is_empty() && !rhs.is_empty() {
                    return Some((lhs, rhs));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Resolve a variable (direct lookup or dotted path).
fn resolve_var(name: &str, vars: &HashMap<String, Value>) -> Option<Value> {
    if let Some(val) = vars.get(name) {
        return Some(val.clone());
    }
    resolve_dotted(name, vars)
}

/// Compare for equality.
fn compare_eq(lhs: &str, rhs: &str, vars: &HashMap<String, Value>) -> bool {
    if let Some(val) = resolve_var(lhs, vars) {
        return match &val {
            Value::String(s) => s.as_str() == rhs,
            Value::Number(n) => {
                // Try numeric comparison first
                if let (Some(a), Ok(b)) = (n.as_f64(), rhs.parse::<f64>()) {
                    a == b
                } else {
                    n.to_string() == rhs
                }
            }
            Value::Bool(b) => b.to_string() == rhs,
            Value::Null => rhs == "null",
            _ => false,
        };
    }
    false
}

/// Compare using an ordering function.
fn compare_ord(
    lhs: &str,
    rhs: &str,
    vars: &HashMap<String, Value>,
    cmp: impl Fn(f64, f64) -> bool,
) -> bool {
    let lhs_val = resolve_var(lhs, vars);
    let lhs_num = lhs_val.as_ref().and_then(|v| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    });
    let rhs_num = rhs.parse::<f64>().ok();

    match (lhs_num, rhs_num) {
        (Some(a), Some(b)) => cmp(a, b),
        _ => false,
    }
}

/// Check if a JSON value is truthy.
fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => true,
    }
}

/// Split a `contains` expression: `lhs contains rhs`
fn split_contains(expr: &str) -> Option<(&str, &str)> {
    // Find " contains " token outside parens/quotes
    let needle = " contains ";
    let idx = find_keyword_outside_parens(expr, needle)?;
    let lhs = expr[..idx].trim();
    let rhs = expr[idx + needle.len()..].trim().trim_matches('"');
    if !lhs.is_empty() && !rhs.is_empty() {
        Some((lhs, rhs))
    } else {
        None
    }
}

/// Split an `in` expression: `item in collection`
fn split_in(expr: &str) -> Option<(&str, &str)> {
    let needle = " in ";
    let idx = find_keyword_outside_parens(expr, needle)?;
    let item = expr[..idx].trim().trim_matches('"');
    let collection = expr[idx + needle.len()..].trim();
    if !item.is_empty() && !collection.is_empty() {
        Some((item, collection))
    } else {
        None
    }
}

/// Find the position of a keyword token in an expression, respecting parens and quotes.
fn find_keyword_outside_parens(expr: &str, keyword: &str) -> Option<usize> {
    let bytes = expr.as_bytes();
    let kw_bytes = keyword.as_bytes();
    let kw_len = kw_bytes.len();
    let mut depth = 0i32;
    let mut in_quotes = false;

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
        } else if !in_quotes {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ if depth == 0
                    && i + kw_len <= bytes.len()
                    && &bytes[i..i + kw_len] == kw_bytes =>
                {
                    return Some(i);
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Evaluate a `contains` check. Works for:
/// - Arrays: checks if the array contains the value
/// - Strings: checks if the string contains the substring
fn eval_contains(collection_expr: &str, item_expr: &str, vars: &HashMap<String, Value>) -> bool {
    let collection = resolve_var(collection_expr, vars);
    match collection {
        Some(Value::Array(arr)) => {
            // Check if any element matches the item
            let item_val = resolve_var(item_expr, vars);
            match item_val {
                Some(val) => arr.contains(&val),
                None => {
                    // Treat as literal string
                    let item_as_value = Value::String(item_expr.to_string());
                    arr.contains(&item_as_value)
                        || arr.iter().any(|el| match el {
                            Value::Number(n) => n.to_string() == item_expr,
                            _ => false,
                        })
                }
            }
        }
        Some(Value::String(s)) => {
            // String containment
            let sub = match resolve_var(item_expr, vars) {
                Some(Value::String(v)) => v,
                _ => item_expr.to_string(),
            };
            s.contains(&sub)
        }
        _ => false,
    }
}

/// Resolve dotted variable access with arbitrary depth, including bracket indexing.
///
/// Supports paths like:
/// - `result.status` — nested object access
/// - `items[0]` — array index
/// - `items[0].name` — array index then object access
/// - `data["key"]` — bracket key access on objects
/// - `response.data.items[2].id` — mixed paths
fn resolve_dotted(path: &str, vars: &HashMap<String, Value>) -> Option<Value> {
    let segments = parse_path_segments(path);
    if segments.len() < 2 {
        return None;
    }

    // First segment must be a key (root variable name)
    let root_key = segments[0].as_str()?;
    let root = vars.get(root_key)?;
    let mut current = root;

    for segment in &segments[1..] {
        current = resolve_segment(current, segment)?;
    }

    Some(current.clone())
}

/// A path segment: either a string key or a numeric index.
#[derive(Debug, Clone, PartialEq)]
enum PathSegment {
    Key(String),
    Index(usize),
}

impl PathSegment {
    fn as_str(&self) -> Option<&str> {
        match self {
            PathSegment::Key(s) => Some(s.as_str()),
            PathSegment::Index(_) => None,
        }
    }
}

/// Parse a path string into segments.
///
/// Examples:
/// - `"foo.bar"` → `[Key("foo"), Key("bar")]`
/// - `"items[0]"` → `[Key("items"), Index(0)]`
/// - `"items[0].name"` → `[Key("items"), Index(0), Key("name")]`
/// - `"data[\"key\"]"` → `[Key("data"), Key("key")]`
fn parse_path_segments(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = path.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '.' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
                // Parse bracket content
                i += 1;
                let mut bracket_content = String::new();
                while i < chars.len() && chars[i] != ']' {
                    bracket_content.push(chars[i]);
                    i += 1;
                }
                // Determine if it's a numeric index or a string key
                let trimmed = bracket_content.trim();
                if let Ok(idx) = trimmed.parse::<usize>() {
                    segments.push(PathSegment::Index(idx));
                } else {
                    // Strip quotes if present: ["key"] or ['key']
                    let key = trimmed
                        .trim_start_matches('"')
                        .trim_end_matches('"')
                        .trim_start_matches('\'')
                        .trim_end_matches('\'');
                    segments.push(PathSegment::Key(key.to_string()));
                }
            }
            ']' => {
                // Already consumed by '[' handler
            }
            c => {
                current.push(c);
            }
        }
        i += 1;
    }

    if !current.is_empty() {
        segments.push(PathSegment::Key(current));
    }

    segments
}

/// Resolve a single segment against a JSON value.
fn resolve_segment<'a>(value: &'a Value, segment: &PathSegment) -> Option<&'a Value> {
    match segment {
        PathSegment::Key(key) => value.get(key.as_str()),
        PathSegment::Index(idx) => value.get(*idx),
    }
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
            self.results
                .get(name)
                .cloned()
                .ok_or_else(|| ExecutionError::UnknownAction(name.to_string()))
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
        assert_eq!(
            result.variables.get("data"),
            Some(&json!({"status": "ok", "count": 42}))
        );
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
        assert!(matches!(
            result.unwrap_err(),
            ExecutionError::UnknownAction(_)
        ));
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
        let vars = HashMap::from([("result".to_string(), json!({"status": "green", "count": 3}))]);

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

        let handler = MockHandler::new().with_result("check_window", json!("open"));

        let result = execute(&proc_data, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.procedure_name, "deploy_check");
        assert_eq!(result.variables.get("window_status"), Some(&json!("open")));
        // "open" != "blocked", so match falls to default "_"
        assert_eq!(result.step_results[1].output, Some(json!("proceed")));
    }

    #[test]
    fn execute_loop_over_array() {
        let handler = MockHandler::new().with_result("process_item", json!("processed"));

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
        let handler = MockHandler::new().with_result("tick", json!("tock"));

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
        assert_eq!(
            result.variables.get("ticks"),
            Some(&json!(["tock", "tock", "tock"]))
        );
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
        let handler = MockHandler::new().with_result("fallback", json!("recovered"));

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
        let handler = MockHandler::new().with_result("safe_action", json!("ok"));

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
        use crate::px::{compiler::compile, parse};

        // Use valid .px grammar syntax
        let source = "procedure greet_user:\n  trigger: manual\n  say_hello {} -> $greeting\n";

        let doc = parse(source).expect("parse failed");
        let records = compile(&doc);
        assert_eq!(records.len(), 1);

        let handler = MockHandler::new().with_result("say_hello", json!("hello world"));

        let result = execute(&records[0].data, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.procedure_name, "greet_user");
        assert_eq!(
            result.variables.get("greeting"),
            Some(&json!("hello world"))
        );
    }

    // ── Logical operator tests ────────────────────────────────────────────────

    #[test]
    fn condition_and_operator() {
        let vars = HashMap::from([
            ("status".to_string(), json!("ok")),
            ("count".to_string(), json!(5)),
            ("flag".to_string(), json!(true)),
        ]);

        // Both true
        assert!(default_evaluate_condition("status == ok && flag", &vars));
        // First false
        assert!(!default_evaluate_condition(
            "status == error && flag",
            &vars
        ));
        // Second false
        assert!(!default_evaluate_condition(
            "flag && status == error",
            &vars
        ));
        // Triple AND
        assert!(default_evaluate_condition(
            "status == ok && flag && count == 5",
            &vars
        ));
        assert!(!default_evaluate_condition(
            "status == ok && flag && count == 99",
            &vars
        ));
    }

    #[test]
    fn condition_or_operator() {
        let vars = HashMap::from([
            ("status".to_string(), json!("ok")),
            ("flag".to_string(), json!(false)),
        ]);

        // First true
        assert!(default_evaluate_condition("status == ok || flag", &vars));
        // Second true (first false)
        assert!(default_evaluate_condition(
            "status == error || status == ok",
            &vars
        ));
        // Both false
        assert!(!default_evaluate_condition(
            "status == error || flag",
            &vars
        ));
    }

    #[test]
    fn condition_not_operator() {
        let vars = HashMap::from([
            ("flag".to_string(), json!(true)),
            ("empty".to_string(), json!(false)),
        ]);

        assert!(!default_evaluate_condition("!flag", &vars));
        assert!(default_evaluate_condition("!empty", &vars));
        assert!(default_evaluate_condition("!nonexistent", &vars));
        // Double negation
        assert!(default_evaluate_condition("!!flag", &vars));
    }

    #[test]
    fn condition_comparison_operators() {
        let vars = HashMap::from([
            ("count".to_string(), json!(5)),
            ("score".to_string(), json!(85.5)),
        ]);

        // Greater than
        assert!(default_evaluate_condition("count > 3", &vars));
        assert!(!default_evaluate_condition("count > 5", &vars));
        assert!(!default_evaluate_condition("count > 10", &vars));

        // Less than
        assert!(default_evaluate_condition("count < 10", &vars));
        assert!(!default_evaluate_condition("count < 5", &vars));
        assert!(!default_evaluate_condition("count < 3", &vars));

        // Greater or equal
        assert!(default_evaluate_condition("count >= 5", &vars));
        assert!(default_evaluate_condition("count >= 4", &vars));
        assert!(!default_evaluate_condition("count >= 6", &vars));

        // Less or equal
        assert!(default_evaluate_condition("count <= 5", &vars));
        assert!(default_evaluate_condition("count <= 6", &vars));
        assert!(!default_evaluate_condition("count <= 4", &vars));

        // Float comparisons
        assert!(default_evaluate_condition("score > 80", &vars));
        assert!(default_evaluate_condition("score < 90", &vars));
        assert!(default_evaluate_condition("score >= 85.5", &vars));
    }

    #[test]
    fn condition_combined_logical_and_comparison() {
        let vars = HashMap::from([
            ("status".to_string(), json!("open")),
            ("priority".to_string(), json!(3)),
            ("assigned".to_string(), json!(true)),
        ]);

        // AND with comparison
        assert!(default_evaluate_condition(
            "status == open && priority > 2",
            &vars
        ));
        assert!(!default_evaluate_condition(
            "status == open && priority > 5",
            &vars
        ));

        // OR with comparison
        assert!(default_evaluate_condition(
            "priority > 10 || assigned",
            &vars
        ));

        // Mixed
        assert!(default_evaluate_condition(
            "status == open && (priority > 2 || !assigned)",
            &vars
        ));
        assert!(!default_evaluate_condition(
            "status == closed && (priority > 2 || assigned)",
            &vars
        ));
    }

    #[test]
    fn condition_parentheses() {
        let vars = HashMap::from([
            ("a".to_string(), json!(true)),
            ("b".to_string(), json!(false)),
            ("c".to_string(), json!(true)),
        ]);

        // Without parens: a && b || c => (a && b) || c => false || true => true
        assert!(default_evaluate_condition("a && b || c", &vars));
        // With parens: a && (b || c) => true && (false || true) => true && true => true
        assert!(default_evaluate_condition("a && (b || c)", &vars));
        // a && (b || !c) => true && (false || false) => false
        assert!(!default_evaluate_condition("a && (b || !c)", &vars));
    }

    #[test]
    fn deep_dotted_path_resolution() {
        let vars = HashMap::from([
            ("response".to_string(), json!({
                "data": {
                    "items": [1, 2, 3],
                    "meta": {
                        "count": 3,
                        "status": "ok"
                    }
                },
                "status": 200
            })),
        ]);

        // Two levels deep
        assert!(default_evaluate_condition("response.data.meta.status == ok", &vars));
        assert!(default_evaluate_condition("response.data.meta.count == 3", &vars));
        assert!(default_evaluate_condition("response.status == 200", &vars));
        assert!(!default_evaluate_condition("response.data.meta.status == error", &vars));
    }

    #[test]
    fn contains_operator_array() {
        let vars = HashMap::from([
            ("tags".to_string(), json!(["rust", "wasm", "praxis"])),
            ("numbers".to_string(), json!([1, 2, 3, 5, 8])),
            ("needle".to_string(), json!("rust")),
        ]);

        // Array contains string literal
        assert!(default_evaluate_condition("tags contains \"rust\"", &vars));
        assert!(default_evaluate_condition("tags contains \"praxis\"", &vars));
        assert!(!default_evaluate_condition("tags contains \"python\"", &vars));

        // Array contains via variable reference
        assert!(default_evaluate_condition("tags contains needle", &vars));

        // Array contains number
        assert!(default_evaluate_condition("numbers contains 3", &vars));
        assert!(!default_evaluate_condition("numbers contains 4", &vars));
    }

    #[test]
    fn contains_operator_string() {
        let vars = HashMap::from([
            ("message".to_string(), json!("hello world, welcome!")),
            ("sub".to_string(), json!("world")),
        ]);

        assert!(default_evaluate_condition("message contains \"world\"", &vars));
        assert!(default_evaluate_condition("message contains \"hello\"", &vars));
        assert!(!default_evaluate_condition("message contains \"goodbye\"", &vars));

        // Contains with variable as substring
        assert!(default_evaluate_condition("message contains sub", &vars));
    }

    #[test]
    fn in_operator() {
        let vars = HashMap::from([
            ("roles".to_string(), json!(["admin", "editor", "viewer"])),
            ("role".to_string(), json!("editor")),
        ]);

        // "value" in collection
        assert!(default_evaluate_condition("\"admin\" in roles", &vars));
        assert!(!default_evaluate_condition("\"superuser\" in roles", &vars));

        // Variable in collection
        assert!(default_evaluate_condition("role in roles", &vars));
    }

    #[test]
    fn contains_with_logical_operators() {
        let vars = HashMap::from([
            ("tags".to_string(), json!(["rust", "async"])),
            ("status".to_string(), json!("active")),
        ]);

        assert!(default_evaluate_condition(
            "tags contains \"rust\" && status == active",
            &vars
        ));
        assert!(!default_evaluate_condition(
            "tags contains \"python\" && status == active",
            &vars
        ));
        assert!(default_evaluate_condition(
            "tags contains \"python\" || status == active",
            &vars
        ));
    }

    #[test]
    fn bracket_array_indexing() {
        let vars = HashMap::from([
            ("items".to_string(), json!(["alpha", "beta", "gamma"])),
            (
                "users".to_string(),
                json!([{"name": "alice", "age": 30}, {"name": "bob", "age": 25}]),
            ),
        ]);

        // Simple array index
        assert!(default_evaluate_condition("items[0] == alpha", &vars));
        assert!(default_evaluate_condition("items[1] == beta", &vars));
        assert!(default_evaluate_condition("items[2] == gamma", &vars));
        assert!(!default_evaluate_condition("items[0] == beta", &vars));

        // Array index with nested object access
        assert!(default_evaluate_condition("users[0].name == alice", &vars));
        assert!(default_evaluate_condition("users[1].name == bob", &vars));
        assert!(default_evaluate_condition("users[0].age == 30", &vars));
        assert!(default_evaluate_condition("users[1].age == 25", &vars));
        assert!(!default_evaluate_condition("users[0].name == bob", &vars));
    }

    #[test]
    fn bracket_object_key_access() {
        let vars = HashMap::from([(
            "headers".to_string(),
            json!({"content-type": "application/json", "x-request-id": "abc123"}),
        )]);

        // Bracket key access (for keys with hyphens that can't use dot notation)
        assert!(default_evaluate_condition(
            "headers[\"content-type\"] == application/json",
            &vars
        ));
        assert!(default_evaluate_condition(
            "headers[\"x-request-id\"] == abc123",
            &vars
        ));
    }

    #[test]
    fn mixed_dot_and_bracket_paths() {
        let vars = HashMap::from([(
            "response".to_string(),
            json!({
                "data": {
                    "items": [
                        {"id": 1, "status": "active"},
                        {"id": 2, "status": "inactive"}
                    ],
                    "meta": {"total": 2}
                }
            }),
        )]);

        // Deep mixed path: dot, bracket index, dot
        assert!(default_evaluate_condition(
            "response.data.items[0].status == active",
            &vars
        ));
        assert!(default_evaluate_condition(
            "response.data.items[1].status == inactive",
            &vars
        ));
        assert!(default_evaluate_condition(
            "response.data.items[0].id == 1",
            &vars
        ));
        assert!(default_evaluate_condition(
            "response.data.meta.total == 2",
            &vars
        ));
    }

    #[test]
    fn bracket_index_out_of_bounds() {
        let vars = HashMap::from([("items".to_string(), json!(["a", "b"]))]);

        // Out-of-bounds returns false (not found)
        assert!(!default_evaluate_condition("items[5] == a", &vars));
        // Truthy check on out-of-bounds
        assert!(!default_evaluate_condition("items[99]", &vars));
        // In-bounds truthy check
        assert!(default_evaluate_condition("items[0]", &vars));
    }

    #[test]
    fn bracket_indexing_with_operators() {
        let vars = HashMap::from([(
            "scores".to_string(),
            json!([85, 92, 78, 95]),
        )]);

        // Comparison operators with bracket indexing
        assert!(default_evaluate_condition("scores[0] > 80", &vars));
        assert!(default_evaluate_condition("scores[1] >= 92", &vars));
        assert!(default_evaluate_condition("scores[2] < 80", &vars));
        assert!(default_evaluate_condition("scores[3] <= 95", &vars));
        assert!(!default_evaluate_condition("scores[0] > 90", &vars));
    }

    #[test]
    fn parse_path_segments_unit() {
        use super::PathSegment::*;

        assert_eq!(
            super::parse_path_segments("foo.bar"),
            vec![Key("foo".into()), Key("bar".into())]
        );
        assert_eq!(
            super::parse_path_segments("items[0]"),
            vec![Key("items".into()), Index(0)]
        );
        assert_eq!(
            super::parse_path_segments("items[0].name"),
            vec![Key("items".into()), Index(0), Key("name".into())]
        );
        assert_eq!(
            super::parse_path_segments("data[\"key\"]"),
            vec![Key("data".into()), Key("key".into())]
        );
        assert_eq!(
            super::parse_path_segments("a.b[2].c.d[0]"),
            vec![
                Key("a".into()),
                Key("b".into()),
                Index(2),
                Key("c".into()),
                Key("d".into()),
                Index(0)
            ]
        );
    }

    #[test]
    fn execute_parallel_branches() {
        let handler = MockHandler::new()
            .with_result("fetch_a", json!("result_a"))
            .with_result("fetch_b", json!("result_b"))
            .with_result("fetch_c", json!("result_c"));

        let procedure = json!({
            "type": "procedure",
            "name": "parallel_test",
            "steps": [
                {
                    "kind": "parallel",
                    "branches": [
                        {
                            "name": "alpha",
                            "steps": [
                                { "kind": "call", "name": "fetch_a", "params": {} }
                            ]
                        },
                        {
                            "name": "beta",
                            "steps": [
                                { "kind": "call", "name": "fetch_b", "params": {} }
                            ]
                        },
                        {
                            "name": "gamma",
                            "steps": [
                                { "kind": "call", "name": "fetch_c", "params": {} }
                            ]
                        }
                    ],
                    "output_var": "results"
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        let results = result.variables.get("results").unwrap();
        assert_eq!(results["alpha"], json!("result_a"));
        assert_eq!(results["beta"], json!("result_b"));
        assert_eq!(results["gamma"], json!("result_c"));
    }

    #[test]
    fn execute_parallel_branch_isolation() {
        // Branches should not see each other's variable mutations
        let handler = MockHandler::new()
            .with_result("set_val", json!("modified"))
            .with_result("read_val", json!("original"));

        let procedure = json!({
            "type": "procedure",
            "name": "isolation_test",
            "steps": [
                {
                    "kind": "parallel",
                    "branches": [
                        {
                            "name": "writer",
                            "steps": [
                                { "kind": "call", "name": "set_val", "params": {}, "output_var": "shared" }
                            ]
                        },
                        {
                            "name": "reader",
                            "steps": [
                                { "kind": "call", "name": "read_val", "params": { "ref": "$shared" } }
                            ]
                        }
                    ],
                    "output_var": "par_results"
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        // The "shared" var set by writer should NOT be in the parent scope
        assert!(result.variables.get("shared").is_none());
        // But output_var should have the map
        assert!(result.variables.get("par_results").is_some());
    }

    #[test]
    fn execute_parallel_error_propagates() {
        // If a branch fails, the whole parallel step fails
        let handler = MockHandler::new().with_result("ok_action", json!("fine"));

        let procedure = json!({
            "type": "procedure",
            "name": "error_test",
            "steps": [
                {
                    "kind": "parallel",
                    "branches": [
                        {
                            "name": "good",
                            "steps": [
                                { "kind": "call", "name": "ok_action", "params": {} }
                            ]
                        },
                        {
                            "name": "bad",
                            "steps": [
                                { "kind": "call", "name": "nonexistent", "params": {} }
                            ]
                        }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler);
        assert!(result.is_err());
    }

    #[test]
    fn try_retry_succeeds_on_second_attempt() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FlakeHandler {
            call_count: AtomicUsize,
        }

        impl ActionHandler for FlakeHandler {
            fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                match name {
                    "flaky" => {
                        if count == 0 {
                            Err(ExecutionError::ActionFailed {
                                action: "flaky".into(),
                                message: "transient error".into(),
                            })
                        } else {
                            Ok(json!("success_on_retry"))
                        }
                    }
                    _ => Err(ExecutionError::UnknownAction(name.into())),
                }
            }
        }

        let handler = FlakeHandler {
            call_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "retry_test",
            "steps": [
                {
                    "kind": "try",
                    "retry": 2,
                    "steps": [
                        { "kind": "call", "name": "flaky", "params": {}, "output_var": "result" }
                    ],
                    "catch": [
                        { "kind": "emit", "event": "should_not_reach" }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        // Succeeded on retry — catch was not reached
        assert_eq!(result.variables.get("result"), Some(&json!("success_on_retry")));
        // retry_count cleaned up
        assert!(result.variables.get("retry_count").is_none());
        // error cleared
        assert!(result.variables.get("error").is_none());
        // Was called exactly 2 times (first fail + second success)
        assert_eq!(handler.call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn try_retry_exhausted_runs_catch() {
        let handler = MockHandler::new().with_result("fallback", json!("caught_after_retries"));

        let procedure = json!({
            "type": "procedure",
            "name": "retry_exhausted",
            "steps": [
                {
                    "kind": "try",
                    "retry": 3,
                    "steps": [
                        { "kind": "call", "name": "always_fails", "params": {} }
                    ],
                    "catch": [
                        { "kind": "call", "name": "fallback", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.step_results[0].output, Some(json!("caught_after_retries")));
        // error variable was set before catch ran
        assert!(result.variables.get("error").is_some());
    }

    #[test]
    fn try_retry_zero_is_default_no_retry() {
        let handler = MockHandler::new().with_result("fallback", json!("immediate_catch"));

        let procedure = json!({
            "type": "procedure",
            "name": "no_retry",
            "steps": [
                {
                    "kind": "try",
                    "steps": [
                        { "kind": "call", "name": "always_fails", "params": {} }
                    ],
                    "catch": [
                        { "kind": "call", "name": "fallback", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        // Without retry field, catch runs immediately
        assert_eq!(result.step_results[0].output, Some(json!("immediate_catch")));
    }
}
