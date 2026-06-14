//! Core types for the model pool system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

// ── Provider ─────────────────────────────────────────────────────────────────

/// The kind of provider (determines discovery protocol).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    GithubCopilot,
    OpenAi,
    Anthropic,
    Ollama,
    Custom,
}

/// How this provider's model list is discovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryMode {
    /// Live discovery from API endpoint (GET /models or equivalent).
    Refreshable,
    /// Statically known list (embedded or from config).
    Static,
}

/// Authentication method for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "method")]
pub enum ProviderAuth {
    /// Use `gh auth token` for GitHub Copilot.
    GhToken,
    /// Read from environment variable.
    Env { var: String },
    /// Direct API key value (for config file, NOT recommended).
    Key { value: String },
    /// No authentication needed (local providers).
    None,
}

/// Health status of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderStatus {
    /// Provider is responding normally.
    Active,
    /// Provider is responding but with elevated errors or latency.
    Degraded,
    /// Provider is not reachable.
    Offline,
    /// Provider hasn't been checked yet.
    Unknown,
}

/// A configured model provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub auth: ProviderAuth,
    pub api: String,
    pub discovery: DiscoveryMode,
    pub enabled: bool,
    #[serde(default)]
    pub status: ProviderStatus,
    #[serde(default)]
    pub last_checked: Option<SystemTime>,
    #[serde(default)]
    pub last_discovery: Option<SystemTime>,
}

impl Default for ProviderStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

// ── Model ────────────────────────────────────────────────────────────────────

/// Cost per 1M tokens (USD).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
}

impl ModelCost {
    /// Returns true if this model has zero cost (subscription/free tier).
    pub fn is_free(&self) -> bool {
        self.input == 0.0 && self.output == 0.0
    }
}

/// A model discovered from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    /// Model identifier (e.g., "claude-opus-4.8", "gpt-5.4-mini").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Which provider offers this model.
    pub provider: String,
    /// Vendor (Anthropic, OpenAI, Google, etc.).
    #[serde(default)]
    pub vendor: Option<String>,
    /// Model category for UI grouping.
    #[serde(default)]
    pub category: Option<String>,
    /// API transport override (when different from provider default).
    #[serde(default)]
    pub api: Option<String>,
    /// Maximum context window in tokens.
    #[serde(default)]
    pub context_window: u64,
    /// Maximum output tokens.
    #[serde(default)]
    pub max_output: u64,
    /// Supported input types.
    #[serde(default)]
    pub input_types: Vec<String>,
    /// Whether the model supports reasoning/chain-of-thought.
    #[serde(default)]
    pub reasoning: bool,
    /// Available reasoning effort levels (if reasoning is supported).
    #[serde(default)]
    pub reasoning_levels: Vec<String>,
    /// Cost per 1M tokens.
    #[serde(default)]
    pub cost: ModelCost,
    /// Whether this is a preview/experimental model.
    #[serde(default)]
    pub preview: bool,
    /// Whether this model is enabled for selection.
    /// All models are enabled by default on discovery.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl DiscoveredModel {
    /// Composite key: "provider/model_id"
    pub fn key(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }

    /// Does this model support vision/image input?
    pub fn supports_vision(&self) -> bool {
        self.input_types.iter().any(|t| t == "image")
    }

    /// Does this model support tool calling?
    pub fn supports_tools(&self) -> bool {
        // All modern models support tools; only exclude if explicitly absent.
        // For now, assume true unless we get explicit data.
        true
    }
}

// ── User Override ────────────────────────────────────────────────────────────

/// A user-specified override for a discovered model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelOverride {
    /// Model ID to override.
    pub id: String,
    /// Provider (scopes the override to a specific provider's offering).
    pub provider: String,
    /// Whether this model is enabled for selection.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Reason for disabling (shown in /model list).
    #[serde(default)]
    pub reason: Option<String>,
    /// Soft preference boost (increases selection score).
    #[serde(default)]
    pub prefer: bool,
    /// When this override was created/last modified.
    #[serde(default)]
    pub updated_at: Option<SystemTime>,
}

