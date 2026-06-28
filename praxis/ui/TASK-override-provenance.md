# Task: Stage 1 — override provenance + practice rationale (the foundation for guidance-on-override)

Architect: mswork. Base: my local HEAD 584e8a3 (two feature commits edfce8f+1ef03ed are on
origin; 584e8a3 doc commit is local-only pending a push that's blocked by an unrelated src-tauri
scaffold in the tree — IGNORE that scaffold, do NOT stage/commit/touch Cargo*/src-tauri/ROADMAP.md).

## Strategic frame (why this exists)
Objective A+B (kbristol 2026-06-27, see praxis/ui/DESIGN-ui-schema-engine.md §1a): best practices
resolve CORRECT layout/density/theme BY DEFAULT (A). Overrides are first-class; when an author
overrides a resolved default, surface the rule's RATIONALE at authoring time (B) — inline hint for
humans, structured note for the AI composer. This Stage 1 builds the ONLY new primitive both
presenters need: **override provenance + a rationale string per practice.** It is a PURE engine
change, no UI, no IO. Stages 2 (AI/MCP presenter) and 3 (human/Dojo presenter) consume it later.

## The precise shape of an "override" (from the resolver, already read)
In ui-resolve.ts:resolveNode, a *default-kind* practice is guarded by a `when` that FAILS when the
author supplied an explicit value. The 4 real override points:
- direction: default `ui_layout_stack_below_md` guarded `...hasResponsiveDirection === false` → an
  explicit responsive.direction suppresses it.
- padding: default `ui_density_padding_scale` guarded `hasResponsivePadding === false` → explicit
  responsive.padding suppresses it.
- gap: default `ui_density_gap_scale` guarded `hasResponsiveGap === false` → explicit responsive.gap
  suppresses it.
- color: default `ui_theme_text_color` guarded `hasThemeToken === true && hasExplicitColor === false`
  → an explicit props.color suppresses it.
The flags (hasResponsiveDirection/Padding/Gap, hasThemeToken, hasExplicitColor) are already computed
in resolveNode's NodeEvalContext (ui-resolve.ts ~209-225). Provenance today: NONE — applyPractice
writes bare props[attr]=value with no record of why or that an explicit value won.

## Deliverables (all REAL — C-NOSTUB-001; pure, fully unit-tested)

### D1. Add `rationale` to UiPractice (lift existing prose into data, drift-guarded)
- In ui-practices.ts, extend the `UiPractice` interface with `rationale: string` (REQUIRED). Populate
  it for EVERY practice in UI_PRACTICES, UI_DENSITY_PRACTICES, UI_THEME_PRACTICES using the prose that
  already exists in the .px comments / doc-comments (the scout quoted them). Keep each rationale ONE
  sentence, author-facing, explaining WHY the default exists and that an explicit value overrides it.
  Example for ui_density_gap_scale: "Gap scales with the active display density so spacing stays
  consistent across compact/comfortable/spacious; set an explicit responsive.gap to override."
  Example for ui_theme_text_color: "Text color is derived from your semantic theme token so it stays
  legible and consistent in light/dark; an explicit color prop overrides this."
- Mirror the rationale into the .px sources (praxis/ui/ui-layout.px, ui-density.px, ui-theme.px) as a
  structured, parseable line per practice (e.g. a `rationale: "..."` line in each constraint block, OR
  a dedicated comment marker the existing drift parser can read). EXTEND the existing drift tests
  (tests/ui-practices.sync.test.ts and the density/theme sync tests) to assert rationale parity
  (name→rationale) between .px and TS. C-DRIFT-001: the .px stays the human source of truth.
- READ the existing sync tests FIRST to match how they parse the .px (do not break their current
  name/kind/appliesTo/set/source assertions; ADD rationale to them).

### D2. Pure override detector: detectOverrides(tree, facts) → OverrideNotice[]
- New file packages/canvas-runtime/src/ui-overrides.ts. Export:
  - `interface OverrideNotice { nodeId: string; nodeType: string; attr: string; practiceName: string;
     rationale: string; defaultValue: unknown; explicitValue: unknown; }`
  - `function detectOverrides(root: CanvasNodeLike, facts: UiRuntimeFacts): OverrideNotice[]`
