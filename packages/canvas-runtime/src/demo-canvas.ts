/**
 * Demo Canvas — a REAL authored responsive canvas that exercises the shipped
 * best-practice engine (resolve practices: responsive layout + theme tokens).
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY THIS LIVES IN canvas-runtime/src (not app src)
 * The behavior test (tests/canvas-demo-resolve.test.ts) and the running app
 * (src/routes/canvas/+page.svelte + src/lib/plugins/canvas/CanvasView.svelte)
 * must drive the EXACT SAME tree — otherwise the test guards a different demo
 * than the one a human sees (drift, C-DRIFT-001). Importing app `$lib`/`$app`
 * source into a canvas-runtime vitest is awkward (SvelteKit aliases don't
 * resolve there), so the single source of truth lives HERE and is:
 *   - imported by the test relatively (`../src/demo-canvas.js`), and
 *   - re-exported from the package index for the app (`@plures/canvas-runtime`).
 *
 * WHAT IT EXERCISES (only real registry types + real props — C-NOSTUB-001)
 *   - Root Box (container kind) with responsive.direction {base:'column', md:'row'},
 *     responsive.gap {base:'8px', md:'24px'}, responsive.padding {base:'8px', lg:'32px'}.
 *     → narrow stacks (column, 8px); wide is a row (24px) with roomier padding at lg.
 *   - A sidebar Box with responsive.hidden {base:true, md:false}
 *     → hidden on mobile, revealed at md+.
 *   - Text nodes carrying themeToken ('fg' / 'accent')
 *     → recolored by the theme bridge (ui:theme) on light/dark toggle.
 *
 * Authored intent stays pristine: the resolver clones before resolving, so this
 * module is a frozen, reusable constant.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { CanvasDocument, CanvasNode } from './format.js';

/**
 * A CanvasNode extended with the authored `themeToken` field.
 *
 * WHY THIS LOCAL TYPE: `themeToken` is authored intent the THEME resolve
 * practices consume (see ui-facts.ts `CanvasNodeLike` / ui-practices.ts), but it
 * is not declared on `format.ts`'s `CanvasNode` yet (adding it there is owned by
 * another thread). Rather than edit format.ts, the demo declares the field
 * locally — it is structurally a superset of CanvasNode, so the document's
 * `tree: CanvasNode` slot still accepts it. The resolver reads `themeToken` off
 * the node regardless of which interface named it.
 */
export interface DemoCanvasNode extends CanvasNode {
  /** Semantic colour token resolved to a concrete `color` per theme mode. */
  themeToken?: string;
  /** Children may also be themed demo nodes. */
  children?: DemoCanvasNode[];
}

/**
 * The authored responsive demo TREE (the root node + children). This is the
 * single source of truth the resolver runs against. Exported on its own so the
 * behavior test can resolve it directly without constructing a full document.
 */
export const DEMO_CANVAS_TREE: DemoCanvasNode = {
  id: 'demo-root',
  type: 'Box',
  props: { align: 'stretch' },
  // Root layout intent: stack on mobile, side-by-side at md+, roomier at lg.
  responsive: {
    direction: { base: 'column', md: 'row' },
    gap: { base: '8px', md: '24px' },
    padding: { base: '8px', lg: '32px' },
  },
  children: [
    // ── Sidebar: hidden on mobile, revealed at md+ ──
    {
      id: 'demo-sidebar',
      type: 'Box',
      props: { padding: '12px', gap: '8px' },
      responsive: {
        hidden: { base: true, md: false },
      },
      children: [
        {
          id: 'demo-sidebar-title',
          type: 'Heading',
          props: { level: 3 },
          children: [{ id: 'demo-sidebar-title-text', type: 'Text', themeToken: 'fg', props: {} }],
        },
        { id: 'demo-sidebar-link', type: 'Text', themeToken: 'accent', props: {} },
      ],
    },
    // ── Main column: always visible, holds the header row + body copy ──
    {
      id: 'demo-main',
      type: 'Box',
      props: { gap: '12px', padding: '12px' },
      children: [
        {
          id: 'demo-heading',
          type: 'Heading',
          props: { level: 1 },
          children: [{ id: 'demo-heading-text', type: 'Text', themeToken: 'fg', props: {} }],
        },
        // Body copy — a themed Text (fg) the test asserts recolors in dark mode.
        { id: 'demo-body', type: 'Text', themeToken: 'fg', props: {} },
        { id: 'demo-accent', type: 'Text', themeToken: 'accent', props: {} },
      ],
    },
  ],
};

/**
 * Build a full, importable CanvasDocument wrapping DEMO_CANVAS_TREE. The app
 * `put`s this into `canvas:_active` to make the loop observable on /canvas.
 * Returns a fresh document each call (fresh timestamps/id) but always the SAME
 * tree constant, so app and test stay in lockstep.
 */
export function getDemoCanvas(): CanvasDocument {
  const now = new Date().toISOString();
  return {
    version: '1.0.0',
    meta: {
      title: 'Responsive Demo',
      description:
        'A live example of the canvas best-practice engine: resize narrow↔wide to reflow ' +
        '(column↔row, reveal the sidebar, widen gaps/padding); toggle the app theme to recolor text.',
      author: 'plures',
      createdAt: now,
      modifiedAt: now,
      tags: ['demo', 'responsive', 'theme'],
      id: 'demo-responsive-canvas',
    },
    tree: DEMO_CANVAS_TREE,
    data: {},
    rules: [],
    procedures: [],
    schema: [],
  };
}