// ── Selection ────────────────────────────────────────────────────────────────

/// What the task needs from a model (fed into selection algorithm).
#[derive(Debug, Clone, Default)]
pub struct TaskRequirements {
    /// Estimated input tokens for this task.
    pub estimated_input_tokens: u64,
    /// Estimated output tokens needed.
    pub estimated_output_tokens: u64,
    /// Does the task need reasoning capability?
    pub needs_reasoning: bool,
    /// Does the task need vision/image input?
    pub needs_vision: bool,
    /// Does the task need tool calling?
    pub needs_tools: bool,
    /// Task type label (for RSI scoring).
    pub task_type: Option<String>,
    /// Urgency (affects speed weight).
    pub urgency: Urgency,
}

/// How urgent is this task?
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Urgency {
    /// User is waiting interactively — prefer speed.
    High,
    /// Normal request.
    #[default]
    Normal,
    /// Background/batch work — optimize for cost/quality.
    Low,
}

/// Result of model selection.
#[derive(Debug, Clone)]
pub struct ModelSelection {
    /// The selected model.
    pub model: DiscoveredModel,
    /// Why this model was selected (for logging/debugging).
    pub score: f64,
    /// Score breakdown for transparency.
    pub score_breakdown: ScoreBreakdown,
    /// Fallback models (ordered by score, used if primary fails).
    pub fallbacks: Vec<DiscoveredModel>,
}

/// Score breakdown for a model selection decision.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub capability: f64,
    pub rsi: f64,
    pub cost: f64,
    pub speed: f64,
    pub prefer_boost: f64,
}

// ── Selection Weights ────────────────────────────────────────────────────────

/// Configurable weights for the selection algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionWeights {
    pub capability: f64,
    pub rsi: f64,
    pub cost: f64,
    pub speed: f64,
}

impl Default for SelectionWeights {
    fn default() -> Self {
        Self {
            capability: 0.35,
            rsi: 0.30,
            cost: 0.20,
            speed: 0.15,
        }
    }
}

// ── Performance tracking ─────────────────────────────────────────────────────

/// RSI performance record for a model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPerformance {
    /// Model key (provider/id).
    pub model_key: String,
    /// Scores per task type (running average, 0.0-1.0).
    #[serde(default)]
    pub task_scores: HashMap<String, f64>,
    /// Overall success rate (0.0-1.0).
    #[serde(default)]
    pub success_rate: f64,
    /// Average latency in milliseconds.
    #[serde(default)]
    pub avg_latency_ms: u64,
    /// Error rate (0.0-1.0).
    #[serde(default)]
    pub error_rate: f64,
    /// Total invocations recorded.
    #[serde(default)]
    pub total_invocations: u64,
    /// Last time this model was used.
    #[serde(default)]
    pub last_used: Option<SystemTime>,
}

/// Feedback after a model completes a task (fed into RSI).
#[derive(Debug, Clone)]
pub struct PerformanceFeedback {
    /// Model key (provider/id).
    pub model_key: String,
    /// Task type label.
    pub task_type: String,
    /// Was the result acceptable? (true = success)
    pub success: bool,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Input tokens used.
    pub input_tokens: u64,
    /// Output tokens generated.
    pub output_tokens: u64,
}

// ── Pool Status ──────────────────────────────────────────────────────────────

/// Snapshot of the model pool state (for /status display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatus {
    /// Active providers.
    pub providers: Vec<ProviderSummary>,
    /// Total discovered models.
    pub total_models: usize,
    /// Models currently enabled.
    pub enabled_models: usize,
    /// Models disabled by user.
    pub disabled_models: usize,
    /// Last discovery refresh time.
    pub last_refresh: Option<SystemTime>,
}

/// Provider summary for status display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSummary {
    pub name: String,
    pub kind: ProviderKind,
    pub status: ProviderStatus,
    pub model_count: usize,
    pub enabled: bool,
}
