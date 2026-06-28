/**
 * UI Resolver — the RESOLVE half of the best-practice engine.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * resolveUiTree(authoredTree, facts) → resolvedTree
 *
 * A PURE function. It deep-clones the authored tree and, for every node, applies
 * the resolve practices (UI_PRACTICES + density + theme) whose `appliesTo`
 * schema-kind matches the node and whose `when` guard passes — writing concrete
 * `props[attr]` values for the active breakpoint / density level / theme mode.
 * The authored tree is NEVER mutated (source vs derived, C-PLURES-004 /
 * C-DRIFT-001): callers persist the result to canvas:tree:resolved and Unum
 * renders that.
 *
 * Practices are DATA (ui-practices.ts, mirrored from the .px sources). This
 * resolver is a generic interpreter of them — it has no per-component branches.
 * Adding a practice is a data change, drift-guarded by tests.
 *
 * PRECEDENCE (mirrors the stack-below-md pattern): the responsive practices
 * (UI_PRACTICES) run FIRST and write any explicit responsive value. The density
 * and theme practices are DEFAULTS — each is guarded by a `when` clause that only
 * holds when the node declared NO explicit responsive map (padding/gap) or NO
 * explicit literal (color) for that attribute. So an explicit author value always
 * wins; the default only fills an otherwise-empty slot.
 *
 * FLAT SURFACE: the only "evaluation" is (a) a flat-boolean `when` over a small
 * context object and (b) table lookups (pickResponsive, DENSITY_SCALE,
 * THEME_TOKENS). No tree walking in author space, no functions.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { CanvasNodeLike } from './ui-facts.js';
import { resolveComponent } from './registry.js';
import {
  kindForComponent,
  pickResponsive,
  RESPONSIVE_ATTR_SET,
  type Breakpoint,
  type SchemaKind,
} from './ui-schema.js';
import {
  UI_PRACTICES,
  UI_DENSITY_PRACTICES,
  UI_THEME_PRACTICES,
  DEFAULT_BEHAVIORS,
  THEMEABLE_ATTR_SET,
  type UiPractice,
  type DensityLevel,
  type ThemeMode,
  type ThemeTokenColors,
} from './ui-practices.js';
import {
  activeBreakpoint,
  defaultDirectionFor,
  densityLevelOf,
  densityValueFor,
  nodeFlags,
  themeColorFor,
  themeModeOf,
} from './ui-resolve-helpers.js';

// ── Runtime facts ─────────────────────────────────────────────────────────────

export interface ViewportFact {
  width: number;
  height?: number;
  /** Optional precomputed breakpoint; derived from width if absent. */
  breakpoint?: Breakpoint;
}

/** ui:density trigger fact — the global spacing-tightness knob. */
export interface DensityFact {
  level: DensityLevel;
}

/** ui:theme trigger fact — light/dark mode + optional per-token colour overrides. */
export interface ThemeFact {
  mode: ThemeMode;
  /** Override/extend the built-in token palette, per token name. */
  tokens?: Record<string, Partial<ThemeTokenColors>>;
}

export interface UiRuntimeFacts {
  viewport?: ViewportFact;
  /** ui:theme — resolved by UI_THEME_PRACTICES (text color tokens). */
  theme?: ThemeFact;
  /** ui:density — resolved by UI_DENSITY_PRACTICES (container padding/gap). */
  density?: DensityFact;
}

// ── Per-node evaluation context (flat) ────────────────────────────────────────

interface NodeEvalContext {
  node: {
    childCount: number;
    hasResponsiveDirection: boolean;
    hasResponsivePadding: boolean;
    hasResponsiveGap: boolean;
    hasThemeToken: boolean;
    hasExplicitColor: boolean;
    schemaKind: SchemaKind | null;
  };
  viewport: { width: number; breakpoint: Breakpoint } | null;
}

