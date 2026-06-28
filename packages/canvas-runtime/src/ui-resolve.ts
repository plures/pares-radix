/**
 * UI Resolver — the RESOLVE half of the best-practice engine.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * resolveUiTree(authoredTree, facts) → resolvedTree
 *
 * A PURE function. It deep-clones the authored tree and, for every node, applies
 * the resolve practices (UI_PRACTICES) whose `appliesTo` schema-kind matches the
 * node and whose `when` guard passes — writing concrete `props[attr]` values for
 * the active breakpoint. The authored tree is NEVER mutated (source vs derived,
 * C-PLURES-004 / C-DRIFT-001): callers persist the result to canvas:tree:resolved
 * and Unum renders that.
 *
 * Practices are DATA (ui-practices.ts, mirrored from ui-layout.px). This resolver
 * is a generic interpreter of them — it has no per-component branches. Adding a
 * practice is a data change, drift-guarded by tests.
 *
 * FLAT SURFACE: the only "evaluation" is (a) a flat-boolean `when` over a small
 * context object and (b) pickResponsive (a table lookup). No tree walking in
 * author space, no functions.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { CanvasNodeLike } from './ui-facts.js';
import { resolveComponent } from './registry.js';
import {
  breakpointFor,
  kindForComponent,
  pickResponsive,
  RESPONSIVE_ATTR_SET,
  type Breakpoint,
  type SchemaKind,
} from './ui-schema.js';
import { UI_PRACTICES, DEFAULT_BEHAVIORS, type UiPractice } from './ui-practices.js';

// ── Runtime facts ─────────────────────────────────────────────────────────────

export interface ViewportFact {
  width: number;
  height?: number;
  /** Optional precomputed breakpoint; derived from width if absent. */
  breakpoint?: Breakpoint;
}

export interface UiRuntimeFacts {
  viewport?: ViewportFact;
  // theme / density reserved for follow-on practice sets (DESIGN §8).
  theme?: unknown;
  density?: unknown;
}

// ── Per-node evaluation context (flat) ────────────────────────────────────────

interface NodeEvalContext {
  node: {
    childCount: number;
    hasResponsiveDirection: boolean;
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
 *   context.node.hasResponsiveDirection === true|false
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
  // hasResponsiveDirection === true|false
  m = /^context\.node\.hasResponsiveDirection\s*===\s*(true|false)$/.exec(atom);
  if (m) return ctx.node.hasResponsiveDirection === (m[1] === 'true');
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
 */
function resolveNode(node: CanvasNodeLike, facts: UiRuntimeFacts): void {
  const kind = schemaKindOf(node);
  if (kind === null) return; // unregistered node — leave untouched

  const vp = facts.viewport;
  const bp: Breakpoint | null = vp
    ? vp.breakpoint ?? breakpointFor(vp.width)
    : null;

  const responsive = node.responsive as Record<string, Record<string, unknown>> | undefined;
  const ctx: NodeEvalContext = {
    node: {
      childCount: Array.isArray(node.children) ? node.children.length : 0,
      hasResponsiveDirection: !!responsive && Object.prototype.hasOwnProperty.call(responsive, 'direction'),
      schemaKind: kind,
    },
    viewport: vp && bp ? { width: vp.width, breakpoint: bp } : null,
  };

  const props: Record<string, unknown> = (node.props ??= {});

  for (const practice of UI_PRACTICES) {
    if (practice.appliesTo !== kind) continue;
    if (!whenHolds(practice.when, ctx)) continue;
    applyPractice(practice, node, props, responsive, bp);
  }
}

/** Apply one practice's value to `props[practice.set]`. */
function applyPractice(
  practice: UiPractice,
  node: CanvasNodeLike,
  props: Record<string, unknown>,
  responsive: Record<string, Record<string, unknown>> | undefined,
  bp: Breakpoint | null,
): void {
  const attr = practice.set;
  // Honesty guard at runtime: never write an attribute outside the vocabulary.
  if (!RESPONSIVE_ATTR_SET.has(attr)) return;

  if (practice.source.kind === 'responsive') {
    const map = responsive?.[attr];
    if (!map || bp === null) return; // nothing declared / no viewport → no-op
    const value = pickResponsive(map, bp);
    if (value !== undefined) props[attr] = value;
    return;
  }

  // default behavior
  if (practice.source.value === DEFAULT_BEHAVIORS.COLUMN_BELOW_MD) {
    if (bp === null) return;
    // below md → column; md and up → row
    const belowMd = bp === 'base' || bp === 'sm';
    props[attr] = belowMd ? 'column' : 'row';
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
 * @returns a NEW tree with concrete responsive props applied. The input is never
 *          mutated. With no viewport fact, returns a clone with only
 *          viewport-independent defaults (currently none) applied — effectively
 *          an identity clone.
 */
export function resolveUiTree<T extends CanvasNodeLike>(root: T, facts: UiRuntimeFacts = {}): T {
  const clone = deepClone(root);
  resolveRecursive(clone, facts);
  return clone;
}
