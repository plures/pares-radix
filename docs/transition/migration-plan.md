# Pares-Radix Migration Plan

> Master plan for migrating standalone plures apps to the radix plugin platform.

## Overview

Radix provides the opinionated runtime — plugin loader, inference engine, UX contracts, platform APIs. Standalone apps continue development on their existing repos until radix is ready, then migrate incrementally. Each app becomes a `RadixPlugin` (defined in `src/lib/types/plugin.ts`) registered through `registerPlugin()` in the plugin-loader.

## Phase 1: Radix Foundation (Current)

**Status:** In progress

What exists today in `pares-radix/src/lib/`:

| Module | File | Purpose |
|---|---|---|
| Plugin types | `types/plugin.ts` | `RadixPlugin` interface, `PluginContext`, all platform APIs (`SettingsAPI`, `DataAPI`, `LLMAPI`, `InferenceAPI`, `NavigationAPI`, `NotifyAPI`) |
| Plugin loader | `platform/plugin-loader.ts` | Registration, dependency resolution (topological sort), lifecycle (`activateAll`/`deactivateAll`), aggregated registries (routes, nav, settings, widgets, help, onboarding, rules, expectations, constraints) |
| Inference engine | `platform/inference-engine.ts` | Rule execution, confidence scoring, compound confidence merging (`1 - ∏(1 - cᵢ)`), auto-confirm at ≥0.90, user-gate at <0.70, decision ledger |
| UX contracts | `praxis/ux-contracts.ts` | Built-in expectations: no dead-end routes, data prerequisites require empty states, nav items must resolve |

**Remaining work:**
- [ ] Implement `PluginContext` concrete providers (SettingsAPI → Svelte store, DataAPI → PluresDB, LLMAPI, NavigationAPI, NotifyAPI)
- [ ] Build Svelte router that consumes `getAllRoutes()`
- [ ] Wire aggregated registries into layout components
- [ ] Package as `@plures/pares-radix` npm module

## Phase 2: Svelte Shell

**Goal:** The radix app shell that plugins render inside.

