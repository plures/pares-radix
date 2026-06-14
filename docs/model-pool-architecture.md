# Model Pool Architecture

## Problem Statement

The current model system is a fixed 2-3 tier hierarchy (primary/deep/fast) configured at startup. The user manually picks models. The system has dynamic model selection infrastructure (`model-selection.px` + `ModelSelectionActionHandler`) but it's not wired into the runtime — it has a hardcoded model list and no user controls.

We want:
1. The orchestrator dynamically selects the best model for each task
2. `/status` reports configured **providers**, not specific model names
3. `/models` lists all available models across all providers (with live status)
4. Users can **exclude** specific models immediately (bad results, too expensive, etc.)
5. RSI continues to learn and adapt selection over time, but exclusions are immediate
6. Cost optimization is a first-class selection criteria

## Architecture

### Core Concept: Model Pool

```
┌─────────────────────────────────────────────────┐
│                  Model Pool                       │
│                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │ Provider │  │ Provider │  │   Provider    │  │
│  │ (Copilot)│  │ (OpenAI) │  │ (local/Ollama)│ │
│  └────┬─────┘  └────┬─────┘  └──────┬───────┘  │
│       │              │               │           │
│  ┌────▼─────┐  ┌────▼─────┐  ┌─────▼──────┐   │
│  │claude-4.6│  │ gpt-5.2  │  │  mistral   │   │
│  │claude-4  │  │ gpt-4.1  │  │  codestral │   │
│  │ gpt-4.1  │  │ gpt-4o   │  │            │   │
│  └──────────┘  └──────────┘  └────────────┘   │
│                                                   │
│  ┌─────────────────────────────────────────┐    │
│  │           Exclusion List                 │    │
│  │  ✗ gpt-4o (user: "bad at code")         │    │
│  │  ✗ claude-haiku (user: "too dumb")      │    │
│  └─────────────────────────────────────────┘    │
│                                                   │
│  ┌─────────────────────────────────────────┐    │
│  │           Selection Policy               │    │
│  │  1. Filter excluded models               │    │
│  │  2. Filter by provider status (active)   │    │
│  │  3. Score: capability × RSI × cost       │    │
│  │  4. Select top + fallback chain          │    │
│  └─────────────────────────────────────────┘    │
└─────────────────────────────────────────────────┘
```

### Data Model (PluresDB)

```rust
/// A configured model provider (endpoint + auth)
struct Provider {
    name: String,           // "copilot", "openai-direct", "local-ollama"
    kind: ProviderKind,     // Copilot, OpenAI, Anthropic, Local
    base_url: String,
    auth: ProviderAuth,     // ApiKey(String), CopilotOAuth(path), None
    status: ProviderStatus, // Active, Degraded, Offline
    last_checked: Instant,
}

/// A model available through a provider
struct AvailableModel {
    id: String,             // "gpt-5.2", "claude-opus-4.6"
    provider: String,       // which provider offers this
    capabilities: ModelCapabilities,
    context_window: u64,
    cost: ModelCost,        // input_per_1m, output_per_1m, cached_per_1m
    speed_tier: SpeedTier,  // Fast, Medium, Slow
    available: bool,        // provider reports it's available right now
}

/// User-specified model exclusion
struct ModelExclusion {
    model_id: String,
    reason: Option<String>, // why the user excluded it
    excluded_at: DateTime,
    excluded_by: String,    // user/chat id
}

/// RSI performance history for a model
struct ModelPerformance {
    model_id: String,
    task_type_scores: HashMap<String, RunningAverage>,
    latency_p50: Duration,
    error_rate: f64,
    last_used: DateTime,
    total_invocations: u64,
}
```

### PluresDB Keys

```
model_pool:providers:{name}         → Provider
model_pool:models:{provider}:{id}   → AvailableModel
model_pool:exclusions:{model_id}    → ModelExclusion
model_pool:performance:{model_id}   → ModelPerformance
model_pool:config                   → PoolConfig (default cost_weight, etc.)
```

