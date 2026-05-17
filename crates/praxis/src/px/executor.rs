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

    let retry_delay_ms = step
        .get("retry_delay_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let retry_backoff = step
        .get("retry_backoff")
        .and_then(|v| v.as_str())
        .unwrap_or("fixed");

    let retry_max_delay_ms = step
        .get("retry_max_delay_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);

    let retry_jitter = step
        .get("retry_jitter")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut last_err: Option<ExecutionError> = None;

    for attempt in 0..=max_retries {
        vars.insert("retry_count".to_string(), Value::Number(attempt.into()));

        // Delay before retry (not before the first attempt)
        if attempt > 0 && retry_delay_ms > 0 {
            let base_delay = match retry_backoff {
                "exponential" => {
                    let exp_delay = retry_delay_ms.saturating_mul(1u64 << (attempt as u64 - 1));
                    exp_delay.min(retry_max_delay_ms)
                }
                _ => retry_delay_ms.min(retry_max_delay_ms), // "fixed" or unknown
            };
            let delay = if retry_jitter && base_delay > 0 {
                // Full jitter: uniform random in [0, base_delay]
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                attempt.hash(&mut hasher);
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos()
                    .hash(&mut hasher);
                let h = hasher.finish();
                h % (base_delay + 1)
            } else {
                base_delay
            };
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }

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

        // Per-branch retry configuration
        let branch_retry = branch
            .get("retry")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let branch_retry_delay_ms = branch
            .get("retry_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let branch_retry_backoff = branch
            .get("retry_backoff")
            .and_then(|v| v.as_str())
            .unwrap_or("fixed");
        let branch_retry_max_delay_ms = branch
            .get("retry_max_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);
        let branch_retry_jitter = branch
            .get("retry_jitter")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Each branch gets a snapshot of vars (isolation)
        let mut branch_vars = vars.clone();
        let mut last_err = None;
        let mut branch_succeeded = false;

        for attempt in 0..=(branch_retry) {
            // Delay before retry (not before first attempt)
            if attempt > 0 && branch_retry_delay_ms > 0 {
                let base_delay = match branch_retry_backoff {
                    "exponential" => {
                        let exp_delay = branch_retry_delay_ms
                            .saturating_mul(1u64 << (attempt as u64 - 1));
                        exp_delay.min(branch_retry_max_delay_ms)
                    }
                    _ => branch_retry_delay_ms.min(branch_retry_max_delay_ms),
                };
                let delay = if branch_retry_jitter && base_delay > 0 {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    attempt.hash(&mut hasher);
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos()
                        .hash(&mut hasher);
                    let h = hasher.finish();
                    h % (base_delay + 1)
                } else {
                    base_delay
                };
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }

            branch_vars.insert(
                "retry_count".to_string(),
                Value::Number(attempt.into()),
            );

            let mut attempt_vars = branch_vars.clone();
            let mut last_output = Value::Null;
            let mut success = true;

            for (i, nested) in branch_steps.iter().enumerate() {
                match execute_step(nested, i, &mut attempt_vars, handler) {
                    Ok(result) => {
                        if let Some(output) = result.output {
                            last_output = output;
                        }
                    }
                    Err(e) => {
                        last_err = Some(e);
                        success = false;
                        break;
                    }
                }
            }

            if success {
                results_map.insert(branch_name.to_string(), last_output);
                branch_succeeded = true;
                break;
            }
        }

        if !branch_succeeded {
            return Err(last_err.unwrap_or_else(|| ExecutionError::ActionFailed {
                action: branch_name.to_string(),
                message: "branch failed after retries".into(),
            }));
        }
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
        Value::String(s) if s.starts_with('$') && !s.contains("${") => {
            // Whole-string variable reference: "$name" → value of name
            let var_name = &s[1..];
            vars.get(var_name).cloned().unwrap_or_else(|| value.clone())
        }
        Value::String(s) if s.contains("${") => {
            // String interpolation: "Hello, ${name}!" → "Hello, world!"
            Value::String(interpolate_string(s, vars))
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

/// Interpolate `${var}` and `${var.field}` references within a string.
///
/// Supports:
/// - `${name}` — simple variable lookup
/// - `${result.field}` — dotted path access
/// - `${count + 1}` — simple arithmetic (add/subtract with integer literals)
/// - Unresolved references are left as-is
fn interpolate_string(s: &str, vars: &HashMap<String, Value>) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut expr = String::new();
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                    expr.push(c);
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    expr.push(c);
                } else {
                    expr.push(c);
                }
            }
            if depth != 0 {
                // Unterminated — emit raw
                result.push('$');
                result.push('{');
                result.push_str(&expr);
            } else {
                // Try to resolve the expression
                let resolved = resolve_interpolation_expr(expr.trim(), vars);
                result.push_str(&resolved);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Resolve an expression inside `${}`. Supports:
/// - Simple variable: `name`
/// - Dotted access: `result.status`
/// - Arithmetic: `count + 1`, `total - 5`
fn resolve_interpolation_expr(expr: &str, vars: &HashMap<String, Value>) -> String {
    // Try arithmetic: var +/- literal
    if let Some(val) = try_arithmetic_expr(expr, vars) {
        return val;
    }

    // Try variable/dotted resolution
    if let Some(val) = resolve_var(expr, vars) {
        return value_to_interpolation_string(&val);
    }

    // Unresolved — return original
    format!("${{{}}}", expr)
}

/// Attempt to evaluate simple arithmetic: `var + N` or `var - N`
fn try_arithmetic_expr(expr: &str, vars: &HashMap<String, Value>) -> Option<String> {
    // Try ternary: condition ? trueVal : falseVal
    if let Some(result) = try_ternary_expr(expr, vars) {
        return Some(result);
    }

    // Look for +, -, *, /, % operators (not at position 0 for - which could be negative)
    // Lower precedence ops first: + and -
    for op in ['+', '-'] {
        if let Some(idx) = expr[1..].find(op).map(|i| i + 1) {
            let lhs = expr[..idx].trim();
            let rhs = expr[idx + 1..].trim();

            let lhs_num = interp_eval_numeric(lhs, vars)?;
            let rhs_num = interp_eval_numeric(rhs, vars)?;

            let result = match op {
                '+' => lhs_num + rhs_num,
                '-' => lhs_num - rhs_num,
                _ => return None,
            };

            return Some(format_numeric_result(result));
        }
    }

    // Higher precedence: *, /, %
    for op in ['*', '/', '%'] {
        if let Some(idx) = expr.find(op) {
            let lhs = expr[..idx].trim();
            let rhs = expr[idx + 1..].trim();

            let lhs_num = interp_eval_numeric(lhs, vars)?;
            let rhs_num = interp_eval_numeric(rhs, vars)?;

            if (op == '/' || op == '%') && rhs_num == 0.0 {
                return None;
            }

            let result = match op {
                '*' => lhs_num * rhs_num,
                '/' => lhs_num / rhs_num,
                '%' => lhs_num % rhs_num,
                _ => return None,
            };

            return Some(format_numeric_result(result));
        }
    }

    None
}

/// Evaluate a numeric value for interpolation arithmetic (variable or literal).
fn interp_eval_numeric(s: &str, vars: &HashMap<String, Value>) -> Option<f64> {
    // Try as literal number first
    if let Ok(n) = s.parse::<f64>() {
        return Some(n);
    }
    // Try as variable reference
    let val = resolve_var(s, vars)?;
    val.as_f64()
}

/// Format a numeric result: integer if whole, float otherwise.
fn format_numeric_result(result: f64) -> String {
    if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
        (result as i64).to_string()
    } else {
        result.to_string()
    }
}

/// Try to evaluate a ternary expression: `condition ? trueExpr : falseExpr`
///
/// Supports nested ternaries in both branches:
///   `a > 0 ? b > 1 ? "deep" : "shallow" : "negative"`
/// The `?` and `:` are matched by tracking nesting depth.
fn try_ternary_expr(expr: &str, vars: &HashMap<String, Value>) -> Option<String> {
    // Find the first top-level `?` (outside quotes)
    let q_idx = find_top_level_char(expr, '?')?;
    let condition_str = expr[..q_idx].trim();
    let branches = &expr[q_idx + 1..];

    // Find the matching `:` — for every nested `?` we see, skip one `:`.
    let colon_idx = find_matching_colon(branches)?;
    let true_expr = branches[..colon_idx].trim();
    let false_expr = branches[colon_idx + 1..].trim();

    // Evaluate the condition using the same condition evaluator
    let condition_result = default_evaluate_condition(condition_str, vars);

    let chosen = if condition_result { true_expr } else { false_expr };

    // Recursively evaluate if the chosen branch is itself a ternary
    if chosen.contains('?') && chosen.contains(':') {
        if let Some(nested) = try_ternary_expr(chosen, vars) {
            return Some(nested);
        }
    }

    // The chosen branch can be: a quoted string, a variable reference, or a number
    if (chosen.starts_with('"') && chosen.ends_with('"'))
        || (chosen.starts_with('\'') && chosen.ends_with('\''))
    {
        // Quoted string literal — strip quotes
        return Some(chosen[1..chosen.len() - 1].to_string());
    }

    // Try as a variable
    if let Some(val) = resolve_var(chosen, vars) {
        return Some(value_to_interpolation_string(&val));
    }

    // Return as literal
    Some(chosen.to_string())
}

/// Find the first occurrence of `ch` at the top level (not inside quotes).
fn find_top_level_char(expr: &str, ch: char) -> Option<usize> {
    let mut in_double = false;
    let mut in_single = false;
    for (i, c) in expr.char_indices() {
        match c {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            _ if c == ch && !in_double && !in_single => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the `:` that matches the outermost `?` in a branch string.
/// For every nested `?` encountered (outside quotes), we must skip one `:`.
fn find_matching_colon(branches: &str) -> Option<usize> {
    let mut depth: usize = 0;
    let mut in_double = false;
    let mut in_single = false;
    for (i, c) in branches.char_indices() {
        match c {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '?' if !in_double && !in_single => depth += 1,
            ':' if !in_double && !in_single => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Convert a Value to a string suitable for interpolation.
fn value_to_interpolation_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
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

    // `matches` operator: `var matches "pattern"` (regex)
    if let Some((lhs, rhs)) = split_keyword_op(expr, " matches ") {
        return eval_matches(lhs, rhs, vars);
    }

    // `starts_with` operator: `var starts_with "prefix"`
    if let Some((lhs, rhs)) = split_keyword_op(expr, " starts_with ") {
        return eval_starts_with(lhs, rhs, vars);
    }

    // `ends_with` operator: `var ends_with "suffix"`
    if let Some((lhs, rhs)) = split_keyword_op(expr, " ends_with ") {
        return eval_ends_with(lhs, rhs, vars);
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
/// Supports `$variable` prefix notation — the `$` is stripped before lookup.
fn resolve_var(name: &str, vars: &HashMap<String, Value>) -> Option<Value> {
    // Strip leading `$` if present (common in .px procedure expressions)
    let name = name.strip_prefix('$').unwrap_or(name);
    if let Some(val) = vars.get(name) {
        return Some(val.clone());
    }
    resolve_dotted(name, vars)
}

/// Compare for equality. Supports arithmetic expressions.
fn compare_eq(lhs: &str, rhs: &str, vars: &HashMap<String, Value>) -> bool {
    // Try arithmetic on lhs first
    if let Some(lhs_num) = eval_numeric_expr(lhs, vars) {
        if let Ok(rhs_num) = rhs.parse::<f64>() {
            return lhs_num == rhs_num;
        }
        // lhs is numeric but rhs isn't — compare as strings
        if lhs_num.fract() == 0.0 {
            return (lhs_num as i64).to_string() == rhs;
        }
        return lhs_num.to_string() == rhs;
    }

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
/// Supports arithmetic expressions on either side: `count + 1 > threshold`
fn compare_ord(
    lhs: &str,
    rhs: &str,
    vars: &HashMap<String, Value>,
    cmp: impl Fn(f64, f64) -> bool,
) -> bool {
    let lhs_num = eval_numeric_expr(lhs, vars);
    let rhs_num = eval_numeric_expr(rhs, vars);

    match (lhs_num, rhs_num) {
        (Some(a), Some(b)) => cmp(a, b),
        _ => false,
    }
}

/// Evaluate a numeric expression that may contain simple arithmetic.
/// Supports: literal numbers, variable references, and `var +/- N` or `N +/- var`.
fn eval_numeric_expr(expr: &str, vars: &HashMap<String, Value>) -> Option<f64> {
    let expr = expr.trim();

    // Try as a plain number literal first
    if let Ok(n) = expr.parse::<f64>() {
        return Some(n);
    }

    // Try as a variable reference
    if let Some(val) = resolve_var(expr, vars) {
        return match &val {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        };
    }

    // Try arithmetic: look for + or - (not at position 0, which could be a negative sign)
    for op in ['+', '-'] {
        // Find operator outside any leading negative sign
        if let Some(idx) = expr[1..].find(op).map(|i| i + 1) {
            let lhs_part = expr[..idx].trim();
            let rhs_part = expr[idx + 1..].trim();

            // Recursively evaluate both sides (handles var + var, N + var, var + N)
            let a = eval_numeric_expr(lhs_part, vars)?;
            let b = eval_numeric_expr(rhs_part, vars)?;

            return Some(match op {
                '+' => a + b,
                '-' => a - b,
                _ => unreachable!(),
            });
        }
    }

    // Try multiplication, division, and modulo
    for op in ['*', '/', '%'] {
        if let Some(idx) = expr.find(op) {
            let lhs_part = expr[..idx].trim();
            let rhs_part = expr[idx + 1..].trim();

            let a = eval_numeric_expr(lhs_part, vars)?;
            let b = eval_numeric_expr(rhs_part, vars)?;

            return Some(match op {
                '*' => a * b,
                '/' => {
                    if b == 0.0 {
                        return None;
                    }
                    a / b
                }
                '%' => {
                    if b == 0.0 {
                        return None;
                    }
                    a % b
                }
                _ => unreachable!(),
            });
        }
    }

    None
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

/// Split a keyword operator expression generically.
fn split_keyword_op<'a>(expr: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let idx = find_keyword_outside_parens(expr, keyword)?;
    let lhs = expr[..idx].trim();
    let rhs = expr[idx + keyword.len()..].trim().trim_matches('"');
    if !lhs.is_empty() && !rhs.is_empty() {
        Some((lhs, rhs))
    } else {
        None
    }
}

/// Evaluate a `matches` (regex) check.
fn eval_matches(var_expr: &str, pattern: &str, vars: &HashMap<String, Value>) -> bool {
    let val = resolve_var(var_expr, vars);
    let haystack = match &val {
        Some(Value::String(s)) => s.as_str().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return false,
    };
    match regex::Regex::new(pattern) {
        Ok(re) => re.is_match(&haystack),
        Err(_) => false,
    }
}

/// Evaluate a `starts_with` check.
fn eval_starts_with(var_expr: &str, prefix: &str, vars: &HashMap<String, Value>) -> bool {
    let val = resolve_var(var_expr, vars);
    match &val {
        Some(Value::String(s)) => s.starts_with(prefix),
        _ => false,
    }
}

/// Evaluate an `ends_with` check.
fn eval_ends_with(var_expr: &str, suffix: &str, vars: &HashMap<String, Value>) -> bool {
    let val = resolve_var(var_expr, vars);
    match &val {
        Some(Value::String(s)) => s.ends_with(suffix),
        _ => false,
    }
}
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
    fn dollar_prefix_variable_resolution() {
        let vars = HashMap::from([
            ("status".to_string(), json!("ok")),
            ("count".to_string(), json!(42)),
            ("result".to_string(), json!({"code": 200})),
        ]);

        // $variable resolves to same as bare variable
        assert!(default_evaluate_condition("$status == ok", &vars));
        assert!(!default_evaluate_condition("$status == error", &vars));
        assert!(default_evaluate_condition("$count == 42", &vars));
        assert!(default_evaluate_condition("$count > 10", &vars));
        // Truthy check with $prefix
        assert!(default_evaluate_condition("$status", &vars));
        assert!(!default_evaluate_condition("$nonexistent", &vars));
        // Dotted access with $prefix
        assert!(default_evaluate_condition("$result.code == 200", &vars));
    }

    #[test]
    fn matches_operator_regex() {
        let vars = HashMap::from([
            ("email".to_string(), json!("user@example.com")),
            ("version".to_string(), json!("2.14.3")),
            ("name".to_string(), json!("hello-world")),
        ]);

        // Basic regex match
        assert!(default_evaluate_condition(r#"email matches ".*@example\.com""#, &vars));
        assert!(!default_evaluate_condition(r#"email matches ".*@other\.com""#, &vars));
        // Version pattern
        assert!(default_evaluate_condition(r#"version matches "^\d+\.\d+\.\d+$""#, &vars));
        // Character class
        assert!(default_evaluate_condition(r#"name matches "^[a-z-]+$""#, &vars));
        // Invalid regex returns false (no panic)
        assert!(!default_evaluate_condition(r#"name matches "[invalid""#, &vars));
    }

    #[test]
    fn starts_with_operator() {
        let vars = HashMap::from([
            ("path".to_string(), json!("/api/v2/users")),
            ("status".to_string(), json!("error_timeout")),
        ]);

        assert!(default_evaluate_condition(r#"path starts_with "/api/v2""#, &vars));
        assert!(!default_evaluate_condition(r#"path starts_with "/admin""#, &vars));
        assert!(default_evaluate_condition(r#"status starts_with "error""#, &vars));
    }

    #[test]
    fn ends_with_operator() {
        let vars = HashMap::from([
            ("filename".to_string(), json!("report.pdf")),
            ("url".to_string(), json!("https://example.com/api")),
        ]);

        assert!(default_evaluate_condition(r#"filename ends_with ".pdf""#, &vars));
        assert!(!default_evaluate_condition(r#"filename ends_with ".txt""#, &vars));
        assert!(default_evaluate_condition(r#"url ends_with "/api""#, &vars));
    }

    #[test]
    fn combined_new_operators_with_logic() {
        let vars = HashMap::from([
            ("path".to_string(), json!("/api/v2/users")),
            ("method".to_string(), json!("POST")),
            ("tags".to_string(), json!(["auth", "rate-limited"])),
        ]);

        // Combined with && and ||
        assert!(default_evaluate_condition(
            r#"path starts_with "/api" && method == POST"#,
            &vars
        ));
        assert!(default_evaluate_condition(
            r#"path ends_with "/users" || method == GET"#,
            &vars
        ));
        // $prefix with new operators
        assert!(default_evaluate_condition(
            r#"$path starts_with "/api""#,
            &vars
        ));
        assert!(default_evaluate_condition(
            r#"$tags contains "auth" && $method == POST"#,
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

    #[test]
    fn try_retry_with_fixed_delay() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        struct FlakeHandler {
            call_count: AtomicUsize,
        }

        impl ActionHandler for FlakeHandler {
            fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(ExecutionError::ActionFailed {
                        action: "flaky".into(),
                        message: format!("fail #{}", count),
                    })
                } else {
                    Ok(json!("ok"))
                }
            }
        }

        let handler = FlakeHandler {
            call_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "fixed_delay_test",
            "steps": [
                {
                    "kind": "try",
                    "retry": 3,
                    "retry_delay_ms": 10,
                    "steps": [
                        { "kind": "call", "name": "flaky", "params": {} }
                    ]
                }
            ]
        });

        let start = Instant::now();
        let result = execute(&procedure, &handler).unwrap();
        let elapsed = start.elapsed();

        assert!(result.success);
        assert_eq!(result.step_results[0].output, Some(json!("ok")));
        // Should have at least 20ms of delay (2 retries × 10ms each)
        assert!(elapsed.as_millis() >= 18, "Expected >= 18ms, got {}ms", elapsed.as_millis());
    }

    #[test]
    fn try_retry_with_exponential_backoff() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        struct FlakeHandler {
            call_count: AtomicUsize,
        }

        impl ActionHandler for FlakeHandler {
            fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                if count < 3 {
                    Err(ExecutionError::ActionFailed {
                        action: "flaky".into(),
                        message: format!("fail #{}", count),
                    })
                } else {
                    Ok(json!("recovered"))
                }
            }
        }

        let handler = FlakeHandler {
            call_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "exp_backoff_test",
            "steps": [
                {
                    "kind": "try",
                    "retry": 4,
                    "retry_delay_ms": 10,
                    "retry_backoff": "exponential",
                    "steps": [
                        { "kind": "call", "name": "flaky", "params": {} }
                    ]
                }
            ]
        });

        let start = Instant::now();
        let result = execute(&procedure, &handler).unwrap();
        let elapsed = start.elapsed();

        assert!(result.success);
        assert_eq!(result.step_results[0].output, Some(json!("recovered")));
        // Exponential: 10 + 20 + 40 = 70ms for 3 retries
        assert!(elapsed.as_millis() >= 60, "Expected >= 60ms exponential delay, got {}ms", elapsed.as_millis());
    }

    #[test]
    fn try_retry_exponential_backoff_with_max_delay() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        struct FlakeHandler {
            call_count: AtomicUsize,
        }

        impl ActionHandler for FlakeHandler {
            fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                if count < 3 {
                    Err(ExecutionError::ActionFailed {
                        action: "flaky".into(),
                        message: format!("fail #{}", count),
                    })
                } else {
                    Ok(json!("capped"))
                }
            }
        }

        let handler = FlakeHandler {
            call_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "max_delay_test",
            "steps": [
                {
                    "kind": "try",
                    "retry": 4,
                    "retry_delay_ms": 10,
                    "retry_backoff": "exponential",
                    "retry_max_delay_ms": 25,
                    "steps": [
                        { "kind": "call", "name": "flaky", "params": {} }
                    ]
                }
            ]
        });

        let start = Instant::now();
        let result = execute(&procedure, &handler).unwrap();
        let elapsed = start.elapsed();

        assert!(result.success);
        assert_eq!(result.step_results[0].output, Some(json!("capped")));
        // Capped: 10 + 20 + 25 = 55ms (3rd would be 40 but capped at 25)
        assert!(elapsed.as_millis() >= 45, "Expected >= 45ms capped delay, got {}ms", elapsed.as_millis());
        // Should NOT be as long as uncapped (10 + 20 + 40 = 70ms)
        assert!(elapsed.as_millis() < 100, "Delay too long ({}ms) — max cap not working?", elapsed.as_millis());
    }

    #[test]
    fn try_retry_with_jitter() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FlakeHandler {
            call_count: AtomicUsize,
        }

        impl ActionHandler for FlakeHandler {
            fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(ExecutionError::ActionFailed {
                        action: "flaky".into(),
                        message: format!("fail #{}", count),
                    })
                } else {
                    Ok(json!("jittered_success"))
                }
            }
        }

        let handler = FlakeHandler {
            call_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "jitter_test",
            "steps": [
                {
                    "kind": "try",
                    "retry": 3,
                    "retry_delay_ms": 50,
                    "retry_backoff": "exponential",
                    "retry_jitter": true,
                    "steps": [
                        { "kind": "call", "name": "flaky", "params": {} }
                    ]
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        assert_eq!(result.step_results[0].output, Some(json!("jittered_success")));
    }

    #[test]
    fn try_retry_jitter_reduces_total_delay() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        struct AlwaysFail {
            call_count: AtomicUsize,
        }

        impl ActionHandler for AlwaysFail {
            fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err(ExecutionError::ActionFailed {
                    action: "always_fail".into(),
                    message: "nope".into(),
                })
            }
        }

        let handler = AlwaysFail {
            call_count: AtomicUsize::new(0),
        };

        let procedure = json!({
            "type": "procedure",
            "name": "jitter_timing_test",
            "steps": [
                {
                    "kind": "try",
                    "retry": 3,
                    "retry_delay_ms": 30,
                    "retry_backoff": "exponential",
                    "retry_jitter": true,
                    "retry_max_delay_ms": 200,
                    "steps": [
                        { "kind": "call", "name": "always_fail", "params": {} }
                    ],
                    "catch": [
                        { "kind": "emit", "event": "caught" }
                    ]
                }
            ]
        });

        let start = Instant::now();
        let result = execute(&procedure, &handler).unwrap();
        let elapsed = start.elapsed();

        assert!(result.success);
        // Without jitter: 30 + 60 + 120 = 210ms. With jitter: [0,30] + [0,60] + [0,120] <= 210ms
        assert!(elapsed.as_millis() <= 250, "Jitter delay too long: {}ms", elapsed.as_millis());
    }

    #[test]
    fn parallel_branch_retry_sync() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FlakySyncHandler {
            fail_count: AtomicUsize,
            fail_until: usize,
        }

        impl ActionHandler for FlakySyncHandler {
            fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                if name == "flaky" {
                    let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
                    if count < self.fail_until {
                        return Err(ExecutionError::ActionFailed {
                            action: "flaky".into(),
                            message: format!("fail #{}", count),
                        });
                    }
                }
                Ok(json!(format!("{}_done", name)))
            }
        }

        let handler = FlakySyncHandler {
            fail_count: AtomicUsize::new(0),
            fail_until: 2,
        };

        let procedure = json!({
            "type": "procedure",
            "name": "sync_branch_retry",
            "steps": [
                {
                    "kind": "parallel",
                    "branches": [
                        {
                            "name": "stable",
                            "steps": [
                                { "kind": "call", "name": "ok", "params": {} }
                            ]
                        },
                        {
                            "name": "retried",
                            "retry": 3,
                            "retry_delay_ms": 1,
                            "steps": [
                                { "kind": "call", "name": "flaky", "params": {} }
                            ]
                        }
                    ],
                    "output_var": "out"
                }
            ]
        });

        let result = execute(&procedure, &handler).unwrap();
        assert!(result.success);
        let out = result.variables.get("out").unwrap();
        assert_eq!(out["stable"], json!("ok_done"));
        assert_eq!(out["retried"], json!("flaky_done"));
        assert_eq!(handler.fail_count.load(Ordering::SeqCst), 3);
    }

    // ── String Interpolation Tests ────────────────────────────────────────────

    #[test]
    fn test_interpolate_simple_variable() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), json!("world"));
        let result = interpolate_string("Hello, ${name}!", &vars);
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_interpolate_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("first".to_string(), json!("Alice"));
        vars.insert("last".to_string(), json!("Smith"));
        let result = interpolate_string("${first} ${last}", &vars);
        assert_eq!(result, "Alice Smith");
    }

    #[test]
    fn test_interpolate_numeric_variable() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(42));
        let result = interpolate_string("There are ${count} items", &vars);
        assert_eq!(result, "There are 42 items");
    }

    #[test]
    fn test_interpolate_dotted_path() {
        let mut vars = HashMap::new();
        vars.insert("result".to_string(), json!({"status": "ok", "code": 200}));
        let result = interpolate_string("Status: ${result.status} (${result.code})", &vars);
        assert_eq!(result, "Status: ok (200)");
    }

    #[test]
    fn test_interpolate_arithmetic() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(5));
        let result = interpolate_string("Next: ${count + 1}, Prev: ${count - 1}", &vars);
        assert_eq!(result, "Next: 6, Prev: 4");
    }

    #[test]
    fn test_interpolate_unresolved_kept_as_is() {
        let vars = HashMap::new();
        let result = interpolate_string("Hello, ${unknown}!", &vars);
        assert_eq!(result, "Hello, ${unknown}!");
    }

    #[test]
    fn test_interpolate_no_placeholders() {
        let vars = HashMap::new();
        let result = interpolate_string("plain text", &vars);
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_resolve_vars_interpolation_in_params() {
        let mut vars = HashMap::new();
        vars.insert("user".to_string(), json!("alice"));
        vars.insert("host".to_string(), json!("example.com"));

        let params = json!({"url": "https://${host}/api/users/${user}"});
        let resolved = resolve_vars(&params, &vars);
        assert_eq!(resolved["url"], "https://example.com/api/users/alice");
    }

    #[test]
    fn test_resolve_vars_whole_string_still_works() {
        let mut vars = HashMap::new();
        vars.insert("data".to_string(), json!({"nested": true}));

        // Whole-string $var should return the actual value (object), not a string
        let params = json!({"payload": "$data"});
        let resolved = resolve_vars(&params, &vars);
        assert_eq!(resolved["payload"], json!({"nested": true}));
    }

    // ── Arithmetic in When-Guards Tests ───────────────────────────────────────

    #[test]
    fn test_arithmetic_in_comparison_add() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(5));
        // count + 1 > 5 → 6 > 5 → true
        assert!(default_evaluate_condition("count + 1 > 5", &vars));
        // count + 1 > 6 → 6 > 6 → false
        assert!(!default_evaluate_condition("count + 1 > 6", &vars));
    }

    #[test]
    fn test_arithmetic_in_comparison_subtract() {
        let mut vars = HashMap::new();
        vars.insert("total".to_string(), json!(10));
        // total - 3 >= 7 → 7 >= 7 → true
        assert!(default_evaluate_condition("total - 3 >= 7", &vars));
        // total - 3 >= 8 → 7 >= 8 → false
        assert!(!default_evaluate_condition("total - 3 >= 8", &vars));
    }

    #[test]
    fn test_arithmetic_equality() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), json!(4));
        // x + 1 == 5 → true
        assert!(default_evaluate_condition("x + 1 == 5", &vars));
        // x + 1 == 6 → false
        assert!(!default_evaluate_condition("x + 1 == 6", &vars));
    }

    #[test]
    fn test_arithmetic_both_sides() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!(3));
        vars.insert("b".to_string(), json!(5));
        // a + 2 >= b → 5 >= 5 → true
        assert!(default_evaluate_condition("a + 2 >= b", &vars));
        // a > b - 3 → 3 > 2 → true
        assert!(default_evaluate_condition("a > b - 3", &vars));
    }

    #[test]
    fn test_arithmetic_multiply() {
        let mut vars = HashMap::new();
        vars.insert("rate".to_string(), json!(10));
        // rate * 2 == 20 → true
        assert!(default_evaluate_condition("rate * 2 == 20", &vars));
    }

    #[test]
    fn test_arithmetic_divide() {
        let mut vars = HashMap::new();
        vars.insert("total".to_string(), json!(100));
        // total / 4 == 25 → true
        assert!(default_evaluate_condition("total / 4 == 25", &vars));
    }

    #[test]
    fn test_arithmetic_modulo_comparison() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(10));
        // 10 % 3 == 1
        assert!(default_evaluate_condition("count % 3 == 1", &vars));
        // 10 % 5 == 0
        assert!(default_evaluate_condition("count % 5 == 0", &vars));
        // 10 % 4 != 0
        assert!(default_evaluate_condition("count % 4 != 0", &vars));
    }

    #[test]
    fn test_arithmetic_modulo_division_by_zero() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(10));
        // Division by zero should not crash, condition should be false
        assert!(!default_evaluate_condition("count % 0 == 0", &vars));
        assert!(!default_evaluate_condition("count / 0 == 0", &vars));
    }

    #[test]
    fn test_interpolate_modulo() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(17));
        let result = interpolate_string("Remainder: ${count % 5}", &vars);
        assert_eq!(result, "Remainder: 2");
    }

    #[test]
    fn test_interpolate_multiply() {
        let mut vars = HashMap::new();
        vars.insert("price".to_string(), json!(5));
        let result = interpolate_string("Total: ${price * 3}", &vars);
        assert_eq!(result, "Total: 15");
    }

    #[test]
    fn test_interpolate_divide() {
        let mut vars = HashMap::new();
        vars.insert("total".to_string(), json!(100));
        let result = interpolate_string("Half: ${total / 2}", &vars);
        assert_eq!(result, "Half: 50");
    }

    #[test]
    fn test_interpolate_ternary_true() {
        let mut vars = HashMap::new();
        vars.insert("enabled".to_string(), json!(true));
        let result = interpolate_string("Status: ${enabled ? 'on' : 'off'}", &vars);
        assert_eq!(result, "Status: on");
    }

    #[test]
    fn test_interpolate_ternary_false() {
        let mut vars = HashMap::new();
        vars.insert("enabled".to_string(), json!(false));
        let result = interpolate_string("Status: ${enabled ? 'on' : 'off'}", &vars);
        assert_eq!(result, "Status: off");
    }

    #[test]
    fn test_interpolate_ternary_with_comparison() {
        let mut vars = HashMap::new();
        vars.insert("count".to_string(), json!(5));
        let result = interpolate_string("${count > 3 ? 'many' : 'few'}", &vars);
        assert_eq!(result, "many");
    }

    #[test]
    fn test_interpolate_ternary_with_variable_branch() {
        let mut vars = HashMap::new();
        vars.insert("active".to_string(), json!(true));
        vars.insert("name".to_string(), json!("Alice"));
        let result = interpolate_string("User: ${active ? name : 'unknown'}", &vars);
        assert_eq!(result, "User: Alice");
    }

    #[test]
    fn test_interpolate_ternary_numeric_branch() {
        let mut vars = HashMap::new();
        vars.insert("premium".to_string(), json!(false));
        let result = interpolate_string("Limit: ${premium ? 100 : 10}", &vars);
        assert_eq!(result, "Limit: 10");
    }

    #[test]
    fn test_interpolate_nested_ternary_inner_true() {
        // a > 0 ? (b > 1 ? "deep" : "shallow") : "negative"
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!(5));
        vars.insert("b".to_string(), json!(10));
        let result = interpolate_string("${a > 0 ? b > 1 ? \"deep\" : \"shallow\" : \"negative\"}", &vars);
        assert_eq!(result, "deep");
    }

    #[test]
    fn test_interpolate_nested_ternary_inner_false() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!(5));
        vars.insert("b".to_string(), json!(0));
        let result = interpolate_string("${a > 0 ? b > 1 ? \"deep\" : \"shallow\" : \"negative\"}", &vars);
        assert_eq!(result, "shallow");
    }

    #[test]
    fn test_interpolate_nested_ternary_outer_false() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!(-1));
        vars.insert("b".to_string(), json!(10));
        let result = interpolate_string("${a > 0 ? b > 1 ? \"deep\" : \"shallow\" : \"negative\"}", &vars);
        assert_eq!(result, "negative");
    }

    #[test]
    fn test_interpolate_nested_ternary_in_false_branch() {
        // Nesting in the false branch: a > 0 ? "positive" : b > 5 ? "big-neg" : "small-neg"
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!(-1));
        vars.insert("b".to_string(), json!(10));
        let result = interpolate_string("${a > 0 ? \"positive\" : b > 5 ? \"big-neg\" : \"small-neg\"}", &vars);
        assert_eq!(result, "big-neg");
    }

    #[test]
    fn test_interpolate_nested_ternary_false_branch_inner_false() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!(-1));
        vars.insert("b".to_string(), json!(2));
        let result = interpolate_string("${a > 0 ? \"positive\" : b > 5 ? \"big-neg\" : \"small-neg\"}", &vars);
        assert_eq!(result, "small-neg");
    }

    #[test]
    fn test_ternary_with_quoted_colon_in_string() {
        // Colons inside quoted strings should not confuse the parser
        let mut vars = HashMap::new();
        vars.insert("ok".to_string(), json!(true));
        let result = interpolate_string("${ok ? \"time: now\" : \"time: later\"}", &vars);
        assert_eq!(result, "time: now");
    }

    #[test]
    fn test_find_matching_colon_simple() {
        assert_eq!(find_matching_colon(" true : false"), Some(6));
    }

    #[test]
    fn test_find_matching_colon_nested() {
        // "b > 1 ? deep : shallow : negative" — first : is inside nested ternary
        let branches = " b > 1 ? \"deep\" : \"shallow\" : \"negative\"";
        let colon_idx = find_matching_colon(branches).unwrap();
        let false_part = branches[colon_idx + 1..].trim();
        assert_eq!(false_part, "\"negative\"");
    }
}
