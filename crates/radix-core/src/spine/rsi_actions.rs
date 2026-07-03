//! RSI (Recursive Self-Improvement) action handlers.
//!
//! Provides the IO boundary actors that the `.px` RSI procedures call:
//! - `validate_px_syntax` — parse + compile .px source without registering
//! - `register_procedure` — hot-reload a .px procedure into the reactive registry
//! - `compute_stage_stats` — pure stats from stage results
//! - `find_bottleneck` — identify slowest/most-retried stage
//! - `detect_recurring_pattern` — find repeated failure patterns in history
//! - `evaluate_pending_improvements` — compare before/after performance
//! - `find_regressions` — identify improvements that caused quality drops
//! - `check_modification_target` — enforce RSI safety boundaries
//! - `check_rate_limit` — enforce daily modification cap
//! - `all_checks_pass` — AND-gate for multiple boolean checks
//! - `validate_constraints` — validate .px against constraint rules
//! - `increment` — simple numeric increment
//! - `get_last_item` — get last element from a list

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::praxis::write_gate::PraxisWriteGate;
use crate::px_adapter::{load_px_procedures, AsyncActionHandler};
use crate::spine::reactive::ReactiveRegistry;
use pares_radix_praxis::px::executor::ExecutionError;

/// Helper to construct ActionFailed errors concisely.
fn err(action: &str, message: impl Into<String>) -> ExecutionError {
    ExecutionError::ActionFailed {
        action: action.to_string(),
        message: message.into(),
    }
}

/// Constraint ids that the RSI loop may **never** auto-remove or auto-disable,
/// even via the rollback (`undo -> remove_constraint`) path.
///
/// This is the mechanical enforcement of design rail **R1 (`cannot_modify_self`)**
/// and its extension **B1**: a self-improvement loop that could strip its own
/// oversight/safety rails (or the platform's foundational write guards) would be
/// untrustworthy, and rollback could not save us from it (you cannot roll back a
/// loop that already removed the rail that would have caught the bad change).
///
/// The set is intentionally defined **here in the Rust side-effect boundary**,
/// not as `.px` data: the guard list itself must not be editable by the loop it
/// guards. The declarative rail (`constraint cannot_modify_self`) lives in
/// `praxis/procedures/recursive-self-improvement.px`; this predicate is the
/// side-effect gate that makes it real for the removal path.
///
/// A constraint id is self-guarded when it is one of the platform foundational
/// write-gate constraints, or lives in a reserved safety namespace prefix.
pub fn is_self_guard_constraint(id: &str) -> bool {
    // Platform foundational write-gate guards (seeded by PraxisWriteGate::new()).
    const FOUNDATIONAL: &[&str] = &["praxis:no-secrets", "praxis:max-size"];
    if FOUNDATIONAL.contains(&id) {
        return true;
    }
    // Reserved safety namespaces: the RSI rails (R1..R6), any explicitly-tagged
    // safety/oversight constraint, and the OpenClaw safety/tool/prompt layer (B1).
    const GUARD_PREFIXES: &[&str] = &[
        "rsi:guard:",       // the RSI safety rails themselves
        "safety:",          // any constraint tagged as a safety/oversight invariant
        "oversight:",       // human-oversight invariants
        "openclaw:safety:", // B1: OpenClaw safety/tool/prompt layer
    ];
    GUARD_PREFIXES.iter().any(|p| id.starts_with(p))
}

/// RSI action handler — provides boundary actors for recursive self-improvement.
///
/// Holds references to the ReactiveRegistry (for hot-reload) and the shared
/// action handler (for creating new adapters).
pub struct RsiActionHandler {
    registry: Arc<ReactiveRegistry>,
    handler: Arc<dyn AsyncActionHandler>,
    /// Tracks which patterns procedures were registered under (for replacement).
    procedure_patterns: Arc<RwLock<std::collections::HashMap<String, String>>>,
    /// Optional write-gate handle. When present, the RSI rollback path
    /// (`rollback_constraint`) can revert a loop-applied constraint for real by
    /// calling [`PraxisWriteGate::remove_constraint`]. `None` in contexts where
    /// no gate is mounted (the loop then reports the rollback as unavailable
    /// rather than silently succeeding).
    write_gate: Option<Arc<PraxisWriteGate>>,
}

