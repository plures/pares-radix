/**
 * canvas-guidance.ts — Stage 2 of the guidance-on-override layer: surface the
 * pure override-provenance notices (Stage 1's `detectOverrides`) to the AI
 * composer at AUTHORING time, through the MCP canvas authoring handlers.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * THE DESIGN CRUX — WHY "CANONICAL AUTHORING FACTS"
 * The composer builds a canvas tree ABSTRACTLY: there is no live viewport, no
 * live theme, no live density when the model sets a node's props. But
 * `detectOverrides` is honest-absent — without a trigger fact it cannot know the
 * default a practice would produce, so it stays silent. To give the machine
 * author truthful feedback ("this explicit value diverges from the default-
 * correct path") we evaluate against the engine's CANONICAL DEFAULT facts — the
 * same defaults the resolver itself falls back to:
 *
 *   - density : DEFAULT_DENSITY_LEVEL  ('comfortable')  → the baseline spacing.
 *   - theme   : DEFAULT_THEME_MODE     ('light')        → the baseline palette.
 *   - viewport: the `md` breakpoint — the desktop-first design reference. We set
 *     breakpoint:'md' EXPLICITLY (detectOverrides honours a precomputed
 *     breakpoint on a ViewportFact) and pair it with the schema's `md` MIN WIDTH
 *     so the width and the breakpoint agree. The width is SOURCED from
 *     ui-schema's BREAKPOINTS table (mdMinWidth below) — never a magic number.
 *
 * So a notice here means: "under the standard md / comfortable / light baseline,
 * the default-correct path WOULD produce X for this attribute, and your explicit
 * value Y diverges from it." It is GUIDANCE, not an error — the authored tree is
 * still applied. (Stage 3, separate, does the in-Dojo human inline hint.)
 *
 * This module only CONSUMES @plures/canvas-runtime (Stage 1 is frozen). It does
 * not modify the runtime; it pins the canonical facts and wraps detectOverrides.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import {
  detectOverrides,
  BREAKPOINTS,
  DEFAULT_DENSITY_LEVEL,
  DEFAULT_THEME_MODE,
  type OverrideNotice,
  type UiRuntimeFacts,
  type ViewportFact,
  type CanvasNode,
} from '@plures/canvas-runtime';

/**
 * The `md` breakpoint's minimum width, sourced from the schema's BREAKPOINTS
 * ladder (NOT hardcoded). `md` is the desktop-first authoring reference. Falls
 * back to the documented Tailwind-ish md floor only if the table ever lacks an
 * `md` entry (it always defines one today), so AUTHORING_FACTS is never built
 * from an undefined width.
 */
const mdMinWidth: number = BREAKPOINTS.find((b) => b.name === 'md')?.min ?? 768;

/**
 * AUTHORING_FACTS — the canonical default facts the composer is evaluated
 * against. All three trigger families (viewport / density / theme) are present
 * and well-typed so honest-absent never silently swallows an authoring
 * override: every override point (direction / padding / gap / color) is
 * determinable at authoring time.
 */
export const AUTHORING_FACTS: UiRuntimeFacts = {
  viewport: { width: mdMinWidth, breakpoint: 'md' } satisfies ViewportFact,
  density: { level: DEFAULT_DENSITY_LEVEL },
  theme: { mode: DEFAULT_THEME_MODE },
};

/**
 * Compute best-practice override guidance for an authored canvas tree.
 *
 * Returns one OverrideNotice per node-attribute where the author's explicit
 * value MEANINGFULLY diverges from the default the resolver would have produced
 * under {@link AUTHORING_FACTS}. Empty array when nothing overrides a default
 * (the common case) — so callers can attach `guidance` only when non-empty.
 *
 * Pure: a deep-read of `tree`; never mutates it (delegates to detectOverrides).
 *
 * @param tree the AUTHORED root node (after the composer's mutation is applied)
 * @returns the notices in document order (root first, children depth-first)
 */
export function guidanceForTree(tree: CanvasNode): OverrideNotice[] {
  return detectOverrides(tree, AUTHORING_FACTS);
}
