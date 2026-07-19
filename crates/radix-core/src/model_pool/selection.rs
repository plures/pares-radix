//! Model selection algorithm.
//!
//! Scores each eligible model against task requirements and selection weights,
//! then returns the best match + fallbacks.

use super::types::*;

/// Score a single model against task requirements.
pub fn score_model(
    model: &DiscoveredModel,
    task: &TaskRequirements,
    weights: &SelectionWeights,
    performance: Option<&ModelPerformance>,
    preferred: bool,
) -> Option<(f64, ScoreBreakdown)> {
    // ── Hard filters (model is ineligible) ───────────────────────────────
    if !model.enabled {
        return None;
    }

    // Context window must accommodate input + output
    let tokens_needed = task.estimated_input_tokens + task.estimated_output_tokens;
    if model.context_window > 0 && model.context_window < tokens_needed {
        return None;
    }

    // Must support required capabilities
    if task.needs_reasoning && !model.reasoning {
        return None;
    }
    if task.needs_vision && !model.supports_vision() {
        return None;
    }

    // ── Capability score (0.0-1.0) ───────────────────────────────────────
    let capability = {
        let mut score: f64 = 0.5; // base score for any eligible model

        // Bonus for reasoning capability when task benefits from it
        if model.reasoning {
            score += 0.15;
        }

        // Bonus for large context window (room to grow)
        if model.context_window >= 500_000 {
            score += 0.1;
        } else if model.context_window >= 200_000 {
            score += 0.05;
        }

        // Bonus for vision when task uses it
        if task.needs_vision && model.supports_vision() {
            score += 0.1;
        }

        // Bonus for large output capacity
        if model.max_output >= 64_000 {
            score += 0.1;
        }

        // Category-based bonus
        match model.category.as_deref() {
            Some("powerful") => score += 0.1,
            Some("versatile") => score += 0.05,
            _ => {}
        }

        score.min(1.0)
    };

    // ── RSI score (0.0-1.0) ──────────────────────────────────────────────
    let rsi = match performance {
        Some(perf) => {
            let task_score = task
                .task_type
                .as_deref()
                .and_then(|tt| perf.task_scores.get(tt))
                .copied()
                .unwrap_or(0.5); // unknown task type = neutral

            // Weight by success rate and task-specific score
            let base = (task_score + perf.success_rate) / 2.0;

            // Penalize high error rate
            let penalty = perf.error_rate * 0.3;

            (base - penalty).clamp(0.0, 1.0)
        }
        // No performance data = neutral (encourage exploration)
        None => 0.5,
    };

    // ── Cost score (0.0-1.0, higher = cheaper) ───────────────────────────
    let cost = if model.cost.is_free() {
        1.0 // subscription/free = maximum cost score
    } else {
        // Normalize: cheaper models score higher.
        // Use output cost as primary signal (usually dominates spend).
        let output_cost = model.cost.output;
        if output_cost <= 1.0 {
            0.9
        } else if output_cost <= 5.0 {
            0.7
        } else if output_cost <= 15.0 {
            0.5
        } else if output_cost <= 50.0 {
            0.3
        } else {
            0.1 // very expensive models
        }
    };

    // ── Speed score (0.0-1.0) ────────────────────────────────────────────
    let speed = {
        // Use historical latency if available
        let latency_score = match performance {
            Some(perf) if perf.avg_latency_ms > 0 => {
                if perf.avg_latency_ms < 1000 {
                    1.0
                } else if perf.avg_latency_ms < 3000 {
                    0.7
                } else if perf.avg_latency_ms < 8000 {
                    0.4
                } else {
                    0.2
                }
            }
            _ => {
                // Estimate from model category
                match model.category.as_deref() {
                    Some("lightweight") => 0.9,
                    Some("versatile") => 0.6,
                    Some("powerful") => 0.4,
                    _ => 0.5,
                }
            }
        };

        // Adjust for urgency
        match task.urgency {
            Urgency::High => latency_score,               // speed matters a lot
            Urgency::Normal => latency_score * 0.8 + 0.2, // moderate influence
            Urgency::Low => 0.5,                          // speed doesn't matter, equalize
        }
    };

    // ── Preference boost ─────────────────────────────────────────────────
    let prefer_boost = if preferred { 0.15 } else { 0.0 };

    // ── Weighted total ───────────────────────────────────────────────────
    let total = capability * weights.capability
        + rsi * weights.rsi
        + cost * weights.cost
        + speed * weights.speed
        + prefer_boost;

    let breakdown = ScoreBreakdown {
        capability,
        rsi,
        cost,
        speed,
        prefer_boost,
    };

    Some((total, breakdown))
}

