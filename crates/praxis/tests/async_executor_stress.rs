//! Stress tests for the async executor — high concurrency fan-out, cancellation
//! token propagation, backpressure handling, and resource contention.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::{Mutex, Semaphore};

use pares_radix_praxis::px::async_executor::{execute_async, AsyncActionHandler};
use pares_radix_praxis::px::executor::ExecutionError;

// ── Test Helpers ──────────────────────────────────────────────────────────────

/// Handler that simulates a resource-constrained backend (e.g., limited DB connections).
struct BackpressureHandler {
    semaphore: Arc<Semaphore>,
    concurrent_count: Arc<AtomicUsize>,
    peak_concurrent: Arc<AtomicUsize>,
    total_calls: Arc<AtomicUsize>,
    processing_time: Duration,
}

impl BackpressureHandler {
    fn new(max_concurrent: usize, processing_time: Duration) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            concurrent_count: Arc::new(AtomicUsize::new(0)),
            peak_concurrent: Arc::new(AtomicUsize::new(0)),
            total_calls: Arc::new(AtomicUsize::new(0)),
            processing_time,
        }
    }

    fn peak(&self) -> usize {
        self.peak_concurrent.load(Ordering::SeqCst)
    }

    fn total(&self) -> usize {
        self.total_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AsyncActionHandler for BackpressureHandler {
    async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
        let _permit = self.semaphore.acquire().await.unwrap();
        let current = self.concurrent_count.fetch_add(1, Ordering::SeqCst) + 1;

        // Track peak concurrency
        let mut peak = self.peak_concurrent.load(Ordering::SeqCst);
        while current > peak {
            match self.peak_concurrent.compare_exchange_weak(
                peak,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }

        tokio::time::sleep(self.processing_time).await;

        self.concurrent_count.fetch_sub(1, Ordering::SeqCst);
        let n = self.total_calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"call_number": n + 1}))
    }
}

/// Handler that tracks cancellation via a shared flag.
struct CancellableHandler {
    cancelled: Arc<AtomicBool>,
    calls_before_cancel: usize,
    call_count: Arc<AtomicUsize>,
}

