//! Scenario runner — executes compiled `.px` scenarios for testing.
//!
//! A scenario defines: setup → run procedure → check expectations.
//! This module provides a sync runner that leverages the existing
//! procedure executor infrastructure.
//!
//! # Architecture
//!
//! ```text
//! PxScenario → compile_scenario → CompiledRecord (JSON)
//!                                       ↓
//!                              run_scenario()
//!                                       ↓
//!                         1. Execute setup steps
//!                         2. Run named procedure
//!                         3. Check expectations
//!                                       ↓
//!                              ScenarioResult
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::executor::{ActionHandler, ExecutionError};

// ── Types ─────────────────────────────────────────────────────────────────────

/// Result of running a single scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario name.
    pub name: String,
    /// Human-readable description (from `given:`).
    pub given: Option<String>,
    /// Whether all expectations passed.
    pub passed: bool,
    /// Individual expectation results.
    pub expectations: Vec<ExpectationResult>,
    /// Error if the scenario failed to execute (setup/run failure).
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Result of checking a single expectation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectationResult {
    /// The expectation check name (e.g. "has_entry", "event_emitted").
    pub check: String,
    /// Parameters passed to the check.
    pub params: Value,
    /// Whether this was negated (NOT).
    pub negated: bool,
    /// Whether the expectation passed.
    pub passed: bool,
    /// Reason for failure (if failed).
    pub reason: Option<String>,
}

/// Aggregate result of running multiple scenarios.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSuiteResult {
    /// Source file (if known).
    pub source: Option<String>,
    /// Individual scenario results.
    pub results: Vec<ScenarioResult>,
    /// Total scenarios.
    pub total: usize,
    /// Passed count.
    pub passed: usize,
    /// Failed count.
    pub failed: usize,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

// ── Expectation Checker Trait ─────────────────────────────────────────────────

/// Trait for evaluating scenario expectations against post-execution state.
///
/// Implementors provide domain-specific checks like `has_entry`, `event_emitted`,
/// `constraint_violated`, etc. The scenario runner calls these after executing
/// setup + procedure.
pub trait ExpectationChecker: Send + Sync {
    /// Check whether an expectation is satisfied.
    ///
    /// Returns `Ok(true)` if the condition holds, `Ok(false)` if it doesn't,
    /// or `Err(reason)` if the check itself failed (e.g. unknown check name).
    fn check(&self, name: &str, params: &Value, state: &ExecutionState) -> Result<bool, String>;

    /// List available check names (for error messages / discovery).
    fn available_checks(&self) -> Vec<&str> {
        vec![]
    }
}

/// Post-execution state available to expectation checkers.
#[derive(Debug, Clone, Default)]
pub struct ExecutionState {
    /// Variable bindings from procedure execution.
    pub variables: HashMap<String, Value>,
    /// Events emitted during execution.
    pub emitted_events: Vec<Value>,
    /// Constraint violations triggered during execution.
    pub constraint_violations: Vec<String>,
    /// Entries in the simulated store (key → value).
    pub store: HashMap<String, Value>,
}

// ── Built-in Expectation Checker ──────────────────────────────────────────────

/// Default expectation checker that handles common built-in checks.
pub struct BuiltinChecker;