impl RsiActionHandler {
    pub fn new(registry: Arc<ReactiveRegistry>, handler: Arc<dyn AsyncActionHandler>) -> Self {
        Self {
            registry,
            handler,
            procedure_patterns: Arc::new(RwLock::new(std::collections::HashMap::new())),
            write_gate: None,
        }
    }

    /// Construct with a live [`PraxisWriteGate`] so the rollback path can
    /// actually remove/disable constraints from the enforcement set.
    pub fn with_write_gate(
        registry: Arc<ReactiveRegistry>,
        handler: Arc<dyn AsyncActionHandler>,
        write_gate: Arc<PraxisWriteGate>,
    ) -> Self {
        Self {
            registry,
            handler,
            procedure_patterns: Arc::new(RwLock::new(std::collections::HashMap::new())),
            write_gate: Some(write_gate),
        }
    }

    /// Roll back a previously auto-applied constraint by id.
    ///
    /// This is the wiring that closes the correction/undo loop: when the
    /// correction engine's `undo` fires for a constraint-origin correction it
    /// yields the `constraint_id`; this action turns that id into a REAL removal
    /// (or, with `"disable": true`, a reversible disable) from the live
    /// [`PraxisWriteGate`].
    ///
    /// **Safety leash (R1/B1):** ids for which [`is_self_guard_constraint`]
    /// returns true are REFUSED — the loop cannot use its own rollback path to
    /// strip its oversight/safety rails or the platform's foundational write
    /// guards. Refusal is reported as `{ "rolled_back": false, "refused":
    /// "self_guard" }`, never as a silent success.
    ///
    /// Params: `{ "constraint_id": string, "disable"?: bool }`.
    async fn rollback_constraint(&self, params: &Value) -> Result<Value, ExecutionError> {
        let constraint_id = params
            .get("constraint_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("rollback_constraint", "requires 'constraint_id' string"))?;

        // R1/B1 self-guard: never let the loop remove its own rails.
        if is_self_guard_constraint(constraint_id) {
            tracing::warn!(
                constraint_id,
                "RSI: refused to roll back a self-guard/safety constraint (R1/B1)"
            );
            return Ok(json!({
                "rolled_back": false,
                "refused": "self_guard",
                "constraint_id": constraint_id,
                "reason": "constraint is a self-guard/safety rail and is exempt from auto-rollback (R1/B1)"
            }));
        }

        let gate = match &self.write_gate {
            Some(g) => g,
            None => {
                // Honest "not available" — not a fake success.
                return Ok(json!({
                    "rolled_back": false,
                    "unavailable": "no_write_gate",
                    "constraint_id": constraint_id,
                    "reason": "no PraxisWriteGate is mounted in this context; cannot remove constraint"
                }));
            }
        };

        let disable = params
            .get("disable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let changed = if disable {
            gate.set_constraint_enabled(constraint_id, false)
        } else {
            gate.remove_constraint(constraint_id)
        };

        tracing::info!(
            constraint_id,
            disable,
            changed,
            "RSI: rollback_constraint applied to live write-gate"
        );

        Ok(json!({
            "rolled_back": changed,
            "constraint_id": constraint_id,
            "mode": if disable { "disabled" } else { "removed" },
            // false here means the id was simply not present (idempotent no-op).
            "found": changed
        }))
    }

    /// Validate .px source — parse and compile without registering.
    fn validate_px_syntax(&self, params: &Value) -> Result<Value, ExecutionError> {
        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("validate_px_syntax", "requires 'content' string"))?;

        match load_px_procedures(content, self.handler.clone()) {
            Ok(adapters) => {
                let names: Vec<String> = adapters.iter().map(|a| a.name().to_string()).collect();
                Ok(json!({
                    "valid": true,
                    "procedures": names,
                    "count": names.len()
                }))
            }
            Err(e) => Ok(json!({
                "valid": false,
                "error": e
            })),
        }
    }