- It walks the tree (same recursion as resolveRecursive) and, for each node, determines for each of the
  4 override points whether the author supplied an explicit value that SUPPRESSED a default practice.
  For each genuine override it computes BOTH:
    - explicitValue = what the author's explicit responsive/color resolves to at the active
      breakpoint/mode (reuse pickResponsive / the same resolution the resolver would do).
    - defaultValue = what the DEFAULT practice WOULD have produced for this node at the active facts
      (column-below-md result; DENSITY_SCALE[level].gap/padding; themeColorFor(token,mode)).
  - **Only emit a notice when explicitValue !== defaultValue** (a MEANINGFUL deviation). If the author's
    explicit value equals what the default would have produced, that's not a real override → no notice
    (keeps guidance signal-high, non-annoying — directly serves objective B "guidance only on deviation").
  - Honesty: if the relevant trigger fact is absent (no density for padding/gap; no theme for color; no
    viewport for direction), the default isn't determinable → do NOT emit a notice for that attr (can't
    claim an override against an unknown default). Document this.
  - nodeId: use node.id if present, else a stable path like "root/children/2" — say which you chose.
- detectOverrides MUST NOT mutate the tree (pure; deep-read only). It does NOT depend on resolveUiTree
  having run. Factor shared helpers (the flag computation, default-value computation) so detector and
  resolver agree — but do NOT change resolveUiTree's existing behavior or signature.

### D3. Reuse, don't duplicate logic
- The default-value computations (column-below-md, density scale, theme color) already exist inside
  applyPractice in ui-resolve.ts. Extract the minimal pure helpers (e.g. defaultDirectionFor(bp),
  densityValueFor(attr,facts), themeColorFor already exists) into a shared spot (top of ui-resolve.ts
  or a tiny ui-resolve-helpers.ts) and use them in BOTH applyPractice and detectOverrides so they can
  never drift. Keep applyPractice's outward behavior identical (resolver tests must stay green).

### D4. Export
- Add to packages/canvas-runtime/src/index.ts (ADD-ONLY): `export { detectOverrides } from
  './ui-overrides.js'; export type { OverrideNotice } from './ui-overrides.js';`

### D5. Tests (test-first; new file)
- New file packages/canvas-runtime/tests/ui-overrides.test.ts. Cover, with the standard beforeAll
  registerComponent stubs (Box=layout, Text=content/text, Heading=content/text):
  - explicit responsive.gap that DIFFERS from the density default at the active density → 1 notice with
    correct practiceName (ui_density_gap_scale), rationale, defaultValue, explicitValue.
  - explicit responsive.gap that EQUALS the density default → NO notice (meaningful-deviation gate).
  - explicit responsive.direction differing from column-below-md (e.g. author forces 'row' at base) →
    notice; equal → none.
  - explicit props.color differing from the token's theme color → notice; equal → none.
  - no theme fact → color override NOT reported (honest-absent); no density fact → gap/padding not
    reported; no viewport → direction not reported.
  - nested children: a deviating child deep in the tree is detected with the right nodeId/path.
  - pure: detectOverrides does not mutate the input tree (deep-equal before/after).

## GATE (must pass before reporting done)
- packages/canvas-runtime: ..\..\node_modules\.bin\vitest run  (FULL suite green; report counts; baseline 183)
- repo root C:\Projects\pares-radix: ..\..\Projects\pares-radix\node_modules\.bin\svelte-check --tsconfig .\tsconfig.json --threshold error  (report errors; 1 pre-existing design-dojo Heading warning is fine; fix any NEW errors)
Keep pwsh SHORT, output BOUNDED (Select-Object -Last 12). Do NOT commit/push (architect integrates).
Do NOT modify resolveUiTree's signature/behavior, CanvasRenderer.svelte, the contrast linter, or the
app src/ wiring. Do NOT touch Cargo*/src-tauri/ROADMAP.md (unrelated concurrent scaffold).

## Report back
Files created/modified; the rationale strings you wrote per practice; how detectOverrides computes
default vs explicit and where you put the shared helpers; the nodeId scheme; both gate results
(vitest counts + svelte-check); and confirm the meaningful-deviation gate + honest-absent (no-fact)
behavior are tested.