impl ExpectationChecker for BuiltinChecker {
    fn check(&self, name: &str, params: &Value, state: &ExecutionState) -> Result<bool, String> {
        match name {
            "has_entry" => {
                let key = params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "has_entry requires 'key' param".to_string())?;
                Ok(state.store.contains_key(key))
            }
            "event_emitted" => {
                let event_name = params
                    .get("event")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "event_emitted requires 'event' param".to_string())?;
                let matched = state.emitted_events.iter().any(|e| {
                    if let Some(ev) = e.get("event").and_then(|v| v.as_str()) {
                        if ev != event_name {
                            return false;
                        }
                    } else {
                        return false;
                    }
                    // If additional params given, all must match
                    if let Some(obj) = params.as_object() {
                        for (k, v) in obj {
                            if k == "event" {
                                continue;
                            }
                            if e.get(k) != Some(v) {
                                return false;
                            }
                        }
                    }
                    true
                });
                Ok(matched)
            }
            "constraint_violated" => {
                let constraint_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "constraint_violated requires 'name' param".to_string())?;
                Ok(state
                    .constraint_violations
                    .contains(&constraint_name.to_string()))
            }
            "var_equals" => {
                let var = params
                    .get("var")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "var_equals requires 'var' param".to_string())?;
                let expected = params
                    .get("value")
                    .ok_or_else(|| "var_equals requires 'value' param".to_string())?;
                match state.variables.get(var) {
                    Some(actual) => Ok(actual == expected),
                    None => Ok(false),
                }
            }
            "store_value" => {
                let key = params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "store_value requires 'key' param".to_string())?;
                let expected = params
                    .get("value")
                    .ok_or_else(|| "store_value requires 'value' param".to_string())?;
                match state.store.get(key) {
                    Some(actual) => Ok(actual == expected),
                    None => Ok(false),
                }
            }
            "is_healthy" => Ok(true), // stub for simple health checks
            other => Err(format!("unknown expectation check: '{other}'")),
        }
    }

    fn available_checks(&self) -> Vec<&str> {
        vec![
            "has_entry",
            "event_emitted",
            "constraint_violated",
            "var_equals",
            "store_value",
            "is_healthy",
        ]
    }
}

// ── Scenario Action Handler ───────────────────────────────────────────────────

/// An ActionHandler that captures state for scenario testing.
///
/// Intercepts emits and store operations to build the ExecutionState
/// for expectation checking.
pub struct ScenarioActionHandler {
    state: std::sync::Mutex<ExecutionState>,
}

impl ScenarioActionHandler {
    /// Create a new scenario action handler.
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(ExecutionState::default()),
        }
    }

    /// Extract the captured execution state.
    pub fn into_state(self) -> ExecutionState {
        self.state.into_inner().unwrap_or_default()
    }
}

impl Default for ScenarioActionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionHandler for ScenarioActionHandler {
    fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            // Intercept emit calls
            "emit" => {
                let mut state = self.state.lock().unwrap();
                state.emitted_events.push(params.clone());
                Ok(Value::Null)
            }
            // Intercept store operations
            "put_entry" | "put" => {
                let mut state = self.state.lock().unwrap();
                if let Some(key) = params.get("key").and_then(|v| v.as_str()) {
                    let value = params.get("value").cloned().unwrap_or(params.clone());
                    state.store.insert(key.to_string(), value);
                }
                Ok(Value::Null)
            }
            // Intercept delete operations
            "delete_entry" | "delete" => {
                let mut state = self.state.lock().unwrap();
                if let Some(key) = params.get("key").and_then(|v| v.as_str()) {
                    state.store.remove(key);
                }
                Ok(Value::Null)
            }
            // Intercept advance_time (test utility — just acknowledge)
            "advance_time" => Ok(Value::Null),
            // Intercept constraint violation reporting
            "violate_constraint" => {
                let mut state = self.state.lock().unwrap();
                if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
                    state.constraint_violations.push(name.to_string());
                }
                Ok(Value::Null)
            }
            // Accept any other call silently (permissive for testing)
            _ => Ok(Value::Null),
        }
    }
}

// ── Scenario Runner ───────────────────────────────────────────────────────────

