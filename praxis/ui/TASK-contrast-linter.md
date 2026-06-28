# Task: validate-mode contrast linter (UI engine, validate half)

Architect: mswork. Base: 212a3dd (theme/density/contrast-math + renderer integration shipped).
Strategic objective: honestly enforce UI best practices over the schema. This adds the
WCAG-AA contrast CONSTRAINT to the VALIDATE half, consuming the contrast MATH (ui-contrast.ts)
Thread 1 already shipped.

## The honesty problem this solves (decided by architect, not optional)
A contrast ratio needs foreground AND background. Today:
- foreground: a text node's themeToken → THEME_TOKENS[token][mode] (concrete color). Real.
- background: NO container exposes a `background` prop, and THEME_TOKENS has no surface/bg.
  => there is currently NO honest background to check against.

DECISION (architect): add a canonical per-mode SURFACE (background) color to the theme palette
as a declared theme constant — NOT a faked prop, NOT an invented per-node attribute. This is a
real theme value (every theme defines its base surface). The linter checks each USED text token
against the active mode's surface. This is honest, buildable, testable.

## Scope (validate half only — do NOT touch resolve precedence)
1. **Theme surface constant** — in ui-practices.ts add:
   `export const THEME_SURFACE: Readonly<Record<ThemeMode,{background:string}>> =
     { light:{background:'#ffffff'}, dark:{background:'#0b0b0b'} }`  (concrete, documented).
   Mirror this in ui-theme.px (it's theme data; extend the .px table + its drift test so
   ui-theme.sync stays green — add the surface table parse + assert).
   Verify the BUILT-IN token pairs actually pass AA against these surfaces using ui-contrast.ts;
   if any built-in token FAILS AA against its mode surface, that's a real bug — fix the token
   color (document why), do not ship a palette that violates its own linter.

2. **Facts** — extend ui-facts.ts UiFacts + extractUiFacts:
   - This rule is theme-aware, but extractUiFacts currently has no theme mode input. Keep the
     extractor pure/flat: add fact `lowContrastTextCount: number` and `contrastChecked: boolean`.
   - HOW: the contrast check needs the active theme mode. extractUiFacts signature today is
     (root). Add an OPTIONAL second arg `opts?: { themeMode?: ThemeMode }`. When themeMode is
     provided, for every text node that has a themeToken (or explicit hex color prop), compute
     contrast vs THEME_SURFACE[mode].background and count those below AA (4.5) into
     lowContrastTextCount; set contrastChecked=true. When themeMode is absent, contrastChecked
     =false and lowContrastTextCount=0 (rule inert — honest: we can't know the surface).
   - Only count tokens/colors that are REAL: themeToken present in THEME_TOKENS (or facts-time
     tokens) OR props.color is a hex string. Do not guess.
   - Preserve the existing (root)-only call sites: second arg optional, default {}.

3. **Constraint** — add to praxis/ui/ui-best-practices.px + ui-constraints.ts mirror:
   `ui_text_contrast_aa`: severity ERROR (WCAG AA is correctness), phase ui,
   when: context.ui.contrastChecked === true,
   require: context.ui.lowContrastTextCount === 0,
   message names the count. Keep the drift guard (ui-constraints.sync) green — add the rule to
   BOTH files identically.
   HONESTY: this constraint reads ONLY facts extractUiFacts emits (contrastChecked,
   lowContrastTextCount) — both added in step 2.

4. **validateUi / validateCanvas plumbing** — validateUi(root) currently calls
   extractUiFacts(root). Add an optional themeMode passthrough: validateUi(root, opts?) →
   extractUiFacts(root, opts). Thread it from validateCanvas IF a theme mode is available there;
   if not available at that layer, leave validateUi callable with the mode and have validateCanvas
   pass nothing (rule stays inert in the string[] path until a caller supplies mode) — document
   this honestly. Do NOT fabricate a mode.

## Tests (test-first)
- tests/ui-contrast.test.ts already covers the math; ADD: assert every THEME_TOKENS entry passes
  AA (>=4.5) vs THEME_SURFACE for BOTH modes (this is the "palette obeys its own linter" guard).
- tests/ui-facts.test.ts: add cases — text node with a known-bad hex color (e.g. #cccccc on light
  #ffffff ~1.6) + themeMode:'light' → lowContrastTextCount>=1, contrastChecked true; same tree
  with NO themeMode → contrastChecked false, count 0; a good token (fg) → count 0.
- tests/ui-constraints.test.ts: ui_text_contrast_aa fires (violation) on the bad tree+mode,
  passes on the good tree, and is ABSENT/inert when contrastChecked false.
- ui-constraints.sync + ui-theme.sync drift guards stay green.

## GATE (must pass before reporting done)
- packages/canvas-runtime: ..\..\node_modules\.bin\vitest run  (FULL suite green; report counts)
- repo root: ..\..\Projects\pares-radix\node_modules\.bin\svelte-check --tsconfig .\tsconfig.json --threshold error (0 errors)
Short pwsh, bounded output. Do NOT commit/push. Do NOT touch CanvasRenderer.svelte or resolve
precedence. Report files changed, counts, and anything left honestly absent.
