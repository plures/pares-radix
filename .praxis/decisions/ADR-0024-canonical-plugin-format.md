# ADR-0024: Canonical Plugin Format & Capability Dependency Registration

## Status: Accepted

## Date: 2026-06-26

> Driver: kbristol, 2026-06-26. Resolves the central finding of the Radix Plugin Program
> (`workspace/memory/plan-radix-plugins-2026-06-26.md`): **two divergent plugin formats exist
> in parallel** and must be reconciled into one canonical model before the plugin estate is
> refactored or new plugins are built. Also formalizes plugin dependency registration
> (VS Code `extensionDependencies`-style) to eliminate duplication (vault/netops both rolling
> their own secret store).

## Context

Two plugin formats grew up side by side and were never reconciled:

| Format | Home | Shape | Governs |
|--------|------|-------|---------|
| **capability / `.px`** | `pares-radix/plugins/*` | `plugin.toml` + pure `.px` procedures + thin TS adapter at the IO boundary + `[capabilities.provided/required]`; cross-plugin interaction is **mediated** (events / PluresDB nodes), no direct refs | ADR-0022 + ADR-0011 |
| **`RadixPlugin` / TS** | `pares-modulus/plugins/*` | `manifest.json` + TS `src/index.ts` exporting a `RadixPlugin` object + Svelte `pages/` + `rules/` + `stores/` | `docs/architecture/plugin-system.md`, modulus README |

This is the same split VS Code resolved long ago: a **manifest** (declarative metadata,
contribution points, dependencies) plus **activation code** (what runs), with the editor
mediating everything in between. Our two formats each capture half of that and disagree on
the other half:

- The **capability/`.px`** model is correct about *logic and boundaries*: pure decision logic
  in `.px` (compiles to PluresDB procedures), IO only at a declared adapter seam, cross-plugin
  calls mediated through events/nodes (auditable, CRDT-native, C-PLURES-003/004). But it says
  little about **UI**.
- The **`RadixPlugin`/TS** model is correct about *UI contribution*: routes, nav items,
  settings, dashboard widgets, help, onboarding — a rich contribution surface the host
  aggregates. But it puts **logic in TS** (`rules/`, `evaluate()` callbacks, `stores/` writing
  to `localStorage`), which violates C-PLURES-003/004 and ADR-0022's mediated boundary, and it
  predates the capability/CID system entirely (no plugin references a CID).

