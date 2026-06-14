//! Provider discovery — fetches available models from provider APIs.

use super::types::{DiscoveredModel, ModelCost, ProviderConfig, ProviderKind, ProviderStatus};
use serde::Deserialize;
use tracing::{debug, error, info};

/// Result of a discovery attempt.
#[derive(Debug)]
pub struct DiscoveryResult {
    pub provider: String,
    pub models: Vec<DiscoveredModel>,
    pub status: ProviderStatus,
    pub error: Option<String>,
}

/// Discover models from a provider based on its kind.
pub async fn discover(provider: &ProviderConfig) -> DiscoveryResult {
    if !provider.enabled {
        return DiscoveryResult {
            provider: provider.name.clone(),
            models: vec![],
            status: ProviderStatus::Offline,
            error: Some("provider disabled".into()),
        };
    }

    let result = match provider.kind {
        ProviderKind::GithubCopilot => discover_copilot(provider).await,
        ProviderKind::OpenAi => discover_openai(provider).await,
        ProviderKind::Anthropic => discover_anthropic(provider).await,
        ProviderKind::Ollama => discover_ollama(provider).await,
        ProviderKind::Custom => {
            // Custom providers use static config only
            Ok(vec![])
        }
    };

    match result {
        Ok(models) => {
            info!(
                provider = %provider.name,
                models_found = models.len(),
                "discovery completed"
            );
            DiscoveryResult {
                provider: provider.name.clone(),
                models,
                status: ProviderStatus::Active,
                error: None,
            }
        }
        Err(e) => {
            error!(
                provider = %provider.name,
                error = %e,
                "discovery failed"
            );
            DiscoveryResult {
                provider: provider.name.clone(),
                models: vec![],
                status: ProviderStatus::Offline,
                error: Some(e),
            }
        }
    }
}

// ── GitHub Copilot Discovery ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CopilotModelsResponse {
    data: Vec<CopilotModel>,
}

#[derive(Debug, Deserialize)]
struct CopilotModel {
    id: String,
    name: Option<String>,
    vendor: Option<String>,
    model_picker_category: Option<String>,
    #[serde(default)]
    preview: bool,
    #[serde(default)]
    capabilities: Option<CopilotCapabilities>,
}

#[derive(Debug, Deserialize)]
struct CopilotCapabilities {
    #[serde(rename = "type")]
    cap_type: Option<String>,
    limits: Option<CopilotLimits>,
    supports: Option<CopilotSupports>,
}

#[derive(Debug, Deserialize)]
struct CopilotLimits {
    max_context_window_tokens: Option<u64>,
    max_output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CopilotSupports {
    vision: Option<bool>,
    tool_calls: Option<bool>,
    #[serde(default)]
    reasoning_effort: Option<Vec<String>>,
}

async fn discover_copilot(provider: &ProviderConfig) -> Result<Vec<DiscoveredModel>, String> {
    let token = resolve_auth(&provider.auth).await?;
    let url = format!("{}/models", provider.endpoint.trim_end_matches('/'));

    debug!(url = %url, "fetching copilot models");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("API returned {}", resp.status()));
    }

    let body: CopilotModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    let models = body
        .data
        .into_iter()
        .filter(|m| {
            // Only include chat-capable models
            m.capabilities
                .as_ref()
                .and_then(|c| c.cap_type.as_deref())
                .map(|t| t == "chat")
                .unwrap_or(true) // include if no type specified
        })
        .map(|m| {
            let caps = m.capabilities.as_ref();
            let limits = caps.and_then(|c| c.limits.as_ref());
            let supports = caps.and_then(|c| c.supports.as_ref());

            let mut input_types = vec!["text".to_string()];
            if supports.and_then(|s| s.vision).unwrap_or(false) {
                input_types.push("image".to_string());
            }

            let reasoning = supports
                .and_then(|s| s.reasoning_effort.as_ref())
                .map(|r| !r.is_empty())
                .unwrap_or(false);

            let reasoning_levels = supports
                .and_then(|s| s.reasoning_effort.clone())
                .unwrap_or_default();

            DiscoveredModel {
                id: m.id.clone(),
                name: m.name.unwrap_or_else(|| m.id.clone()),
                provider: provider.name.clone(),
                vendor: m.vendor,
                category: m.model_picker_category,
                api: None, // use provider default
                context_window: limits.and_then(|l| l.max_context_window_tokens).unwrap_or(128_000),
                max_output: limits.and_then(|l| l.max_output_tokens).unwrap_or(8_192),
                input_types,
                reasoning,
                reasoning_levels,
                cost: ModelCost::default(), // Copilot is subscription ($0)
                preview: m.preview,
                enabled: true,
            }
        })
        .collect();

    Ok(models)
}

