//! .px Configuration Loader
//!
//! Reads radix.px config files and extracts settings into a structured map.
//! Config blocks compile to PluresDB records with key `px:config/<name>`.
//!
//! Lookup order:
//!   1. --config <path> CLI flag
//!   2. $PARES_CONFIG env var
//!   3. ./radix.px (current directory)
//!   4. ~/.pares-radix/radix.px (user home)

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use tracing::{debug, info, warn};

/// Parsed configuration from a .px file.
#[derive(Debug, Clone, Default)]
pub struct PxConfig {
    /// Merged config entries: "block.key" → value
    pub entries: HashMap<String, Value>,
    /// Source file path (for diagnostics)
    pub source: Option<PathBuf>,
}

impl PxConfig {
    /// Get a string value by dotted path (e.g., "radix.channel")
    pub fn get_str(&self, path: &str) -> Option<&str> {
        self.entries.get(path).and_then(|v| v.as_str())
    }

    /// Get a bool value by dotted path
    pub fn get_bool(&self, path: &str) -> Option<bool> {
        self.entries.get(path).and_then(|v| v.as_bool())
    }

    /// Get a number value by dotted path
    pub fn get_f64(&self, path: &str) -> Option<f64> {
        self.entries.get(path).and_then(|v| v.as_f64())
    }

    /// Get a value, resolving "env:VAR_NAME" references to env vars
    pub fn get_resolved(&self, path: &str) -> Option<String> {
        self.entries.get(path).and_then(|v| {
            let s = v.as_str()?;
            if let Some(var_name) = s.strip_prefix("env:") {
                std::env::var(var_name).ok()
            } else {
                Some(s.to_string())
            }
        })
    }
}

/// Find and load the .px config file.
///
/// Returns None if no config file is found (that's OK — CLI flags still work).
pub fn load_config(explicit_path: Option<&str>) -> Option<PxConfig> {
    let path = resolve_config_path(explicit_path)?;
    info!(path = %path.display(), "Loading .px config");

    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "Failed to read config file");
            return None;
        }
    };

    match parse_config(&source) {
        Ok(mut config) => {
            config.source = Some(path);
            debug!(entries = config.entries.len(), "Config loaded");
            Some(config)
        }
        Err(e) => {
            warn!(error = %e, "Failed to parse config file");
            None
        }
    }
}

/// Resolve config file path from explicit path, env var, or default locations.
fn resolve_config_path(explicit: Option<&str>) -> Option<PathBuf> {
    // 1. Explicit --config flag
    if let Some(p) = explicit {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
        warn!(path = %path.display(), "Explicit config path not found");
        return None;
    }

    // 2. PARES_CONFIG env var
    if let Ok(p) = std::env::var("PARES_CONFIG") {
        let path = PathBuf::from(&p);
        if path.exists() {
            return Some(path);
        }
    }

    // 3. ./radix.px
    let local = PathBuf::from("radix.px");
    if local.exists() {
        return Some(local);
    }

    // 4. ~/.pares-radix/radix.px
    if let Ok(home) = std::env::var("HOME") {
        let user_config = PathBuf::from(home).join(".pares-radix/radix.px");
        if user_config.exists() {
            return Some(user_config);
        }
    }

    None
}

/// Parse a .px source string into a PxConfig.
fn parse_config(source: &str) -> Result<PxConfig, String> {
    // Use praxis-native (via pares-radix-praxis re-export) to parse
    let doc = pares_radix_praxis::px::parse(source)
        .map_err(|e| format!("parse error: {e}"))?;

    let doc_json = serde_json::to_value(&doc)
        .map_err(|e| format!("json error: {e}"))?;

    let mut entries = HashMap::new();

    if let Some(configs) = doc_json.get("configs").and_then(|v| v.as_array()) {
        for config in configs {
            let block_name = config.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(config_entries) = config.get("entries").and_then(|v| v.as_array()) {
                for entry in config_entries {
                    let key = entry.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(value) = entry.get("value") {
                        let dotted_key = format!("{}.{}", block_name, key);
                        entries.insert(dotted_key, value.clone());
                    }
                }
            }
        }
    }

    Ok(PxConfig { entries, source: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_basic() {
        let source = r#"
config radix:
  channel: "telegram"
  model: "claude-sonnet-4.5"
  use_copilot: true

config heartbeat:
  enabled: true
  interval_minutes: 120
"#;
        let config = parse_config(source).expect("parse failed");
        assert_eq!(config.get_str("radix.channel"), Some("telegram"));
        assert_eq!(config.get_str("radix.model"), Some("claude-sonnet-4.5"));
        assert_eq!(config.get_bool("radix.use_copilot"), Some(true));
        assert_eq!(config.get_bool("heartbeat.enabled"), Some(true));
        assert_eq!(config.get_f64("heartbeat.interval_minutes"), Some(120.0));
    }

    #[test]
    fn test_env_resolution() {
        let source = r#"
config telegram:
  token: "env:TEST_PX_TOKEN"
"#;
        std::env::set_var("TEST_PX_TOKEN", "bot123456");
        let config = parse_config(source).expect("parse failed");
        assert_eq!(config.get_resolved("telegram.token"), Some("bot123456".to_string()));
        std::env::remove_var("TEST_PX_TOKEN");
    }

    #[test]
    fn test_empty_config() {
        let config = parse_config("").expect("parse failed");
        assert!(config.entries.is_empty());
    }
}
