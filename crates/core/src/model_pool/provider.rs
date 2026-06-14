//! Provider management — config loading, health checks, lifecycle.

use super::types::{DiscoveryMode, ProviderAuth, ProviderConfig, ProviderKind, ProviderStatus};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// Load providers from a TOML config file.
pub fn load_from_config(path: &Path) -> Result<Vec<ProviderConfig>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let config: ConfigFile =
        toml::from_str(&content).map_err(|e| format!("failed to parse TOML: {e}"))?;

    let providers = config
        .providers
        .into_iter()
        .map(|(name, p)| ProviderConfig {
            name,
            kind: p.kind,
            endpoint: p.endpoint,
            auth: parse_auth(&p.auth),
            api: p.api.unwrap_or_else(|| "openai-responses".into()),
            discovery: p.discovery.unwrap_or(DiscoveryMode::Refreshable),
            enabled: p.enabled.unwrap_or(true),
            status: ProviderStatus::Unknown,
            last_checked: None,
            last_discovery: None,
        })
        .collect();

    Ok(providers)
}

/// Load model overrides from config file.
pub fn load_overrides_from_config(path: &Path) -> Result<Vec<super::types::ModelOverride>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let config: ConfigFile =
        toml::from_str(&content).map_err(|e| format!("failed to parse TOML: {e}"))?;

    let overrides = config
        .models
        .and_then(|m| m.overrides)
        .unwrap_or_default()
        .into_iter()
        .map(|o| super::types::ModelOverride {
            id: o.id,
            provider: o.provider,
            enabled: o.enabled,
            reason: o.reason,
            prefer: o.prefer.unwrap_or(false),
            updated_at: None,
        })
        .collect();

    Ok(overrides)
}

/// Load selection weights from config file.
pub fn load_weights_from_config(path: &Path) -> Result<super::types::SelectionWeights, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let config: ConfigFile =
        toml::from_str(&content).map_err(|e| format!("failed to parse TOML: {e}"))?;

    Ok(config
        .pool
        .and_then(|p| p.selection_weights)
        .map(|w| super::types::SelectionWeights {
            capability: w.capability.unwrap_or(0.35),
            rsi: w.rsi.unwrap_or(0.30),
            cost: w.cost.unwrap_or(0.20),
            speed: w.speed.unwrap_or(0.15),
        })
        .unwrap_or_default())
}

// ── Config file schema ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    pool: Option<PoolSection>,
    #[serde(default)]
    providers: HashMap<String, ProviderEntry>,
    #[serde(default)]
    models: Option<ModelsSection>,
}

#[derive(Debug, Deserialize)]
struct PoolSection {
    #[serde(default)]
    selection_weights: Option<WeightsEntry>,
    #[serde(default)]
    discovery: Option<DiscoverySection>,
}

#[derive(Debug, Deserialize)]
struct DiscoverySection {
    #[serde(default)]
    refresh_interval: Option<u64>,
    #[serde(default)]
    refresh_on_startup: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WeightsEntry {
    capability: Option<f64>,
    rsi: Option<f64>,
    cost: Option<f64>,
    speed: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ProviderEntry {
    kind: ProviderKind,
    endpoint: String,
    #[serde(default)]
    auth: String,
    #[serde(default)]
    api: Option<String>,
    #[serde(default)]
    discovery: Option<DiscoveryMode>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ModelsSection {
    #[serde(default)]
    overrides: Option<Vec<OverrideEntry>>,
}

#[derive(Debug, Deserialize)]
struct OverrideEntry {
    id: String,
    provider: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    prefer: Option<bool>,
}

// ── Auth parsing ─────────────────────────────────────────────────────────────

fn parse_auth(raw: &str) -> ProviderAuth {
    match raw {
        "gh-token" => ProviderAuth::GhToken,
        "" | "none" => ProviderAuth::None,
        s if s.starts_with("env:") => ProviderAuth::Env {
            var: s[4..].to_string(),
        },
        s => ProviderAuth::Key {
            value: s.to_string(),
        },
    }
}

/// Write overrides back to config file (sync runtime → file).
pub fn save_overrides_to_config(
    path: &Path,
    overrides: &[&super::types::ModelOverride],
) -> Result<(), String> {
    // Read existing config, patch the overrides section, write back.
    // This preserves all other config content.
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    // Find the [models.overrides] section and replace it.
    // For robustness, we rebuild the overrides section and append/replace.
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    // Remove existing [[models.overrides]] blocks
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "[[models.overrides]]" {
            // Remove this block until next section header or end
            let start = i;
            i += 1;
            while i < lines.len() && !lines[i].starts_with('[') && !lines[i].trim().is_empty() {
                i += 1;
            }
            // Also remove trailing blank line
            if i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            lines.drain(start..i);
            i = start;
        } else {
            i += 1;
        }
    }

    // Append new overrides
    if !overrides.is_empty() {
        lines.push(String::new());
        for ov in overrides {
            lines.push("[[models.overrides]]".to_string());
            lines.push(format!("id = \"{}\"", ov.id));
            lines.push(format!("provider = \"{}\"", ov.provider));
            if let Some(enabled) = ov.enabled {
                lines.push(format!("enabled = {enabled}"));
            }
            if let Some(reason) = &ov.reason {
                lines.push(format!("reason = \"{}\"", reason));
            }
            if ov.prefer {
                lines.push("prefer = true".to_string());
            }
            lines.push(String::new());
        }
    }

    let output = lines.join("\n");
    std::fs::write(path, output).map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    info!(path = %path.display(), overrides = overrides.len(), "saved model overrides to config");
    Ok(())
}