/// Run a single compiled scenario record.
///
/// The `scenario_data` is the `data` field of a `CompiledRecord` with `type: "scenario"`.
/// The `procedures` map contains compiled procedure records by name (for `run:` references).
pub fn run_scenario(
    scenario_data: &Value,
    procedures: &HashMap<String, Value>,
    checker: &dyn ExpectationChecker,
) -> ScenarioResult {
    let start = std::time::Instant::now();

    let name = scenario_data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let given = scenario_data
        .get("given")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let handler = ScenarioActionHandler::new();

    // 1. Execute setup steps
    if let Some(setup_steps) = scenario_data.get("setup").and_then(|v| v.as_array()) {
        for step in setup_steps {
            if let Err(e) = execute_step(step, &handler) {
                return ScenarioResult {
                    name,
                    given,
                    passed: false,
                    expectations: vec![],
                    error: Some(format!("setup failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                };
            }
        }
    }

    // 2. Execute the run procedure (if specified)
    if let Some(run_info) = scenario_data.get("run").filter(|v| !v.is_null()) {
        let proc_name = if let Some(name_str) = run_info.as_str() {
            name_str.to_string()
        } else if let Some(n) = run_info.get("procedure").and_then(|v| v.as_str()) {
            n.to_string()
        } else {
            return ScenarioResult {
                name,
                given,
                passed: false,
                expectations: vec![],
                error: Some("invalid run clause".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        };

        if let Some(proc_data) = procedures.get(&proc_name) {
            if let Some(steps) = proc_data.get("steps").and_then(|v| v.as_array()) {
                for step in steps {
                    if let Err(e) = execute_step(step, &handler) {
                        return ScenarioResult {
                            name,
                            given,
                            passed: false,
                            expectations: vec![],
                            error: Some(format!("procedure '{proc_name}' failed: {e}")),
                            duration_ms: start.elapsed().as_millis() as u64,
                        };
                    }
                }
            }
        } else {
            return ScenarioResult {
                name,
                given,
                passed: false,
                expectations: vec![],
                error: Some(format!("procedure '{proc_name}' not found")),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    }

    // 3. Check expectations
    let state = handler.into_state();
    let mut expectations = vec![];
    let mut all_passed = true;

    if let Some(expect_list) = scenario_data.get("expectations").and_then(|v| v.as_array()) {
        for expectation in expect_list {
            let check_name = expectation
                .get("check")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let params = expectation.get("params").cloned().unwrap_or(Value::Null);
            let negated = expectation
                .get("negated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let result = checker.check(check_name, &params, &state);

            let (passed, reason) = match result {
                Ok(satisfied) => {
                    let effective = if negated { !satisfied } else { satisfied };
                    let reason = if !effective {
                        if negated {
                            Some(format!("expected NOT {check_name} but it was satisfied"))
                        } else {
                            Some(format!("expected {check_name} but it was not satisfied"))
                        }
                    } else {
                        None
                    };
                    (effective, reason)
                }
                Err(err) => (false, Some(format!("check error: {err}"))),
            };

            if !passed {
                all_passed = false;
            }

            expectations.push(ExpectationResult {
                check: check_name.to_string(),
                params,
                negated,
                passed,
                reason,
            });
        }
    }

    ScenarioResult {
        name,
        given,
        passed: all_passed,
        expectations,
        error: None,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Run all scenarios from compiled records, returning aggregate results.
pub fn run_scenarios(
    scenarios: &[Value],
    procedures: &HashMap<String, Value>,
    checker: &dyn ExpectationChecker,
) -> ScenarioSuiteResult {
    let start = std::time::Instant::now();
    let mut results = vec![];

    for scenario_data in scenarios {
        let result = run_scenario(scenario_data, procedures, checker);
        results.push(result);
    }

    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    ScenarioSuiteResult {
        source: None,
        results,
        total,
        passed,
        failed,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ── Internal Helpers ──────────────────────────────────────────────────────────

/// Execute a single compiled step via the handler.
fn execute_step(step: &Value, handler: &dyn ActionHandler) -> Result<Value, ExecutionError> {
    let kind = step
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("call");

    match kind {
        "call" => {
            let name = step
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let params = step.get("params").cloned().unwrap_or(Value::Null);
            handler.call(name, &params)
        }
        "emit" => {
            let event = step.get("event").cloned().unwrap_or(Value::Null);
            handler.call("emit", &event)
        }
        "when" => {
            let condition = step
                .get("condition")
                .and_then(|v| v.as_str())
                .unwrap_or("true");
            if handler.evaluate_condition(condition, &HashMap::new()) {
                if let Some(steps) = step.get("steps").and_then(|v| v.as_array()) {
                    for s in steps {
                        execute_step(s, handler)?;
                    }
                }
            }
            Ok(Value::Null)
        }
        "loop" => {
            if let Some(times) = step.get("times").and_then(|v| v.as_u64()) {
                if let Some(steps) = step.get("steps").and_then(|v| v.as_array()) {
                    for _ in 0..times.min(10_000) {
                        for s in steps {
                            execute_step(s, handler)?;
                        }
                    }
                }
            }
            Ok(Value::Null)
        }
        _ => {
            // Treat unknown kinds as calls
            let name = step
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(kind);
            let params = step.get("params").cloned().unwrap_or(Value::Null);
            handler.call(name, &params)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scenario_with_no_expectations_passes() {
        let scenario = json!({
            "name": "empty_scenario",
            "given": "Nothing to check",
            "setup": [],
            "expectations": []
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);

        assert!(result.passed);
        assert_eq!(result.name, "empty_scenario");
        assert!(result.expectations.is_empty());
    }

    #[test]
    fn scenario_has_entry_passes_when_entry_exists() {
        let scenario = json!({
            "name": "entry_exists",
            "given": "Entry was put in setup",
            "setup": [
                {"kind": "call", "name": "put_entry", "params": {"key": "mykey", "value": "myval"}}
            ],
            "expectations": [
                {"check": "has_entry", "params": {"key": "mykey"}, "negated": false}
            ]
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);

        assert!(result.passed, "expected pass, got: {:?}", result);
        assert_eq!(result.expectations.len(), 1);
        assert!(result.expectations[0].passed);
    }

    #[test]
    fn scenario_has_entry_negated_passes_when_entry_missing() {
        let scenario = json!({
            "name": "entry_missing",
            "given": "Entry was never put",
            "setup": [],
            "expectations": [
                {"check": "has_entry", "params": {"key": "nokey"}, "negated": true}
            ]
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);
        assert!(result.passed);
    }

    #[test]
    fn scenario_has_entry_negated_fails_when_entry_exists() {
        let scenario = json!({
            "name": "entry_should_not_exist",
            "setup": [
                {"kind": "call", "name": "put_entry", "params": {"key": "badkey", "value": "x"}}
            ],
            "expectations": [
                {"check": "has_entry", "params": {"key": "badkey"}, "negated": true}
            ]
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);

        assert!(!result.passed);
        assert!(!result.expectations[0].passed);
        assert!(result.expectations[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("NOT"));
    }

    #[test]
    fn scenario_event_emitted_passes() {
        let scenario = json!({
            "name": "event_check",
            "setup": [
                {"kind": "emit", "event": {"event": "cache.invalidated", "key": "old"}}
            ],
            "expectations": [
                {"check": "event_emitted", "params": {"event": "cache.invalidated", "key": "old"}, "negated": false}
            ]
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);
        assert!(result.passed, "expected pass, got: {:?}", result);
    }

    #[test]
    fn scenario_event_not_emitted_passes() {
        let scenario = json!({
            "name": "no_event",
            "setup": [],
            "expectations": [
                {"check": "event_emitted", "params": {"event": "never.fired"}, "negated": true}
            ]
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);
        assert!(result.passed);
    }

    #[test]
    fn scenario_runs_procedure_then_checks() {
        let scenario = json!({
            "name": "with_procedure",
            "given": "Procedure puts an entry",
            "setup": [],
            "run": "my_proc",
            "expectations": [
                {"check": "has_entry", "params": {"key": "proc_key"}, "negated": false}
            ]
        });

        let mut procedures = HashMap::new();
        procedures.insert(
            "my_proc".to_string(),
            json!({
                "name": "my_proc",
                "steps": [
                    {"kind": "call", "name": "put_entry", "params": {"key": "proc_key", "value": "proc_val"}}
                ]
            }),
        );

        let result = run_scenario(&scenario, &procedures, &BuiltinChecker);
        assert!(result.passed, "expected pass, got: {:?}", result);
    }

    #[test]
    fn scenario_fails_when_procedure_not_found() {
        let scenario = json!({
            "name": "missing_proc",
            "run": "nonexistent",
            "expectations": []
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);

        assert!(!result.passed);
        assert!(result.error.as_ref().unwrap().contains("not found"));
    }

    #[test]
    fn scenario_delete_removes_entry() {
        let scenario = json!({
            "name": "delete_test",
            "setup": [
                {"kind": "call", "name": "put_entry", "params": {"key": "to_delete", "value": "x"}},
                {"kind": "call", "name": "delete_entry", "params": {"key": "to_delete"}}
            ],
            "expectations": [
                {"check": "has_entry", "params": {"key": "to_delete"}, "negated": true}
            ]
        });

        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);
        assert!(result.passed);
    }

    #[test]
    fn scenario_constraint_violated_check() {
        let state = ExecutionState {
            constraint_violations: vec!["ttl_positive".to_string()],
            ..Default::default()
        };

        let result = BuiltinChecker
            .check("constraint_violated", &json!({"name": "ttl_positive"}), &state)
            .unwrap();
        assert!(result);

        let result = BuiltinChecker
            .check("constraint_violated", &json!({"name": "other"}), &state)
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn run_scenarios_aggregates_results() {
        let scenarios = vec![
            json!({
                "name": "pass1",
                "setup": [],
                "expectations": []
            }),
            json!({
                "name": "pass2",
                "setup": [
                    {"kind": "call", "name": "put_entry", "params": {"key": "k", "value": "v"}}
                ],
                "expectations": [
                    {"check": "has_entry", "params": {"key": "k"}, "negated": false}
                ]
            }),
            json!({
                "name": "fail1",
                "setup": [],
                "expectations": [
                    {"check": "has_entry", "params": {"key": "missing"}, "negated": false}
                ]
            }),
        ];

        let suite = run_scenarios(&scenarios, &HashMap::new(), &BuiltinChecker);

        assert_eq!(suite.total, 3);
        assert_eq!(suite.passed, 2);
        assert_eq!(suite.failed, 1);
    }

    #[test]
    fn full_cache_invalidation_scenario() {
        // Mirrors the design doc example
        let scenario = json!({
            "name": "expired_entries_removed",
            "given": "Cache has entries with expired TTLs",
            "setup": [
                {"kind": "call", "name": "put_entry", "params": {"key": "old", "value": "stale"}},
                {"kind": "call", "name": "put_entry", "params": {"key": "fresh", "value": "good"}},
                {"kind": "call", "name": "advance_time", "params": {"secs": 10}}
            ],
            "run": "invalidate_expired",
            "expectations": [
                {"check": "has_entry", "params": {"key": "old"}, "negated": true},
                {"check": "has_entry", "params": {"key": "fresh"}, "negated": false},
                {"check": "event_emitted", "params": {"event": "cache.invalidated", "key": "old"}, "negated": false}
            ]
        });

        // The procedure deletes "old" and emits an event
        let mut procedures = HashMap::new();
        procedures.insert(
            "invalidate_expired".to_string(),
            json!({
                "name": "invalidate_expired",
                "steps": [
                    {"kind": "call", "name": "delete_entry", "params": {"key": "old"}},
                    {"kind": "emit", "event": {"event": "cache.invalidated", "key": "old"}}
                ]
            }),
        );

        let result = run_scenario(&scenario, &procedures, &BuiltinChecker);

        assert!(result.passed, "expected pass, got: {:?}", result);
        assert_eq!(result.expectations.len(), 3);
        assert!(result.expectations[0].passed); // NOT has_entry "old"
        assert!(result.expectations[1].passed); // has_entry "fresh"
        assert!(result.expectations[2].passed); // event_emitted cache.invalidated
    }

    #[test]
    fn scenario_with_run_object_format() {
        // Test the {"procedure": "name", "params": {...}} format
        let scenario = json!({
            "name": "run_object",
            "setup": [],
            "run": {"procedure": "my_proc", "params": {"x": 1}},
            "expectations": [
                {"check": "has_entry", "params": {"key": "k"}, "negated": false}
            ]
        });

        let mut procedures = HashMap::new();
        procedures.insert(
            "my_proc".to_string(),
            json!({
                "name": "my_proc",
                "steps": [
                    {"kind": "call", "name": "put_entry", "params": {"key": "k", "value": "v"}}
                ]
            }),
        );

        let result = run_scenario(&scenario, &procedures, &BuiltinChecker);
        assert!(result.passed, "expected pass, got: {:?}", result);
    }

    // ── Mutation gap coverage tests ─────────────────────────────────────────

    #[test]
    fn builtin_checker_available_checks_returns_known_checks() {
        // Catches: replace available_checks -> vec!["xyzzy"] / vec![""] / vec![]
        let checks = BuiltinChecker.available_checks();
        assert!(checks.contains(&"has_entry"));
        assert!(checks.contains(&"event_emitted"));
        assert!(checks.contains(&"constraint_violated"));
        assert!(checks.contains(&"var_equals"));
        assert!(checks.contains(&"store_value"));
        assert!(checks.contains(&"is_healthy"));
        assert_eq!(checks.len(), 6);
    }

    #[test]
    fn default_trait_available_checks_returns_empty() {
        // Catches: replace ExpectationChecker::available_checks default with vec![""]/vec!["xyzzy"]
        struct Minimal;
        impl ExpectationChecker for Minimal {
            fn check(&self, _name: &str, _params: &Value, _state: &ExecutionState) -> Result<bool, String> {
                Ok(true)
            }
        }
        let checks = Minimal.available_checks();
        assert!(checks.is_empty());
    }

    #[test]
    fn event_emitted_check_rejects_wrong_event_name() {
        // Catches: replace == with != at line 143 (event name comparison)
        let state = ExecutionState {
            emitted_events: vec![json!({"event": "cache.hit", "key": "x"})],
            ..Default::default()
        };
        // Correct event matches
        let result = BuiltinChecker
            .check("event_emitted", &json!({"event": "cache.hit"}), &state)
            .unwrap();
        assert!(result);
        // Wrong event does NOT match
        let result = BuiltinChecker
            .check("event_emitted", &json!({"event": "cache.miss"}), &state)
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn event_emitted_check_additional_params_must_match() {
        // Catches: replace == with != at line 173 (additional param comparison)
        let state = ExecutionState {
            emitted_events: vec![json!({"event": "order.placed", "amount": 100})],
            ..Default::default()
        };
        // Matching params
        let result = BuiltinChecker
            .check("event_emitted", &json!({"event": "order.placed", "amount": 100}), &state)
            .unwrap();
        assert!(result);
        // Non-matching params
        let result = BuiltinChecker
            .check("event_emitted", &json!({"event": "order.placed", "amount": 999}), &state)
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn constraint_violated_check_exact_name_match() {
        // Catches: replace == with != at line 186 (constraint name contains)
        let state = ExecutionState {
            constraint_violations: vec!["ttl_positive".to_string()],
            ..Default::default()
        };
        assert!(BuiltinChecker
            .check("constraint_violated", &json!({"name": "ttl_positive"}), &state)
            .unwrap());
        assert!(!BuiltinChecker
            .check("constraint_violated", &json!({"name": "wrong_name"}), &state)
            .unwrap());
    }

    #[test]
    fn advance_time_action_is_handled() {
        // Catches: delete match arm "advance_time" in ScenarioActionHandler
        let handler = ScenarioActionHandler::new();
        let result = handler.call("advance_time", &json!({"secs": 60}));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Null);
    }

    #[test]
    fn violate_constraint_action_records_violation() {
        // Catches: delete match arm "violate_constraint" in ScenarioActionHandler
        let handler = ScenarioActionHandler::new();
        handler
            .call("violate_constraint", &json!({"name": "max_ttl"}))
            .unwrap();
        let state = handler.into_state();
        assert_eq!(state.constraint_violations, vec!["max_ttl".to_string()]);
    }

    #[test]
    fn run_scenarios_failed_count_is_total_minus_passed() {
        // Catches: replace - with / in run_scenarios (line 441)
        let scenarios = vec![
            json!({"name": "p1", "setup": [], "expectations": []}),
            json!({"name": "p2", "setup": [], "expectations": []}),
            json!({
                "name": "f1",
                "setup": [],
                "expectations": [{"check": "has_entry", "params": {"key": "x"}, "negated": false}]
            }),
        ];
        let suite = run_scenarios(&scenarios, &HashMap::new(), &BuiltinChecker);
        assert_eq!(suite.total, 3);
        assert_eq!(suite.passed, 2);
        assert_eq!(suite.failed, 1); // Must be 3 - 2 = 1, not 3 / 2 = 1 (catches integer division edge)

        // Edge case: 5 pass, 2 fail => failed=2 (not 5/7=0)
        let scenarios5 = vec![
            json!({"name": "a", "setup": [], "expectations": []}),
            json!({"name": "b", "setup": [], "expectations": []}),
            json!({"name": "c", "setup": [], "expectations": []}),
            json!({"name": "d", "setup": [], "expectations": []}),
            json!({"name": "e", "setup": [], "expectations": []}),
            json!({"name": "f", "setup": [], "expectations": [{"check": "has_entry", "params": {"key": "x"}, "negated": false}]}),
            json!({"name": "g", "setup": [], "expectations": [{"check": "has_entry", "params": {"key": "y"}, "negated": false}]}),
        ];
        let suite5 = run_scenarios(&scenarios5, &HashMap::new(), &BuiltinChecker);
        assert_eq!(suite5.total, 7);
        assert_eq!(suite5.passed, 5);
        assert_eq!(suite5.failed, 2); // 7 - 5 = 2, not 7 / 5 = 1
    }

    #[test]
    fn execute_step_call_kind_invokes_handler() {
        // Catches: delete match arm "call" in execute_step
        let scenario = json!({
            "name": "call_test",
            "setup": [
                {"kind": "call", "name": "put_entry", "params": {"key": "via_call", "value": "yes"}}
            ],
            "expectations": [
                {"check": "has_entry", "params": {"key": "via_call"}, "negated": false}
            ]
        });
        let result = run_scenario(&scenario, &HashMap::new(), &BuiltinChecker);
        assert!(result.passed, "call step should invoke handler: {:?}", result);
    }

    #[test]
    fn execute_step_when_kind_executes_conditionally() {
        // Catches: delete match arm "when" in execute_step
        let handler = ScenarioActionHandler::new();
        let step = json!({
            "kind": "when",
            "condition": "true",
            "steps": [
                {"kind": "call", "name": "put_entry", "params": {"key": "when_ran", "value": "1"}}
            ]
        });
        let result = execute_step(&step, &handler);
        assert!(result.is_ok());
        let state = handler.into_state();
        assert!(state.store.contains_key("when_ran"), "when block should have executed");
    }

    #[test]
    fn execute_step_loop_kind_repeats() {
        // Catches: delete match arm "loop" in execute_step
        let handler = ScenarioActionHandler::new();
        let step = json!({
            "kind": "loop",
            "times": 3,
            "steps": [
                {"kind": "emit", "event": {"event": "tick"}}
            ]
        });
        let result = execute_step(&step, &handler);
        assert!(result.is_ok());
        let state = handler.into_state();
        assert_eq!(state.emitted_events.len(), 3, "loop should repeat 3 times");
    }

    #[test]
    fn var_equals_check_compares_actual_to_expected() {
        // Catches: replace == with != at line 173 (var_equals)
        let state = ExecutionState {
            variables: {
                let mut m = HashMap::new();
                m.insert("status".to_string(), json!("active"));
                m
            },
            ..Default::default()
        };
        // Matching value
        assert!(BuiltinChecker
            .check("var_equals", &json!({"var": "status", "value": "active"}), &state)
            .unwrap());
        // Non-matching value
        assert!(!BuiltinChecker
            .check("var_equals", &json!({"var": "status", "value": "inactive"}), &state)
            .unwrap());
    }

    #[test]
    fn store_value_check_compares_stored_to_expected() {
        // Catches: replace == with != at line 186 (store_value)
        let state = ExecutionState {
            store: {
                let mut m = HashMap::new();
                m.insert("config".to_string(), json!({"ttl": 300}));
                m
            },
            ..Default::default()
        };
        // Matching value
        assert!(BuiltinChecker
            .check("store_value", &json!({"key": "config", "value": {"ttl": 300}}), &state)
            .unwrap());
        // Non-matching value
        assert!(!BuiltinChecker
            .check("store_value", &json!({"key": "config", "value": {"ttl": 999}}), &state)
            .unwrap());
    }

    #[test]
    fn advance_time_does_not_modify_state() {
        // Catches: delete "advance_time" arm — verifies it doesn't trigger emit/put side effects
        let handler = ScenarioActionHandler::new();
        handler.call("advance_time", &json!({"secs": 60})).unwrap();
        handler.call("advance_time", &json!({"secs": 120})).unwrap();
        let state = handler.into_state();
        // advance_time should NOT add events or store entries
        assert!(state.emitted_events.is_empty());
        assert!(state.store.is_empty());
        assert!(state.constraint_violations.is_empty());
    }

    #[test]
    fn execute_step_call_specifically_uses_name_field() {
        // The "call" arm is functionally equivalent to the _ fallback when a name
        // field exists (both extract it). When name is missing, "call" uses
        // "unknown" vs _ uses kind, but neither is a recognized action.
        // This test confirms the call path works correctly.
        let handler = ScenarioActionHandler::new();
        let step = json!({
            "kind": "call",
            "name": "emit",
            "params": {"event": "test.event"}
        });
        execute_step(&step, &handler).unwrap();
        let state = handler.into_state();
        assert_eq!(state.emitted_events.len(), 1);
    }
}
