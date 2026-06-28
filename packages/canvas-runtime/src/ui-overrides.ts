/**
 * ui-overrides.ts — PURE override-provenance detector (Stage 1 of the
 * guidance-on-override layer).
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHAT AN "OVERRIDE" IS (from the resolver)
 * A *default-kind* resolve practice is guarded by a `when` that FAILS when the
 * author supplied an explicit value, so the author's value wins. There are
 * exactly four such override points today:
 *
 *   attr       default practice            suppressed when the author set…
 *   ───────    ─────────────────────────   ──────────────────────────────────
 *   direction  ui_layout_stack_below_md    responsive.direction (multi-child box)
 *   padding    ui_density_padding_scale    responsive.padding
 *   gap        ui_density_gap_scale        responsive.gap
 *   color      ui_theme_text_color         props.color (node has a themeToken)
 *
 * detectOverrides walks an AUTHORED tree and, for each node, emits an
 * OverrideNotice for each override point where the author's explicit value
 * actually SUPPRESSED a default AND that explicit value DIFFERS from what the
 * default practice would have produced (a MEANINGFUL deviation). It computes the
 * default with the SAME shared helpers the resolver uses (ui-resolve-helpers.ts),
 * so the two can never drift.
 *
 * ── HONEST ABSENCE ───────────────────────────────────────────────────────────
 * A default value is only knowable when its trigger fact is present:
 *   - direction needs ui:viewport  (to pick a breakpoint → column/row)
 *   - padding / gap need ui:density (to pick the spacing value)
 *   - color needs ui:theme         (to resolve the token → concrete colour)
 * If the trigger fact is absent the default is NOT determinable, so we do NOT
 * emit a notice for that attribute — we never claim an override against an
 * unknown default. Likewise, if the default practice would not have fired at all
 * for this node (e.g. a single-child container for the stack default, or an
 * unknown theme token), there is nothing being overridden → no notice.
 *
 * ── PURITY ───────────────────────────────────────────────────────────────────
 * detectOverrides is a deep-READ only. It never mutates the input tree and does
 * NOT require resolveUiTree to have run. It is independent of the resolved tree.
 *
 * ── nodeId SCHEME ────────────────────────────────────────────────────────────
 * Each notice carries a `nodeId`: the node's own `id` when it is a non-empty
 * string, otherwise a stable structural path — `"root"` for the root and
 * `"<parentPath>/children/<index>"` for descendants. So a deviating child two
 * levels deep with no id reads e.g. `root/children/2/children/0`. The path is
 * derived purely from tree position and is stable across calls.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { CanvasNodeLike } from './ui-facts.js';
import type { UiRuntimeFacts } from './ui-resolve.js';
import { resolveComponent } from './registry.js';
import { kindForComponent, pickResponsive, type Breakpoint, type SchemaKind } from './ui-schema.js';
import {
  UI_DENSITY_PRACTICES,
  UI_PRACTICES,
  UI_THEME_PRACTICES,
} from './ui-practices.js';
import {
  activeBreakpoint,
  defaultDirectionFor,
  densityLevelOf,
  densityValueFor,
  explicitColorOf,
  nodeFlags,
  themeColorFor,
  themeModeOf,
} from './ui-resolve-helpers.js';

/**
 * A single override provenance record: the author explicitly set `attr` to
 * `explicitValue`, which suppressed the named default practice that would
 * otherwise have produced `defaultValue`. `rationale` is the practice's
 * author-facing sentence (surfaced as guidance at authoring time).
 */
export interface OverrideNotice {
  /** node.id when present, else a structural path (see module header). */
  nodeId: string;
  /** The node's component type (e.g. 'Box', 'Text'). */
  nodeType: string;
  /** The overridden attribute: 'direction' | 'padding' | 'gap' | 'color'. */
  attr: string;
  /** The default practice that was suppressed. */
  practiceName: string;
  /** The suppressed practice's author-facing rationale. */
  rationale: string;
  /** What the default practice WOULD have produced at the active facts. */
  defaultValue: unknown;
  /** What the author's explicit value resolves to at the active facts. */
  explicitValue: unknown;
}

/** The rationale string for a named practice (single source: the practice data). */
function rationaleOf(practiceName: string): string {
  const all = [...UI_PRACTICES, ...UI_DENSITY_PRACTICES, ...UI_THEME_PRACTICES];
  const found = all.find((p) => p.name === practiceName);
  return found ? found.rationale : '';
}

/** Resolve a node's schema kind via the registry (null if unregistered). */
function schemaKindOf(node: CanvasNodeLike): SchemaKind | null {
  const meta = resolveComponent(node.type);
  if (!meta) return null;
  return kindForComponent(meta.schemaKind, meta.category);
}

/** The author's explicit responsive map for `attr`, if any. */
function responsiveMapOf(node: CanvasNodeLike, attr: string): Record<string, unknown> | undefined {
  const responsive = node.responsive as Record<string, Record<string, unknown>> | undefined;
  return responsive?.[attr];
}

/**
 * Detect every meaningful override on a single node, appending notices to `out`.
 * Pure: reads `node` and `facts`, writes only to `out`.
 */