    /// Register (hot-reload) a .px procedure into the reactive registry.
    async fn register_procedure_action(&self, params: &Value) -> Result<Value, ExecutionError> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("register_procedure", "requires 'name' string"))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("register_procedure", "requires 'content' string"))?;

        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("register_procedure", "requires 'pattern' string"))?;

        // Parse and compile
        let adapters = load_px_procedures(content, self.handler.clone())
            .map_err(|e| err("register_procedure", format!("Failed to compile .px: {e}")))?;

        // Find the specific procedure by name
        let adapter = adapters
            .into_iter()
            .find(|a| a.name() == name)
            .ok_or_else(|| {
                err(
                    "register_procedure",
                    format!("Procedure '{}' not found in compiled output", name),
                )
            })?;

        // Register in the reactive registry (this is the hot-reload!)
        self.registry
            .register_procedure(pattern, Arc::new(adapter))
            .await;

        // Track the pattern for this procedure
        self.procedure_patterns
            .write()
            .await
            .insert(name.to_string(), pattern.to_string());

        tracing::info!(
            procedure = name,
            pattern = pattern,
            "RSI: hot-reloaded procedure into reactive registry"
        );

        Ok(json!({
            "registered": true,
            "name": name,
            "pattern": pattern
        }))
    }

    /// Compute stats from a list of stages.
    fn compute_stage_stats(&self, params: &Value) -> Result<Value, ExecutionError> {
        let stages = params
            .get("stages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| err("compute_stage_stats", "requires 'stages' array"))?;

        let total = stages.len();
        let mut passed_first_try = 0u64;
        let mut total_retries = 0u64;
        let mut total_duration_ms = 0u64;

        for stage in stages {
            let attempts = stage.get("attempts").and_then(|v| v.as_u64()).unwrap_or(1);
            let duration = stage.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
            let status = stage.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");

            if status == "passed" && attempts == 1 {
                passed_first_try += 1;
            }
            if attempts > 1 {
                total_retries += attempts - 1;
            }
            total_duration_ms += duration;
        }

        Ok(json!({
            "total_stages": total,
            "passed_first_try": passed_first_try,
            "total_retries": total_retries,
            "total_duration_ms": total_duration_ms,
            "avg_duration_ms": if total > 0 { total_duration_ms / total as u64 } else { 0 }
        }))
    }

    /// Find the bottleneck stage (most retries, or longest duration if tied).
    fn find_bottleneck(&self, params: &Value) -> Result<Value, ExecutionError> {
        let stages = params
            .get("stages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| err("find_bottleneck", "requires 'stages' array"))?;

        let mut worst_name = String::new();
        let mut worst_score: u64 = 0;

        for stage in stages {
            let name = stage.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let attempts = stage.get("attempts").and_then(|v| v.as_u64()).unwrap_or(1);
            let duration = stage.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);

            let score = (attempts - 1) * 10_000 + duration;
            if score > worst_score {
                worst_score = score;
                worst_name = name.to_string();
            }
        }

        if worst_name.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(json!({"stage": worst_name, "score": worst_score}))
        }
    }

    /// Detect recurring patterns in performance history.
    fn detect_recurring_pattern(&self, params: &Value) -> Result<Value, ExecutionError> {
        let history = params
            .get("history")
            .and_then(|v| v.as_array())
            .ok_or_else(|| err("detect_recurring_pattern", "requires 'history' array"))?;

        let min_occurrences = params
            .get("min_occurrences")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;

        let lookback = params
            .get("lookback")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let recent: Vec<&Value> = history.iter().rev().take(lookback).collect();

        let mut bottleneck_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for signal in &recent {
            if let Some(bottleneck) = signal.get("bottleneck").and_then(|b| b.get("stage")) {
                if let Some(name) = bottleneck.as_str() {
                    *bottleneck_counts.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }

        let patterns: Vec<Value> = bottleneck_counts
            .into_iter()
            .filter(|(_, count)| *count >= min_occurrences)
            .map(|(stage, count)| {
                json!({
                    "type": "recurring_bottleneck",
                    "stage": stage,
                    "occurrences": count,
                    "lookback_size": recent.len()
                })
            })
            .collect();

        if patterns.is_empty() {
            Ok(json!({"actionable": false, "patterns": []}))
        } else {
            Ok(json!({"actionable": true, "patterns": patterns}))
        }
    }

    /// Check if a modification target is allowed (safety boundary).
    fn check_modification_target(&self, params: &Value) -> Result<Value, ExecutionError> {
        let target = params
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("check_modification_target", "requires 'target' string"))?;

        let forbidden = params
            .get("forbidden")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>())
            .unwrap_or_default();

        let safe = !forbidden.contains(&target);

        Ok(json!({
            "safe": safe,
            "target": target,
            "reason": if safe { "Target is modifiable" } else { "Target is in forbidden list" }
        }))
    }

    /// Check rate limit for modifications.
    fn check_rate_limit(&self, params: &Value) -> Result<Value, ExecutionError> {
        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let max = params.get("max").and_then(|v| v.as_u64()).unwrap_or(3);

        Ok(json!({
            "within_limit": count < max,
            "current": count,
            "max": max
        }))
    }

    /// AND-gate: returns true only if all input checks passed.
    fn all_checks_pass(&self, params: &Value) -> Result<Value, ExecutionError> {
        let obj = params
            .as_object()
            .ok_or_else(|| err("all_checks_pass", "requires an object"))?;

        let mut all_pass = true;
        let mut failures: Vec<String> = Vec::new();

        for (key, value) in obj {
            let passed = value
                .get("safe")
                .or_else(|| value.get("within_limit"))
                .or_else(|| value.get("valid"))
                .or_else(|| value.get("pass"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !passed {
                all_pass = false;
                failures.push(key.clone());
            }
        }

        Ok(json!({"pass": all_pass, "failures": failures}))
    }

    /// Increment a numeric value.
    fn increment(&self, params: &Value) -> Result<Value, ExecutionError> {
        let value = params.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
        Ok(json!(value + 1))
    }

    /// Get last item from a list.
    fn get_last_item(&self, params: &Value) -> Result<Value, ExecutionError> {
        let list = params
            .get("list")
            .and_then(|v| v.as_array())
            .ok_or_else(|| err("get_last_item", "requires 'list' array"))?;

        Ok(list.last().cloned().unwrap_or(Value::Null))
    }

    /// Check if a pattern is actionable.
    fn check_actionable(&self, params: &Value) -> Result<Value, ExecutionError> {
        let pattern = params.get("pattern").unwrap_or(&Value::Null);
        let actionable = pattern.get("actionable").and_then(|v| v.as_bool()).unwrap_or(false);
        Ok(json!({"actionable": actionable}))
    }

    /// Compute quality signal from task results.
    fn compute_quality_signal(&self, params: &Value) -> Result<Value, ExecutionError> {
        let stats = params.get("stats").unwrap_or(&Value::Null);

        let total = stats.get("total_stages").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let first_try = stats.get("passed_first_try").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let retries = stats.get("total_retries").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let quality = if total > 0.0 {
            (first_try / total) * (1.0 - (retries * 0.1).min(0.5))
        } else {
            0.0
        };

        Ok(json!({
            "quality": quality,
            "first_try_rate": if total > 0.0 { first_try / total } else { 0.0 },
            "retry_penalty": (retries * 0.1).min(0.5)
        }))
    }

    /// Evaluate pending improvements for regression detection.
    fn evaluate_pending_improvements(&self, params: &Value) -> Result<Value, ExecutionError> {
        let pending = params
            .get("pending")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let current_signal = params.get("current_signal").unwrap_or(&Value::Null);
        let threshold = params
            .get("regression_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.9);

        let current_quality = current_signal
            .get("quality")
            .and_then(|q| q.get("quality"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);

        let evaluations: Vec<Value> = pending
            .iter()
            .map(|improvement| {
                let perf_before = improvement
                    .get("performance_before")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);

                let procedure = improvement
                    .get("procedure")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let ratio = if perf_before > 0.0 {
                    current_quality / perf_before
                } else {
                    1.0
                };

                json!({
                    "procedure": procedure,
                    "performance_before": perf_before,
                    "performance_after": current_quality,
                    "ratio": ratio,
                    "regressed": ratio < threshold
                })
            })
            .collect();

        Ok(json!(evaluations))
    }

    /// Find regressions from evaluations.
    fn find_regressions(&self, params: &Value) -> Result<Value, ExecutionError> {
        let evaluations = params
            .get("evaluations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let regressions: Vec<&Value> = evaluations
            .iter()
            .filter(|e| e.get("regressed").and_then(|v| v.as_bool()).unwrap_or(false))
            .collect();

        if regressions.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(json!(regressions))
        }
    }

    /// Update a running average with a new data point (EMA, alpha=0.2).
    fn update_running_average(&self, params: &Value) -> Result<Value, ExecutionError> {
        let stats = params.get("stats").unwrap_or(&Value::Null);
        let new_quality = params.get("new_quality").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let new_latency = params.get("new_latency").and_then(|v| v.as_u64()).unwrap_or(0);

        let prev_avg_quality = stats.get("avg_quality").and_then(|v| v.as_f64()).unwrap_or(new_quality);
        let prev_avg_latency = stats.get("avg_latency_ms").and_then(|v| v.as_u64()).unwrap_or(new_latency);
        let count = stats.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

        let alpha = 0.2;
        let avg_quality = prev_avg_quality * (1.0 - alpha) + new_quality * alpha;
        let avg_latency = (prev_avg_latency as f64 * (1.0 - alpha) + new_latency as f64 * alpha) as u64;

        Ok(json!({
            "avg_quality": avg_quality,
            "avg_latency_ms": avg_latency,
            "count": count + 1,
            "last_quality": new_quality,
            "last_latency_ms": new_latency
        }))
    }
}

#[async_trait::async_trait]
impl AsyncActionHandler for RsiActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "validate_px_syntax" => self.validate_px_syntax(params),
            "register_procedure" => self.register_procedure_action(params).await,
            "rollback_constraint" => self.rollback_constraint(params).await,
            "compute_stage_stats" => self.compute_stage_stats(params),
            "find_bottleneck" => self.find_bottleneck(params),
            "detect_recurring_pattern" => self.detect_recurring_pattern(params),
            "check_modification_target" => self.check_modification_target(params),
            "check_rate_limit" => self.check_rate_limit(params),
            "all_checks_pass" => self.all_checks_pass(params),
            "check_actionable" => self.check_actionable(params),
            "compute_quality_signal" => self.compute_quality_signal(params),
            "evaluate_pending_improvements" => self.evaluate_pending_improvements(params),
            "find_regressions" => self.find_regressions(params),
            "increment" => self.increment(params),
            "get_last_item" => self.get_last_item(params),
            "update_running_average" => self.update_running_average(params),
            _ => Err(ExecutionError::UnknownAction(name.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoOpHandler;

    #[async_trait::async_trait]
    impl AsyncActionHandler for NoOpHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(Value::Null)
        }
    }

    fn make_handler() -> RsiActionHandler {
        let registry = Arc::new(ReactiveRegistry::new());
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);
        RsiActionHandler::new(registry, handler)
    }

    #[test]
    fn validate_px_syntax_missing_content() {
        let rsi = make_handler();
        let result = rsi.validate_px_syntax(&json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn compute_stage_stats_works() {
        let rsi = make_handler();
        let params = json!({
            "stages": [
                {"name": "analyze", "status": "passed", "attempts": 1, "duration_ms": 5000},
                {"name": "fix", "status": "passed", "attempts": 2, "duration_ms": 8000},
                {"name": "test", "status": "passed", "attempts": 1, "duration_ms": 3000}
            ]
        });

        let result = rsi.compute_stage_stats(&params).unwrap();
        assert_eq!(result["total_stages"], 3);
        assert_eq!(result["passed_first_try"], 2);
        assert_eq!(result["total_retries"], 1);
        assert_eq!(result["total_duration_ms"], 16000);
    }

    #[test]
    fn find_bottleneck_picks_most_retried() {
        let rsi = make_handler();
        let params = json!({
            "stages": [
                {"name": "analyze", "attempts": 1, "duration_ms": 10000},
                {"name": "fix", "attempts": 3, "duration_ms": 2000},
                {"name": "test", "attempts": 1, "duration_ms": 5000}
            ]
        });

        let result = rsi.find_bottleneck(&params).unwrap();
        assert_eq!(result["stage"], "fix");
    }

    #[test]
    fn check_modification_target_blocks_self() {
        let rsi = make_handler();
        let params = json!({
            "target": "recursive-self-improvement",
            "forbidden": ["recursive-self-improvement", "validate_improvement"]
        });

        let result = rsi.check_modification_target(&params).unwrap();
        assert_eq!(result["safe"], false);

        let params_safe = json!({
            "target": "model-selection",
            "forbidden": ["recursive-self-improvement"]
        });
        let result_safe = rsi.check_modification_target(&params_safe).unwrap();
        assert_eq!(result_safe["safe"], true);
    }

    #[test]
    fn check_rate_limit_enforces_max() {
        let rsi = make_handler();
        assert_eq!(rsi.check_rate_limit(&json!({"count": 2, "max": 3})).unwrap()["within_limit"], true);
        assert_eq!(rsi.check_rate_limit(&json!({"count": 3, "max": 3})).unwrap()["within_limit"], false);
    }

    #[test]
    fn all_checks_pass_gates_correctly() {
        let rsi = make_handler();

        let all_good = json!({
            "target_safe": {"safe": true},
            "syntax_valid": {"valid": true},
            "within_limit": {"within_limit": true}
        });
        assert_eq!(rsi.all_checks_pass(&all_good).unwrap()["pass"], true);

        let one_bad = json!({
            "target_safe": {"safe": true},
            "syntax_valid": {"valid": false}
        });
        assert_eq!(rsi.all_checks_pass(&one_bad).unwrap()["pass"], false);
    }

    #[test]
    fn increment_works() {
        let rsi = make_handler();
        assert_eq!(rsi.increment(&json!({"value": 5})).unwrap(), 6);
    }

    #[test]
    fn update_running_average_uses_ema() {
        let rsi = make_handler();
        let params = json!({
            "stats": {"avg_quality": 0.8, "avg_latency_ms": 1000, "count": 10},
            "new_quality": 1.0,
            "new_latency": 500
        });

        let result = rsi.update_running_average(&params).unwrap();
        let avg_q = result["avg_quality"].as_f64().unwrap();
        assert!((avg_q - 0.84).abs() < 0.001);
        assert_eq!(result["count"], 11);
    }

    // ── Scope 2: undo -> remove_constraint wiring (Gate-Zero rollback proof) ──

    use crate::praxis::write_gate::{PraxisWriteGate, WriteConstraint, WriteSeverity};
    use pares_radix_praxis::px::executor::ExecutionError as PxErr;

    /// A trivial always-pass write-check used to register throwaway constraints.
    struct AlwaysOk;
    impl crate::praxis::write_gate::WriteCheck for AlwaysOk {
        fn check(&self, _key: &str, _data: &serde_json::Value) -> Result<(), String> {
            Ok(())
        }
    }

    fn handler_with_gate(gate: Arc<PraxisWriteGate>) -> RsiActionHandler {
        let registry = Arc::new(ReactiveRegistry::new());
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);
        RsiActionHandler::with_write_gate(registry, handler, gate)
    }

    fn add_constraint(gate: &mut PraxisWriteGate, id: &str) {
        gate.add_constraint(
            WriteConstraint {
                id: id.to_string(),
                name: id.to_string(),
                description: "test constraint".into(),
                severity: WriteSeverity::Warning,
                enabled: true,
            },
            Box::new(AlwaysOk),
        );
    }

    /// PROOF for scope 2: a constraint added via the gate is *actually removed*
    /// from `constraint_ids()` when the RSI rollback path fires for its id.
    #[tokio::test]
    async fn rollback_constraint_removes_from_live_gate() {
        // Build a gate, add a loop-applied (non-guard) constraint.
        let mut gate = PraxisWriteGate::new();
        add_constraint(&mut gate, "rsi:learned:no-empty-response");
        let gate = Arc::new(gate);
        assert!(
            gate.constraint_ids()
                .contains(&"rsi:learned:no-empty-response".to_string()),
            "precondition: constraint is present before rollback"
        );

        let rsi = handler_with_gate(Arc::clone(&gate));

        // Trigger undo -> remove for that constraint id.
        let out = rsi
            .rollback_constraint(&json!({"constraint_id": "rsi:learned:no-empty-response"}))
            .await
            .unwrap();

        assert_eq!(out["rolled_back"], true, "rollback must report success");
        assert_eq!(out["mode"], "removed");
        // The real proof: it is GONE from the live enforcement set.
        assert!(
            !gate
                .constraint_ids()
                .contains(&"rsi:learned:no-empty-response".to_string()),
            "constraint must be removed from constraint_ids() after rollback"
        );
    }

    /// The `disable: true` mode keeps the constraint registered but flips enabled.
    #[tokio::test]
    async fn rollback_constraint_disable_mode_keeps_but_disables() {
        let mut gate = PraxisWriteGate::new();
        add_constraint(&mut gate, "rsi:learned:some-rule");
        let gate = Arc::new(gate);
        let rsi = handler_with_gate(Arc::clone(&gate));

        let out = rsi
            .rollback_constraint(&json!({"constraint_id": "rsi:learned:some-rule", "disable": true}))
            .await
            .unwrap();
        assert_eq!(out["rolled_back"], true);
        assert_eq!(out["mode"], "disabled");
        // Still registered (disable, not remove).
        assert!(gate
            .constraint_ids()
            .contains(&"rsi:learned:some-rule".to_string()));
    }

    /// Rolling back an absent id is an idempotent no-op (not an error).
    #[tokio::test]
    async fn rollback_constraint_absent_id_is_noop() {
        let gate = Arc::new(PraxisWriteGate::new());
        let rsi = handler_with_gate(gate);
        let out = rsi
            .rollback_constraint(&json!({"constraint_id": "rsi:learned:does-not-exist"}))
            .await
            .unwrap();
        assert_eq!(out["rolled_back"], false);
        assert_eq!(out["found"], false);
    }

    /// With no gate mounted, rollback reports `unavailable` honestly (no fake success).
    #[tokio::test]
    async fn rollback_constraint_without_gate_reports_unavailable() {
        let rsi = make_handler(); // no write_gate
        let out = rsi
            .rollback_constraint(&json!({"constraint_id": "rsi:learned:x"}))
            .await
            .unwrap();
        assert_eq!(out["rolled_back"], false);
        assert_eq!(out["unavailable"], "no_write_gate");
    }

    #[tokio::test]
    async fn rollback_constraint_missing_id_errors() {
        let rsi = make_handler();
        let res = rsi.rollback_constraint(&json!({})).await;
        assert!(matches!(res, Err(PxErr::ActionFailed { .. })));
    }

    // ── Scope 4: self-guard exemption (R1/B1) ──

    #[test]
    fn is_self_guard_constraint_covers_foundational_and_namespaces() {
        // Platform foundational write guards.
        assert!(is_self_guard_constraint("praxis:no-secrets"));
        assert!(is_self_guard_constraint("praxis:max-size"));
        // Reserved safety namespaces (R1..R6 rails, B1 OpenClaw safety).
        assert!(is_self_guard_constraint("rsi:guard:cannot_modify_self"));
        assert!(is_self_guard_constraint("safety:no-self-harm"));
        assert!(is_self_guard_constraint("oversight:human-approval"));
        assert!(is_self_guard_constraint("openclaw:safety:tool-policy"));
        // Ordinary learned constraints are NOT self-guarded.
        assert!(!is_self_guard_constraint("rsi:learned:no-empty-response"));
        assert!(!is_self_guard_constraint("praxis:some-other"));
    }

    /// PROOF for scope 4: the rollback path REFUSES to remove a self-guard
    /// constraint, and the constraint remains in the live enforcement set.
    #[tokio::test]
    async fn rollback_refuses_to_strip_self_guard_constraint() {
        let mut gate = PraxisWriteGate::new();
        // A safety rail the loop must never be able to auto-remove.
        add_constraint(&mut gate, "rsi:guard:cannot_modify_self");
        let gate = Arc::new(gate);
        let rsi = handler_with_gate(Arc::clone(&gate));

        let out = rsi
            .rollback_constraint(&json!({"constraint_id": "rsi:guard:cannot_modify_self"}))
            .await
            .unwrap();

        assert_eq!(out["rolled_back"], false, "self-guard rollback must be refused");
        assert_eq!(out["refused"], "self_guard");
        // The rail is STILL enforced.
        assert!(
            gate.constraint_ids()
                .contains(&"rsi:guard:cannot_modify_self".to_string()),
            "self-guard constraint must remain after a refused rollback"
        );
    }

    /// Even the platform foundational guards are exempt from auto-rollback.
    #[tokio::test]
    async fn rollback_refuses_to_strip_foundational_guard() {
        // PraxisWriteGate::new() seeds praxis:no-secrets + praxis:max-size.
        let gate = Arc::new(PraxisWriteGate::new());
        let rsi = handler_with_gate(Arc::clone(&gate));

        let out = rsi
            .rollback_constraint(&json!({"constraint_id": "praxis:no-secrets"}))
            .await
            .unwrap();
        assert_eq!(out["refused"], "self_guard");
        assert!(gate
            .constraint_ids()
            .contains(&"praxis:no-secrets".to_string()));
    }
}
