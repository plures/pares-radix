//! Telegram command integration for the ModelPool.
//!
//! Provides the `TelegramPoolControl` trait + adapter that replaces `TelegramModelControl`.
//! This module lives here (not in channels) to avoid circular deps.

use super::pool::ModelPool;
use super::types::*;
use std::sync::Arc;

/// New-style model control trait for Telegram `/model` and `/status` commands.
/// Replaces the old `TelegramModelControl` (primary/deep pair) with pool-aware operations.
#[async_trait::async_trait]
pub trait PoolControl: Send + Sync {
    /// Status line for `/status` display (e.g. "14 models (3 providers)").
    async fn status_line(&self) -> String;

    /// Full model list for `/model list`.
    async fn model_list(&self) -> String;

    /// Disable a model: `/model disable <id> [reason]`.
    async fn disable(&self, model_id: &str, reason: Option<&str>) -> Result<String, String>;

    /// Enable a model: `/model enable <id>`.
    async fn enable(&self, model_id: &str) -> Result<String, String>;

    /// Set preference: `/model prefer <id>`.
    async fn prefer(&self, model_id: &str) -> Result<String, String>;

    /// Show stats: `/model stats [id]`.
    async fn stats(&self, model_id: Option<&str>) -> String;

    /// Reset all user overrides: `/model reset`.
    async fn reset(&self) -> Result<String, String>;

    /// Trigger re-discovery from all providers: `/model refresh`.
    async fn refresh(&self) -> String;

    /// Legacy compat: return (primary, deep) strings for old status display.
    async fn legacy_model_pair(&self) -> (String, String);

    /// Structured, paginated catalog snapshot. Triggers a discovery refresh
    /// before slicing so the snapshot reflects live provider state.
    /// `page` is 0-indexed; `page_size` of 0 is treated as "all in one page".
    async fn catalog(&self, page: usize, page_size: usize) -> ModelCatalogPage;

    /// Select a model by its exact provider/model key ("provider/model_id")
    /// or by bare model_id when unambiguous across providers.
    async fn select_by_key(&self, key: &str) -> Result<ModelCatalogEntry, String>;
}

/// Adapter: wraps a `ModelPool` to implement `PoolControl`.
pub struct PoolControlAdapter {
    pool: Arc<ModelPool>,
}