### New Trait: `ModelPool`

Replaces `TelegramModelControl` for model management:

```rust
#[async_trait]
pub trait ModelPool: Send + Sync {
    /// List all configured providers with status
    async fn providers(&self) -> Vec<ProviderInfo>;

    /// List all available models (across all providers, excluding excluded)
    async fn available_models(&self) -> Vec<ModelInfo>;

    /// List all models including excluded ones (for /models display)
    async fn all_models(&self) -> Vec<ModelInfo>;

    /// Get current exclusion list
    async fn exclusions(&self) -> Vec<ModelExclusion>;

    /// Exclude a model (immediate effect)
    async fn exclude_model(&self, model_id: &str, reason: Option<&str>) -> Result<(), String>;

    /// Re-include a previously excluded model
    async fn include_model(&self, model_id: &str) -> Result<(), String>;

    /// Select best model for a task (the core selection logic)
    async fn select_for_task(&self, task: &TaskRequirements) -> ModelSelection;

    /// Get a ModelClient for a specific model (used after selection)
    async fn client_for(&self, model_id: &str) -> Result<Arc<dyn ModelClient>, String>;

    /// Record performance feedback (called after generation completes)
    async fn record_performance(&self, model_id: &str, feedback: &PerformanceFeedback);
}
```

### Selection Algorithm

```
select_for_task(task):
  1. models = all_models.filter(not excluded)
  2. models = models.filter(provider.status == Active)
  3. models = models.filter(context_window >= task.estimated_tokens * 1.2)
  4. for each model:
     - capability_score = match capabilities vs task.requirements (0-1)
     - rsi_score = historical_performance[model][task_type] or 0.5 (unknown)
     - cost_score = 1.0 - (cost / max_cost)  // normalized, inverted
     - speed_score = match speed_tier vs task.urgency
     - score = capability_score * 0.35
            + rsi_score * 0.30
            + cost_score * 0.20
            + speed_score * 0.15
  5. sort by score descending
  6. return top + next 2 as fallbacks
```

Weight tuning is persisted in `model_pool:config` and adjustable via `/model config`.

### /status Change

Before:
```
🧠 Model: gpt-4.1 + gpt-5.2
```

After:
```
🧠 Providers: Copilot (active) · OpenAI (active)
   Models: 6 available · 1 excluded
```

### /models Command (New or Updated)

```
/models                    — list all available models with scores
/models exclude <name>     — exclude a model ("never use this")
/models include <name>     — re-include a previously excluded model
/models exclusions         — show current exclusion list
/models providers          — show provider status
/models stats              — show RSI performance stats per model
```

Example output for `/models`:
```
📊 Available Models

Provider: Copilot (active)
  ✓ claude-opus-4.6    reasoning|code|vision   200K  slow    $$$
  ✓ claude-sonnet-4    reasoning|code|fast     200K  medium  $$
  ✓ gpt-4.1            code|fast|vision        1M    fast    $$
  ✓ gpt-5.2            reasoning|code|vision   1M    medium  $$$

Provider: Local (active)
  ✓ mistral-nemo       code|fast               128K  fast    free

Excluded:
  ✗ gpt-4o — "bad at code tasks" (excluded 2h ago)
```

### /model Command (Simplified)

The old `/model` command (set primary/deep) becomes a **preference hint** rather than a hard lock:

```
/model prefer <name>   — bias selection toward this model (soft preference, not exclusive)
/model reset           — clear all preferences, return to pure dynamic selection
```

Or we remove `/model` entirely since `/models exclude` + dynamic selection replaces its purpose. The orchestrator doesn't need user-specified models — just exclusions and the algorithm.

### Integration with ModelChain → ModelPool

The current `ModelChain` (3 fixed tiers) is replaced by `ModelPool`. The `ModelChain::select()` method (which uses the cerebellum classifier) becomes part of the `ModelPool::select_for_task()` pipeline. The cerebellum's `MessageClassification` feeds into `TaskRequirements`.

