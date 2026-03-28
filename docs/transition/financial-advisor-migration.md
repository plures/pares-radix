# FinancialAdvisor → Radix Plugin Migration Guide

> Step-by-step guide for migrating FinancialAdvisor from standalone SvelteKit app to a pares-radix plugin.

## Current Architecture

### Routes (`src/routes/`)

| Route | Component | Purpose |
|---|---|---|
| `/` | `+page.svelte` | Dashboard/home |
| `/accounts` | `accounts/+page.svelte` | Account management |
| `/transactions` | `transactions/+page.svelte` | Transaction list |
| `/budgets` | `budgets/+page.svelte` | Budget tracking |
| `/goals` | `goals/+page.svelte` | Savings goals |
| `/reports` | `reports/+page.svelte` | Financial reports |
| `/review` | `review/+page.svelte` | Categorization review |
| `/review/import` | `review/import/+page.svelte` | Import review |
| `/review/merchants` | `review/merchants/+page.svelte` | Merchant review |
| `/review/categories` | `review/categories/+page.svelte` | Category review |
| `/review/recurring` | `review/recurring/+page.svelte` | Recurring detection review |
| `/settings` | `settings/+page.svelte` | App settings |
| `/help` | `help/+page.svelte` | Help content |

### Packages (`packages/`)

| Package | LOC (approx) | Purpose | Migration target |
|---|---|---|---|
| `storage/` | ~2,500 | Schema, stores (account, transaction, merchant, posting, import-session, review-decision, recurring-series, hash) | → `DataAPI.collection()` |
| `analytics/` | ~3,000 | Budget, cash-flow, net-worth, goals, recurring, subscription, debt, runway, burn, variance, investment, timeline, predictive, scenario | Plugin-internal (no change) |
| `ledger/` | ~1,000 | Snapshots, account-integration-service | Plugin-internal |
| `advice/` | ~1,700 | Contextual financial advice | Plugin-internal, optionally wire through `LLMAPI` |
| `ai-integration/` | ~2,000 | AI orchestration layer | → Wrap through `LLMAPI` |
| `ai-providers/` | ~2,000 | Provider implementations (OpenAI, etc.) | → Replaced by radix LLMAPI provider |
| `vscode-extension/` | ~400 | VS Code extension | Stays separate |

### Key Library Files

| File | Purpose |
|---|---|
| `src/lib/stores/financial.ts` | Svelte stores for financial data |
| `src/lib/stores/review.ts` | Review workflow state |
| `src/lib/pluresdb/store.ts` | PluresDB integration |
| `src/lib/ai/categorizer.ts` | AI-powered categorization |
| `src/lib/praxis/schema.ts` | Praxis domain schema |
| `src/lib/praxis/logic.ts` | Praxis business logic |
| `src/lib/praxis/lifecycle.ts` | Praxis lifecycle hooks |

## Plugin Manifest

Already exists at `pares-modulus/plugins/financial-advisor/manifest.json`:

```json
{
  "id": "financial-advisor",
  "name": "Financial Advisor",
  "version": "0.1.0",
  "description": "AI-powered personal finance management with praxis inference",
  "author": "plures",
  "license": "MIT",
  "icon": "💰",
  "entry": "src/index.ts",
  "radix": ">=0.1.0",
  "dependencies": [],
  "peerDependencies": { "@plures/design-dojo": ">=0.1.0" }
}
```

## Plugin Entry (Already Scaffolded)

The plugin implementation exists at `pares-modulus/plugins/financial-advisor/src/index.ts`. It already defines:

- **8 routes** with data requirements and empty states
- **7 nav items** (nested under Finance)
- **3 settings** (currency, auto-categorize, confidence threshold)
- **3 dashboard widgets** (net worth, budget status, recent transactions)
- **2 help sections** (getting started, how categorization works)
- **2 onboarding steps** (add account → import transactions, dependency-ordered)
- **4 inference rules** (recurring-amount, vendor-clustering, refund-detection, tax-variance)
- **2 expectations** (immutable imports, reports require data)
- Lifecycle hooks (`onActivate`, `onDeactivate`, `onDataImport`, `onDataExport`)

## Migration Steps

### Step 1: Extract Route Components

Move each page component from standalone SvelteKit routes to plugin-owned components:

```
FinancialAdvisor/src/routes/accounts/+page.svelte
  → pares-modulus/plugins/financial-advisor/src/pages/Accounts.svelte

FinancialAdvisor/src/routes/transactions/+page.svelte
  → pares-modulus/plugins/financial-advisor/src/pages/Transactions.svelte
```

**Key change:** Remove SvelteKit `$app/stores` usage (e.g., `$page`). Replace with radix `NavigationAPI` from `PluginContext`.

Each route is already mapped in `index.ts`:
```typescript
{ path: '/transactions', component: () => import('./pages/Transactions.svelte'), ... }
```

Routes auto-namespace to `/financial-advisor/transactions` via the plugin-loader's `getAllRoutes()`.

### Step 2: Extract Settings

The standalone app's `src/routes/settings/+page.svelte` has plugin-specific settings. These are already declared in the plugin's `settings[]` array. The radix shell renders them in the unified settings page.

**What moves to radix base:** The settings page layout/component pattern.  
**What stays in plugin:** The 3 `PluginSetting` declarations — no UI needed, radix renders the form.

### Step 3: Wire Inference Rules

