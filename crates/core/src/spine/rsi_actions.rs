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

/// RSI action handler — provides boundary actors for recursive self-improvement.
///
/// Holds references to the ReactiveRegistry (for hot-reload) and the shared
/// action handler (for creating new adapters).
pub struct RsiActionHandler {
    registry: Arc<ReactiveRegistry>,
    handler: Arc<dyn AsyncActionHandler>,
    /// Tracks which patterns procedures were registered under (for replacement).
    procedure_patterns: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

impl RsiActionHandler {
    pub fn new(registry: Arc<ReactiveRegistry>, handler: Arc<dyn AsyncActionHandler>) -> Self {
        Self {
            registry,
            handler,
            procedure_patterns: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
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
}