Migration path:
1. `ModelChain` wraps `ModelPool` initially (backward compat)
2. The `select()` logic inside `ModelChain` delegates to `ModelPool::select_for_task()`
3. Once stable, remove `ModelChain` entirely

### Provider Auto-Discovery

For known provider types, we can auto-discover available models:
- **Copilot**: Hit the models endpoint to enumerate what's available
- **OpenAI-compatible**: `GET /v1/models` returns the list
- **Ollama**: `GET /api/tags` returns local models
- **Anthropic**: Known fixed list (or API enumeration)

This replaces the hardcoded list in `ModelSelectionActionHandler::list_available_models()`.

### Cost Tracking

Each model gets a `ModelCost` struct:

```rust
struct ModelCost {
    input_per_1m_tokens: f64,   // USD
    output_per_1m_tokens: f64,
    cached_input_per_1m: Option<f64>,
    // Derived
    estimated_cost_per_task: f64, // based on average task token usage
}
```

Costs are either:
- Hardcoded for known models (updated periodically)
- Discovered from provider pricing APIs
- Manually set by user for custom/local models

### Immediate Actions (User Model Exclusion)

When a user says `/models exclude gpt-4o "too expensive for what it delivers"`:
1. Write exclusion to PluresDB immediately
2. Next model selection skips gpt-4o
3. No RSI feedback loop needed — this is instant

When RSI detects a model performing badly:
1. RSI adjusts the model's performance score (gradual)
2. Selection naturally deprioritizes it (over time)
3. If it drops below a configurable threshold, alert the user
4. RSI does NOT auto-exclude — that's a user action

This preserves the "RSI is slow by design" philosophy while giving users an immediate override.

## Implementation Plan

### Phase 1: Core ModelPool trait + PluresDB storage
- Define `ModelPool` trait in `crates/core/src/model_pool.rs`
- Implement PluresDB-backed storage for providers/models/exclusions
- Unit tests for CRUD operations

### Phase 2: Selection algorithm
- Port scoring from `ModelSelectionActionHandler` to `ModelPool::select_for_task()`
- Add exclusion filtering
- Add cost weight to scoring
- Wire RSI performance data into selection

### Phase 3: Provider auto-discovery
- Implement model listing for Copilot, OpenAI, Ollama
- Periodic background refresh (every 5 min)
- Provider health checks (mark degraded/offline on errors)

### Phase 4: Telegram commands
- Update `/status` to show providers + model count
- Implement `/models` with subcommands (list, exclude, include, stats)
- Remove or simplify `/model` (prefer → exclude/include)

### Phase 5: Wire into runtime
- Replace `ModelChain` usage in the agent factory
- `select_for_task()` replaces hardcoded model selection
- Performance feedback recorded after each generation

### Phase 6: Update model-selection.px
- Connect `list_available_models` to real `ModelPool::available_models()`
- Add exclusion handling to the .px procedure
- Wire cost optimization into scoring

## Files to Create/Modify

### New files:
- `crates/core/src/model_pool.rs` — trait + PluresDB impl
- `crates/core/src/model_pool/provider.rs` — provider auto-discovery
- `crates/core/src/model_pool/selection.rs` — scoring algorithm
- `crates/core/src/model_pool/exclusion.rs` — exclusion CRUD
- `crates/core/src/model_pool/cost.rs` — cost data + estimation

### Modified files:
- `crates/core/src/model_chain.rs` — delegate to ModelPool
- `crates/core/src/spine/model_selection_actions.rs` — use real ModelPool
- `crates/channels/src/telegram.rs` — /status, /models, /model commands
- `crates/cli/src/main.rs` — construct ModelPool, wire into adapter

## Non-Goals (for now)
- Multi-provider load balancing (one request → multiple providers in parallel)
- Automatic cost budgeting (hard spend caps)
- Provider failover at the HTTP level (already handled by ModelRouter)
- Token-level streaming cost estimation