function detectNode(
  node: CanvasNodeLike,
  facts: UiRuntimeFacts,
  nodeId: string,
  out: OverrideNotice[],
): void {
  const kind = schemaKindOf(node);
  if (kind === null) return; // unregistered → no practices apply, nothing to override

  const flags = nodeFlags(node);
  const bp: Breakpoint | null = activeBreakpoint(facts.viewport);

  // ── 1) direction: ui_layout_stack_below_md (container) ──────────────────────
  // Default fires only for a multi-child container with NO explicit responsive
  // direction. The override is an explicit responsive.direction on such a box.
  if (kind === 'container' && flags.childCount > 1 && flags.hasResponsiveDirection) {
    // Honest absence: without a viewport the default (column/row) is unknowable.
    if (bp !== null) {
      const explicitValue = pickResponsive(responsiveMapOf(node, 'direction'), bp);
      // Only meaningful when the author's value is actually active at this bp.
      if (explicitValue !== undefined) {
        const defaultValue = defaultDirectionFor(bp);
        if (explicitValue !== defaultValue) {
          out.push({
            nodeId,
            nodeType: node.type,
            attr: 'direction',
            practiceName: 'ui_layout_stack_below_md',
            rationale: rationaleOf('ui_layout_stack_below_md'),
            defaultValue,
            explicitValue,
          });
        }
      }
    }
  }

  // ── 2) & 3) padding / gap: ui_density_*_scale (container) ────────────────────
  // Default fires when ui:density is present and the node declared NO explicit
  // responsive map for that attr. The override is an explicit responsive.padding
  // / responsive.gap. The default needs ui:density; the explicit value resolves
  // at the active breakpoint, so it also needs a viewport to be concrete — absent
  // either trigger fact, the comparison isn't determinable → no notice.
  if (kind === 'container') {
    const level = densityLevelOf(facts.density);
    if (level !== null && bp !== null) {
      for (const [attr, practiceName, has] of [
        ['padding', 'ui_density_padding_scale', flags.hasResponsivePadding] as const,
        ['gap', 'ui_density_gap_scale', flags.hasResponsiveGap] as const,
      ]) {
        if (!has) continue; // no explicit responsive map → nothing suppressed
        const explicitValue = pickResponsive(responsiveMapOf(node, attr), bp);
        if (explicitValue === undefined) continue; // not active at this bp
        const defaultValue = densityValueFor(attr, level);
        if (defaultValue !== undefined && explicitValue !== defaultValue) {
          out.push({
            nodeId,
            nodeType: node.type,
            attr,
            practiceName,
            rationale: rationaleOf(practiceName),
            defaultValue,
            explicitValue,
          });
        }
      }
    }
  }

  // ── 4) color: ui_theme_text_color (text) ────────────────────────────────────
  // Default fires when ui:theme is present, the node has a themeToken, and NO
  // explicit literal color. The override is an explicit props.color on a node
  // that ALSO carries a themeToken (so a default would otherwise have applied).
  if (kind === 'text' && flags.hasThemeToken && flags.hasExplicitColor) {
    const mode = themeModeOf(facts.theme);
    // Honest absence: no ui:theme → token colour unknowable → no notice.
    if (mode !== null) {
      const token = node.themeToken as string;
      const defaultValue = themeColorFor(token, mode, facts.theme?.tokens);
      // Unknown token → default not determinable → nothing to claim.
      if (defaultValue !== undefined) {
        const explicitValue = explicitColorOf(node);
        if (explicitValue !== defaultValue) {
          out.push({
            nodeId,
            nodeType: node.type,
            attr: 'color',
            practiceName: 'ui_theme_text_color',
            rationale: rationaleOf('ui_theme_text_color'),
            defaultValue,
            explicitValue,
          });
        }
      }
    }
  }
}

/** Recursively walk node + children, deriving the structural nodeId path. */
function detectRecursive(
  node: CanvasNodeLike,
  facts: UiRuntimeFacts,
  path: string,
  out: OverrideNotice[],
): void {
  const id = typeof node.id === 'string' && node.id.length > 0 ? node.id : path;
  detectNode(node, facts, id, out);
  if (Array.isArray(node.children)) {
    for (let i = 0; i < node.children.length; i += 1) {
      detectRecursive(node.children[i], facts, `${path}/children/${i}`, out);
    }
  }
}

/**
 * Detect every meaningful override in an AUTHORED canvas tree against the active
 * runtime facts.
 *
 * For each node and each of the four override points (direction / padding / gap /
 * color), an OverrideNotice is emitted ONLY when:
 *   1. the author supplied an explicit value that suppresses the default practice,
 *   2. the default's trigger fact is present (so the default is determinable), and
 *   3. the explicit value DIFFERS from what the default would have produced.
 *
 * @param root  the AUTHORED root node (never mutated)
 * @param facts the active runtime facts (viewport / density / theme)
 * @returns the notices in document order (root first, children depth-first)
 */
export function detectOverrides(
  root: CanvasNodeLike,
  facts: UiRuntimeFacts = {},
): OverrideNotice[] {
  const out: OverrideNotice[] = [];
  detectRecursive(root, facts, 'root', out);
  return out;
}