Already done. Four rules exist in `plugins/financial-advisor/src/rules/`:

| Rule | File | Base Confidence |
|---|---|---|
| Recurring Amount Pattern | `recurring-amount.ts` | 0.85 |
| Vendor Clustering | `vendor-clustering.ts` | — |
| Refund Detection | `refund-detection.ts` | — |
| Tax Variance | `tax-variance.ts` | — |

These implement the `InferenceRule` interface from `@plures/pares-radix`. The inference engine (`platform/inference-engine.ts`) discovers them via `getAllInferenceRules()` and runs them against `sourceType: 'transaction'` records.

Compound confidence: when multiple rules fire for the same field+value, scores merge as `1 - ∏(1 - cᵢ)`. Auto-confirm threshold: 0.90.

### Step 4: Migrate Storage to PluresDB via DataAPI

**Before (standalone):** `packages/storage/` with direct store classes (AccountStore, TransactionStore, etc.)

**After (plugin):** Use `PluginContext.data.collection(name)` — PluresDB collections namespaced to the plugin.

```typescript
// Before
import { AccountStore } from '@financial-advisor/storage';
const accounts = new AccountStore(db);

// After
async onActivate(ctx: PluginContext) {
  const accounts = ctx.data.collection('accounts');
  const transactions = ctx.data.collection('transactions');
  const merchants = ctx.data.collection('merchants');
  const inferences = ctx.data.collection('inferences');
  // etc.
}
```

Collections needed (from `packages/storage/src/`):
- `accounts` (account-store)
- `transactions` / `raw-transactions` (canonical + raw)
- `merchants` (merchant-store)
- `import-sessions` (import-session-store)
- `review-decisions` (review-decision-store)
- `recurring-series` (recurring-series-store)
- `postings` (posting-store)

**Migration path:** Write a `onDataImport` handler that reads localStorage/IndexedDB data and writes to PluresDB collections.

### Step 5: UX Components — What Moves vs. What Stays

**Moves to radix base shell (Phase 2):**
- Sidebar layout pattern (from `+layout.svelte` — collapsible, mobile-responsive)
- Settings page renderer (renders `PluginSetting[]` as forms)
- Help page aggregator (renders `HelpSection[]`)
- Onboarding wizard (renders `OnboardingStep[]`, tracks completion)
- Dashboard grid (renders `DashboardWidget[]`)
- Empty state component (renders `DataRequirement` unmet states)

**Stays in plugin:**
- All page components (Accounts, Transactions, Budgets, Goals, Reports, Review, Import)
- Review sub-pages (merchants, categories, recurring, import)
- Dashboard widget components (NetWorth, BudgetStatus, RecentTransactions)
- Analytics visualizations (charts, graphs)

### Step 6: Import Parsers

CSV and OFX parsers stay as plugin-specific code. No radix API needed — these are pure data transformation functions.

```
FinancialAdvisor/src/routes/review/import/+page.svelte
  → plugins/financial-advisor/src/pages/Import.svelte
  → plugins/financial-advisor/src/parsers/csv.ts
  → plugins/financial-advisor/src/parsers/ofx.ts
```

### Step 7: AI Packages → LLMAPI

**Before:** Direct provider management via `packages/ai-providers/` (OpenAI keys, model selection, token counting).

**After:** Use `PluginContext.llm` (`LLMAPI`):

```typescript
// Before
import { OpenAIProvider } from '@financial-advisor/ai-providers';
const result = await provider.complete(prompt);

// After
if (ctx.llm.available()) {
  const result = await ctx.llm.complete(prompt, { transactions, categories });
}
```

The `LLMAPI` abstracts provider selection, token budgets, and API keys — configured at the radix platform level, not per-plugin.

**What stays plugin-internal:** `packages/advice/` (prompt construction, context assembly). The advice package builds prompts; LLMAPI sends them.

**Key benefit:** Praxis inference rules handle 80%+ of categorization. LLM is only called for novel vendors or contextual advice, saving significant tokens.

### Step 8: Testing Strategy

| Layer | What to test | How |
|---|---|---|
| Inference rules | Rule evaluation, confidence scoring, edge cases | Unit tests (vitest) — `rules/*.test.ts` |
| Analytics | Budget calculations, cash flow, predictions | Unit tests — already pure functions |
| Storage migration | localStorage → PluresDB data integrity | Integration test with mock DataAPI |
| Plugin registration | Routes resolve, nav items present, settings render | Integration test with mock PluginContext |
| UX contracts | No dead ends, empty states, nav resolution | Run `validateUxExpectations()` in CI |
| E2E | Import → categorize → review → budget flow | Playwright against running radix shell |

**Priority:** Inference rule tests first (currently zero test coverage on AI code), then storage migration tests.

## Checklist

- [ ] Phase 2 shell components built (blocks all below)
- [ ] Route components extracted from SvelteKit to standalone Svelte
- [ ] `$app/stores` replaced with `PluginContext.navigation`
- [ ] Storage classes replaced with `DataAPI.collection()` calls
- [ ] Data migration handler in `onDataImport()`
- [ ] AI providers replaced with `LLMAPI`
- [ ] Import parsers moved to plugin `src/parsers/`
- [ ] Inference rule unit tests written
- [ ] UX contract validation passing in CI
- [ ] E2E smoke test: account → import → categorize → review
- [ ] Standalone repo archived with pointer to plugin