// ── OpenAI Discovery ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

async fn discover_openai(provider: &ProviderConfig) -> Result<Vec<DiscoveredModel>, String> {
    let token = resolve_auth(&provider.auth).await?;
    let url = format!("{}/models", provider.endpoint.trim_end_matches('/'));

    debug!(url = %url, "fetching openai models");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("API returned {}", resp.status()));
    }

    let body: OpenAiModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    // OpenAI /v1/models returns minimal info — we augment with known data.
    let models = body
        .data
        .into_iter()
        .filter(|m| is_chat_model(&m.id))
        .map(|m| {
            let (context_window, max_output, reasoning) = openai_model_specs(&m.id);
            DiscoveredModel {
                id: m.id.clone(),
                name: openai_display_name(&m.id),
                provider: provider.name.clone(),
                vendor: m.owned_by.or_else(|| Some("OpenAI".into())),
                category: Some(openai_category(&m.id)),
                api: None,
                context_window,
                max_output,
                input_types: vec!["text".into(), "image".into()],
                reasoning,
                reasoning_levels: if reasoning {
                    vec!["low".into(), "medium".into(), "high".into()]
                } else {
                    vec![]
                },
                cost: openai_cost(&m.id),
                preview: m.id.contains("preview"),
                enabled: true,
            }
        })
        .collect();

    Ok(models)
}

/// Filter: only include models that are chat-capable (not embeddings, tts, etc.)
fn is_chat_model(id: &str) -> bool {
    let prefixes = ["gpt-", "o1", "o3", "o4", "chatgpt"];
    prefixes.iter().any(|p| id.starts_with(p))
        && !id.contains("embed")
        && !id.contains("tts")
        && !id.contains("dall-e")
        && !id.contains("whisper")
}

/// Known specs for OpenAI models (the /models endpoint doesn't provide these).
fn openai_model_specs(id: &str) -> (u64, u64, bool) {
    // (context_window, max_output, reasoning)
    match id {
        s if s.starts_with("gpt-5.5") => (1_000_000, 128_000, true),
        s if s.starts_with("gpt-5.4-pro") => (1_050_000, 128_000, true),
        s if s.starts_with("gpt-5.4-nano") => (400_000, 128_000, true),
        s if s.starts_with("gpt-5.4-mini") => (400_000, 128_000, true),
        s if s.starts_with("gpt-5.4") => (272_000, 128_000, true),
        s if s.starts_with("gpt-5.3") => (400_000, 128_000, true),
        s if s.starts_with("gpt-5") => (264_000, 64_000, true),
        s if s.starts_with("gpt-4.1") => (128_000, 16_384, false),
        s if s.starts_with("o3") || s.starts_with("o4") || s.starts_with("o1") => {
            (200_000, 100_000, true)
        }
        _ => (128_000, 16_384, false),
    }
}

/// Known costs for OpenAI models (per 1M tokens).
fn openai_cost(id: &str) -> ModelCost {
    match id {
        "gpt-5.5" => ModelCost { input: 5.0, output: 30.0, cache_read: 0.5, cache_write: 0.0 },
        "gpt-5.5-pro" => ModelCost { input: 30.0, output: 180.0, cache_read: 0.0, cache_write: 0.0 },
        "gpt-5.4" => ModelCost { input: 2.5, output: 15.0, cache_read: 0.25, cache_write: 0.0 },
        "gpt-5.4-pro" => ModelCost { input: 30.0, output: 180.0, cache_read: 0.0, cache_write: 0.0 },
        "gpt-5.4-mini" => ModelCost { input: 0.75, output: 4.5, cache_read: 0.075, cache_write: 0.0 },
        "gpt-5.4-nano" => ModelCost { input: 0.2, output: 1.25, cache_read: 0.02, cache_write: 0.0 },
        "gpt-5.3-codex" => ModelCost { input: 1.75, output: 14.0, cache_read: 0.175, cache_write: 0.0 },
        "o3" => ModelCost { input: 2.0, output: 8.0, cache_read: 0.5, cache_write: 0.0 },
        "o3-pro" => ModelCost { input: 20.0, output: 80.0, cache_read: 0.0, cache_write: 0.0 },
        "o4-mini" => ModelCost { input: 1.1, output: 4.4, cache_read: 0.28, cache_write: 0.0 },
        _ => ModelCost::default(),
    }
}