/** Resolve a node's schema kind via the registry (null if unregistered). */
function schemaKindOf(node: CanvasNodeLike): SchemaKind | null {
  const meta = resolveComponent(node.type);
  if (!meta) return null;
  return kindForComponent(meta.schemaKind, meta.category);
}

/**
 * Evaluate a practice's flat-boolean `when` against the node context.
 * Only the handful of predicates the practices actually use are supported —
 * matching the flat-evaluator contract (no general expression engine here).
 *
 * Supported atoms (joined by &&):
 *   context.node.childCount > <n>
 *   context.node.childCount >= <n>
 *   context.node.hasResponsiveDirection === true|false
 *   context.node.hasResponsivePadding === true|false
 *   context.node.hasResponsiveGap === true|false
 *   context.node.hasThemeToken === true|false
 *   context.node.hasExplicitColor === true|false
 */
function whenHolds(when: string | undefined, ctx: NodeEvalContext): boolean {
  if (!when || when.trim() === '') return true;
  const clauses = when.split('&&').map((c) => c.trim());
  for (const clause of clauses) {
    if (!atomHolds(clause, ctx)) return false;
  }
  return true;
}

function atomHolds(atom: string, ctx: NodeEvalContext): boolean {
  // childCount > N
  let m = /^context\.node\.childCount\s*>\s*(\d+)$/.exec(atom);
  if (m) return ctx.node.childCount > Number(m[1]);
  // childCount >= N
  m = /^context\.node\.childCount\s*>=\s*(\d+)$/.exec(atom);
  if (m) return ctx.node.childCount >= Number(m[1]);
  // boolean node flags: <flag> === true|false
  m = /^context\.node\.(hasResponsiveDirection|hasResponsivePadding|hasResponsiveGap|hasThemeToken|hasExplicitColor)\s*===\s*(true|false)$/.exec(
    atom,
  );
  if (m) {
    const flag = m[1] as keyof NodeEvalContext['node'];
    return ctx.node[flag] === (m[2] === 'true');
  }
  // Unknown atom → fail closed (do not apply a practice we can't evaluate).
  return false;
}

// ── Resolution ────────────────────────────────────────────────────────────────

/** Deep clone via structuredClone when available, else JSON fallback. */
function deepClone<T>(v: T): T {
  if (typeof structuredClone === 'function') return structuredClone(v);
  return JSON.parse(JSON.stringify(v)) as T;
}

/**
 * Apply all matching practices to a single node (in place on the CLONE).
 * Returns nothing; mutates `node.props`.
 *
 * Order: responsive practices FIRST (explicit values), then density, then theme
 * (defaults guarded to only fill empty slots). This realizes the precedence
 * contract without the resolver needing to compare values.
 */
function resolveNode(node: CanvasNodeLike, facts: UiRuntimeFacts): void {
  const kind = schemaKindOf(node);
  if (kind === null) return; // unregistered node — leave untouched

  const vp = facts.viewport;
  const bp: Breakpoint | null = activeBreakpoint(vp);

  const responsive = node.responsive as Record<string, Record<string, unknown>> | undefined;
  const flags = nodeFlags(node);

  const ctx: NodeEvalContext = {
    node: {
      childCount: flags.childCount,
      hasResponsiveDirection: flags.hasResponsiveDirection,
      hasResponsivePadding: flags.hasResponsivePadding,
      hasResponsiveGap: flags.hasResponsiveGap,
      hasThemeToken: flags.hasThemeToken,
      hasExplicitColor: flags.hasExplicitColor,
      schemaKind: kind,
    },
    viewport: vp && bp ? { width: vp.width, breakpoint: bp } : null,
  };

  const props: Record<string, unknown> = (node.props ??= {});

  // 1) Responsive layout practices (explicit author values win).
  for (const practice of UI_PRACTICES) {
    if (practice.appliesTo !== kind) continue;
    if (!whenHolds(practice.when, ctx)) continue;
    applyPractice(practice, node, props, responsive, bp, facts, ctx);
  }

  // 2) Density defaults (padding/gap) — only when ui:density present.
  if (facts.density) {
    for (const practice of UI_DENSITY_PRACTICES) {
      if (practice.appliesTo !== kind) continue;
      if (!whenHolds(practice.when, ctx)) continue;
      applyPractice(practice, node, props, responsive, bp, facts, ctx);
    }
  }

  // 3) Theme defaults (text color) — only when ui:theme present.
  if (facts.theme) {
    for (const practice of UI_THEME_PRACTICES) {
      if (practice.appliesTo !== kind) continue;
      if (!whenHolds(practice.when, ctx)) continue;
      applyPractice(practice, node, props, responsive, bp, facts, ctx);
    }
  }
}