Evidence from the estate (assessments 2026-06-26):
- Every working modulus plugin persists to `window.localStorage` ("Future: replace with
  PluresDB") — **none use `ctx.data.collection()`**. State lives outside PluresDB.
- `vault` and `netops-toolkit` each implement their **own** credential/secret store →
  duplication that a dependency-registration mechanism would remove.
- `netops-toolkit` (modulus) uses an **obsolete** `RadixPlugin` interface shape
  (`settings.schema`, `widgets`, `onLoad`) that no longer type-checks, and is a mock subset of
  the real `plures/netops-toolkit-app` (Svelte 5 + Tauri 2 + Rust, GUI+TUI).
- `design-dojo` (`@plures/design-dojo`) is the established shared Svelte 5 UI kit (dual
  GUI/TUI tokens, praxis/security/sync/telemetry modules) — the natural home for shared plugin
  UI and side-effect-handler UI.

Foundation facts that constrain the decision (research 2026-06-26):
- Canonical `.px` engine is **`pluresdb/crates/pluresdb-px` (v3.0.1)**; the **grammar is stable**
  (frozen at `195c67b`, 2026-06-13; active work is engine internals, not language surface).
- There is **no `capability` grammar construct** and none planned → per ADR-0022, **CIDs are
  TOML host-contracts** (`capabilities/<name>.cid.toml`), not `.px`.
- The modulus registry schema **already has a `dependencies` field** (currently unused for
  capability resolution) → dependency registration extends an existing field, not a new one.

This ADR extends ADR-0010 (agens-first), ADR-0011 (plugin security), and ADR-0022 (capability
host contract). It does not replace them; it unifies the *authoring format* on top of them.

## Decision

### 1. One canonical plugin: manifest (`plugin.toml`) + `.px` logic + adapter IO + UI contribution

A Radix plugin is exactly one directory containing:

```
plugins/<id>/
  plugin.toml              # the single manifest (TOML) — metadata, capabilities, deps, contributions
  procedures/*.px          # ALL decision/inference/validation logic (compiles to PluresDB procedures)
  adapter/*.ts             # thin IO actors ONLY (network, fs, crypto, LLM) — the side-effect boundary
  ui/*.svelte              # contribution components, built on @plures/design-dojo
  capabilities/*.cid.toml  # (provider plugins only) the CID(s) this plugin implements, if host-local
  tests/                   # plugin tests (see §6)
  README.md
```

The **`RadixPlugin` TS object is demoted from "the plugin" to a generated/thin binding**: the
host derives routes/nav/settings/widgets from `plugin.toml` `[contributes.*]` and lazy-loads
`ui/*.svelte`. Authors do not hand-write a `RadixPlugin` literal with `evaluate()` logic
anymore. `manifest.json` (modulus) becomes a **registry projection** of `plugin.toml`
(generated by `build-registry`), not a second source of truth (C-DRIFT-001).

**The three-way split is mandatory and inspectable:**

| Concern | Lives in | Never in |
|---------|----------|----------|
| Decision/inference/validation logic | `procedures/*.px` → PluresDB procedures | TS, Svelte |
| Side effects (network, fs, crypto, LLM, system) | `adapter/*.ts` (declared, permission-gated) | `.px`, UI |
| State | PluresDB via `ctx.data.collection()` | `localStorage`, ad-hoc files, in-memory maps |
| UI contribution | `ui/*.svelte` on `@plures/design-dojo` | `.px`, adapter |
| Cross-plugin interaction | mediated events / PluresDB nodes per a CID | direct function refs |

`rules`/`constraints`/`expectations` that the old `RadixPlugin` interface expressed as TS
callbacks are re-expressed as `.px` `rule`/`constraint` constructs (the grammar already has
them). The plugin-system.md aggregators (`getAllInferenceRules`, `getAllConstraints`, …)
continue to work — they now aggregate over compiled `.px` artifacts, not TS callbacks.

### 2. `plugin.toml` schema (superset, unifies both formats)

```toml
[plugin]
id = "financial-advisor"
name = "Financial Advisor"
version = "0.2.0"
icon = "💰"
description = "Local-first personal finance with .px inference and unum UI"
trust = "community"            # verified | community | local (ADR-0011 §5)

# ── Capabilities (ADR-0022) ──────────────────────────────────────────────
[capabilities.required]        # provider capabilities (versioned interfaces), resolved by the loader
secrets = "^1.0"               # e.g. depends on the vault-provided secrets capability
[capabilities.optional]
llm = "^1.0"                   # feature-detected; absent => degrade
[capabilities.provided]        # (provider plugins only) CID(s) this plugin implements
# (none — financial-advisor is a pure consumer)

# ── Platform permissions (ADR-0011) — closed, host-owned set ──────────────
[permissions]
storage = true                 # PluresDB collections
network = "user-approve"       # if it does any IO
llm = "budgeted"

# ── Plugin dependencies (VS Code-style) — see §3 ──────────────────────────
[dependencies]
plugins = []                   # hard plugin deps loaded first (topo-sort, as today)
capabilities = ["secrets@^1.0"]# capability deps: resolved to a PROVIDER plugin, auto-installed

# ── UI contributions (replaces the RadixPlugin UI fields) ─────────────────
[[contributes.routes]]
path = "/"
component = "ui/Dashboard.svelte"
title = "Overview"
[[contributes.navItems]]
href = "/financial-advisor/"
label = "Finances"
icon = "💰"
[[contributes.settings]]
key = "currency"               # namespaced -> financial-advisor.currency
type = "select"
[[contributes.dashboardWidgets]]
component = "ui/SpendByCategory.svelte"
colspan = 2
priority = 10
```

Platform capabilities (`network`, `storage`, `system`, `ui`, `notify`, `llm`) under
`[permissions]` route to the ADR-0011 gate; provider capabilities under `[capabilities.*]`
route to the ADR-0022 resolver. The loader distinguishes them by the closed host-owned
known-platform-capability registry (ADR-0022 §6).

### 3. Capability dependency registration (the VS Code `extensionDependencies` analogue)

The missing mechanism kbristol named: a plugin **declares a dependency on a capability
provider**, and the toolchain **resolves + installs + binds** it — so consumers stop
re-implementing shared functionality.

- **Declaration:** `[dependencies].capabilities = ["secrets@^1.0", …]` in `plugin.toml`.
  This is sugar over ADR-0022 `[capabilities.required]` PLUS an **install-time guarantee**:
  unlike a bare `required` (which only *resolves against already-installed* providers), a
  declared *dependency* instructs modulus to **fetch and install a satisfying provider** if one
  is not present (exactly like VS Code installing an extension's `extensionDependencies`).
- **Resolution (modulus, install time):**
  1. Read `[dependencies].capabilities` from the plugin being installed.
  2. For each `cap@range`, query the registry index for plugins whose
     `[capabilities.provided]` satisfies the range (registry now indexes `provided` CIDs).
  3. Pick a provider by ADR-0022 §4 policy (pin > highest compatible > trust tier > prompt),
     install it (recursively resolving ITS deps), record the choice.
  4. Write the dependency edge so radix's loader (ADR-0022 resolver) binds consumer→provider
     at activation and orders the topo-sort (provider activates first).
- **Default providers:** the host MAY declare a default provider for a capability
  (`secrets → vault`) so resolution is unambiguous out of the box; the user can override via the
  ADR-0022 pin (`radix:capability:pin:secrets`).
- **Deduplication outcome:** `vault` becomes the **provider** of `secrets@1.x`; `financial-advisor`,
  `pim`, `hyperswarm-git`, and a refactored `netops-toolkit` all **depend on** `secrets@^1.0`
  instead of each shipping a credential store. One implementation, many consumers — the explicit
  goal of directive #2.

### 4. Registry (`pares-modulus`) changes

- `registry/schema.json`: make `dependencies` capability-aware (`{ plugins: [...],
  capabilities: ["name@range", …] }`) and index each plugin's `[capabilities.provided]` so the
  resolver can find providers. (ADR-0022 step 6 intentionally deferred these schema fields to
  avoid risk; this ADR un-defers them as deliberate, tested work.)
- `build-registry` generates `manifest.json`/`index.json` **from `plugin.toml`** (single source
  of truth; C-DRIFT-001). A plugin author edits `plugin.toml` only.
- New gate `validate-dependencies`: every declared `capabilities` dep has at least one provider
  in the registry (or is a known host built-in) → else block (actionable: "no provider for
  secrets@^1.0").
- Existing `validate-cid-surface` (ADR-0022) continues to validate provider surface against the
  referenced CID.

### 5. UI home: `@plures/design-dojo`

- Shared plugin UI primitives, and **radix side-effect-handler UI** (progress, approval prompts,
  connection status for long-lived IO actors), live in **design-dojo** — it already carries
  widgets/data-viz/security/sync/telemetry + dual GUI/TUI tokens, which is what gives the
  netops app its GUI/TUI parity. Adding to design-dojo when strategically beneficial is
  pre-authorized (directive #4).
- **Product-specific** screens stay in the consuming plugin's `ui/` (or the standalone app),
  not in design-dojo. design-dojo is the kit, not the catalog of every product's pages.
- Plugins consume via subpaths: `@plures/design-dojo/primitives`, `/surfaces`, `/tokens.css`.

### 6. Testing gate (closes the estate-wide gap)

- Tests **BLOCK** (not warn) for any plugin declaring `[capabilities.provided]` — a provider
  with no tests cannot be trusted against its CID (C-TEST-002). For pure consumers, tests remain
  strongly encouraged (warn) but the **build-the-binary/hit-the-API** discipline (C-TEST-001/002)
  applies to the host integration test that loads the plugin.
- QA is channel-independent: load the plugin in a real radix instance, drive its capability
  events / `ctx.data` through the host API, assert PluresDB state — never through a chat adapter.

### 7. Migration path for the existing estate

Ordered, gated, lifecycle-driven (no in-place stub upgrades — C-NOSTUB-001):

1. **commerce** — already canonical; becomes the reference template. (Pull its e2e in per ADR-0022 follow-up.)
2. **financial-advisor** — highest-value, real logic. Re-author: TS `rules/` → `procedures/*.px`;
   `stores/`+`localStorage` → `ctx.data` PluresDB collections; pages → `ui/` on design-dojo;
   manifest.json → `plugin.toml`. This is also the **localStorage→PluresDB migration exemplar**.
3. **vault** → **secrets@1.x provider**, backed by the real `plures-vault`. Authoring of the
   `secrets.cid.toml` CID is part of this step. Unblocks dedup for everyone.
4. **netops-toolkit** — do NOT refactor the mock plugin in place; **wrap `plures/netops-toolkit-app`**
   (consume design-dojo) with the Python `netops-toolkit` engine as the IO actor; depend on
   `secrets@^1.0`. The mock plugin is throwaway.
5. **agent-console** — refactor as a consumer of ADR-0023 procedure-observability events.
6. **omniscient** — RE-SCOPE/DEFER (depends on non-existent `bitnet`/`rector` providers; revisit
   when those are real). The real `crates/omniscient` engine is fine; only the plugin binding waits.
7. **pedantic / sprint-log** — DEFER / wrap the real standalone `sprint-log` app later (P3).

### 8. What this explicitly forbids going forward

- Hand-written `RadixPlugin` TS objects carrying decision logic (`evaluate()`/`validate()` bodies).
- Any plugin state in `localStorage`/ad-hoc files/in-memory maps (must be `ctx.data`/PluresDB).
- A second manifest source of truth (`manifest.json` is generated from `plugin.toml`).
- Each plugin shipping its own copy of a shared capability (use a declared capability dependency).
- Direct cross-plugin function references (must be CID-mediated events/nodes).

## Consequences

**Positive**
- One format. Authors learn `plugin.toml` + `.px` + `ui/` once. The VS Code mental model
  (manifest + contributions + activation, host-mediated) transfers directly.
- C-PLURES-003/004 finally hold across the estate: logic in PluresDB procedures, state in
  PluresDB collections, IO only at declared adapters.
- Duplication dies: `secrets@1.x` (vault) is implemented once; consumers depend on it.
- Registry is single-source (`plugin.toml`), drift-proof (C-DRIFT-001), and dependency-aware.
- design-dojo becomes the coherent UI/runtime-UI home (incl. side-effect-handler UI), giving
  new plugins GUI/TUI parity for free.

**Negative / costs**
- Real migration cost for the 6 modulus plugins (financial-advisor, vault, netops are the
  meaningful ones). Done as gated lifecycle work, not a flag day.
- `build-registry` must learn `plugin.toml`→`manifest.json` projection; modulus gains two gates
  (`validate-dependencies`, plus the un-deferred schema fields). More registry surface to test.
- The host needs a `plugin.toml` contribution loader (routes/nav/settings/widgets from TOML +
  lazy Svelte) replacing the hand-written `RadixPlugin` aggregation path.

**Risks**
- Projection drift between `plugin.toml` and generated `manifest.json` (mitigation: generate in
  CI, never hand-edit `manifest.json`, gate on regeneration — C-DRIFT-001).
- Capability dependency install loops / diamond deps (mitigation: ADR-0022 cycle detection over
  the combined deps+capability graph already rejects cycles; recursive install is bounded by it).
- design-dojo becoming a dumping ground for product UI (mitigation: §5 — kit vs. catalog rule;
  product screens stay in the consuming plugin).

## Implementation outline (lifecycle work, gated; design = Pillar 1 only here)

1. **`plugin.toml` superset + contribution loader** in pares-radix: parse `[contributes.*]`,
   `[permissions]`, `[dependencies]`; derive the aggregated views (routes/nav/settings/widgets)
   the host already exposes; lazy-load `ui/*.svelte`. Reuse the single parse path from ADR-0022
   step 1 (the C-DRIFT-001 fix) — do not add a third TOML parser.
2. **Capability dependency resolution** in modulus: schema fields + `provided` indexing +
   `validate-dependencies` gate + install-time provider fetch (recursive) + persisted choice.
   Build against the existing ADR-0022 resolver/binding-policy; this is the install-time
   front-end to it.
3. **`build-registry` projection** `plugin.toml`→`manifest.json`/`index.json` (single source).
4. **secrets@1.x CID + vault provider** (`capabilities/secrets.cid.toml` + port `plures-vault`)
   as the first dependency-registration proof, then migrate financial-advisor to depend on it.
5. **Test gate** flip to BLOCK for providers; host integration test that loads a plugin from
   `plugin.toml` and drives it via `ctx.data`/events.
6. **Author guide update** (`docs/PLUGIN-AUTHOR-GUIDE.md`): the canonical format, the three-way
   split, capability dependencies, design-dojo UI, the testing gate.

## References
- ADR-0010 — Agens-first plugin model
- ADR-0011 — Plugin security (platform capabilities, mediated IO, trust tiers)
- ADR-0021 — Grammar as generated artifact (`.px` grammar is generated from praxis `px-ast`)
- ADR-0022 — Capability host contract (provider capabilities, CIDs, resolver, binding policy)
- ADR-0023 — Procedure observability event contract (agent-console consumes this)
- `docs/architecture/plugin-system.md` — the RadixPlugin contribution surface (now derived from `plugin.toml`)
- `capabilities/commerce.cid.toml` — the reference CID this format builds on
- C-PLURES-003 / C-PLURES-004 — state in PluresDB; pure logic in PluresDB, IO at the boundary
- C-NOSTUB-001 — no stubs; migrate for real or leave absent
- C-DRIFT-001 — generated artifacts (manifest.json) must derive from source (plugin.toml) in CI
- C-TEST-001 / C-TEST-002 — channel-independent QA; build the binary, hit the API
- Research (2026-06-26): `workspace/memory/research-px-consolidation-2026-06-26.md` (canonical engine
  = pluresdb-px v3.0.1, grammar stable, no `capability` construct),
  `research-netops-designdojo-2026-06-26.md` (netops-toolkit-app prior art; design-dojo UI home)
- Program plan: `workspace/memory/plan-radix-plugins-2026-06-26.md`