fn openai_display_name(id: &str) -> String {
    id.replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn openai_category(id: &str) -> String {
    if id.contains("nano") || id.contains("mini") {
        "lightweight".into()
    } else if id.contains("pro") || id.starts_with("o3") || id.starts_with("o1") {
        "powerful".into()
    } else {
        "versatile".into()
    }
}

// ── Anthropic Discovery ──────────────────────────────────────────────────────

async fn discover_anthropic(provider: &ProviderConfig) -> Result<Vec<DiscoveredModel>, String> {
    let _token = resolve_auth(&provider.auth).await?;

    // Anthropic doesn't have a /models endpoint — use known models.
    // This is the one place we're somewhat static, but it's isolated here
    // and easily updated when they add an API.
    let models = vec![
        anthropic_model("claude-opus-4-8", "Claude Opus 4.8", 1_048_576, 128_000, true,
            ModelCost { input: 15.0, output: 75.0, cache_read: 1.5, cache_write: 0.0 },
            &provider.name),
        anthropic_model("claude-opus-4-7", "Claude Opus 4.7", 200_000, 64_000, true,
            ModelCost { input: 15.0, output: 75.0, cache_read: 1.5, cache_write: 0.0 },
            &provider.name),
        anthropic_model("claude-sonnet-4-6", "Claude Sonnet 4.6", 200_000, 64_000, true,
            ModelCost { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 0.0 },
            &provider.name),
        anthropic_model("claude-opus-4-6", "Claude Opus 4.6", 200_000, 64_000, true,
            ModelCost { input: 15.0, output: 75.0, cache_read: 1.5, cache_write: 0.0 },
            &provider.name),
    ];

    Ok(models)
}

fn anthropic_model(
    id: &str,
    name: &str,
    context_window: u64,
    max_output: u64,
    reasoning: bool,
    cost: ModelCost,
    provider: &str,
) -> DiscoveredModel {
    DiscoveredModel {
        id: id.to_string(),
        name: name.to_string(),
        provider: provider.to_string(),
        vendor: Some("Anthropic".into()),
        category: Some("powerful".into()),
        api: Some("anthropic-messages".into()),
        context_window,
        max_output,
        input_types: vec!["text".into(), "image".into()],
        reasoning,
        reasoning_levels: if reasoning {
            vec!["low".into(), "medium".into(), "high".into(), "max".into()]
        } else {
            vec![]
        },
        cost,
        preview: false,
        enabled: true,
    }
}

// ── Ollama Discovery ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    details: Option<OllamaDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaDetails {
    #[serde(default)]
    parameter_size: Option<String>,
}

async fn discover_ollama(provider: &ProviderConfig) -> Result<Vec<DiscoveredModel>, String> {
    let url = format!("{}/api/tags", provider.endpoint.trim_end_matches('/'));

    debug!(url = %url, "fetching ollama models");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("API returned {}", resp.status()));
    }

    let body: OllamaTagsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse failed: {e}"))?;

    let models = body
        .models
        .into_iter()
        .map(|m| DiscoveredModel {
            id: m.name.clone(),
            name: m.name.clone(),
            provider: provider.name.clone(),
            vendor: Some("Local".into()),
            category: Some("local".into()),
            api: None,
            context_window: 128_000, // conservative default for local models
            max_output: 32_000,
            input_types: vec!["text".into()],
            reasoning: false,
            reasoning_levels: vec![],
            cost: ModelCost::default(), // local = free
            preview: false,
            enabled: true,
        })
        .collect();

    Ok(models)
}

// ── Auth Resolution ──────────────────────────────────────────────────────────

use super::types::ProviderAuth;

async fn resolve_auth(auth: &ProviderAuth) -> Result<String, String> {
    match auth {
        ProviderAuth::GhToken => {
            // Shell out to `gh auth token`
            let output = tokio::process::Command::new("gh")
                .args(["auth", "token"])
                .output()
                .await
                .map_err(|e| format!("failed to run `gh auth token`: {e}"))?;
            if !output.status.success() {
                return Err("gh auth token failed — not authenticated".into());
            }
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        ProviderAuth::Env { var } => {
            std::env::var(var).map_err(|_| format!("environment variable {var} not set"))
        }
        ProviderAuth::Key { value } => Ok(value.clone()),
        ProviderAuth::None => Ok(String::new()),
    }
}
