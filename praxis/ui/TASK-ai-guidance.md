# Task: Stage 2 — surface override guidance to the AI composer (MCP authoring seam)

Architect: mswork. Base: origin/main @ c796ec7 (Stage 1 shipped: detectOverrides + OverrideNotice +
rationale on every practice are live in @plures/canvas-runtime). Tree is CLEAN. Do NOT touch
Cargo*/src-tauri/ROADMAP.md or the untracked ADR/TASK scratch files.

## Strategic frame (objective B, second presenter)
Stage 1 built the pure primitive: `detectOverrides(root, facts) -> OverrideNotice[]` (each notice =
{nodeId,nodeType,attr,practiceName,rationale,defaultValue,explicitValue}), firing ONLY on a meaningful
deviation (explicit value differs from what the default practice would produce), honest-absent when a
trigger fact is missing. Stage 2 delivers those notices to the **AI composer** — the programmatic
author — so when the model sets an explicit value that overrides a best-practice default, it gets the
rule's rationale back as structured feedback (NOT an error; the tree is still applied). This is the
"guidance fires on override, at authoring time" objective for the machine author. (Stage 3, separate,
does the human-in-Dojo inline hint.)

## The seam (already located)
`packages/mcp-dev-server/src/index.ts`:
- `canvas.addNode` handler (~line 240): `activeCanvas = toolCanvasAddNode(...); dbPut('canvas:_active',
  activeCanvas); return { ok: true, tree: activeCanvas.tree };`
- `canvas.setTree` handler (~line 300): same shape, `return { ok: true, tree: activeCanvas.tree };`
These are the two authoring entrypoints where a node tree / props get set by the composer. They return
no engine feedback today. That's where `guidance` goes.

## Honest facts at authoring time (the design crux — implement EXACTLY this)
The composer builds a tree abstractly; there is NO live viewport/theme/density. To get TRUTHFUL
override notices we evaluate against the engine's **canonical default facts** — the same defaults the
resolver falls back to — so we're telling the author what the default-correct path WOULD produce under
the standard baseline, and which of their explicit values diverge from it:
- density: `DEFAULT_DENSITY_LEVEL` ('comfortable') — already exported.
- theme:   `DEFAULT_THEME_MODE` ('light') — already exported.
- viewport: a canonical authoring breakpoint. Use the `md` breakpoint as the reference baseline
  (desktop-first design baseline). Construct a ViewportFact whose width maps to `md` (reuse the
  schema's breakpoint width table / `breakpointFor`; pick a width that yields 'md', e.g. the md min
  width). If a `DEFAULT_BREAKPOINT`-style helper does not exist, derive the md width from ui-schema's
  breakpoint definitions — do NOT hardcode a magic number without sourcing it from the schema.
Build `const AUTHORING_FACTS: UiRuntimeFacts = { viewport: <md baseline>, density: {level:
DEFAULT_DENSITY_LEVEL}, theme: {mode: DEFAULT_THEME_MODE} }` — match the EXACT shapes of ViewportFact /
the density fact / the theme fact as defined in ui-resolve.ts (read them; density/theme fact shapes
matter). Put this constant + a small `guidanceForTree(tree)` wrapper in a NEW tiny module
`packages/mcp-dev-server/src/canvas-guidance.ts` (keep index.ts edits minimal). guidanceForTree calls
`detectOverrides(tree, AUTHORING_FACTS)` and returns the OverrideNotice[].
- DO NOT modify detectOverrides or anything in canvas-runtime. Stage 1 is frozen. You only CONSUME it.

## Deliverables (all REAL — C-NOSTUB-001)

### D1. canvas-guidance.ts (new, mcp-dev-server)
- Export `AUTHORING_FACTS` and `guidanceForTree(tree: CanvasNode): OverrideNotice[]` (import
  detectOverrides + OverrideNotice + the DEFAULT_* + UiRuntimeFacts/ViewportFact types +
  breakpoint/width helper from '@plures/canvas-runtime'). Verify those are all exported from the
  package entry (Stage 1 added detectOverrides/OverrideNotice; DEFAULT_DENSITY_LEVEL/DEFAULT_THEME_MODE
  and UiRuntimeFacts/ViewportFact are already exported — confirm by reading canvas-runtime/src/index.ts).
- One small doc-comment explaining the canonical-facts rationale (so a future reader knows WHY md/
  comfortable/light, and that notices are "vs the default baseline").

### D2. Wire guidance into the two handlers (index.ts, minimal additive edits)
- `canvas.addNode`: after the dbPut, compute `const guidance = guidanceForTree(activeCanvas.tree);`
  and return `{ ok: true, tree: activeCanvas.tree, ...(guidance.length ? { guidance } : {}) }`.
- `canvas.setTree`: same — append `guidance` to the existing return when non-empty.
- Only attach `guidance` when there's at least one notice (don't bloat the common no-override return).
- Update each tool's `description` string to mention it returns best-practice override guidance when an
  explicit value overrides a resolved default (so the model knows to read it). One short clause each.
- Do NOT change inputSchema, the mutation logic, dbPut, or any other handler.

### D3. Tests (test-first; new file in mcp-dev-server's test setup)
- Find how mcp-dev-server tests run (look for existing *.test.ts in packages/mcp-dev-server, e.g.
  simple-eval.test.ts, and its vitest config / package.json test script). Match that harness.
- New `packages/mcp-dev-server/src/canvas-guidance.test.ts` (or /tests, matching the existing
  convention). Cover:
  - guidanceForTree on a tree where a node sets explicit props.color overriding a themeToken (differing
    from the light-mode token color) → returns 1 notice with the theme rationale.
  - guidanceForTree on a tree with an explicit responsive.gap that differs from the comfortable-density
    default gap → 1 notice with the density-gap rationale.
  - guidanceForTree on a tree with NO overrides (all defaults / explicit==default) → returns [].
  - AUTHORING_FACTS has all three fact families present and well-typed (so honest-absent never silently
    swallows an authoring override).
- If wiring the two handlers is cleanly unit-testable in the existing harness (the handlers reference a
  module-level `activeCanvas`), add a handler-level test that addNode of an overriding node yields a
  result object carrying `guidance`. If the handler harness is awkward to instantiate, it's acceptable
  to test `guidanceForTree` thoroughly + assert the handler returns guidance via a thin exported helper
  — but DO NOT fake it; if you can exercise the real handler, do.

## GATE (must pass before reporting done)
- mcp-dev-server tests: run them the SAME way the package already does (report the command you used +
  exact counts; everything green).
- canvas-runtime full suite still green (you changed nothing there, but confirm no accidental break):
  from packages/canvas-runtime: ..\..\node_modules\.bin\vitest run (expect 207).
- repo root svelte-check: ..\..\Projects\pares-radix\node_modules\.bin\svelte-check --tsconfig
  .\tsconfig.json --threshold error (0 errors; the 1 pre-existing design-dojo Heading warning is fine).
  NOTE: mcp-dev-server is a TS package — if svelte-check doesn't cover it, ALSO typecheck mcp-dev-server
  the way that package does (tsc --noEmit / its build script) and report it.
Keep pwsh SHORT, output BOUNDED (Select-Object -Last 12). Do NOT commit or push. Do NOT touch
canvas-runtime source, CanvasRenderer.svelte, app src/ wiring, or Cargo*/src-tauri/ROADMAP.md.

## Report back
Files created/modified; the exact ViewportFact/density/theme fact shapes you used for AUTHORING_FACTS +
how you sourced the md width from the schema (no magic numbers); the guidance return shape; whether you
tested the real handler or the wrapper + why; and all gate results (every command + counts).