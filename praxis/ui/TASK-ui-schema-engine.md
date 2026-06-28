# Task: UI Schema & Reactive Best-Practice Engine — v1 build

Architect: mswork (main session). Workers: staged subagents.
Spec: `praxis/ui/DESIGN-ui-schema-engine.md` (authoritative).
Decisions locked (kbristol 2026-06-27): derived tree · vertical slice (layout first) ·
add `hidden` attribute · infer schemaKind from category with explicit override.

## Invariants every stage must honor
- HONESTY: a rule/attribute may only reference a prop a registered component actually
  exposes (verify against `packages/canvas-runtime/src/registry.ts`).
- FLAT EVALUATOR: author-space stays flat boolean/arithmetic (simpleEval contract).
  `resolve` RHS is a pre-flattened breakpoint table lookup, no functions/tree-walk.
- SOURCE vs DERIVED: authored `canvas:tree` stays pristine; resolver writes
  `canvas:tree:resolved`. Never mutate authored intent.
- C-DRIFT-001: any `.px` ↔ TS mirror gets a drift-guard test (like ui-constraints.sync).
- NO STUBS (C-NOSTUB-001): real impls only; if not built, leave absent + say so.
- TEST-FIRST: each stage ships its own tests; verification gate = vitest + tsc green
  for `packages/canvas-runtime` before the dependent stage starts.

## Stages & gates

### Stage 1 — Schema (`ui-schema.ts`) [GATE: tsc + unit test green]
- New `packages/canvas-runtime/src/ui-schema.ts`:
  - `SchemaKind` union: container|text|control|media|navigation|group|feedback.
  - `RESPONSIVE_ATTRS`: the set of attributes that resolve per breakpoint
    (direction, padding, gap, align, justify, wrap, columns, hidden, size, maxLines).
  - `BREAKPOINTS`: ordered ladder base/sm640/md768/lg1024/xl1280 (+ helper
    `breakpointFor(width): name` and `pickResponsive(map, bp): value` — flat table pick).
  - `kindForComponent(id, category?)`: infer kind from category, allow explicit override
    via optional `schemaKind` on ComponentMeta. Default map documented inline.
  - Add OPTIONAL `schemaKind?: SchemaKind` to `ComponentMeta` in registry.ts (non-breaking).
- Tests `tests/ui-schema.test.ts`: breakpointFor boundaries (639→base,640→sm,767→sm,
  768→md,1024→lg,1280→xl), pickResponsive fallback (missing bp falls back to nearest
  smaller defined, base ultimate fallback), kindForComponent for every registered id.

### Stage 2 — Resolver (`ui-resolve.ts`) [depends S1; GATE: tsc + unit test green]
- `responsive?: Record<string, Record<string,unknown>>` added to CanvasNode (format.ts),
  optional, non-breaking. Document: keys are attribute names, values are breakpoint maps.
- New `packages/canvas-runtime/src/ui-resolve.ts`:
  - `UiFactsRuntime = { viewport?: {width,height,breakpoint}, theme?, density? }`.
  - `resolveUiTree(root, facts)`: PURE. Deep-clones tree; for each node, for each
    `responsive[attr]`, picks value for active breakpoint via pickResponsive and writes
    `props[attr]`. Type-based defaults: a `container` with >1 child & no explicit
    `responsive.direction` gets `direction: column` below md. Returns NEW tree (authored
    tree untouched — assert via deep-equal of input before/after).
  - Never throws on missing facts: no viewport ⇒ returns tree unchanged (identity).
- Tests `tests/ui-resolve.test.ts`: identity when no facts; responsive.direction collapses
  base→column / md→row at widths 500 vs 900; authored tree NOT mutated (deep-equal guard);
  default container-stacking below md when no explicit intent; nested nodes resolved.

### Stage 3 — Resolve practices (`.px` + mirror) [depends S1; GATE: tsc + tests + DRIFT green]
- `praxis/ui/ui-layout.px`: `practice` blocks in resolve mode (new grammar in spec §4).
  At minimum: stack-below-md, gap-shrink-below-md, hide-by-breakpoint. Header documents
  the resolve grammar + honesty contract.
- TS mirror `packages/canvas-runtime/src/ui-practices.ts`: `UI_PRACTICES` array mirroring
  the .px (kind/appliesTo/when/set or require/severity/message), + `applyPractices` that
  the resolver consumes (so defaults live as data, not hardcoded branches).
- Drift guard `tests/ui-practices.sync.test.ts`: parse ui-layout.px, assert count + field
  parity with UI_PRACTICES (mirror ui-constraints.sync.test.ts pattern).
- NOTE: keep validate-half (ui-constraints) untouched; this adds the resolve-half.

### Stage 4 — Bridge + reactive wiring [depends S2,S3; GATE: tsc + integration test green]
- `packages/canvas-runtime/src/ui-viewport-bridge.ts`: edge listener factory
  `attachViewportBridge(graph, win=globalThis)` → on resize/matchMedia writes
  `ui:viewport {width,height,breakpoint}`; returns detach fn. Guard for non-DOM (SSR/test):
  if no window, no-op + return noop detach. THIS IS THE ONLY IO BOUNDARY.
- Reactive wiring `packages/canvas-runtime/src/ui-reactive.ts`:
  `wireResolvedTree(graph, {authoredKey='canvas:tree', resolvedKey='canvas:tree:resolved'})`
  → subscribePrefix('ui:', …) + subscribe(authoredKey,…); on any change, read authored +
  facts, run resolveUiTree, put resolvedKey. Returns detach. Reuses reactive-graph.ts.
- Tests `tests/ui-reactive.test.ts` (uses createReactiveGraph + an in-memory base): put
  authored tree + put ui:viewport(500) ⇒ resolvedKey has column; put ui:viewport(900) ⇒
  resolvedKey has row; authored key never changes. Bridge test: fake window with
  innerWidth + dispatch resize ⇒ ui:viewport written with correct breakpoint; no-window ⇒
  noop, no throw.

### Stage 5 — Verify + export + docs [depends S4; GATE: full suite + tsc green]
- Export new API from `index.ts` (schema, resolver, practices, bridge, reactive, types).
- Run FULL `vitest run` for canvas-runtime + the repo `check` (svelte-check) for the
  package's files; confirm zero regressions (format/canvas-plugin/reactive-graph still green).
- Update DESIGN doc §6 status: mark built items. Append a short "How to use" section.
- Report: what shipped, test counts, any deferred (honestly) items.

## Out of scope v1 (explicitly absent, not stubbed)
- theme/contrast + density resolve practices (engine supports them; practices are follow-on).
- maxLines truncation runtime (attribute reserved in schema; no resolver branch yet) —
  unless trivial; if not built, leave absent and say so.
- Unum renderer change to read `canvas:tree:resolved` (separate repo/surface; note as the
  one integration handoff).
