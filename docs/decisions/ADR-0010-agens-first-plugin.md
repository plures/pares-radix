# ADR-0010: Pares-Agens as First Radix Plugin — Praxis-Native Architecture

## Date: 2026-04-08
## Status: APPROVED
## Decision Makers: kbristol (Paradox)

---

## Thesis

Pares-radix is not a Tauri app with a plugin system. It IS a praxis application — a set of Facts, Events, Rules, and Constraints that define what a plugin platform does. PluresDB is not "connected" — it emerges from praxis as the persistence layer. UI is not "built" — it's generated from schemas via design-dojo. Pares-agens is the first plugin because it's the meta-plugin: the agent that builds all future plugins.

**The old thinking (wrong):**
```
Wire Tauri → Connect PluresDB → Build plugin loader → Mount components
```

**The praxis way (correct):**
```
Define Facts → Define Rules → Define Constraints → Everything else is derived
```

---

## Core Principle: Praxis Logic First

Every behavior in radix and agens is expressed as praxis primitives:

| Primitive | What it means in radix |
|-----------|----------------------|
| **Fact** | `plugin.registered`, `route.active`, `nav.item.visible`, `agent.status.ready` |
| **Event** | `plugin.install.requested`, `user.navigated`, `agent.message.received` |
| **Rule** | "When `plugin.install.requested`, validate manifest → emit `plugin.registered`" |
| **Constraint** | "A registered plugin MUST have all dependencies satisfied" |
| **Contract** | Every rule has examples, invariants, and edge cases documented |
| **Gate** | `plugin-ready` gate blocks activation until all constraints pass |

There are NO `if/else` chains. There are NO imperative plugin loaders. There are NO direct database calls. The praxis engine processes events, fires rules, checks constraints, and PluresDB persists the resulting facts automatically.

---

## What Radix Declares (v0.3 — The Registry)

### Domain: Platform Shell

```typescript
// Facts
const PluginRegistered = defineFact<'plugin.registered', { id: string; manifest: PluginManifest }>('plugin.registered');
const PluginActivated = defineFact<'plugin.activated', { id: string }>('plugin.activated');
const RouteActive = defineFact<'route.active', { path: string; pluginId: string }>('route.active');
const NavVisible = defineFact<'nav.visible', { items: NavItem[] }>('nav.visible');
const ThemeApplied = defineFact<'theme.applied', { theme: ThemeConfig }>('theme.applied');

// Events
const PluginInstallRequested = defineEvent<'plugin.install.requested', { manifest: PluginManifest }>('plugin.install.requested');
const UserNavigated = defineEvent<'user.navigated', { path: string }>('user.navigated');
const AppBooted = defineEvent<'app.booted', {}>('app.booted');

// Rules
const pluginRegistrationRule = defineRule({
  id: 'platform.plugin.register',
  description: 'Validate and register a plugin from its manifest',
  eventTypes: 'plugin.install.requested',
  contract: defineContract({
    ruleId: 'platform.plugin.register',
    behavior: 'Validates manifest, resolves dependencies, emits plugin.registered or plugin.rejected',
    examples: [
      { given: 'Valid manifest with satisfied deps', when: 'plugin.install.requested', then: 'Emits plugin.registered' },
      { given: 'Manifest with missing dependency', when: 'plugin.install.requested', then: 'Emits plugin.rejected with reason' },
    ],
    invariants: ['A plugin can only be registered once', 'Dependencies must form a DAG (no cycles)'],
  }),
  impl: (state) => { /* rule logic */ },
});

// Constraints
const pluginDependencyConstraint = defineConstraint({
  id: 'platform.plugin.deps-satisfied',
  description: 'All registered plugins must have their dependencies satisfied',
  contract: defineContract({ /* ... */ }),
  impl: (state) => {
    // Check all plugin.registered facts — each must have deps in the registry
    // Returns true or error message
  },
});
```

### Domain: Agent (agens plugin)

```typescript
// The three-agent cognitive architecture as praxis primitives
const CerebellumRouted = defineFact<'agent.cerebellum.routed', { intent: string; targets: string[] }>('agent.cerebellum.routed');
const ConsciousExecuted = defineFact<'agent.conscious.executed', { taskId: string; result: unknown }>('agent.conscious.executed');
const SubconsciousInsight = defineFact<'agent.subconscious.insight', { topic: string; insight: string }>('agent.subconscious.insight');

// Cerebellum routing is a RULE, not an if/else chain
const cerebellumRoutingRule = defineRule({
  id: 'agent.cerebellum.route',
  description: 'Classify user prompt and route to conscious/subconscious',
  eventTypes: 'agent.message.received',
  contract: defineContract({
    ruleId: 'agent.cerebellum.route',
    behavior: 'Autorecalls context, classifies intent, formulates targeted prompts for conscious and subconscious',
    invariants: ['Every message gets routed', 'Conscious always receives a targeted prompt, never raw memories'],
  }),
  impl: (state) => { /* cerebellum logic */ },
});
```

---

## What is NOT in radix code

