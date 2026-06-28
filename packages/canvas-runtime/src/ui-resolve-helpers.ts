/**
 * ui-resolve-helpers.ts — the minimal PURE helpers shared by the resolver
 * (ui-resolve.ts:applyPractice) and the override detector (ui-overrides.ts).
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY THIS FILE EXISTS
 * The override detector must compute, for a node + active facts, the SAME default
 * value the resolver would write (the column-below-md direction, the density
 * padding/gap, the theme token colour) so it can compare that default against the
 * author's explicit value and only flag a MEANINGFUL deviation. If the detector
 * re-implemented those computations they could drift from the resolver and the
 * guidance would lie. Extracting them here — and consuming them from BOTH places —
 * makes drift impossible: there is exactly one definition of each default.
 *
 * Everything here is a pure function over plain data: no tree walking, no IO, no
 * mutation. The resolver keeps its outward behaviour identical (it just calls
 * these instead of inlining the same expressions).
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { CanvasNodeLike } from './ui-facts.js';
import { breakpointFor, type Breakpoint } from './ui-schema.js';
import {
  DEFAULT_DENSITY_LEVEL,
  DEFAULT_THEME_MODE,
  DENSITY_SCALE,
  THEME_TOKENS,
  type DensityLevel,
  type ThemeMode,
  type ThemeTokenColors,
} from './ui-practices.js';

// ── Node flag computation (the override-suppression predicates) ───────────────

/**
 * The flat per-node booleans the default practices guard on. Computed once,
 * identically, for both the resolver's NodeEvalContext and the detector. These
 * are exactly the flags that decide whether a default practice is SUPPRESSED by
 * an explicit author value (the precise definition of an "override").
 */
export interface NodeFlags {
  childCount: number;
  hasResponsiveDirection: boolean;
  hasResponsivePadding: boolean;
  hasResponsiveGap: boolean;
  hasThemeToken: boolean;
  hasExplicitColor: boolean;
}

/** True when `node.responsive[attr]` is an own, present key. */
export function hasResponsiveAttr(node: CanvasNodeLike, attr: string): boolean {
  const responsive = node.responsive as Record<string, unknown> | undefined;
  return !!responsive && Object.prototype.hasOwnProperty.call(responsive, attr);
}

/** The author's explicit literal `props.color`, or undefined. */
export function explicitColorOf(node: CanvasNodeLike): unknown {
  return (node.props as Record<string, unknown> | undefined)?.color;
}

/** Compute the flat suppression flags for a node (pure, read-only). */
export function nodeFlags(node: CanvasNodeLike): NodeFlags {
  const explicitColor = explicitColorOf(node);
  return {
    childCount: Array.isArray(node.children) ? node.children.length : 0,
    hasResponsiveDirection: hasResponsiveAttr(node, 'direction'),
    hasResponsivePadding: hasResponsiveAttr(node, 'padding'),
    hasResponsiveGap: hasResponsiveAttr(node, 'gap'),
    hasThemeToken: typeof node.themeToken === 'string' && node.themeToken.length > 0,
    hasExplicitColor: typeof explicitColor === 'string' && explicitColor.length > 0,
  };
}

// ── Active-fact normalization (mirrors the resolver's private helpers) ─────────

/** Resolve the active breakpoint from a viewport fact, or null if none. */
export function activeBreakpoint(
  viewport: { width: number; breakpoint?: Breakpoint } | undefined,
): Breakpoint | null {
  if (!viewport) return null;
  return viewport.breakpoint ?? breakpointFor(viewport.width);
}

/** Normalize a possibly-missing density level to a known one (null when absent). */
export function densityLevelOf(
  density: { level: DensityLevel } | undefined,
): DensityLevel | null {
  if (!density) return null;
  const level = density.level;
  return level in DENSITY_SCALE ? level : DEFAULT_DENSITY_LEVEL;
}

/** Normalize a possibly-missing theme mode to a known one (null when absent). */
export function themeModeOf(theme: { mode: ThemeMode } | undefined): ThemeMode | null {
  if (!theme) return null;
  const mode = theme.mode;
  return mode === 'light' || mode === 'dark' ? mode : DEFAULT_THEME_MODE;
}

// ── Default-value computations (the single source of each default) ─────────────

/**
 * The default `direction` the `column-below-md` behavior produces for a given
 * active breakpoint: column below md (base/sm), row at md and up.
 */
export function defaultDirectionFor(bp: Breakpoint): 'column' | 'row' {
  return bp === 'base' || bp === 'sm' ? 'column' : 'row';
}

/**
 * The default spacing value the `scale-by-density` behavior produces for a
 * padding/gap attribute at a density level, or undefined for any other attr.
 */
export function densityValueFor(attr: string, level: DensityLevel): string | undefined {
  const scaled = DENSITY_SCALE[level];
  if (attr === 'padding') return scaled.padding;
  if (attr === 'gap') return scaled.gap;
  return undefined;
}

/**
 * Resolve a semantic token → concrete colour for the active mode, honouring the
 * optional facts.theme.tokens override (which may partially override one mode of
 * a built-in token, or define a wholly new token). Returns undefined for an
 * unknown token / missing colour (honest absence — no fake value).
 */
export function themeColorFor(
  token: string,
  mode: ThemeMode,
  overrides: Record<string, Partial<ThemeTokenColors>> | undefined,
): string | undefined {
  const builtin = THEME_TOKENS[token];
  const override = overrides?.[token];
  const fromOverride = override?.[mode];
  if (typeof fromOverride === 'string' && fromOverride.length > 0) return fromOverride;
  const fromBuiltin = builtin?.[mode];
  if (typeof fromBuiltin === 'string' && fromBuiltin.length > 0) return fromBuiltin;
  return undefined;
}
