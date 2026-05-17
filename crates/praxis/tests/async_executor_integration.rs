//! Integration tests for the async executor — concurrent execution,
//! shared state, cancellation semantics, and timeout handling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use pares_radix_praxis::px::async_executor::{
    execute_async, execute_async_with_vars, AsyncActionHandler,
};
use pares_radix_praxis::px::executor::ExecutionError;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Handler that tracks call count per action and allows configurable delays.
struct InstrumentedHandler {
    call_counts: Arc<Mutex<HashMap<String, usize>>>,
    results: HashMap<String, Value>,
    delays: HashMap<String, Duration>,
    shared_state: Arc<Mutex<Vec<String>>>,
}

impl InstrumentedHandler {
    fn new() -> Self {
        Self {
            call_counts: Arc::new(Mutex::new(HashMap::new())),
            results: HashMap::new(),
            delays: HashMap::new(),
            shared_state: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_result(mut self, name: &str, value: Value) -> Self {
        self.results.insert(name.to_string(), value);
        self
    }

    fn with_delay(mut self, name: &str, duration: Duration) -> Self {
        self.delays.insert(name.to_string(), duration);
        self
    }

    fn with_shared_state(mut self, state: Arc<Mutex<Vec<String>>>) -> Self {
        self.shared_state = state;
        self
    }

    async fn get_call_count(&self, name: &str) -> usize {
        *self.call_counts.lock().await.get(name).unwrap_or(&0)
    }
}

#[async_trait]
impl AsyncActionHandler for InstrumentedHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        // Track call count
        {
            let mut counts = self.call_counts.lock().await;
            *counts.entry(name.to_string()).or_insert(0) += 1;
        }

        // Apply delay if configured
        if let Some(delay) = self.delays.get(name) {
            tokio::time::sleep(*delay).await;
        }

        // Record to shared state
        {
            let mut state = self.shared_state.lock().await;
            let entry = if params.is_null() {
                name.to_string()
            } else {
                format!("{}({})", name, params)
            };
            state.push(entry);
        }

        self.results
            .get(name)
            .cloned()
            .ok_or_else(|| ExecutionError::UnknownAction(name.to_string()))
    }
}

/// Handler that fails on specific call counts (for testing retry/catch patterns).
struct FailingHandler {
    call_count: Arc<AtomicUsize>,
    fail_on_calls: Vec<usize>, // 1-indexed call numbers that should fail
}

impl FailingHandler {
    fn new(fail_on_calls: Vec<usize>) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            fail_on_calls,
        }
    }
}

#[async_trait]
impl AsyncActionHandler for FailingHandler {
    async fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
        let call_num = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_calls.contains(&call_num) {
            Err(ExecutionError::ActionFailed {
                action: name.to_string(),
                message: format!("simulated failure on call #{call_num}"),
            })
        } else {
            Ok(json!({"call": call_num, "action": name}))
        }
    }
}

// ── Concurrent Execution Tests ────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_procedures_share_no_state() {
    // Two procedures running concurrently should not interfere with each other's variables.
    let handler1 = Arc::new(
        InstrumentedHandler::new()
            .with_result("set_x", json!(42))
            .with_result("get_x", json!(42)),
    );
    let handler2 = Arc::new(
        InstrumentedHandler::new()
            .with_result("set_x", json!(99))
            .with_result("get_x", json!(99)),
    );

    let proc1 = json!({
        "name": "proc1",
        "steps": [
            { "kind": "call", "name": "set_x", "params": {}, "output_var": "x" },
            { "kind": "call", "name": "get_x", "params": {} }
        ]
    });

    let proc2 = json!({
        "name": "proc2",
        "steps": [
            { "kind": "call", "name": "set_x", "params": {}, "output_var": "x" },
            { "kind": "call", "name": "get_x", "params": {} }
        ]
    });

    let h1 = handler1.clone();
    let h2 = handler2.clone();
    let p1 = proc1.clone();
    let p2 = proc2.clone();

    let (r1, r2) = tokio::join!(
        execute_async(&p1, h1.as_ref()),
        execute_async(&p2, h2.as_ref()),
    );

    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    // Each procedure has its own variable space
    assert_eq!(r1.variables.get("x"), Some(&json!(42)));
    assert_eq!(r2.variables.get("x"), Some(&json!(99)));
}

