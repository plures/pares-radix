//! The ModelPool — dynamic model management, discovery, and selection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::RwLock;
use tracing::info;

use super::discovery;
use super::exclusion::OverrideStore;
use super::provider;
use super::selection;
use super::types::*;

/// The core model pool — thread-safe, async, dual-mode (config + PluresDB).
pub struct ModelPool {
    /// Path to config/models.toml for dual-mode persistence.
    config_path: PathBuf,
    /// Inner mutable state behind a RwLock.
    inner: Arc<RwLock<PoolInner>>,
}

struct PoolInner {
    /// Configured providers.
    providers: Vec<ProviderConfig>,
    /// All discovered models (from all providers).
    models: Vec<DiscoveredModel>,
    /// User overrides (enable/disable/prefer).
    overrides: OverrideStore,
    /// Selection weights.
    weights: SelectionWeights,
    /// RSI performance data per model (keyed by "provider/model_id").
    performance: HashMap<String, ModelPerformance>,
    /// Last successful discovery time.
    last_refresh: Option<SystemTime>,
}

impl ModelPool {
    /// Create a new ModelPool from a config file path.
    ///
    /// Loads providers + overrides + weights from the config file.
    /// Call `discover_all()` after construction to populate models.
    pub fn from_config(config_path: impl AsRef<Path>) -> Result<Self, String> {
        let path = config_path.as_ref().to_path_buf();

        let providers = provider::load_from_config(&path)?;
        let overrides_list = provider::load_overrides_from_config(&path)?;
        let weights = provider::load_weights_from_config(&path)?;

        info!(
            providers = providers.len(),
            overrides = overrides_list.len(),
            "loaded model pool config"
        );

        let inner = PoolInner {
            providers,
            models: vec![],
            overrides: OverrideStore::from_config(overrides_list),
            weights,
            performance: HashMap::new(),
            last_refresh: None,
        };

        Ok(Self {
            config_path: path,
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    /// Discover models from all enabled providers.
    ///
    /// This is the primary way models enter the pool.
    /// Called on startup and periodically.
    pub async fn discover_all(&self) {
        let providers = {
            let inner = self.inner.read().await;
            inner.providers.clone()
        };

        let mut all_models = Vec::new();
        let mut provider_statuses: Vec<(String, ProviderStatus)> = Vec::new();

        for p in &providers {
            if !p.enabled {
                continue;
            }

            let result = discovery::discover(p).await;
            provider_statuses.push((result.provider.clone(), result.status));
            all_models.extend(result.models);
        }

        let mut inner = self.inner.write().await;

        // Apply user overrides to discovered models
        inner.overrides.apply(&mut all_models);

        // Update provider statuses
        for (name, status) in &provider_statuses {
            if let Some(p) = inner.providers.iter_mut().find(|p| &p.name == name) {
                p.status = *status;
                p.last_checked = Some(SystemTime::now());
                if *status == ProviderStatus::Active {
                    p.last_discovery = Some(SystemTime::now());
                }
            }
        }

        info!(
            total_models = all_models.len(),
            enabled = all_models.iter().filter(|m| m.enabled).count(),
            "discovery complete"
        );

        inner.models = all_models;
        inner.last_refresh = Some(SystemTime::now());
    }

    /// Select the best model for a task.
    pub async fn select_for_task(&self, task: &TaskRequirements) -> Option<ModelSelection> {
        let inner = self.inner.read().await;
        let prefs = inner.overrides.preferred_keys();
        selection::select_best(
            &inner.models,
            task,
            &inner.weights,
            &inner.performance,
            &prefs,
        )
    }

    /// Get pool status (for /status display).
    pub async fn status(&self) -> PoolStatus {
        let inner = self.inner.read().await;
        let providers = inner
            .providers
            .iter()
            .map(|p| ProviderSummary {
                name: p.name.clone(),
                kind: p.kind.clone(),
                status: p.status,
                model_count: inner
                    .models
                    .iter()
                    .filter(|m| m.provider == p.name)
                    .count(),
                enabled: p.enabled,
            })
            .collect();

        let enabled = inner.models.iter().filter(|m| m.enabled).count();
        let disabled = inner.models.len() - enabled;

        PoolStatus {
            providers,
            total_models: inner.models.len(),
            enabled_models: enabled,
            disabled_models: disabled,
            last_refresh: inner.last_refresh,
        }
    }

    /// List all discovered models.
    pub async fn all_models(&self) -> Vec<DiscoveredModel> {
        self.inner.read().await.models.clone()
    }

    /// List enabled models only.
    pub async fn enabled_models(&self) -> Vec<DiscoveredModel> {
        self.inner
            .read()
            .await
            .models
            .iter()
            .filter(|m| m.enabled)
            .cloned()
            .collect()
    }

    /// Disable a model by user request. Immediately effective + persisted to config.
    pub async fn disable_model(
        &self,
        provider: &str,
        model_id: &str,
        reason: Option<String>,
    ) -> Result<(), String> {
        {
            let mut inner = self.inner.write().await;
            inner.overrides.disable(provider, model_id, reason);
            // Apply overrides: set enabled flag on matching models
            let key = format!("{provider}/{model_id}");
            for model in &mut inner.models {
                if model.key() == key {
                    model.enabled = false;
                }
            }
        }
        self.sync_overrides_to_config().await
    }

    /// Enable a model by user request.
    pub async fn enable_model(&self, provider: &str, model_id: &str) -> Result<(), String> {
        {
            let mut inner = self.inner.write().await;
            inner.overrides.enable(provider, model_id);
            // Apply overrides: set enabled flag on matching models
            let key = format!("{provider}/{model_id}");
            for model in &mut inner.models {
                if model.key() == key {
                    model.enabled = true;
                }
            }
        }
        self.sync_overrides_to_config().await
    }

    /// Set preference on a model.
    pub async fn prefer_model(&self, provider: &str, model_id: &str, prefer: bool) {
        let mut inner = self.inner.write().await;
        inner.overrides.set_prefer(provider, model_id, prefer);
    }

    /// Record performance feedback (RSI learning).
    pub async fn record_feedback(&self, feedback: PerformanceFeedback) {
        let mut inner = self.inner.write().await;
        let entry = inner
            .performance
            .entry(feedback.model_key.clone())
            .or_insert_with(|| ModelPerformance {
                model_key: feedback.model_key.clone(),
                ..Default::default()
            });

        // Update running averages
        entry.total_invocations += 1;
        let n = entry.total_invocations as f64;

        // Task-specific score
        let task_score = entry
            .task_scores
            .entry(feedback.task_type.clone())
            .or_insert(0.5);
        // Exponential moving average (alpha = 0.1)
        let alpha = 0.1;
        let success_val = if feedback.success { 1.0 } else { 0.0 };
        *task_score = *task_score * (1.0 - alpha) + success_val * alpha;

        // Overall success rate (simple running average)
        entry.success_rate =
            entry.success_rate * ((n - 1.0) / n) + success_val * (1.0 / n);

        // Average latency
        entry.avg_latency_ms = ((entry.avg_latency_ms as f64 * (n - 1.0) / n)
            + (feedback.latency_ms as f64 / n)) as u64;

        // Error rate
        let error_val = if feedback.success { 0.0 } else { 1.0 };
        entry.error_rate = entry.error_rate * (1.0 - alpha) + error_val * alpha;

        entry.last_used = Some(SystemTime::now());
    }

    /// Get performance data for all models (for /model stats display).
    pub async fn performance_stats(&self) -> HashMap<String, ModelPerformance> {
        self.inner.read().await.performance.clone()
    }

    /// Get provider list.
    pub async fn providers(&self) -> Vec<ProviderConfig> {
        self.inner.read().await.providers.clone()
    }

    /// Reset all user overrides and preferences.
    pub async fn reset(&self) -> Result<(), String> {
        {
            let mut inner = self.inner.write().await;
            inner.overrides = OverrideStore::new();
            // Re-enable all models
            for model in &mut inner.models {
                model.enabled = true;
            }
        }
        self.sync_overrides_to_config().await
    }

    /// Sync current overrides back to the config file (dual-mode persistence).
    async fn sync_overrides_to_config(&self) -> Result<(), String> {
        let overrides = {
            let inner = self.inner.read().await;
            inner
                .overrides
                .all_overrides()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        let refs: Vec<&_> = overrides.iter().collect();
        provider::save_overrides_to_config(&self.config_path, &refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_test_config() -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(
            f,
            r#"
[pool.selection_weights]
capability = 0.4
rsi = 0.3
cost = 0.2
speed = 0.1

[providers.test]
kind = "ollama"
endpoint = "http://localhost:11434"
auth = "none"
enabled = false

[[models.overrides]]
id = "bad-model"
provider = "test"
enabled = false
reason = "too slow"
"#
        )
        .unwrap();
        f
    }

    #[test]
    fn loads_from_config() {
        let config = write_test_config();
        let pool = ModelPool::from_config(config.path()).unwrap();
        // Sync check: can't call async in sync test, but construction succeeded
        let _ = pool;
    }

    #[tokio::test]
    async fn status_empty_pool() {
        let config = write_test_config();
        let pool = ModelPool::from_config(config.path()).unwrap();
        let status = pool.status().await;
        assert_eq!(status.total_models, 0); // no discovery yet
        assert_eq!(status.providers.len(), 1);
    }
}