/// Select the best model(s) from a pool for a given task.
pub fn select_best(
    models: &[DiscoveredModel],
    task: &TaskRequirements,
    weights: &SelectionWeights,
    performance: &std::collections::HashMap<String, ModelPerformance>,
    preferences: &std::collections::HashSet<String>,
) -> Option<ModelSelection> {
    let mut scored: Vec<(usize, f64, ScoreBreakdown)> = models
        .iter()
        .enumerate()
        .filter_map(|(i, model)| {
            let perf = performance.get(&model.key());
            let preferred = preferences.contains(&model.key());
            score_model(model, task, weights, perf, preferred)
                .map(|(score, breakdown)| (i, score, breakdown))
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (primary_idx, primary_score, primary_breakdown) = scored.first()?;

    let fallbacks: Vec<DiscoveredModel> = scored
        .iter()
        .skip(1)
        .take(2) // top 2 fallbacks
        .map(|(idx, _, _)| models[*idx].clone())
        .collect();

    Some(ModelSelection {
        model: models[*primary_idx].clone(),
        score: *primary_score,
        score_breakdown: primary_breakdown.clone(),
        fallbacks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model(id: &str, category: &str, reasoning: bool, cost_output: f64) -> DiscoveredModel {
        DiscoveredModel {
            id: id.to_string(),
            name: id.to_string(),
            provider: "test".to_string(),
            vendor: None,
            category: Some(category.to_string()),
            api: None,
            context_window: 200_000,
            max_output: 64_000,
            input_types: vec!["text".into(), "image".into()],
            reasoning,
            reasoning_levels: if reasoning {
                vec!["low".into(), "high".into()]
            } else {
                vec![]
            },
            cost: ModelCost {
                input: cost_output / 2.0,
                output: cost_output,
                ..Default::default()
            },
            preview: false,
            enabled: true,
        }
    }

    #[test]
    fn disabled_model_excluded() {
        let mut model = test_model("disabled", "powerful", true, 0.0);
        model.enabled = false;
        let task = TaskRequirements::default();
        let weights = SelectionWeights::default();
        assert!(score_model(&model, &task, &weights, None, false).is_none());
    }

    #[test]
    fn context_window_filter() {
        let model = test_model("small-ctx", "powerful", true, 0.0);
        let task = TaskRequirements {
            estimated_input_tokens: 150_000,
            estimated_output_tokens: 100_000,
            ..Default::default()
        };
        let weights = SelectionWeights::default();
        // 200K context can't fit 250K tokens
        assert!(score_model(&model, &task, &weights, None, false).is_none());
    }

    #[test]
    fn reasoning_requirement_filter() {
        let model = test_model("no-reasoning", "versatile", false, 0.0);
        let task = TaskRequirements {
            needs_reasoning: true,
            ..Default::default()
        };
        let weights = SelectionWeights::default();
        assert!(score_model(&model, &task, &weights, None, false).is_none());
    }

    #[test]
    fn free_models_score_higher_on_cost() {
        let free = test_model("free", "powerful", true, 0.0);
        let expensive = test_model("expensive", "powerful", true, 75.0);
        let task = TaskRequirements::default();
        let weights = SelectionWeights::default();

        let (free_score, _) = score_model(&free, &task, &weights, None, false).unwrap();
        let (exp_score, _) = score_model(&expensive, &task, &weights, None, false).unwrap();
        assert!(free_score > exp_score);
    }

    #[test]
    fn prefer_boost_works() {
        let model = test_model("preferred", "versatile", false, 5.0);
        let task = TaskRequirements::default();
        let weights = SelectionWeights::default();

        let (normal, _) = score_model(&model, &task, &weights, None, false).unwrap();
        let (boosted, _) = score_model(&model, &task, &weights, None, true).unwrap();
        assert!(boosted > normal);
        assert!((boosted - normal - 0.15).abs() < 0.001);
    }

    #[test]
    fn select_best_returns_primary_and_fallbacks() {
        let models = vec![
            test_model("cheap", "lightweight", false, 1.0),
            test_model("mid", "versatile", true, 15.0),
            test_model("expensive", "powerful", true, 75.0),
        ];
        let task = TaskRequirements::default();
        let weights = SelectionWeights::default();
        let perf = std::collections::HashMap::new();
        let prefs = std::collections::HashSet::new();

        let selection = select_best(&models, &task, &weights, &perf, &prefs).unwrap();
        // Free/cheap models should win for a default task
        assert_eq!(selection.fallbacks.len(), 2);
    }
}