Components to build (patterns already proven in FinancialAdvisor's `src/routes/+layout.svelte`):

| Component | Source of truth | Notes |
|---|---|---|
| Layout shell | FA's `+layout.svelte` sidebar/content pattern | Extract, make plugin-aware |
| Sidebar | `getAllNavItems()` | Collapsible, mobile-responsive, badge support |
| Settings page | `getAllSettings()` | Unified settings from all plugins, grouped by `PluginSetting.group` |
| Help page | `getAllHelpSections()` | Aggregated, priority-sorted |
| Onboarding wizard | `getAllOnboardingSteps()` | Dependency-ordered, completion tracking |
| Dashboard | `getAllDashboardWidgets()` | Grid layout, priority-sorted, `colspan` support |
| Empty states | `checkDataRequirements()` from `ux-contracts.ts` | Per-route, with fulfillment actions |

**Estimated effort:** 2-3 weeks

## Phase 3: FinancialAdvisor Migration (First Plugin)

**Why first:** Most complex app, proves every plugin API surface. Already has a working plugin stub in modulus (`plugins/financial-advisor/src/index.ts`).

See [financial-advisor-migration.md](./financial-advisor-migration.md) for the detailed guide.

### Current State

| Metric | Value |
|---|---|
| Total LOC (packages/) | ~17,000 TypeScript |
| Total LOC (all src/) | ~197,000 (includes generated/deps) |
| Packages | storage, analytics, ledger, advice, ai-integration, ai-providers, vscode-extension |
| Routes | `/`, `/accounts`, `/transactions`, `/budgets`, `/goals`, `/reports`, `/review`, `/review/*`, `/settings`, `/help` |
| Test coverage (AI code) | None |
| Praxis integration | `src/lib/praxis/` (schema, logic, lifecycle) |
| PluresDB | `src/lib/pluresdb/store.ts` |
| Design system | `@plures/design-dojo` |

### Migration Mapping

| Standalone Feature | RadixPlugin API | Complexity |
|---|---|---|
| `src/routes/*` (10 routes) | `RadixPlugin.routes[]` — already mapped in modulus index.ts | Easy |
| Sidebar nav items | `RadixPlugin.navItems[]` — already mapped | Easy |
| Settings page | `RadixPlugin.settings[]` — 3 settings already defined | Easy |
| Dashboard widgets | `RadixPlugin.dashboardWidgets[]` — 3 widgets defined | Easy |
| Help content | `RadixPlugin.helpSections[]` — 2 sections defined | Easy |
| Onboarding steps | `RadixPlugin.onboardingSteps[]` — 2 steps defined | Easy |
| Inference rules | `RadixPlugin.rules[]` — 4 rules in modulus (recurring-amount, vendor-clustering, refund-detection, tax-variance) | Done |
| `packages/storage/` | Migrate to `DataAPI.collection()` (PluresDB) | Medium |
| `packages/analytics/` | Stays as plugin-internal library | Easy |
| `packages/ai-integration/` + `ai-providers/` | Wrap through `LLMAPI` | Medium |
| CSV/OFX import parsers | Plugin-specific code, no change needed | Easy |
| `src/lib/stores/` (Svelte stores) | Plugin-internal, use `PluginContext` for platform state | Medium |
| MCP server (`src/mcp-server/`) | Stays standalone or becomes separate integration | Low priority |

**Migration complexity:** Medium — mostly mechanical mapping, AI packages need wrapping  
**Estimated effort:** 2-3 weeks  
**Breaking changes:** localStorage → PluresDB, direct store imports → PluginContext APIs  
**Deprecation:** Standalone app repo archived after plugin is stable

## Phase 4: Remaining App Migrations

### 4a. sprint-log → `sprint-log` plugin

| Metric | Value |
|---|---|
| LOC | ~18,000 TypeScript/Svelte |
| Current features | Sprint tracking, daily notes, chat, ADO sync, memory indexing, chronicle, praxis |
| Framework | SvelteKit + Tauri |
| Design system | Custom components (TitleBar, StatusBar, ActivityBar, etc.) |
| Manifest | `plugins/sprint-log/manifest.json` in modulus |

**What maps to RadixPlugin:**

| Feature | Plugin API |
|---|---|
| Routes (dashboard, chat, history, settings) | `routes[]` |
| Activity bar items | `navItems[]` |
| Settings | `settings[]` |
| Sprint context | Dashboard widget |

**What stays plugin-internal:** ADO client, note parser, memory client, chronicle, sync services, OpenClaw gateway integration

**Complexity:** Medium-Hard — heavy custom UX (ActivityBar, StatusBar, CommandPalette), deep ADO integration, Tauri-specific code  
**Estimated effort:** 3-4 weeks  
**Breaking changes:** Custom shell components must either move to radix base or become plugin-specific panels  
**Key risk:** Tauri integration — may need to stay as standalone for desktop, plugin for web

### 4b. plures-vault → `vault` plugin

| Metric | Value |
|---|---|
| LOC | ~5,000 Rust |
| Current features | Secret management, graph-native secrets, P2P sync, MCP server |
| Framework | Rust CLI (clap) |
| Branch | `master` (not `main`) |
| Manifest | `plugins/vault/manifest.json` in modulus |

**What maps to RadixPlugin:**

| Feature | Plugin API |
|---|---|
| Secret browsing UI (to build) | `routes[]` |
| Vault nav | `navItems[]` |
| Master password, sync settings | `settings[]` |
| Secret graph visualization | Dashboard widget |

**What stays standalone:** Rust core (`vault-core`, `vault-graph`, `vault-mcp`, `vault-sync`) — these are the actual vault. The plugin is a web UI wrapper.

**Complexity:** Hard — need to build a web UI that doesn't exist yet, bridge Rust backend via WASM or API  
**Estimated effort:** 4-6 weeks  
**Breaking changes:** None (additive — CLI stays)  
**Architecture:** Rust core compiles to WASM or exposes HTTP API; Svelte plugin consumes it

### 4c. netops-toolkit-app → `netops-toolkit` plugin

| Metric | Value |
|---|---|
| LOC | ~6,200 TypeScript/Svelte |
| Current features | Network scanning, health monitoring, config management, config diff |
| Framework | SvelteKit |
| Open issues | 9 unmilestoned |
| Manifest | `plugins/netops-toolkit/manifest.json` in modulus |

**What maps to RadixPlugin:**

| Feature | Plugin API |
|---|---|
| Routes (scan, health, config, config/diff, config/[hostname]) | `routes[]` |
| Nav items | `navItems[]` |
| Settings | `settings[]` |
| Network health overview | Dashboard widget |

**What stays plugin-internal:** Scanner logic, config parser, diff engine

**Complexity:** Easy — clean SvelteKit app, minimal custom shell (bare `+layout.svelte`), straightforward route extraction  
**Estimated effort:** 1-2 weeks  
**Breaking changes:** Minimal  
**Note:** Resolve 9 open issues before or during migration

### 4d. pares-agens → `agent-console` plugin

| Metric | Value |
|---|---|
| LOC | ~50,500 Rust |
| Current features | Agent orchestration, inference, privacy, marketplace, training, MCP client, sync, audit, GPU management |
| Framework | Rust workspace (22 crates) + Tauri desktop |
| Active milestone | v0.6.0 (8 issues) |
| Manifest | `plugins/agent-console/manifest.json` in modulus |

**What maps to RadixPlugin:**

| Feature | Plugin API |
|---|---|
| Agent management UI | `routes[]` |
| Agent nav | `navItems[]` |
| Model settings, provider config | `settings[]` |
| Active agents overview | Dashboard widget |

**What stays standalone:** All Rust crates — this is the actual agent runtime. Like vault, the plugin is a web UI for monitoring/control.

**Complexity:** Hard — massive Rust codebase, Tauri app, active development (v0.6.0)  
**Estimated effort:** 4-6 weeks (UI only, after v0.6.0 milestone)  
**Breaking changes:** None (additive)  
**Key decision:** Wait for v0.6.0 milestone completion before starting migration  
**Architecture:** Rust backend exposes API; Svelte plugin provides management console

## Migration Order

```
Phase 1 (now)     → Radix foundation
Phase 2 (next)    → Svelte shell
Phase 3           → FinancialAdvisor plugin (proves pattern)
Phase 4a          → netops-toolkit (easiest remaining)
Phase 4b          → sprint-log (medium complexity)
Phase 4c          → vault (needs web UI)
Phase 4d          → agent-console (after agens v0.6.0)
```

## Principles

1. **Standalone repos stay alive** until the plugin is proven stable
2. **Plugin = UI + glue.** Domain logic stays in packages/crates
3. **Inference over LLM** — praxis rules handle 80%+ of categorization
4. **Immutable data** — imports are immutable, inferences go to separate collections
5. **UX contracts enforced** — no dead ends, empty states required, nav must resolve
6. **One plugin proves the pattern** before migrating the rest