#[tokio::test]
async fn concurrent_procedures_with_shared_handler_state() {
    // Multiple procedures can safely share a handler with interior mutability.
    let shared_log = Arc::new(Mutex::new(Vec::<String>::new()));

    let handler = Arc::new(
        InstrumentedHandler::new()
            .with_result("step_a", json!("a"))
            .with_result("step_b", json!("b"))
            .with_result("step_c", json!("c"))
            .with_shared_state(shared_log.clone()),
    );

    let proc_ab = json!({
        "name": "proc_ab",
        "steps": [
            { "kind": "call", "name": "step_a", "params": {} },
            { "kind": "call", "name": "step_b", "params": {} }
        ]
    });

    let proc_c = json!({
        "name": "proc_c",
        "steps": [
            { "kind": "call", "name": "step_c", "params": {} }
        ]
    });

    let h1 = handler.clone();
    let h2 = handler.clone();

    let (r1, r2) = tokio::join!(
        execute_async(&proc_ab, h1.as_ref()),
        execute_async(&proc_c, h2.as_ref()),
    );

    assert!(r1.unwrap().success);
    assert!(r2.unwrap().success);

    // All calls should be recorded in the shared state
    let log = shared_log.lock().await;
    assert_eq!(log.len(), 3);
    // Entries are formatted as "name(params)" when params are non-null
    assert!(log.iter().any(|e| e.starts_with("step_a")));
    assert!(log.iter().any(|e| e.starts_with("step_b")));
    assert!(log.iter().any(|e| e.starts_with("step_c")));
}

// ── Timeout Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn timeout_does_not_block_other_steps() {
    // A step with a short timeout should fail fast without blocking subsequent procedures.
    let handler = InstrumentedHandler::new()
        .with_result("fast", json!("quick"))
        .with_delay("slow", Duration::from_secs(10));

    // Procedure with a slow step that times out
    let procedure = json!({
        "name": "timeout_proc",
        "steps": [
            { "kind": "call", "name": "slow", "params": {}, "timeout_ms": 50 }
        ]
    });

    let start = Instant::now();
    let result = execute_async(&procedure, &handler).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // Should fail in ~50ms, not 10s
    assert!(elapsed < Duration::from_secs(1));
}

