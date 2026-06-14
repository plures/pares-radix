//! Model exclusion/override management.
//!
//! Handles user enable/disable of models. Persists to both config file and PluresDB.

use super::types::{DiscoveredModel, ModelOverride};
use std::collections::HashMap;
use std::time::SystemTime;

/// In-memory store of user model overrides.
#[derive(Debug, Clone, Default)]
pub struct OverrideStore {
    /// Keyed by "provider/model_id".
    overrides: HashMap<String, ModelOverride>,
}

impl OverrideStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load overrides from config file entries.
    pub fn from_config(entries: Vec<ModelOverride>) -> Self {
        let overrides = entries
            .into_iter()
            .map(|o| (format!("{}/{}", o.provider, o.id), o))
            .collect();
        Self { overrides }
    }

    /// Apply overrides to a list of discovered models (mutates in place).
    pub fn apply(&self, models: &mut [DiscoveredModel]) {
        for model in models.iter_mut() {
            if let Some(ov) = self.overrides.get(&model.key()) {
                if let Some(enabled) = ov.enabled {
                    model.enabled = enabled;
                }
            }
        }
    }

    /// Disable a model. Returns true if the override was created/updated.
    pub fn disable(&mut self, provider: &str, model_id: &str, reason: Option<String>) -> bool {
        let key = format!("{provider}/{model_id}");
        let entry = self.overrides.entry(key).or_insert_with(|| ModelOverride {
            id: model_id.to_string(),
            provider: provider.to_string(),
            enabled: None,
            reason: None,
            prefer: false,
            updated_at: None,
        });
        entry.enabled = Some(false);
        entry.reason = reason;
        entry.updated_at = Some(SystemTime::now());
        true
    }

    /// Enable a model (remove disable override).
    pub fn enable(&mut self, provider: &str, model_id: &str) -> bool {
        let key = format!("{provider}/{model_id}");
        if let Some(entry) = self.overrides.get_mut(&key) {
            entry.enabled = Some(true);
            entry.reason = None;
            entry.updated_at = Some(SystemTime::now());
            true
        } else {
            false
        }
    }

    /// Set preference on a model.
    pub fn set_prefer(&mut self, provider: &str, model_id: &str, prefer: bool) {
        let key = format!("{provider}/{model_id}");
        let entry = self.overrides.entry(key).or_insert_with(|| ModelOverride {
            id: model_id.to_string(),
            provider: provider.to_string(),
            enabled: None,
            reason: None,
            prefer: false,
            updated_at: None,
        });
        entry.prefer = prefer;
        entry.updated_at = Some(SystemTime::now());
    }

    /// Get set of preferred model keys.
    pub fn preferred_keys(&self) -> std::collections::HashSet<String> {
        self.overrides
            .iter()
            .filter(|(_, v)| v.prefer)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Get all overrides (for serialization back to config file).
    pub fn all_overrides(&self) -> Vec<&ModelOverride> {
        self.overrides.values().collect()
    }

    /// Get disabled model summaries (for /model list display).
    pub fn disabled_models(&self) -> Vec<(&str, Option<&str>)> {
        self.overrides
            .values()
            .filter(|o| o.enabled == Some(false))
            .map(|o| (o.id.as_str(), o.reason.as_deref()))
            .collect()
    }

    /// Check if a specific model is disabled by override.
    pub fn is_disabled(&self, provider: &str, model_id: &str) -> bool {
        let key = format!("{provider}/{model_id}");
        self.overrides
            .get(&key)
            .and_then(|o| o.enabled)
            .map(|e| !e)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_and_enable() {
        let mut store = OverrideStore::new();
        store.disable("copilot", "gpt-4o", Some("too slow".into()));
        assert!(store.is_disabled("copilot", "gpt-4o"));

        store.enable("copilot", "gpt-4o");
        assert!(!store.is_disabled("copilot", "gpt-4o"));
    }

    #[test]
    fn apply_disables_model() {
        let mut store = OverrideStore::new();
        store.disable("copilot", "gpt-4o", None);

        let mut models = vec![DiscoveredModel {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            provider: "copilot".into(),
            vendor: None,
            category: None,
            api: None,
            context_window: 128_000,
            max_output: 16_384,
            input_types: vec!["text".into()],
            reasoning: false,
            reasoning_levels: vec![],
            cost: Default::default(),
            preview: false,
            enabled: true,
        }];

        store.apply(&mut models);
        assert!(!models[0].enabled);
    }

    #[test]
    fn prefer_tracked() {
        let mut store = OverrideStore::new();
        store.set_prefer("copilot", "claude-opus-4.8", true);
        let prefs = store.preferred_keys();
        assert!(prefs.contains("copilot/claude-opus-4.8"));
    }
}
