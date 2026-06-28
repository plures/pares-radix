/**
 * UI Schema — the typed, closed vocabulary of element KINDS × ATTRIBUTES that
 * UI best-practice rules operate over.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY THIS EXISTS
 * A UI best practice is "a rule about the values of a known set of attributes on
 * a known set of elements." This module defines that known set:
 *   - SchemaKind: the element kinds (container, text, control, …)
 *   - RESPONSIVE_ATTRS: attributes that resolve per-breakpoint
 *   - BREAKPOINTS: the responsive ladder + flat lookup helpers
 *   - kindForComponent: maps a registered component → its schema kind
 *
 * HONESTY INVARIANT
 * Every attribute named here maps to a prop a registered design-dojo component
 * actually exposes (verified against registry.ts). The *curation* of this set IS
 * the guardrail: a bad practice cannot be expressed because the vocabulary only
 * contains good attributes.
 *
 * FLAT-EVALUATOR CONTRACT
 * The helpers here (breakpointFor, pickResponsive) are plain table lookups — no
 * tree walking, no author-space functions. They pre-flatten responsive maps so
 * the rule evaluator never has to.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { ComponentMeta } from './registry.js';

// ── Element kinds ─────────────────────────────────────────────────────────────

/**
 * The closed set of UI element kinds. Best-practice rules target a kind, never a
 * concrete component name, so one rule (e.g. "container stacks below md") covers
 * every component that maps to that kind.
 */
export type SchemaKind =
  | 'container'
  | 'text'
  | 'control'
  | 'media'
  | 'navigation'
  | 'group'
  | 'feedback';

export const SCHEMA_KINDS: readonly SchemaKind[] = [
  'container',
  'text',
  'control',
  'media',
  'navigation',
  'group',
  'feedback',
] as const;

/**
 * Default kind for each `ComponentMeta.category`. A component may override this
 * via an explicit `schemaKind` on its registration.
 *
 * category → kind rationale:
 *   layout     → container  (box-model / position / flow)
 *   display    → text       (typography / color; most display nodes are textual)
 *   input      → control    (state / labeling / affordance)
 *   navigation → navigation (target / current-state)
 *   feedback   → feedback   (actionability / visibility)
 *   data       → group      (structure / semantics: tables, lists)
 *   custom     → container  (safest default: treat as a generic box)
 */
const CATEGORY_TO_KIND: Record<ComponentMeta['category'], SchemaKind> = {
  layout: 'container',
  display: 'text',
  input: 'control',
  navigation: 'navigation',
  feedback: 'feedback',
  data: 'group',
  custom: 'container',
};

/**
 * Resolve the schema kind for a component.
 *
 * @param explicit  the component's optional `schemaKind` override (wins if set)
 * @param category  the component's `category` (used for the default mapping)
 */
export function kindForComponent(
  explicit: SchemaKind | undefined,
  category: ComponentMeta['category'],
): SchemaKind {
  return explicit ?? CATEGORY_TO_KIND[category];
}

// ── Attribute vocabulary ──────────────────────────────────────────────────────

/**
 * Attributes that resolve PER BREAKPOINT (responsive). A node may carry a
 * `responsive[attr]` breakpoint-map for any of these; the resolver collapses it
 * to a concrete `props[attr]` for the active breakpoint.
 *
 * Each maps to a real prop:
 *   direction,padding,gap,align,justify,wrap → Box
 *   columns                                  → DashboardGrid (derived)
 *   hidden                                   → generic show/hide (new, container/all)
 *   size                                     → Text.size
 *   maxLines                                 → Text truncation (reserved; see DESIGN §8)
 */
export const RESPONSIVE_ATTRS: readonly string[] = [
  'direction',
  'padding',
  'gap',
  'align',
  'justify',
  'wrap',
  'columns',
  'hidden',
  'size',
  'maxLines',
] as const;

/** Set form for O(1) membership checks. */
export const RESPONSIVE_ATTR_SET: ReadonlySet<string> = new Set(RESPONSIVE_ATTRS);

// ── Breakpoint ladder ─────────────────────────────────────────────────────────

/** Ordered breakpoint name list, smallest → largest. */
export type Breakpoint = 'base' | 'sm' | 'md' | 'lg' | 'xl';

/**
 * Minimum width (px) at which each breakpoint becomes active. `base` is the
 * implicit floor (0). Standard Tailwind-ish ladder.
 */
export const BREAKPOINTS: ReadonlyArray<{ name: Breakpoint; min: number }> = [
  { name: 'base', min: 0 },
  { name: 'sm', min: 640 },
  { name: 'md', min: 768 },
  { name: 'lg', min: 1024 },
  { name: 'xl', min: 1280 },
] as const;

/** Breakpoint names smallest → largest (for fallback walking). */
export const BREAKPOINT_ORDER: readonly Breakpoint[] = BREAKPOINTS.map((b) => b.name);

/**
 * The active breakpoint for a given viewport width.
 * e.g. 639→base, 640→sm, 767→sm, 768→md, 1024→lg, 1280→xl.
 */
export function breakpointFor(width: number): Breakpoint {
  let active: Breakpoint = 'base';
  for (const bp of BREAKPOINTS) {
    if (width >= bp.min) active = bp.name;
    else break;
  }
  return active;
}

/**
 * Pick the value from a responsive map for the active breakpoint, falling back
 * to the nearest SMALLER defined breakpoint (mobile-first cascade), and finally
 * to `base`. Returns `undefined` if the map defines nothing at or below the
 * active breakpoint.
 *
 * This is the flat table lookup the evaluator relies on — no functions, no
 * tree-walking.
 *
 * @example pickResponsive({ base:'column', md:'row' }, 'lg') === 'row'
 * @example pickResponsive({ md:'row' }, 'sm') === undefined
 */
export function pickResponsive<T = unknown>(
  map: Record<string, T> | undefined,
  bp: Breakpoint,
): T | undefined {
  if (!map) return undefined;
  const activeIdx = BREAKPOINT_ORDER.indexOf(bp);
  for (let i = activeIdx; i >= 0; i--) {
    const name = BREAKPOINT_ORDER[i];
    if (Object.prototype.hasOwnProperty.call(map, name)) {
      return map[name];
    }
  }
  return undefined;
}
