# Task: UI engine — Thread 1 (theme/density practices) + Thread 2 (renderer integration)

Architect: mswork. Base commit: c8931ac (UI schema+engine, 98/98 green).
Spec: praxis/ui/DESIGN-ui-schema-engine.md §11 (follow-on) + §10 (integration handoff).

Run BOTH in parallel — disjoint file sets, no conflicts.

## Shared invariants (both threads)
- HONESTY: a rule/attribute only references a prop a registered component exposes
  (verify vs packages/canvas-runtime/src/registry.ts).
- FLAT EVALUATOR surface; resolve RHS = pre-flattened lookups, no functions/tree-walk.
- SOURCE vs DERIVED: authored tree pristine; resolved output is derived.
- C-DRIFT-001: every .px ↔ TS mirror gets a parity/drift test.
- NO STUBS (C-NOSTUB-001): real impls only; absent-not-faked if unbuilt.
- TEST-FIRST gate per thread: vitest + svelte-check (root tsconfig) green before done.
- Bound exec output; short pwsh commands (TOOLS.md). Run vitest from packages/canvas-runtime
  via ..\..\node_modules\.bin\vitest run <file>. svelte-check from repo root:
  ..\..\Projects\pares-radix\node_modules\.bin\svelte-check --tsconfig .\tsconfig.json --threshold error

────────────────────────────────────────────────────────────────────────────────
## THREAD 1 — theme/contrast + density resolve practices (engine follow-on)

Goal: add two new RESOLVE practice sets on the EXISTING engine. The resolver already
plumbs facts.theme and facts.density (ui-resolve.ts UiRuntimeFacts) but ignores them.
Extend the resolver interpreter + practices data + .px source + drift tests + unit tests.

### Density (simpler — do first)
- Trigger fact: ui:density → { level: 'compact'|'comfortable'|'spacious' }.
- New practice(s) in a NEW file praxis/ui/ui-density.px + mirror in ui-practices.ts
  (extend UI_PRACTICES, OR add a parallel UI_DENSITY_PRACTICES array — your call, but
  keep one drift test per .px). Practices (resolve mode, appliesTo container/control):
    * density scales padding + gap on containers (compact→tight, spacious→loose).
  Use a NAMED default behavior (like COLUMN_BELOW_MD) e.g. 'scale-by-density', resolved
  in applyPractice via a density→multiplier/þvalue table. Keep values concrete + documented.
- Resolver: thread facts.density into NodeEvalContext; add the density branch in
  applyPractice. Explicit responsive.padding/gap still WINS over density default
  (responsive practices run and set props; density default only applies when no explicit
  value — mirror the stack-below-md precedence pattern).

### Theme / contrast (the WCAG one)
- Trigger fact: ui:theme → { mode:'light'|'dark', tokens?: Record<string,string> }.
- Practices in praxis/ui/ui-theme.px + mirror. appliesTo: text (color) and container
  (background). resolve mode: when a node has responsive/themed color tokens, pick by
  theme mode; ALSO a validate-style contrast guard belongs in the VALIDATE half — but for
  v1 resolve, focus on: map a token name → concrete color for the active theme mode.
- Contrast MATH: add a pure helper (e.g. ui-contrast.ts) computing WCAG relative luminance
  + contrast ratio between two hex colors. Unit-test the known pairs (#000/#fff = 21:1,
  same color = 1:1, a mid pair). This is real, testable, no stub.
- HONESTY: only set `color` (Text) / `background` (container-capable). Do NOT invent props.
  If background isn't a real prop on any container component, set it via the `class`/style
  path that exists, OR limit theme resolve to `color` on text + document background as
  follow-on. Verify against registry before writing the practice.

### Thread 1 deliverables
- praxis/ui/ui-density.px, praxis/ui/ui-theme.px
- ui-practices.ts extended (+ DEFAULT_BEHAVIORS entries), ui-resolve.ts density+theme branches
- ui-contrast.ts (+ tests/ui-contrast.test.ts)
- tests: extend ui-resolve.test.ts (density collapse, theme color pick, precedence) +
  ui-density.sync.test.ts + ui-theme.sync.test.ts drift guards
- index.ts exports for any new public API (contrast helper, new types)
- GATE: full vitest + svelte-check green; report counts.

────────────────────────────────────────────────────────────────────────────────
## THREAD 2 — renderer-side responsive integration (close the loop)

Goal: make CanvasRenderer.svelte render the RESOLVED tree and re-render on viewport
change, so every existing usage becomes responsive with ZERO caller changes.
Decision (locked): renderer-side (option B), because the renderer already owns
dbGet/dbSubscribe and the design is "renderer reacts to data".

### Implementation (packages/canvas-runtime/src/CanvasRenderer.svelte)
- Add an internal reactive viewport read: subscribe to 'ui:viewport' via the existing
  dbSubscribe prop (NOTE: renderer prefixes keys with `prefix`='canvas:' — ui:viewport is
  NOT under canvas:, so subscribe to the RAW 'ui:viewport' key, bypassing the prefix; add
  a small dedicated subscription, do not route it through collectBindingKeys).
- Derive the rendered tree: const rendered = resolveUiTree(document.tree, { viewport })
  whenever document or viewport changes ($derived/$state in Svelte 5 runes). Render
  `rendered` instead of `document.tree` in the final {@render renderNode(...)} and in the
  $effect that collects binding keys (collect from the resolved tree).
- If no ui:viewport present yet → resolveUiTree returns identity clone → unchanged behavior.
- Honor `hidden` resolved attribute: a node whose resolved props.hidden === true must not
  render. Extend isVisible(node) to also return false when node.props?.hidden === true
  (after resolution the prop is concrete). Keep existing visible-condition logic.
- Do NOT mutate `document`. resolveUiTree already clones.

### Thread 2 tests
- The renderer is Svelte; full DOM render tests may be heavy. Minimum honest coverage:
  add tests/canvas-renderer-responsive.test.ts that tests the EXTRACTED logic — i.e.
  factor the "resolve + decide hidden" into a tiny pure helper if needed, OR assert via
  resolveUiTree that a tree with responsive.hidden/direction yields the expected props at
  given widths (this already exists in ui-resolve.test.ts; add a renderer-focused case:
  hidden=true at base hides). If a Svelte component test harness exists in the repo
  (check tests/ for any .svelte render test), use it; otherwise do not fabricate one —
  cover the logic at the function level and NOTE the DOM test as a follow-on honestly.
- Verify no regression in existing CanvasRenderer consumers compiles (svelte-check).

### Thread 2 deliverables
- CanvasRenderer.svelte responsive integration (resolved tree + hidden + viewport sub)
- tests covering resolved-render logic + hidden behavior
- GATE: full vitest + svelte-check green; report counts + confirm both consumers
  (CanvasView.svelte, routes/canvas/+page.svelte) still typecheck.

────────────────────────────────────────────────────────────────────────────────
## After both threads report green
Architect (mswork) will: re-run full gate, update DESIGN doc §10/§11 status, commit +
push as one changeset, and report to kbristol.