| Anti-pattern | Why it's wrong | What to do instead |
|---|---|---|
| `if (plugin.isValid()) loadPlugin(plugin)` | Imperative logic | Rule: `plugin.install.requested` → validate → emit `plugin.registered` |
| `<button onclick={navigate}>` | Raw HTML | Design-dojo `<Button>` generated from schema |
| `db.put('plugins', manifest)` | Direct DB call | Praxis emits fact → PluresDB persists automatically |
| `import PluresDB from '@plures/pluresdb'` | Manual wiring | Praxis adapter handles persistence |
| `router.push('/agent/chat')` | Imperative routing | Event `user.navigated` → Rule resolves route → Fact `route.active` |
| `if (deps.length > 0) { checkDeps() }` | Ad-hoc validation | Constraint `plugin.deps-satisfied` checked on every engine step |

---

## Implementation Plan

### Phase 1: Praxis Registry (v0.3) — Define the application

**Issue 1: Define platform shell praxis module**
- Facts: plugin.registered, plugin.activated, plugin.rejected, route.active, nav.visible, theme.applied, settings.updated
- Events: app.booted, plugin.install.requested, user.navigated, settings.changed
- Rules: plugin registration, route resolution, navigation aggregation, settings persistence
- Constraints: dependency DAG, no duplicate plugins, active route must resolve to a registered plugin
- Contracts on every rule
- Gate: `app-ready` — all core plugins registered and activated

**Issue 2: Define agens plugin praxis module**
- Facts: cerebellum.routed, conscious.executed, subconscious.insight, memory.recalled, agent.status
- Events: message.received, procedure.triggered, tool.invoked
- Rules: cerebellum routing, context assembly, response composition, memory capture
- Constraints: conscious never receives raw memories (only cerebellum-curated context)
- Contracts on every rule

**Issue 3: Schema-driven UI generation**
- Platform shell schemas → design-dojo Svelte components
- Sidebar, command palette, plugin content area, status bar — all generated from schemas
- No raw HTML, no inline styles, no ad-hoc components
- praxis-svelte reactive bindings: facts → Svelte stores → UI updates

**Issue 4: PluresDB persistence layer**
- praxis-core PluresDB adapter wired to platform engine
- Facts persist to graph automatically
- Plugin data namespaced by plugin ID in PluresDB
- CRDT sync enabled — multi-device from day one

**Issue 5: Tauri 2 desktop shell**
- Generated from svelte-tauri-template
- Tauri commands are praxis event emitters (invoke = emit event)
- Tauri state is praxis fact state (read facts, not Rust structs)
- Window management, tray, autostart — all as praxis rules

### Phase 2: Agens Integration (v0.4) — First living plugin

**Issue 6: Agens manifest as RadixPlugin**
- Plugin manifest declares routes, nav, widgets, settings, backend crate
- Manifest is a praxis schema — validated by platform constraints
- Backend commands mapped to praxis events (Tauri invoke → event → rule → fact)

**Issue 7: Three-agent cognitive loop as praxis procedures**
- Cerebellum: PluresDB procedure for autorecall + routing
- Conscious: praxis rule that receives targeted context, executes, returns result
- Subconscious: background PluresDB procedure for deep reasoning
- All stored as praxis facts in the decision ledger

**Issue 8: Agent builds a plugin (self-bootstrap proof)**
- Agens running inside radix receives instruction: "create a hello-world plugin"
- Agent generates: manifest (praxis schema), module (facts/rules/constraints), UI (design-dojo component)
- Plugin is installed via praxis event → validated by constraints → activated
- This proves the architecture: the product builds itself

### Phase 3: Blueprint Automation (v0.5) — Scale to the org

**Issue 9: Plugin scaffold generator**
- `npx create-radix-plugin` → generates praxis module + design-dojo components + manifest
- Template enforces: no raw HTML, no imperative logic, all praxis primitives
- Completeness audit built in: warns if rules lack contracts

**Issue 10: Migration workflow for existing repos**
- GitHub Action: analyze existing Svelte repo → generate praxis module + plugin manifest PR
- Covers: FinancialAdvisor, sprint-log, netops-toolkit-app, runebook, pares-bastion

**Issue 11: Continuous development enforcement**
- Workflow creates milestone issues from ROADMAP.md
- Praxis completeness gate: PRs must maintain or improve rule coverage percentage
- SSoD gate: praxis artifact changes require derived output updates
- Applied to all non-forked repos in plures org

---

## Success Criteria

- [ ] `praxis validate` passes on pares-radix with 100% contract coverage for platform module
- [ ] Zero raw HTML in radix — every component from design-dojo, generated from schemas
- [ ] Zero imperative logic outside praxis rules — no if/else for domain decisions
- [ ] PluresDB contains all platform state as facts — no in-memory-only state
- [ ] Agens plugin loads, processes messages, and returns responses — all via praxis events/rules
- [ ] Agent creates a new plugin from inside radix (self-bootstrap)
- [ ] `praxis scan:rules` shows 0 rules without contracts

---

## What This Replaces

The previous ADR-0010 (same date, earlier version) described radix as "add Tauri, connect PluresDB, wire IPC." That was the old thinking — infrastructure-first, praxis as an afterthought. This version inverts it: praxis logic first, everything else derived. The infrastructure (Tauri, PluresDB, design-dojo) serves the logic, not the other way around.