impl PoolControlAdapter {
    pub fn new(pool: Arc<ModelPool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PoolControl for PoolControlAdapter {
    async fn status_line(&self) -> String {
        let status = self.pool.status().await;
        let active_providers = status
            .providers
            .iter()
            .filter(|p| p.status == ProviderStatus::Active)
            .count();
        format!(
            "{} models ({} enabled, {} providers)",
            status.total_models, status.enabled_models, active_providers
        )
    }

    async fn model_list(&self) -> String {
        let models = self.pool.all_models().await;
        if models.is_empty() {
            return "No models discovered yet. Run /model refresh to discover.".to_string();
        }

        let stats = self.pool.performance_stats().await;
        let mut lines = Vec::new();
        lines.push(format!("📋 <b>Model Pool</b> ({} total)\n", models.len()));

        // Group by provider
        let mut by_provider: std::collections::HashMap<&str, Vec<&DiscoveredModel>> =
            std::collections::HashMap::new();
        for m in &models {
            by_provider.entry(m.provider.as_str()).or_default().push(m);
        }

        for (provider, provider_models) in &by_provider {
            lines.push(format!(
                "<b>{}</b> ({} models)",
                provider,
                provider_models.len()
            ));
            for m in provider_models {
                let status_icon = if m.enabled { "✅" } else { "❌" };
                let perf_hint = stats.get(&m.key()).map(|p| {
                    format!(
                        " · {:.0}% success · {}ms avg",
                        p.success_rate * 100.0,
                        p.avg_latency_ms
                    )
                });
                let cost_hint = if m.cost.is_free() {
                    " · $0".to_string()
                } else {
                    format!(" · ${:.2}/1M out", m.cost.output)
                };
                let preview_tag = if m.preview { " 🧪" } else { "" };
                lines.push(format!(
                    "  {status_icon} <code>{}</code>{preview_tag}{cost_hint}{}",
                    m.id,
                    perf_hint.unwrap_or_default()
                ));
            }
            lines.push(String::new());
        }

        lines.join("\n")
    }

    async fn disable(&self, model_id: &str, reason: Option<&str>) -> Result<String, String> {
        // Find the model across all providers
        let models = self.pool.all_models().await;
        let found: Vec<&DiscoveredModel> = models.iter().filter(|m| m.id == model_id).collect();

        match found.len() {
            0 => Err(format!("Model '{model_id}' not found in pool.")),
            1 => {
                let provider = &found[0].provider;
                self.pool
                    .disable_model(provider, model_id, reason.map(|s| s.to_string()))
                    .await?;
                Ok(format!(
                    "❌ Disabled <code>{model_id}</code>{}",
                    reason
                        .map(|r| format!(" (reason: {r})"))
                        .unwrap_or_default()
                ))
            }
            _ => {
                // Multiple providers have this model — disable all
                for m in &found {
                    self.pool
                        .disable_model(&m.provider, model_id, reason.map(|s| s.to_string()))
                        .await?;
                }
                Ok(format!(
                    "❌ Disabled <code>{model_id}</code> across {} providers",
                    found.len()
                ))
            }
        }
    }

    async fn enable(&self, model_id: &str) -> Result<String, String> {
        let models = self.pool.all_models().await;
        let found: Vec<&DiscoveredModel> = models
            .iter()
            .filter(|m| m.id == model_id && !m.enabled)
            .collect();

        if found.is_empty() {
            // Check if it exists but is already enabled
            let exists = models.iter().any(|m| m.id == model_id);
            if exists {
                return Ok(format!("<code>{model_id}</code> is already enabled."));
            }
            return Err(format!("Model '{model_id}' not found in pool."));
        }

        for m in &found {
            self.pool.enable_model(&m.provider, model_id).await?;
        }
        Ok(format!("✅ Enabled <code>{model_id}</code>"))
    }

    async fn prefer(&self, model_id: &str) -> Result<String, String> {
        let models = self.pool.all_models().await;
        let found: Vec<&DiscoveredModel> = models.iter().filter(|m| m.id == model_id).collect();

        if found.is_empty() {
            return Err(format!("Model '{model_id}' not found in pool."));
        }

        for m in &found {
            self.pool.prefer_model(&m.provider, model_id, true).await;
        }
        Ok(format!(
            "⭐ Preferred <code>{model_id}</code> (soft boost in selection)"
        ))
    }

    async fn stats(&self, model_id: Option<&str>) -> String {
        let stats = self.pool.performance_stats().await;

        match model_id {
            Some(id) => {
                // Find matching entries
                let matches: Vec<_> = stats
                    .iter()
                    .filter(|(k, _)| k.ends_with(&format!("/{id}")) || k.as_str() == id)
                    .collect();

                if matches.is_empty() {
                    return format!("No performance data for '{id}' yet.");
                }

                let mut lines = vec![format!("📊 <b>Stats for {id}</b>\n")];
                for (key, perf) in matches {
                    lines.push(format!("<code>{key}</code>"));
                    lines.push(format!(
                        "  Success: {:.1}% ({} invocations)",
                        perf.success_rate * 100.0,
                        perf.total_invocations
                    ));
                    lines.push(format!("  Avg latency: {}ms", perf.avg_latency_ms));
                    lines.push(format!("  Error rate: {:.1}%", perf.error_rate * 100.0));
                    if !perf.task_scores.is_empty() {
                        lines.push("  Task scores:".into());
                        for (task, score) in &perf.task_scores {
                            lines.push(format!("    {task}: {score:.2}"));
                        }
                    }
                    lines.push(String::new());
                }
                lines.join("\n")
            }
            None => {
                if stats.is_empty() {
                    return "No performance data recorded yet.".to_string();
                }

                let mut lines = vec![format!(
                    "📊 <b>Model Performance</b> ({} tracked)\n",
                    stats.len()
                )];
                let mut sorted: Vec<_> = stats.iter().collect();
                sorted.sort_by(|a, b| {
                    b.1.success_rate
                        .partial_cmp(&a.1.success_rate)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for (key, perf) in sorted.iter().take(10) {
                    lines.push(format!(
                        "  <code>{key}</code>: {:.0}% · {}ms · {} calls",
                        perf.success_rate * 100.0,
                        perf.avg_latency_ms,
                        perf.total_invocations
                    ));
                }
                if stats.len() > 10 {
                    lines.push(format!("  ... and {} more", stats.len() - 10));
                }
                lines.join("\n")
            }
        }
    }

    async fn reset(&self) -> Result<String, String> {
        self.pool.reset().await?;
        Ok("🔄 All model overrides and preferences cleared. All models re-enabled.".to_string())
    }

    async fn refresh(&self) -> String {
        self.pool.discover_all().await;
        let status = self.pool.status().await;
        format!(
            "🔄 Discovery complete: {} models from {} providers ({} enabled)",
            status.total_models,
            status
                .providers
                .iter()
                .filter(|p| p.status == ProviderStatus::Active)
                .count(),
            status.enabled_models
        )
    }

    async fn legacy_model_pair(&self) -> (String, String) {
        // For backward compat with /status: pick top 2 enabled models
        let models = self.pool.enabled_models().await;
        let primary = models
            .first()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "none".into());
        let deep = models
            .iter()
            .find(|m| m.reasoning && m.id != primary)
            .or_else(|| models.get(1))
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "none".into());
        (primary, deep)
    }

    async fn catalog(&self, page: usize, page_size: usize) -> ModelCatalogPage {
        // Refresh discovery so the snapshot reflects live provider state.
        self.pool.discover_all().await;

        let models = self.pool.all_models().await;
        build_catalog_page(&models, page, page_size)
    }

    async fn select_by_key(&self, key: &str) -> Result<ModelCatalogEntry, String> {
        let models = self.pool.all_models().await;
        select_entry_by_key(&models, key)
    }
}

/// Build a paginated catalog snapshot from an already-discovered model list.
/// Extracted as a pure function so pagination/ordering logic is unit-testable
/// without requiring live provider discovery.
fn build_catalog_page(models: &[DiscoveredModel], page: usize, page_size: usize) -> ModelCatalogPage {
    let mut sorted: Vec<&DiscoveredModel> = models.iter().collect();
    sorted.sort_by(|a, b| a.provider.cmp(&b.provider).then_with(|| a.id.cmp(&b.id)));
    let total = sorted.len();

    let (start, end) = if page_size == 0 {
        (0, total)
    } else {
        let start = (page * page_size).min(total);
        let end = (start + page_size).min(total);
        (start, end)
    };

    let entries: Vec<ModelCatalogEntry> = sorted[start..end].iter().map(|m| ModelCatalogEntry::from(*m)).collect();
    let has_more = end < total;

    ModelCatalogPage {
        entries,
        total,
        page,
        page_size: if page_size == 0 { total } else { page_size },
        has_more,
        snapshot_at: std::time::SystemTime::now(),
    }
}

/// Select a catalog entry by exact "provider/model_id" key, or by bare
/// model_id when unambiguous across providers. Pure function, unit-testable.
fn select_entry_by_key(models: &[DiscoveredModel], key: &str) -> Result<ModelCatalogEntry, String> {
    if let Some(m) = models.iter().find(|m| m.key() == key) {
        return Ok(ModelCatalogEntry::from(m));
    }

    let matches: Vec<&DiscoveredModel> = models.iter().filter(|m| m.id == key).collect();
    match matches.len() {
        0 => Err(format!("No model found for key '{key}'.")),
        1 => Ok(ModelCatalogEntry::from(matches[0])),
        n => Err(format!(
            "Model id '{key}' is ambiguous across {n} providers; use 'provider/{key}'."
        )),
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    fn model(provider: &str, id: &str, enabled: bool) -> DiscoveredModel {
        DiscoveredModel {
            id: id.to_string(),
            name: format!("{id} display"),
            provider: provider.to_string(),
            vendor: None,
            category: None,
            api: None,
            context_window: 0,
            max_output: 0,
            input_types: vec![],
            reasoning: false,
            reasoning_levels: vec![],
            cost: ModelCost::default(),
            preview: false,
            enabled,
        }
    }

    #[test]
    fn catalog_page_all_in_one_page_when_page_size_zero() {
        let models = vec![
            model("openai", "gpt-5", true),
            model("anthropic", "claude-opus", true),
        ];
        let page = build_catalog_page(&models, 0, 0);
        assert_eq!(page.total, 2);
        assert_eq!(page.entries.len(), 2);
        assert!(!page.has_more);
        // Sorted by provider then id: anthropic before openai
        assert_eq!(page.entries[0].provider, "anthropic");
        assert_eq!(page.entries[1].provider, "openai");
    }

    #[test]
    fn catalog_page_paginates_and_flags_has_more() {
        let models = vec![
            model("a", "m1", true),
            model("a", "m2", true),
            model("a", "m3", true),
        ];
        let page0 = build_catalog_page(&models, 0, 2);
        assert_eq!(page0.entries.len(), 2);
        assert_eq!(page0.total, 3);
        assert!(page0.has_more);

        let page1 = build_catalog_page(&models, 1, 2);
        assert_eq!(page1.entries.len(), 1);
        assert_eq!(page1.entries[0].model_id, "m3");
        assert!(!page1.has_more);
    }

    #[test]
    fn catalog_page_out_of_range_returns_empty() {
        let models = vec![model("a", "m1", true)];
        let page = build_catalog_page(&models, 5, 2);
        assert_eq!(page.entries.len(), 0);
        assert!(!page.has_more);
    }

    #[test]
    fn select_by_composite_key_exact_match() {
        let models = vec![model("openai", "gpt-5", true), model("anthropic", "gpt-5", true)];
        let entry = select_entry_by_key(&models, "openai/gpt-5").unwrap();
        assert_eq!(entry.provider, "openai");
        assert_eq!(entry.model_id, "gpt-5");
    }

    #[test]
    fn select_by_bare_id_unambiguous() {
        let models = vec![model("openai", "gpt-5", true), model("anthropic", "claude-opus", true)];
        let entry = select_entry_by_key(&models, "claude-opus").unwrap();
        assert_eq!(entry.provider, "anthropic");
    }

    #[test]
    fn select_by_bare_id_ambiguous_errors() {
        let models = vec![model("openai", "gpt-5", true), model("anthropic", "gpt-5", true)];
        let err = select_entry_by_key(&models, "gpt-5").unwrap_err();
        assert!(err.contains("ambiguous"));
    }

    #[test]
    fn select_by_key_not_found_errors() {
        let models = vec![model("openai", "gpt-5", true)];
        let err = select_entry_by_key(&models, "nonexistent").unwrap_err();
        assert!(err.contains("No model found"));
    }
}