#[tokio::test]
async fn default_timeout_applies_when_not_specified() {
    // Without explicit timeout_ms, the default (30s) should apply.
    // We test this indirectly by verifying the handler is called.
    let handler = InstrumentedHandler::new()
        .with_result("normal", json!("done"))
        .with_delay("normal", Duration::from_millis(10));

    let procedure = json!({
        "name": "default_timeout_proc",
        "steps": [
            { "kind": "call", "name": "normal", "params": {}, "output_var": "out" }
        ]
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);
    assert_eq!(result.variables.get("out"), Some(&json!("done")));
}

#[tokio::test]
async fn timeout_in_loop_fails_entire_procedure() {
    // A timeout inside a loop iteration should bubble up and fail the procedure.
    let handler = InstrumentedHandler::new()
        .with_result("process", json!("ok"))
        .with_delay("process", Duration::from_secs(5));

    let procedure = json!({
        "name": "loop_timeout",
        "steps": [
            {
                "kind": "loop",
                "times": 3,
                "as": "i",
                "steps": [
                    { "kind": "call", "name": "process", "params": {}, "timeout_ms": 30 }
                ]
            }
        ]
    });

    let start = Instant::now();
    let result = execute_async(&procedure, &handler).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // Should fail on first iteration quickly
    assert!(elapsed < Duration::from_secs(1));
    // Only the first call should have been attempted
    assert_eq!(handler.get_call_count("process").await, 1);
}

// ── Error Recovery & Try/Catch Tests ──────────────────────────────────────────

#[tokio::test]
async fn try_catch_captures_error_and_continues() {
    let handler = FailingHandler::new(vec![1]); // first call fails

    let procedure = json!({
        "name": "try_recovery",
        "steps": [
            {
                "kind": "try",
                "steps": [
                    { "kind": "call", "name": "risky_op", "params": {} }
                ],
                "catch": [
                    { "kind": "emit", "event": { "type": "error_handled", "error": "$error" } }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);
    // The emit in catch should have run
    assert!(!result.step_results[0].skipped);
}

#[tokio::test]
async fn try_without_catch_still_succeeds_procedure() {
    let handler = FailingHandler::new(vec![1]);

    let procedure = json!({
        "name": "try_no_catch",
        "steps": [
            {
                "kind": "try",
                "steps": [
                    { "kind": "call", "name": "fragile", "params": {} }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);
    // Error string should be the output
    assert!(result.step_results[0]
        .output
        .as_ref()
        .unwrap()
        .as_str()
        .unwrap()
        .contains("simulated failure"));
}

#[tokio::test]
async fn try_catch_with_timeout_in_try_block() {
    // Timeout inside a try block should be caught by catch.
    let handler = InstrumentedHandler::new()
        .with_result("recovery", json!("recovered"))
        .with_delay("stalling", Duration::from_secs(10));

    let procedure = json!({
        "name": "try_timeout",
        "steps": [
            {
                "kind": "try",
                "steps": [
                    { "kind": "call", "name": "stalling", "params": {}, "timeout_ms": 30 }
                ],
                "catch": [
                    { "kind": "call", "name": "recovery", "params": {}, "output_var": "recovery_result" }
                ]
            }
        ]
    });

    let start = Instant::now();
    let result = execute_async(&procedure, &handler).await.unwrap();
    let elapsed = start.elapsed();

    assert!(result.success);
    assert!(elapsed < Duration::from_secs(1));
    // Error variable should be set
    assert!(result.variables.contains_key("error"));
    assert!(result.variables["error"]
        .as_str()
        .unwrap()
        .contains("timed out"));
}

// ── Shared State & Ordering Tests ─────────────────────────────────────────────

#[tokio::test]
async fn loop_maintains_execution_order() {
    let shared_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let handler = InstrumentedHandler::new()
        .with_result("log_item", json!("logged"))
        .with_shared_state(shared_log.clone());

    let mut vars = HashMap::new();
    vars.insert(
        "items".to_string(),
        json!(["first", "second", "third", "fourth"]),
    );

    let procedure = json!({
        "name": "order_test",
        "steps": [
            {
                "kind": "loop",
                "over": "$items",
                "as": "item",
                "steps": [
                    { "kind": "call", "name": "log_item", "params": { "val": "$item" } }
                ]
            }
        ]
    });

    let result = execute_async_with_vars(&procedure, &handler, vars)
        .await
        .unwrap();
    assert!(result.success);

    // Verify calls happened in order (sequential loop)
    let log = shared_log.lock().await;
    assert_eq!(log.len(), 4);
    assert!(log[0].contains("\"first\""));
    assert!(log[1].contains("\"second\""));
    assert!(log[2].contains("\"third\""));
    assert!(log[3].contains("\"fourth\""));
}

#[tokio::test]
async fn nested_loops_with_shared_state() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    struct CountingHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AsyncActionHandler for CountingHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(json!(n))
        }
    }

    let handler = CountingHandler { counter };

    let mut vars = HashMap::new();
    vars.insert("rows".to_string(), json!([1, 2, 3]));

    let procedure = json!({
        "name": "nested_loop",
        "steps": [
            {
                "kind": "loop",
                "over": "$rows",
                "as": "row",
                "steps": [
                    {
                        "kind": "loop",
                        "times": 2,
                        "as": "col",
                        "steps": [
                            { "kind": "call", "name": "cell", "params": {} }
                        ]
                    }
                ]
            }
        ]
    });

    let result = execute_async_with_vars(&procedure, &handler, vars)
        .await
        .unwrap();
    assert!(result.success);
    // 3 rows × 2 cols = 6 calls
    assert_eq!(call_count.load(Ordering::SeqCst), 6);
}

// ── Variable Propagation Tests ────────────────────────────────────────────────

#[tokio::test]
async fn output_var_propagates_across_steps() {
    struct ChainHandler;

    #[async_trait]
    impl AsyncActionHandler for ChainHandler {
        async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
            match name {
                "fetch" => Ok(json!({"items": ["a", "b"]})),
                "transform" => {
                    // Should receive the fetched data via $data param
                    let input = params.get("input").cloned().unwrap_or(Value::Null);
                    Ok(json!({"transformed": input}))
                }
                _ => Err(ExecutionError::UnknownAction(name.to_string())),
            }
        }
    }

    let procedure = json!({
        "name": "chain_test",
        "steps": [
            { "kind": "call", "name": "fetch", "params": {}, "output_var": "data" },
            { "kind": "call", "name": "transform", "params": { "input": "$data" }, "output_var": "result" }
        ]
    });

    let result = execute_async(&procedure, &ChainHandler).await.unwrap();
    assert!(result.success);

    let expected_result = json!({"transformed": {"items": ["a", "b"]}});
    assert_eq!(result.variables.get("result"), Some(&expected_result));
}

#[tokio::test]
async fn loop_output_var_collects_all_results() {
    struct IncrementHandler {
        counter: AtomicUsize,
    }

    #[async_trait]
    impl AsyncActionHandler for IncrementHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(json!(n * 10))
        }
    }

    let handler = IncrementHandler {
        counter: AtomicUsize::new(1),
    };

    let procedure = json!({
        "name": "collect_test",
        "steps": [
            {
                "kind": "loop",
                "times": 4,
                "as": "i",
                "output_var": "collected",
                "steps": [
                    { "kind": "call", "name": "compute", "params": {} }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);
    assert_eq!(
        result.variables.get("collected"),
        Some(&json!([10, 20, 30, 40]))
    );
}

// ── Edge Cases ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn empty_procedure_succeeds() {
    let handler = InstrumentedHandler::new();

    let procedure = json!({
        "name": "empty",
        "steps": []
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);
    assert_eq!(result.step_results.len(), 0);
}

#[tokio::test]
async fn missing_steps_field_errors() {
    let handler = InstrumentedHandler::new();

    let procedure = json!({
        "name": "no_steps"
    });

    let result = execute_async(&procedure, &handler).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ExecutionError::InvalidStructure(msg) => {
            assert!(msg.contains("steps"));
        }
        other => panic!("expected InvalidStructure, got: {:?}", other),
    }
}

#[tokio::test]
async fn unknown_step_kind_errors() {
    let handler = InstrumentedHandler::new();

    let procedure = json!({
        "name": "bad_step",
        "steps": [
            { "kind": "dance", "name": "boogie" }
        ]
    });

    let result = execute_async(&procedure, &handler).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn procedure_with_pre_seeded_vars() {
    struct EchoHandler;

    #[async_trait]
    impl AsyncActionHandler for EchoHandler {
        async fn call(&self, _name: &str, params: &Value) -> Result<Value, ExecutionError> {
            Ok(params.clone())
        }
    }

    let mut vars = HashMap::new();
    vars.insert("user".to_string(), json!("kbristol"));
    vars.insert("mode".to_string(), json!("fast"));

    let procedure = json!({
        "name": "seeded_test",
        "steps": [
            { "kind": "call", "name": "echo", "params": { "who": "$user", "how": "$mode" }, "output_var": "result" }
        ]
    });

    let result = execute_async_with_vars(&procedure, &EchoHandler, vars)
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(
        result.variables.get("result"),
        Some(&json!({"who": "kbristol", "how": "fast"}))
    );
}
