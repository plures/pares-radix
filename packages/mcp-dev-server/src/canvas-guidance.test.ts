/**
 * canvas-guidance.test.ts — Stage 2 (guidance-on-override at the AI authoring
 * seam). Verifies the canonical AUTHORING_FACTS and the guidanceForTree wrapper
 * that the canvas.addNode / canvas.setTree MCP handlers call after they mutate
 * the active canvas.
 *
 * detectOverrides (Stage 1) is honest-absent: without a trigger fact it emits
 * nothing. The whole point of AUTHORING_FACTS is to pin the canonical default
 * baseline (md / comfortable / light) so an authoring-time override is
 * determinable and surfaced. These tests assert exactly that, against the SAME
 * registry-backed Box/Text stand-ins the Stage 1 detector test uses.
 *
 * The handler-wiring test composes the REAL exported runtime functions
 * (toolCanvasCreate → toolCanvasAddNode → guidanceForTree) — i.e. the exact two
 * lines canvas.addNode runs after its dbPut — rather than faking the handler.
 * index.ts itself cannot be imported in-process (its module top-level runs a
 * RADIX_DEV process.exit gate and attaches stdio listeners), and its `tools`
 * array / `activeCanvas` are module-private; the honest equivalent is to drive
 * the same real functions the handler delegates to.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import {
  registerComponent,
  toolCanvasCreate,
  toolCanvasAddNode,
  BREAKPOINTS,
  DEFAULT_DENSITY_LEVEL,
  DEFAULT_THEME_MODE,
  type CanvasNode,
} from '@plures/canvas-runtime';
import { AUTHORING_FACTS, guidanceForTree } from './canvas-guidance.js';

// Same stub pattern as canvas-runtime/tests/ui-overrides.test.ts: register
// stand-ins whose CATEGORY yields the right schemaKind so resolveComponent (used
// inside detectOverrides) classifies them.
//   Box → layout → container ; Text → display → text.
beforeAll(() => {
  const base = {
    component: null as unknown as never,
    props: [],
    hasChildren: true,
    description: 'stub',
  };
  registerComponent('Box', { ...base, name: 'Box', category: 'layout' });
  registerComponent('Text', { ...base, name: 'Text', category: 'display' });
});

// ── AUTHORING_FACTS shape (all three trigger families present & well-typed) ────
describe('AUTHORING_FACTS', () => {
  it('carries viewport + density + theme so no override point is honest-absent', () => {
    expect(AUTHORING_FACTS.viewport).toBeDefined();
    expect(AUTHORING_FACTS.density).toBeDefined();
    expect(AUTHORING_FACTS.theme).toBeDefined();
  });

  it('pins the canonical md / comfortable / light baseline', () => {
    expect(AUTHORING_FACTS.density?.level).toBe(DEFAULT_DENSITY_LEVEL); // 'comfortable'
    expect(AUTHORING_FACTS.theme?.mode).toBe(DEFAULT_THEME_MODE); // 'light'
    expect(AUTHORING_FACTS.viewport?.breakpoint).toBe('md');
  });

  it('sources the viewport width from the schema md breakpoint (no magic number)', () => {
    const mdMin = BREAKPOINTS.find((b) => b.name === 'md')?.min;
    expect(mdMin).toBeDefined();
    expect(AUTHORING_FACTS.viewport?.width).toBe(mdMin);
  });

  it('is well-typed: width is a number, level/mode are the expected literals', () => {
    expect(typeof AUTHORING_FACTS.viewport?.width).toBe('number');
    expect(['compact', 'comfortable', 'spacious']).toContain(AUTHORING_FACTS.density?.level);
    expect(['light', 'dark']).toContain(AUTHORING_FACTS.theme?.mode);
  });
});

// ── guidanceForTree — theme color override (light fg = #111111) ────────────────
describe('guidanceForTree — theme color override', () => {
  it('explicit props.color overriding a themeToken (≠ light token color) → 1 theme notice', () => {
    const tree: CanvasNode = {
      id: 'title',
      type: 'Text',
      props: { color: '#ff0000' }, // explicit, diverges from fg/light = #111111
      themeToken: 'fg',
    };
    const guidance = guidanceForTree(tree);
    expect(guidance).toHaveLength(1);
    const n = guidance[0];
    expect(n.attr).toBe('color');
    expect(n.practiceName).toBe('ui_theme_text_color');
    expect(n.defaultValue).toBe('#111111'); // fg @ light, the canonical baseline
    expect(n.explicitValue).toBe('#ff0000');
    expect(n.nodeId).toBe('title');
    expect(n.nodeType).toBe('Text');
    expect(n.rationale).toMatch(/derived from your semantic theme token/);
  });

  it('explicit color EQUAL to the light token color → no notice', () => {
    const tree: CanvasNode = {
      id: 'title',
      type: 'Text',
      props: { color: '#111111' }, // == fg/light default → not a real override
      themeToken: 'fg',
    };
    expect(guidanceForTree(tree)).toHaveLength(0);
  });
});

// ── guidanceForTree — density gap override (comfortable gap = 8px) ─────────────
describe('guidanceForTree — density gap override', () => {
  it('explicit responsive.gap differing from the comfortable default → 1 density-gap notice', () => {
    const tree: CanvasNode = {
      id: 'row',
      type: 'Box',
      props: {},
      responsive: { gap: { base: '20px' } }, // diverges from comfortable gap = 8px
    };
    const guidance = guidanceForTree(tree);
    expect(guidance).toHaveLength(1);
    const n = guidance[0];
    expect(n.attr).toBe('gap');
    expect(n.practiceName).toBe('ui_density_gap_scale');
    expect(n.defaultValue).toBe('8px'); // comfortable gap, the canonical baseline
    expect(n.explicitValue).toBe('20px');
    expect(n.nodeId).toBe('row');
    expect(n.nodeType).toBe('Box');
    expect(n.rationale).toMatch(/Gap scales with the active display density/);
  });

  it('explicit gap EQUAL to the comfortable default → no notice', () => {
    const tree: CanvasNode = {
      id: 'row',
      type: 'Box',
      props: {},
      responsive: { gap: { base: '8px' } }, // == comfortable default → not an override
    };
    expect(guidanceForTree(tree)).toHaveLength(0);
  });
});

// ── guidanceForTree — no overrides → [] ───────────────────────────────────────
describe('guidanceForTree — no overrides', () => {
  it('a tree whose nodes set no overriding explicit values → []', () => {
    const tree: CanvasNode = {
      id: 'root',
      type: 'Box',
      props: {},
      children: [
        { id: 'a', type: 'Text', props: {} }, // no themeToken, no explicit color
        { id: 'b', type: 'Text', props: {} },
      ],
    };
    expect(guidanceForTree(tree)).toEqual([]);
  });

  it('a Text with a themeToken but NO explicit color → default applies, no override → []', () => {
    const tree: CanvasNode = { id: 't', type: 'Text', props: {}, themeToken: 'fg' };
    expect(guidanceForTree(tree)).toEqual([]);
  });
});

// ── handler wiring — drive the REAL functions canvas.addNode delegates to ──────
describe('canvas.addNode wiring (real runtime functions, not a fake handler)', () => {
  it('adding an overriding node yields guidance from the resulting tree', () => {
    // The exact composition canvas.addNode performs after its dbPut:
    //   activeCanvas = toolCanvasAddNode(activeCanvas, parentId, node)
    //   guidance     = guidanceForTree(activeCanvas.tree)
    const canvas = toolCanvasCreate({ title: 'wiring-test' });
    const rootId = canvas.tree.id;
    const overridingNode: CanvasNode = {
      id: 'heading',
      type: 'Text',
      props: { color: '#ff0000' }, // overrides fg/light = #111111
      themeToken: 'fg',
    };
    const updated = toolCanvasAddNode(canvas, rootId, overridingNode);
    const guidance = guidanceForTree(updated.tree);

    // The handler attaches guidance only when non-empty; here it is non-empty.
    expect(guidance.length).toBeGreaterThan(0);
    const result = { ok: true, tree: updated.tree, ...(guidance.length ? { guidance } : {}) };
    expect(result).toHaveProperty('guidance');
    expect((result as { guidance: typeof guidance }).guidance[0].attr).toBe('color');
    expect((result as { guidance: typeof guidance }).guidance[0].nodeId).toBe('heading');
  });

  it('adding a NON-overriding node produces no guidance (handler omits the key)', () => {
    const canvas = toolCanvasCreate({ title: 'wiring-clean' });
    const rootId = canvas.tree.id;
    const plainNode: CanvasNode = { id: 'plain', type: 'Text', props: {} };
    const updated = toolCanvasAddNode(canvas, rootId, plainNode);
    const guidance = guidanceForTree(updated.tree);

    expect(guidance).toEqual([]);
    const result = { ok: true, tree: updated.tree, ...(guidance.length ? { guidance } : {}) };
    expect(result).not.toHaveProperty('guidance');
  });
});
