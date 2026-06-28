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
  /** Scale padding/gap by the active ui:density level (compact|comfortable|spacious). */
  SCALE_BY_DENSITY: 'scale-by-density',
  /** Map a node's semantic colour token → concrete colour for the active theme mode. */
  THEME_TOKEN_COLOR: 'theme-token-color',
} as const;

// ─────────────────────────────────────────────────────────────────────────────
// DENSITY PRACTICES (mirror of praxis/ui/ui-density.px)
//
// Parallel to UI_PRACTICES so the ui-layout.px drift test stays untouched. These
// resolve on the ui:density trigger fact and write the real Box props padding/gap
// (both also in RESPONSIVE_ATTRS), as a DEFAULT only — explicit responsive.padding
// / responsive.gap (resolved by UI_PRACTICES) wins.
//
// ⚠️  C-DRIFT-001: keep in lockstep with praxis/ui/ui-density.px
//     (tests/ui-density.sync.test.ts).
// ─────────────────────────────────────────────────────────────────────────────

/** Density level — the ui:density trigger fact. */
export type DensityLevel = 'compact' | 'comfortable' | 'spacious';

/** Default density when ui:density is present but the level is missing/unknown. */
export const DEFAULT_DENSITY_LEVEL: DensityLevel = 'comfortable';

/**
 * The concrete density → spacing table the 'scale-by-density' behavior resolves.
 * Mirrors the table documented in ui-density.px. Values are concrete CSS strings.
 */
export const DENSITY_SCALE: Readonly<Record<DensityLevel, { padding: string; gap: string }>> = {
  compact: { padding: '4px', gap: '4px' },
  comfortable: { padding: '8px', gap: '8px' },
  spacious: { padding: '16px', gap: '12px' },
} as const;

/**
 * UI_DENSITY_PRACTICES — mirror of ui-density.px (order-preserving).
 */
export const UI_DENSITY_PRACTICES: readonly UiPractice[] = [
  {
    name: 'ui_density_padding_scale',
    kind: 'resolve',
    appliesTo: 'container',
    when: 'context.node.hasResponsivePadding === false',
    set: 'padding',
    source: { kind: 'default', value: 'scale-by-density' },
  },
  {
    name: 'ui_density_gap_scale',
    kind: 'resolve',
    appliesTo: 'container',
    when: 'context.node.hasResponsiveGap === false',
    set: 'gap',
    source: { kind: 'default', value: 'scale-by-density' },
  },
] as const;

// ─────────────────────────────────────────────────────────────────────────────
// THEME PRACTICES (mirror of praxis/ui/ui-theme.px)
//
// Resolve on the ui:theme trigger fact. v1 maps a semantic colour TOKEN on a text
// node → the concrete colour for the active mode, written to the real Text `color`
// prop. `color` is a THEMEABLE attribute (varies by mode), NOT a RESPONSIVE_ATTRS
// attribute — the resolver allow-lists it separately (THEMEABLE_ATTR_SET).
//
// HONESTY: no container `background` practice — no registered container exposes a
// `background` prop, so it is left absent (documented follow-on), not faked.
//
// ⚠️  C-DRIFT-001: keep in lockstep with praxis/ui/ui-theme.px
//     (tests/ui-theme.sync.test.ts).
// ─────────────────────────────────────────────────────────────────────────────

/** Theme mode — part of the ui:theme trigger fact. */
export type ThemeMode = 'light' | 'dark';

/** Default theme mode when ui:theme is present but the mode is missing/unknown. */
export const DEFAULT_THEME_MODE: ThemeMode = 'light';

/** A single token's colour for each theme mode. */
export interface ThemeTokenColors {
  light: string;
  dark: string;
}

/**
 * The built-in semantic colour palette the 'theme-token-color' behavior resolves.
 * Mirrors the table documented in ui-theme.px. facts.theme.tokens entries
 * override/extend this per token name. Pairs clear WCAG AA body-text contrast
 * against the corresponding mode surface (checkable via ui-contrast.ts).
 */
export const THEME_TOKENS: Readonly<Record<string, ThemeTokenColors>> = {
  fg: { light: '#111111', dark: '#f5f5f5' },
  muted: { light: '#555555', dark: '#a0a0a0' },
  accent: { light: '#1d4ed8', dark: '#60a5fa' },
  danger: { light: '#b91c1c', dark: '#f87171' },
} as const;

/**
 * Attributes a THEME practice may write. Parallel to RESPONSIVE_ATTR_SET: the
 * resolver enforces this allow-list at runtime so a theme practice can only ever
 * set a real, theme-varying prop. `color` is the only one today (Text.color).
 */
export const THEMEABLE_ATTRS: readonly string[] = ['color'] as const;
export const THEMEABLE_ATTR_SET: ReadonlySet<string> = new Set(THEMEABLE_ATTRS);

/**
 * UI_THEME_PRACTICES — mirror of ui-theme.px (order-preserving).
 */
export const UI_THEME_PRACTICES: readonly UiPractice[] = [
  {
    name: 'ui_theme_text_color',
    kind: 'resolve',
    appliesTo: 'text',
    when: 'context.node.hasThemeToken === true && context.node.hasExplicitColor === false',
    set: 'color',
    source: { kind: 'default', value: 'theme-token-color' },
  },
] as const;