impl CancellableHandler {
    fn new(cancel_after: usize) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            calls_before_cancel: cancel_after,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn was_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn calls_made(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AsyncActionHandler for CancellableHandler {
    async fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;

        if n >= self.calls_before_cancel {
            self.cancelled.store(true, Ordering::SeqCst);
            return Err(ExecutionError::ActionFailed {
                action: name.to_string(),
                message: format!("cancelled after {n} calls"),
            });
        }

        // Simulate work
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(json!({"completed": n}))
    }
}

/// Handler that simulates variable-latency responses (some fast, some slow).
struct JitteryHandler {
    base_latency: Duration,
    jitter_factor: f64, // multiplier on base for slow calls
    slow_every_n: usize,
    call_count: Arc<AtomicUsize>,
}

impl JitteryHandler {
    fn new(base_latency: Duration, jitter_factor: f64, slow_every_n: usize) -> Self {
        Self {
            base_latency,
            jitter_factor,
            slow_every_n,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl AsyncActionHandler for JitteryHandler {
    async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        let delay = if n.is_multiple_of(self.slow_every_n) {
            self.base_latency.mul_f64(self.jitter_factor)
        } else {
            self.base_latency
        };
        tokio::time::sleep(delay).await;
        Ok(json!({"call": n, "delay_ms": delay.as_millis()}))
    }
}

// ── High Concurrency Fan-Out Tests ────────────────────────────────────────────

#[tokio::test]
async fn fanout_100_concurrent_procedures() {
    // Simulate 100 procedures executing concurrently against a shared handler.
    let handler = Arc::new(BackpressureHandler::new(100, Duration::from_millis(1)));

    let procedure = json!({
        "name": "work_unit",
        "steps": [
            { "kind": "call", "name": "process", "params": {}, "output_var": "result" }
        ]
    });

    let start = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..100 {
        let h = handler.clone();
        let p = procedure.clone();
        handles.push(tokio::spawn(async move {
            execute_async(&p, h.as_ref()).await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;
    let elapsed = start.elapsed();

    // All should succeed
    let successes = results
        .iter()
        .filter(|r| r.as_ref().unwrap().is_ok())
        .count();
    assert_eq!(successes, 100, "all 100 procedures should succeed");

    // Total calls should be 100
    assert_eq!(handler.total(), 100);

    // Should complete in bounded time (100 concurrent * 1ms ≈ a few ms, not 100ms serial)
    assert!(
        elapsed < Duration::from_secs(2),
        "fan-out should complete quickly, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn fanout_with_limited_concurrency_respects_backpressure() {
    // Only 5 concurrent slots available, 50 procedures contend for them.
    let handler = Arc::new(BackpressureHandler::new(5, Duration::from_millis(5)));

    let procedure = json!({
        "name": "contended_work",
        "steps": [
            { "kind": "call", "name": "limited_resource", "params": {}, "output_var": "out" }
        ]
    });

    let mut handles = Vec::new();
    for _ in 0..50 {
        let h = handler.clone();
        let p = procedure.clone();
        handles.push(tokio::spawn(async move {
            execute_async(&p, h.as_ref()).await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed (semaphore queues, doesn't reject)
    let successes = results
        .iter()
        .filter(|r| r.as_ref().unwrap().is_ok())
        .count();
    assert_eq!(successes, 50);

    // Peak concurrency should never exceed 5
    assert!(
        handler.peak() <= 5,
        "peak concurrency was {}, expected <= 5",
        handler.peak()
    );
    assert_eq!(handler.total(), 50);
}

#[tokio::test]
async fn fanout_large_loop_many_steps() {
    // A single procedure with a 500-iteration loop — tests executor memory/perf.
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    struct FastHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AsyncActionHandler for FastHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(json!(n))
        }
    }

    let handler = FastHandler { counter };

    let procedure = json!({
        "name": "big_loop",
        "steps": [
            {
                "kind": "loop",
                "times": 500,
                "as": "i",
                "output_var": "results",
                "steps": [
                    { "kind": "call", "name": "work", "params": { "idx": "$i" } }
                ]
            }
        ]
    });

    let start = Instant::now();
    let result = execute_async(&procedure, &handler).await.unwrap();
    let elapsed = start.elapsed();

    assert!(result.success);
    assert_eq!(call_count.load(Ordering::SeqCst), 500);

    // 500 iterations should be fast (no real I/O)
    assert!(
        elapsed < Duration::from_secs(2),
        "500-iter loop took {:?}",
        elapsed
    );

    // Verify output_var collected all results
    let collected = result.variables.get("results").unwrap().as_array().unwrap();
    assert_eq!(collected.len(), 500);
}

// ── Cancellation Propagation Tests ────────────────────────────────────────────

#[tokio::test]
async fn cancellation_stops_loop_immediately() {
    // If a handler returns an error mid-loop, remaining iterations should not execute.
    let handler = CancellableHandler::new(5); // fails on 5th call

    let procedure = json!({
        "name": "cancel_loop",
        "steps": [
            {
                "kind": "loop",
                "times": 100,
                "as": "i",
                "steps": [
                    { "kind": "call", "name": "work", "params": {} }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &handler).await;
    assert!(result.is_err());
    assert!(handler.was_cancelled());
    // Should have stopped at call 5, not continued to 100
    assert_eq!(handler.calls_made(), 5);
}

#[tokio::test]
async fn cancellation_in_nested_loop_stops_both_levels() {
    // Failure in inner loop should propagate up and stop outer loop too.
    let handler = CancellableHandler::new(7); // fails on 7th call

    let procedure = json!({
        "name": "nested_cancel",
        "steps": [
            {
                "kind": "loop",
                "times": 10,
                "as": "outer",
                "steps": [
                    {
                        "kind": "loop",
                        "times": 5,
                        "as": "inner",
                        "steps": [
                            { "kind": "call", "name": "deep_work", "params": {} }
                        ]
                    }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &handler).await;
    assert!(result.is_err());
    // 10 * 5 = 50 potential calls, should stop at 7
    assert_eq!(handler.calls_made(), 7);
}

#[tokio::test]
async fn try_catch_prevents_cancellation_propagation() {
    // With try/catch, the error is caught and the procedure continues.
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    struct FailOnceHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AsyncActionHandler for FailOnceHandler {
        async fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
            if name == "risky" && n == 1 {
                return Err(ExecutionError::ActionFailed {
                    action: name.to_string(),
                    message: "first call fails".into(),
                });
            }
            Ok(json!({"call": n}))
        }
    }

    let handler = FailOnceHandler { counter };

    let procedure = json!({
        "name": "caught_cancel",
        "steps": [
            {
                "kind": "try",
                "steps": [
                    { "kind": "call", "name": "risky", "params": {} }
                ],
                "catch": [
                    { "kind": "call", "name": "recovery", "params": {}, "output_var": "recovered" }
                ]
            },
            { "kind": "call", "name": "continue_work", "params": {}, "output_var": "final" }
        ]
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);
    // All 3 calls should have been made (risky fails, recovery, continue_work)
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
    assert!(result.variables.contains_key("recovered"));
    assert!(result.variables.contains_key("final"));
}

#[tokio::test]
async fn timeout_cancels_without_leaking_tasks() {
    // When a step times out, the underlying future should be dropped.
    let started = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    let completed_clone = completed.clone();

    struct LeakDetector {
        started: Arc<AtomicBool>,
        completed: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AsyncActionHandler for LeakDetector {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            self.started.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(10)).await;
            self.completed.store(true, Ordering::SeqCst);
            Ok(json!("should never return"))
        }
    }

    let handler = LeakDetector {
        started: started_clone,
        completed: completed_clone,
    };

    let procedure = json!({
        "name": "leak_test",
        "steps": [
            { "kind": "call", "name": "long_running", "params": {}, "timeout_ms": 20 }
        ]
    });

    let result = execute_async(&procedure, &handler).await;
    assert!(result.is_err());
    assert!(started.load(Ordering::SeqCst), "handler should have started");

    // Give a little time for any leaked task to complete (it shouldn't)
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !completed.load(Ordering::SeqCst),
        "handler should NOT have completed — task should be dropped on timeout"
    );
}

// ── Resource Contention & Ordering Tests ──────────────────────────────────────

#[tokio::test]
async fn sequential_procedures_maintain_strict_order() {
    // Even with jittery latency, sequential steps must complete in order.
    let handler = JitteryHandler::new(Duration::from_millis(1), 5.0, 3);

    let procedure = json!({
        "name": "order_guarantee",
        "steps": [
            { "kind": "call", "name": "step", "params": {}, "output_var": "a" },
            { "kind": "call", "name": "step", "params": {}, "output_var": "b" },
            { "kind": "call", "name": "step", "params": {}, "output_var": "c" },
            { "kind": "call", "name": "step", "params": {}, "output_var": "d" },
            { "kind": "call", "name": "step", "params": {}, "output_var": "e" },
            { "kind": "call", "name": "step", "params": {}, "output_var": "f" }
        ]
    });

    let result = execute_async(&procedure, &handler).await.unwrap();
    assert!(result.success);

    // Verify monotonically increasing call numbers
    let vals: Vec<u64> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|k| {
            result.variables[*k]
                .get("call")
                .unwrap()
                .as_u64()
                .unwrap()
        })
        .collect();

    for i in 1..vals.len() {
        assert!(
            vals[i] > vals[i - 1],
            "step order violated: {:?}",
            vals
        );
    }
}

#[tokio::test]
async fn concurrent_procedures_with_contended_state() {
    // Multiple procedures writing to the same shared state through the handler.
    let shared_results = Arc::new(Mutex::new(Vec::<usize>::new()));
    let results_clone = shared_results.clone();

    struct ContentionHandler {
        results: Arc<Mutex<Vec<usize>>>,
        counter: AtomicUsize,
    }

    #[async_trait]
    impl AsyncActionHandler for ContentionHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            // Simulate some work with contention
            tokio::time::sleep(Duration::from_micros(100)).await;
            {
                let mut results = self.results.lock().await;
                results.push(n);
            }
            Ok(json!(n))
        }
    }

    let handler = Arc::new(ContentionHandler {
        results: results_clone,
        counter: AtomicUsize::new(0),
    });

    let procedure = json!({
        "name": "contention",
        "steps": [
            { "kind": "call", "name": "compete", "params": {} },
            { "kind": "call", "name": "compete", "params": {} },
            { "kind": "call", "name": "compete", "params": {} }
        ]
    });

    let mut handles = Vec::new();
    for _ in 0..20 {
        let h = handler.clone();
        let p = procedure.clone();
        handles.push(tokio::spawn(async move {
            execute_async(&p, h.as_ref()).await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;
    let successes = results
        .iter()
        .filter(|r| r.as_ref().unwrap().is_ok())
        .count();
    assert_eq!(successes, 20);

    // All 60 calls (20 procedures × 3 steps) should be recorded
    let final_results = shared_results.lock().await;
    assert_eq!(final_results.len(), 60);

    // All values should be unique (atomic counter guarantees this)
    let mut sorted = final_results.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 60, "all call numbers should be unique");
}

// ── Memory & Resource Limits ──────────────────────────────────────────────────

#[tokio::test]
async fn loop_guard_at_boundary() {
    // Exactly at the limit (10,000) should succeed.
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    struct NoOpHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AsyncActionHandler for NoOpHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(Value::Null)
        }
    }

    let handler = NoOpHandler { counter };

    let procedure = json!({
        "name": "boundary_loop",
        "steps": [
            {
                "kind": "loop",
                "times": 10_000,
                "as": "i",
                "steps": [
                    { "kind": "call", "name": "noop", "params": {} }
                ]
            }
        ]
    });

    let start = Instant::now();
    let result = execute_async(&procedure, &handler).await.unwrap();
    let elapsed = start.elapsed();

    assert!(result.success);
    assert_eq!(call_count.load(Ordering::SeqCst), 10_000);
    // 10k no-op calls should still be bounded
    assert!(
        elapsed < Duration::from_secs(10),
        "10k iterations took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn loop_guard_just_over_boundary_fails() {
    // 10,001 should be rejected.
    struct NoOpHandler;

    #[async_trait]
    impl AsyncActionHandler for NoOpHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(Value::Null)
        }
    }

    let procedure = json!({
        "name": "over_boundary",
        "steps": [
            {
                "kind": "loop",
                "times": 10_001,
                "as": "i",
                "steps": [
                    { "kind": "call", "name": "noop", "params": {} }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &NoOpHandler).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn deeply_nested_procedures_bounded() {
    // Deep nesting (when→loop→when→call) should work within reason.
    struct DepthHandler;

    #[async_trait]
    impl AsyncActionHandler for DepthHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(json!("deep"))
        }

        fn evaluate_condition(&self, _expr: &str, _vars: &HashMap<String, Value>) -> bool {
            true
        }
    }

    let procedure = json!({
        "name": "deep_nest",
        "steps": [
            {
                "kind": "when",
                "condition": "true",
                "steps": [
                    {
                        "kind": "loop",
                        "times": 3,
                        "as": "i",
                        "steps": [
                            {
                                "kind": "when",
                                "condition": "true",
                                "steps": [
                                    {
                                        "kind": "loop",
                                        "times": 3,
                                        "as": "j",
                                        "steps": [
                                            {
                                                "kind": "try",
                                                "steps": [
                                                    { "kind": "call", "name": "leaf", "params": {} }
                                                ]
                                            }
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        ]
    });

    let result = execute_async(&procedure, &DepthHandler).await.unwrap();
    assert!(result.success);
}

// ── Timeout Interaction Tests ─────────────────────────────────────────────────

#[tokio::test]
async fn multiple_timeouts_in_sequence_are_independent() {
    // Each step's timeout is fresh — a prior timeout doesn't affect subsequent steps.
    struct VariableLatencyHandler {
        call_count: AtomicUsize,
    }

    #[async_trait]
    impl AsyncActionHandler for VariableLatencyHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
            // First call is fast, second is slow (would timeout), third is fast
            let delay = match n {
                2 => Duration::from_secs(5),
                _ => Duration::from_millis(1),
            };
            tokio::time::sleep(delay).await;
            Ok(json!({"call": n}))
        }
    }

    let handler = VariableLatencyHandler {
        call_count: AtomicUsize::new(0),
    };

    // First step succeeds, second times out
    let procedure = json!({
        "name": "independent_timeouts",
        "steps": [
            { "kind": "call", "name": "fast", "params": {}, "output_var": "a", "timeout_ms": 100 },
            { "kind": "call", "name": "slow", "params": {}, "output_var": "b", "timeout_ms": 50 }
        ]
    });

    let start = Instant::now();
    let result = execute_async(&procedure, &handler).await;
    let elapsed = start.elapsed();

    // Should fail on second step
    assert!(result.is_err());
    // But should be fast (timeout at 50ms, not wait 5s)
    assert!(elapsed < Duration::from_secs(1));
}

#[tokio::test]
async fn rapid_succession_procedures_no_interference() {
    // Spawning many procedures in rapid succession should not cause interference.
    struct SimpleHandler;

    #[async_trait]
    impl AsyncActionHandler for SimpleHandler {
        async fn call(&self, _name: &str, params: &Value) -> Result<Value, ExecutionError> {
            // Return the input id to verify isolation
            Ok(params.get("id").cloned().unwrap_or(Value::Null))
        }
    }

    let handler = Arc::new(SimpleHandler);

    let mut handles = Vec::new();
    for id in 0..200 {
        let h = handler.clone();
        handles.push(tokio::spawn(async move {
            let procedure = json!({
                "name": format!("proc_{id}"),
                "steps": [
                    { "kind": "call", "name": "echo", "params": { "id": id }, "output_var": "my_id" }
                ]
            });
            let result = execute_async(&procedure, h.as_ref()).await.unwrap();
            (id, result.variables.get("my_id").cloned())
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // Every procedure should get back its own id (no cross-contamination)
    for result in results {
        let (expected_id, actual) = result.unwrap();
        assert_eq!(
            actual,
            Some(json!(expected_id)),
            "procedure {expected_id} got wrong id back"
        );
    }
}
