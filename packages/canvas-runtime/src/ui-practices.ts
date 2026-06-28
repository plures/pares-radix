/**
 * UI Practices (TS mirror of praxis/ui/ui-layout.px) — the RESOLVE-mode half of
 * the best-practice engine.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY A TS MIRROR
 * canvas-runtime must resolve layout WITHOUT a running Praxis engine (in the
 * reactive graph, in CI, in unit tests). So the resolve practices are duplicated
 * here as data. Drift between this file and ui-layout.px is prevented by
 * tests/ui-practices.sync.test.ts (C-DRIFT-001). The .px file is the
 * human-readable source of truth; this is the executable mirror.
 *
 * THE CONTRACT (mirrors the .px header)
 *  - HONESTY: `set` only names a RESPONSIVE_ATTRS attribute on a real prop.
 *  - FLAT SURFACE: `when` is flat boolean over { node, viewport } (see resolver).
 *  - DERIVED-ONLY: consumed by resolveUiTree, which writes canvas:tree:resolved.
 *  - DRIFT-GUARDED: parity test against ui-layout.px.
 *
 * Defaults live HERE as data (not as hardcoded branches in the resolver), so the
 * resolver is a generic interpreter of practices — adding a practice is a data
 * change, mirrored in the .px, guarded by the drift test.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { SchemaKind } from './ui-schema.js';

/** Where a resolve practice gets its value. */
export type PracticeSource =
  | { kind: 'responsive' } // pick from node.responsive[attribute] for active bp
  | { kind: 'default'; value: string }; // apply a named default behavior

/**
 * A single RESOLVE practice. Mirrors one `practice` block in ui-layout.px.
 *
 * Semantics:
 *  - `from: responsive`  → if the node declares `responsive[set]`, pick the value
 *     for the active breakpoint and write `props[set]`. If absent, this practice
 *     contributes nothing (a different practice's default may apply).
 *  - `default: <value>`  → when `when` holds AND the node declares no explicit
 *     `responsive[set]`, apply the named default behavior (resolver-interpreted).
 */
export interface UiPractice {
  name: string;
  kind: 'resolve';
  appliesTo: SchemaKind;
  /** Flat-boolean guard over { node, viewport }. Undefined = always applies. */
  when?: string;
  /** The single attribute this practice writes (must be in RESPONSIVE_ATTRS). */
  set: string;
  /** Source of the value. */
  source: PracticeSource;
}

/**
 * UI_PRACTICES — mirror of ui-layout.px (order-preserving).
 *
 * ⚠️  C-DRIFT-001: keep in lockstep with praxis/ui/ui-layout.px.
 *     tests/ui-practices.sync.test.ts enforces name/kind/appliesTo/set/source parity.
 */
export const UI_PRACTICES: readonly UiPractice[] = [
  {
    name: 'ui_layout_direction_responsive',
    kind: 'resolve',
    appliesTo: 'container',
    set: 'direction',
    source: { kind: 'responsive' },
  },
  {
    name: 'ui_layout_stack_below_md',
    kind: 'resolve',
    appliesTo: 'container',
    when: 'context.node.childCount > 1 && context.node.hasResponsiveDirection === false',
    set: 'direction',
    source: { kind: 'default', value: 'column-below-md' },
  },
  {
    name: 'ui_layout_gap_responsive',
    kind: 'resolve',
    appliesTo: 'container',
    set: 'gap',
    source: { kind: 'responsive' },
  },
  {
    name: 'ui_layout_padding_responsive',
    kind: 'resolve',
    appliesTo: 'container',
    set: 'padding',
    source: { kind: 'responsive' },
  },
  {
    name: 'ui_layout_columns_responsive',
    kind: 'resolve',
    appliesTo: 'container',
    set: 'columns',
    source: { kind: 'responsive' },
  },
  {
    name: 'ui_layout_hidden_responsive',
    kind: 'resolve',
    appliesTo: 'container',
    set: 'hidden',
    source: { kind: 'responsive' },
  },
  {
    name: 'ui_text_size_responsive',
    kind: 'resolve',
    appliesTo: 'text',
    set: 'size',
    source: { kind: 'responsive' },
  },
] as const;

/** Named default behaviors a practice's `default` value can request. */
export const DEFAULT_BEHAVIORS = {
  /** Below the `md` breakpoint → column; at md and up → row. */
  COLUMN_BELOW_MD: 'column-below-md',
} as const;