/** Apply one practice's value to `props[practice.set]`. */
function applyPractice(
  practice: UiPractice,
  node: CanvasNodeLike,
  props: Record<string, unknown>,
  responsive: Record<string, Record<string, unknown>> | undefined,
  bp: Breakpoint | null,
  facts: UiRuntimeFacts,
  ctx: NodeEvalContext,
): void {
  const attr = practice.set;

  if (practice.source.kind === 'responsive') {
    // Honesty guard: responsive practices only write RESPONSIVE_ATTRS.
    if (!RESPONSIVE_ATTR_SET.has(attr)) return;
    const map = responsive?.[attr];
    if (!map || bp === null) return; // nothing declared / no viewport → no-op
    const value = pickResponsive(map, bp);
    if (value !== undefined) props[attr] = value;
    return;
  }

  // ── Named default behaviors ──
  switch (practice.source.value) {
    case DEFAULT_BEHAVIORS.COLUMN_BELOW_MD: {
      if (!RESPONSIVE_ATTR_SET.has(attr)) return;
      if (bp === null) return;
      props[attr] = defaultDirectionFor(bp);
      return;
    }

    case DEFAULT_BEHAVIORS.SCALE_BY_DENSITY: {
      // Honesty: density writes padding/gap, both real props in RESPONSIVE_ATTRS.
      if (!RESPONSIVE_ATTR_SET.has(attr)) return;
      const level = densityLevelOf(facts.density);
      if (level === null) return; // no ui:density → no-op
      const value = densityValueFor(attr, level);
      if (value !== undefined) props[attr] = value;
      return;
    }

    case DEFAULT_BEHAVIORS.THEME_TOKEN_COLOR: {
      // Honesty: theme writes `color`, a real Text prop on the THEMEABLE allow-list.
      if (!THEMEABLE_ATTR_SET.has(attr)) return;
      const mode = themeModeOf(facts.theme);
      if (mode === null) return; // no ui:theme → no-op
      const token = node.themeToken;
      if (typeof token !== 'string' || token.length === 0) return;
      const color = themeColorFor(token, mode, facts.theme?.tokens);
      if (color !== undefined) props[attr] = color;
      return;
    }

    default:
      // Unknown behavior → fail closed (never write a value we don't understand).
      return;
  }
}

/** Recursively resolve a node and its children (mutates the clone). */
function resolveRecursive(node: CanvasNodeLike, facts: UiRuntimeFacts): void {
  resolveNode(node, facts);
  if (Array.isArray(node.children)) {
    for (const child of node.children) resolveRecursive(child, facts);
  }
}

/**
 * Resolve an authored canvas tree against runtime facts.
 *
 * @returns a NEW tree with concrete responsive / density / theme props applied.
 *          The input is never mutated. With no facts, returns a clone with only
 *          viewport-independent defaults (currently none) applied — effectively
 *          an identity clone.
 */
export function resolveUiTree<T extends CanvasNodeLike>(root: T, facts: UiRuntimeFacts = {}): T {
  const clone = deepClone(root);
  resolveRecursive(clone, facts);
  return clone;
}
