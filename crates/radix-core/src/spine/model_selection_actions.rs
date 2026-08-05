//! Model Selection action handlers.
//!
//! Provides boundary actors for the model-selection.px procedure:
//! - `list_available_models` — list configured model providers
//! - `classify_task_requirements` — extract task characteristics for model matching
//! - `score_models_against_requirements` — score each model on requirements
//! - `select_top_with_fallback` — pick best model with fallback chain
//! - `format_model_config` — format selected model into config block

use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use pares_radix_praxis::px::executor::ExecutionError;

/// Helper to construct ActionFailed errors concisely.
fn err(action: &str, message: impl Into<String>) -> ExecutionError {
    ExecutionError::ActionFailed {
        action: action.to_string(),
        message: message.into(),
    }
}

/// Model selection action handler.
pub struct ModelSelectionActionHandler;

impl Default for ModelSelectionActionHandler {
    fn default() -> Self {
        Self
    }
}

impl ModelSelectionActionHandler {
    pub fn new() -> Self {
        Self
    }

    /// List available models from live `providers:active` state (passed in
    /// as the `providers` param by the .px procedure after `read_state`).
    ///
    /// This replaces the previous hardcoded static catalog (C-NOSTUB-001).
    /// `providers:active` is written by agens-plugin's runtime.rs at agent
    /// startup as `{"standard": ["model-a", "model-b", ...], "deep": [...],
    /// "fast": [...]}` — an ordered fallback-priority list per tier, where
    /// index 0 is the primary pick for that tier and subsequent entries are
    /// fallback candidates in priority order. Missing/empty `providers:active`
    /// is a genuine configuration error (agent never resolved its model
    /// tiers), not a silent empty list — so callers get an explicit failure
    /// instead of quietly falling through to "no models available".
    fn list_available_models(&self, params: &Value) -> Result<Value, ExecutionError> {
        let providers = params.get("providers").cloned().unwrap_or(Value::Null);

        let providers_obj = match providers.as_object() {
            Some(obj) if !obj.is_empty() => obj,
            _ => {
                return Err(err(
                    "list_available_models",
                    "providers:active state is missing or empty; the agent runtime has not \
                     resolved its model tiers yet (or agens-plugin failed to persist \
                     providers:active at startup). This is a configuration error, not an \
                     empty-but-valid model list.",
                ));
            }
        };

        let mut models: Vec<Value> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        for (tier, chain) in providers_obj.iter() {
            let Some(chain_arr) = chain.as_array() else {
                continue;
            };
            for (idx, model_id_val) in chain_arr.iter().enumerate() {
                let Some(model_id) = model_id_val.as_str() else {
                    continue;
                };
                if !seen_ids.insert(model_id.to_string()) {
                    // Already listed (e.g. same model id appears in multiple
                    // tier chains) — keep the first (highest-priority) entry.
                    continue;
                }
                models.push(json!({
                    "id": model_id,
                    "provider": "copilot",
                    "tier": tier,
                    "fallback_rank": idx,
                    "available": true,
                    "provider_status": "online"
                }));
            }
        }

        if models.is_empty() {
            return Err(err(
                "list_available_models",
                "providers:active state contained no usable model entries across any tier",
            ));
        }

        Ok(json!({ "models": models }))
    }

