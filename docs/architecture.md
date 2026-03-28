# Praxis Platform Architecture

> **Status**: Proposal  
> **Date**: 2026-03-27  
> **Author**: kbristol + mswork  
> **Scope**: Plures ecosystem — platform consolidation + inference layer

---

## Problem Statement

We have 5+ standalone applications (FinancialAdvisor, plures-vault, sprint-log, netops-toolkit-app, pares-agens) that each reinvent:

- Navigation and layout
- Settings and configuration
- Data import/export
- LLM provider integration
- Help and onboarding
- Authentication and identity
- PluresDB integration

Meanwhile, `svelte-tauri-template` already has a `plugins/` system with praxis, pluresdb, unum, ADP, and state-docs. The template *is* the application. The apps should be plugins.

Additionally, praxis handles business logic well but has no concept of:
- User experience contracts (UX flows, prerequisites, dead-end prevention)
- Inference with uncertainty (fuzzy matching, confidence scores, decision ledgers)
- Proactive context assembly (subconscious preprocessing before LLM invocation)

---

## Architecture

### Layer 1: Praxis Base Application

A single Svelte + Tauri application that provides everything every plugin needs:

```
praxis-app/
├── src/
│   ├── routes/
│   │   ├── +layout.svelte          # Persistent nav, sidebar, theming
│   │   ├── +page.svelte            # Dashboard (aggregates plugin widgets)
│   │   ├── settings/               # Unified settings (display, AI, data, plugins)
│   │   ├── help/                   # Aggregated help from all active plugins
│   │   └── [plugin]/               # Dynamic routes mounted by plugins
│   ├── lib/
│   │   ├── platform/
│   │   │   ├── navigation.ts       # Route registry, breadcrumbs, dead-end prevention
│   │   │   ├── settings.ts         # Settings store, persistence, plugin settings slots
│   │   │   ├── data.ts             # Import/export orchestration, PluresDB connection
│   │   │   ├── llm.ts              # Provider config, token budgeting, context assembly
│   │   │   ├── onboarding.ts       # Plugin-aware setup wizards
│   │   │   └── plugin-loader.ts    # Plugin discovery, lifecycle, dependency resolution
│   │   └── praxis/
│   │       ├── engine.ts           # Core praxis runtime
│   │       ├── inference.ts        # Inference layer (confidence, decision ledger)
│   │       ├── ux-contracts.ts     # UX journey expectations and constraints
│   │       └── subconscious.ts     # Background preprocessing, context management
│   └── plugins/                    # Built-in + user-installed plugins
│       ├── pluresdb/
│       ├── praxis/
│       ├── unum/
│       ├── state-docs/
│       └── svelte-ratatui/
├── plugins/                        # External plugin packages
│   ├── financial-advisor/
│   ├── vault/
│   ├── sprint-log/
│   ├── netops-toolkit/
│   └── pares-agens/
└── plugin-api/                     # Plugin contract / SDK
    ├── types.ts
    ├── hooks.ts
    └── README.md
```

### Plugin Contract

Every plugin provides a manifest and hooks:

```typescript
interface PraxisPlugin {
  id: string;                         // "financial-advisor"
  name: string;                       // "Financial Advisor"
  version: string;
  icon: string;                       // Emoji or icon path
  description: string;

  // What the base app needs from the plugin
  routes: PluginRoute[];              // Pages to mount
  settings: PluginSetting[];          // Settings to add to unified settings
  navItems: NavItem[];                // Sidebar items
  dashboardWidgets: DashboardWidget[];// Home dashboard cards
  helpSections: HelpSection[];        // Help content
  onboardingSteps: OnboardingStep[];  // Getting started steps

  // Praxis integration
  expectations: Expectation[];        // Business + UX expectations
  rules: Rule[];                      // Inference rules
  constraints: Constraint[];          // Validation constraints

  // Lifecycle
  onActivate(ctx: PluginContext): Promise<void>;
  onDeactivate(): Promise<void>;
  onDataImport(data: unknown): Promise<void>;
  onDataExport(): Promise<unknown>;
}
```

The base app handles:
- **Navigation**: Aggregates all plugin `navItems` into the sidebar. Enforces no dead ends.
- **Settings**: Renders all plugin `settings` in the unified settings page.
- **Help**: Combines all plugin `helpSections` into a single searchable guide.
- **Onboarding**: Sequences all plugin `onboardingSteps` based on dependencies.
- **Dashboard**: Lays out all plugin `dashboardWidgets` on the home page.
- **Data**: Orchestrates import/export across all plugins.
- **LLM**: Shared provider config, token budgeting, context assembly.

### What Changes for Existing Apps

| Current Repo | Becomes | What Stays | What Moves to Base |
|---|---|---|---|
| FinancialAdvisor | `plugins/financial-advisor/` | Transaction logic, budget tracking, categorization rules, financial advice | Layout, nav, settings UI, help, import/export shell, LLM config |
| plures-vault | `plugins/vault/` | Secret management, encryption, key rotation | Same |
| sprint-log | `plugins/sprint-log/` | Sprint tracking, velocity charts, retro notes | Same |
| netops-toolkit-app | `plugins/netops-toolkit/` | Network diagnostics, topology, monitoring | Same |
| pares-agens | `plugins/pares-agens/` | Agent orchestration, channel adapters, subconscious | Same |

---

## Layer 2: Praxis Inference Engine

The common pattern across all plugins:

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  Immutable Data  │────▶│  Praxis Inference     │────▶│  LLM Advisory   │
│  (raw imports)   │     │  (rules + confidence) │     │  (contextual)   │
└─────────────────┘     └──────────────────────┘     └─────────────────┘
                               │
                        ┌──────┴──────┐
                        │  Decision   │
                        │  Ledger     │
                        └─────────────┘
```

### Inference Table Schema

```typescript
interface Inference {
  id: string;
  sourceId: string;              // FK to immutable source record
  sourceType: string;            // "transaction" | "memory" | "event" | ...
  field: string;                 // What was inferred ("category", "relevance", ...)
  value: unknown;                // The inferred value
  confidence: number;            // 0.0 – 1.0
  strategy: string;              // Which rule/method produced this
  decisionChain: string[];       // Ordered list of rule IDs that contributed
  confirmed: boolean;
  confirmedBy: "user" | "auto" | "llm";
  createdAt: string;
  updatedAt: string;
}
```

### Decision Ledger

Every inference is auditable:

```typescript
interface DecisionEntry {
  id: string;
  inferenceId: string;
  ruleId: string;
  input: Record<string, unknown>;   // What the rule saw
  output: unknown;                   // What it concluded
  confidenceDelta: number;           // How much this rule moved confidence
  reasoning: string;                 // Human-readable explanation
  timestamp: string;
}
```

### Built-in Inference Rules (Financial Domain Example)

```
Rule: recurring-amount-pattern
  IF: same vendor (±fuzzy), same amount (±5%), monthly interval (±5 days)
  THEN: same category as previous confirmed occurrence
  Confidence: 0.85 base, +0.05 per additional confirmed match

Rule: tax-variance-detection
  IF: recurring vendor, amount increases by consistent small %
      across multiple recurring charges simultaneously
  THEN: tax rate change, inherit category
  Confidence: 0.80

Rule: refund-match
  IF: credit from similar vendor, within 30 days of debit,
      amount matches delta between old and new recurring amounts
  THEN: refund transaction, inherit parent category
  Confidence: 0.90

Rule: vendor-clustering
  IF: vendor names have >0.85 string similarity (Jaro-Winkler)
      OR known alias mapping exists
  THEN: same vendor, inherit category
  Confidence: 0.92 (alias) / 0.78 (fuzzy)