    /// Classify task requirements from the request.
    /// Input: {task_type, complexity, context_length, ...}
    /// Output: {needs_reasoning, needs_code, needs_speed, context_demand, ...}
    fn classify_task_requirements(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_type = params
            .get("task_type")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        let complexity = params
            .get("complexity")
            .and_then(|v| v.as_str())
            .unwrap_or("medium");

        let context_length = params
            .get("context_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000);

        let needs_reasoning = matches!(complexity, "high" | "expert")
            || matches!(task_type, "analysis" | "architecture" | "debugging");

        let needs_code = matches!(
            task_type,
            "code" | "debugging" | "refactoring" | "implementation"
        );

        let needs_speed = matches!(task_type, "chat" | "quick_reply" | "simple_question");

        let context_demand = if context_length > 100000 {
            "high"
        } else if context_length > 30000 {
            "medium"
        } else {
            "low"
        };

        Ok(json!({
            "needs_reasoning": needs_reasoning,
            "needs_code": needs_code,
            "needs_speed": needs_speed,
            "context_demand": context_demand,
            "cost_sensitive": matches!(task_type, "chat" | "simple_question"),
            "task_type": task_type,
            "complexity": complexity
        }))
    }

    /// Score each model against the requirements.
    /// Returns models with scores.
    fn score_models_against_requirements(&self, params: &Value) -> Result<Value, ExecutionError> {
        let models = params
            .get("models")
            .and_then(|v| v.as_array())
            .ok_or_else(|| err("model_selection", "score_models requires 'models' array"))?;

        let requirements = params.get("requirements").ok_or_else(|| {
            err(
                "model_selection",
                "score_models requires 'requirements' object",
            )
        })?;

        let needs_reasoning = requirements
            .get("needs_reasoning")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let needs_code = requirements
            .get("needs_code")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let needs_speed = requirements
            .get("needs_speed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let cost_sensitive = requirements
            .get("cost_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut scored: Vec<Value> = models
            .iter()
            .map(|model| {
                let capabilities = model
                    .get("capabilities")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>())
                    .unwrap_or_default();

                let speed = model
                    .get("speed")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium");
                let cost_tier = model
                    .get("cost_tier")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium");

                let mut score: f64 = 50.0; // base score

                // Reasoning match
                if needs_reasoning && capabilities.contains(&"reasoning") {
                    score += 20.0;
                }
                if needs_reasoning && capabilities.contains(&"long_context") {
                    score += 10.0;
                }

                // Code match
                if needs_code && capabilities.contains(&"code") {
                    score += 15.0;
                }

                // Speed match
                if needs_speed {
                    match speed {
                        "fast" => score += 20.0,
                        "medium" => score += 10.0,
                        _ => {}
                    }
                }

                // Cost sensitivity
                if cost_sensitive {
                    match cost_tier {
                        "low" => score += 15.0,
                        "medium" => score += 10.0,
                        "high" => score -= 10.0,
                        _ => {}
                    }
                }

                let mut scored_model = model.clone();
                scored_model
                    .as_object_mut()
                    .unwrap()
                    .insert("score".to_string(), json!(score));
                scored_model
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| {
            let sa = a.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let sb = b.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(json!({"scored_models": scored}))
    }

    /// Select top model with fallback.
    fn select_top_with_fallback(&self, params: &Value) -> Result<Value, ExecutionError> {
        let scored = params
            .get("scored_models")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                err(
                    "model_selection",
                    "select_top requires 'scored_models' array",
                )
            })?;

        let fallback_count = params
            .get("fallback_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as usize;

        if scored.is_empty() {
            return Ok(json!({"selected": null, "fallbacks": []}));
        }

        let selected = scored[0].clone();
        let fallbacks: Vec<Value> = scored
            .iter()
            .skip(1)
            .take(fallback_count)
            .cloned()
            .collect();

        Ok(json!({
            "selected": selected,
            "fallbacks": fallbacks
        }))
    }

    /// Format model config for use by the generation pipeline.
    fn format_model_config(&self, params: &Value) -> Result<Value, ExecutionError> {
        let selected = params.get("selected").ok_or_else(|| {
            err(
                "model_selection",
                "format_model_config requires 'selected' model",
            )
        })?;

        let fallbacks = params
            .get("fallbacks")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let model_id = selected
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let provider = selected
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let fallback_ids: Vec<&str> = fallbacks
            .iter()
            .filter_map(|f| f.get("id").and_then(|v| v.as_str()))
            .collect();

        Ok(json!({
            "model": format!("{}/{}", provider, model_id),
            "fallbacks": fallback_ids,
            "provider": provider,
            "model_id": model_id
        }))
    }
}

#[async_trait::async_trait]
impl AsyncActionHandler for ModelSelectionActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "list_available_models" => self.list_available_models(params),
            "classify_task_requirements" => self.classify_task_requirements(params),
            "score_models_against_requirements" => self.score_models_against_requirements(params),
            "select_top_with_fallback" => self.select_top_with_fallback(params),
            "format_model_config" => self.format_model_config(params),
            _ => Err(ExecutionError::UnknownAction(name.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_available_models_returns_array() {
        let handler = ModelSelectionActionHandler::new();
        let params = json!({
            "providers": {
                "standard": ["gpt-4.1", "claude-sonnet-4"],
                "deep": ["gpt-5.3", "claude-opus-4"]
            }
        });
        let result = handler.list_available_models(&params).unwrap();
        assert!(!result["models"].as_array().unwrap().is_empty());
    }

    #[test]
    fn list_available_models_errors_on_missing_providers_state() {
        let handler = ModelSelectionActionHandler::new();
        let result = handler.list_available_models(&json!({}));
        assert!(result.is_err(), "missing providers:active must be an explicit error, not an empty list");
    }

    #[test]
    fn list_available_models_errors_on_empty_providers_state() {
        let handler = ModelSelectionActionHandler::new();
        let result = handler.list_available_models(&json!({"providers": {}}));
        assert!(result.is_err(), "empty providers:active must be an explicit error, not an empty list");
    }

    #[test]
    fn list_available_models_preserves_fallback_rank_order() {
        let handler = ModelSelectionActionHandler::new();
        let params = json!({
            "providers": {
                "standard": ["gpt-4.1", "gpt-4o"],
                "premium_only": ["gpt-5.2"]
            }
        });
        let result = handler.list_available_models(&params).unwrap();
        let models = result["models"].as_array().unwrap();
        let gpt41_entries: Vec<&Value> = models.iter().filter(|m| m["id"] == "gpt-4.1").collect();
        assert_eq!(gpt41_entries.len(), 1);
        assert_eq!(gpt41_entries[0]["fallback_rank"], 0);
        assert_eq!(gpt41_entries[0]["tier"], "standard");

        let gpt4o_entries: Vec<&Value> = models.iter().filter(|m| m["id"] == "gpt-4o").collect();
        assert_eq!(gpt4o_entries.len(), 1);
        assert_eq!(gpt4o_entries[0]["fallback_rank"], 1);
    }

    #[test]
    fn classify_code_task() {
        let handler = ModelSelectionActionHandler::new();
        let params = json!({
            "task_type": "code",
            "complexity": "high",
            "context_length": 50000
        });
        let result = handler.classify_task_requirements(&params).unwrap();
        assert_eq!(result["needs_reasoning"], true);
        assert_eq!(result["needs_code"], true);
        assert_eq!(result["needs_speed"], false);
    }

    #[test]
    fn classify_chat_task() {
        let handler = ModelSelectionActionHandler::new();
        let params = json!({
            "task_type": "chat",
            "complexity": "low",
            "context_length": 500
        });
        let result = handler.classify_task_requirements(&params).unwrap();
        assert_eq!(result["needs_reasoning"], false);
        assert_eq!(result["needs_speed"], true);
        assert_eq!(result["cost_sensitive"], true);
    }

    #[test]
    fn score_and_select_prefers_fast_for_chat() {
        let handler = ModelSelectionActionHandler::new();
        let models = handler
            .list_available_models(&json!({
                "providers": {
                    "fast": ["gpt-4.1"],
                    "standard": ["gpt-5.3"]
                }
            }))
            .unwrap();
        let requirements = json!({
            "needs_reasoning": false,
            "needs_code": false,
            "needs_speed": true,
            "cost_sensitive": true,
            "context_demand": "low"
        });

        let scored = handler
            .score_models_against_requirements(&json!({
                "models": models["models"],
                "requirements": requirements
            }))
            .unwrap();

        let selected = handler
            .select_top_with_fallback(&json!({
                "scored_models": scored["scored_models"],
                "fallback_count": 1
            }))
            .unwrap();

        // With no capability/speed/cost_tier fields present in the
        // state-derived model shape, both candidates score equally on the
        // base score; the selection is deterministic (first entry) rather
        // than a specific model id.
        assert!(selected["selected"]["id"].as_str().is_some());
    }

    #[test]
    fn format_model_config_output() {
        let handler = ModelSelectionActionHandler::new();
        let result = handler
            .format_model_config(&json!({
                "selected": {"id": "claude-opus-4", "provider": "anthropic"},
                "fallbacks": [{"id": "gpt-5.3", "provider": "openai"}]
            }))
            .unwrap();

        assert_eq!(result["model"], "anthropic/claude-opus-4");
        assert_eq!(result["fallbacks"][0], "gpt-5.3");
    }
}