```

These rules are praxis primitives. They compose, compound confidence, and produce auditable decision chains.

---

## Layer 3: UX Journey Contracts

New praxis expectation domain: `ux`

```typescript
// UX expectations — validated at build time and runtime
const uxExpectations: Expectation[] = [
  {
    id: "ux-no-dead-ends",
    domain: "ux",
    description: "Every page must have navigation back to parent or home",
    severity: "error",
    validate: (routes) => routes.every(r => r.hasBackNav || r.isHome),
  },
  {
    id: "ux-prereqs",
    domain: "ux",
    description: "Pages with data dependencies show empty states with actions",
    severity: "error",
    validate: (routes) => routes
      .filter(r => r.requiresData)
      .every(r => r.hasEmptyState && r.emptyState.hasAction),
  },
  {
    id: "ux-import-before-reports",
    domain: "ux",
    description: "User must have imported data before viewing reports",
    severity: "warning",
    validate: (state) => !state.viewingReports || state.hasTransactions,
  },
  {
    id: "ux-confidence-gate",
    domain: "ux",
    description: "Inferences below 0.7 confidence require user confirmation",
    severity: "error",
    validate: (inference) => inference.confidence >= 0.7 || inference.pendingConfirmation,
  },
];
```

---

## Layer 4: Subconscious / Proactive Context Assembly

Runs as a background process (like pares-agens already has). Responsibilities:

1. **Preprocessing**: When new data arrives, run all inference rules immediately. Don't wait for the user to ask.
2. **Context management**: Maintain a rolling "briefing" of what matters right now. When the LLM is invoked, it gets pre-assembled context, not raw data.
3. **Rule generation**: Observe patterns in user confirmations. If the user keeps overriding a rule, generate a new rule that captures the exception.
4. **Expectation evolution**: Track which praxis expectations are violated most. Suggest new expectations based on observed failure patterns.
5. **Cross-plugin awareness**: Financial data informs sprint velocity (correlation between team spending patterns and delivery). Network status informs availability expectations.

---

## Layer 5: Federated Intelligence (Future)

Anonymized, PII-free sharing of:

- **Inference rules**: Vendor→category mappings with confidence from aggregate user confirmations
- **Decision→outcome mappings**: "Users who did X saw outcome Y" (anonymized, aggregated)
- **Rule effectiveness scores**: Which rules have the highest confirmation rates across the ecosystem
- **New rule proposals**: Rules discovered by one user's subconscious, shared for others to opt into

Implementation: praxis rules are pure logic — no PII, no transaction data. They can be serialized, signed, and distributed via PluresDB P2P sync or a central registry.

---

## Migration Path

### Phase 1: Base Application (Q2 2026)
- Promote `svelte-tauri-template` to `praxis-app`
- Extract base infrastructure: layout, nav, settings, help, onboarding, data, LLM
- Define plugin API contract
- Port FinancialAdvisor as first plugin (proof of concept)

### Phase 2: Inference Engine (Q2-Q3 2026)
- Implement inference table + decision ledger in PluresDB
- Build 4 core financial inference rules
- Add `ux` expectation domain to praxis
- Implement confidence gating in the UI

### Phase 3: Plugin Migration (Q3 2026)
- Port remaining apps: vault, sprint-log, netops-toolkit, pares-agens
- Shared LLM integration layer
- Cross-plugin data awareness

### Phase 4: Subconscious + Federation (Q3-Q4 2026)
- Background inference processing
- Context assembly for LLM calls
- Rule generation from user behavior
- Anonymized rule sharing network

---

## Implications

1. **svelte-tauri-template becomes the product**: Not a starting point, the actual platform
2. **Existing repos become plugin packages**: Lighter, focused on domain logic only
3. **praxis gains three new domains**: `ux` (journey contracts), `inference` (uncertainty/confidence), `subconscious` (proactive processing)
4. **LLM costs drop dramatically**: 80%+ of inference handled by praxis rules, LLM reserved for novel situations and advisory
5. **PluresLM benefits directly**: Same inference pattern applies to memory — praxis-driven categorization, relevance scoring, and proactive context assembly replace pure embedding similarity
6. **Network effects**: Federated rules improve with scale without compromising privacy

---

## Open Questions

1. Should plugins be NPM packages, Tauri plugins (Rust), or both?
2. How do we handle plugin conflicts (two plugins wanting the same route)?
3. Should the base app ship with *any* built-in plugins, or is everything optional?
4. How does the TUI layer (svelte-ratatui) interact with plugins? Each plugin provides a TUI view?
5. What's the plugin update/versioning strategy?
