/**
 * detectOverrides — PURE override-provenance detector (Stage 1, ui-overrides.ts).
 *
 * Verifies the four override points (direction / padding / gap / color) emit a
 * notice ONLY on a MEANINGFUL deviation (explicit value differs from the default
 * the resolver would have produced), the HONEST-ABSENT behaviour (no trigger
 * fact → no notice), nested-child detection with the structural nodeId path, and
 * that the detector never mutates the input tree.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { detectOverrides } from '../src/ui-overrides.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

// Same stub pattern as ui-resolve.test.ts: register stand-ins with the CATEGORY
// that yields the right schemaKind.
//   Box → layout → container ; Text/Heading → display → text.
beforeAll(() => {
  const base = {
    component: null as unknown as never,
    props: [],
    hasChildren: true,
    description: 'stub',
  };
  registerComponent('Box', { ...base, name: 'Box', category: 'layout' });
  registerComponent('Text', { ...base, name: 'Text', category: 'display' });
  registerComponent('Heading', { ...base, name: 'Heading', category: 'display' });
});

const box = (extra: Partial<CanvasNodeLike> = {}, children: CanvasNodeLike[] = []): CanvasNodeLike => ({
  id: 'b', type: 'Box', props: {}, children, ...extra,
});
const twoKids: CanvasNodeLike[] = [
  { id: 'c1', type: 'Text', props: {} },
  { id: 'c2', type: 'Text', props: {} },
];

// ── direction (ui_layout_stack_below_md) ───────────────────────────────────────
describe('detectOverrides — direction', () => {
  it('explicit responsive.direction that DIFFERS from column-below-md → notice', () => {
    // default at base = column; author forces row at base → meaningful deviation
    const tree = box({ responsive: { direction: { base: 'row' } } }, twoKids);
    const notices = detectOverrides(tree, { viewport: { width: 500 } }); // base
    expect(notices).toHaveLength(1);
    const n = notices[0];
    expect(n.attr).toBe('direction');
    expect(n.practiceName).toBe('ui_layout_stack_below_md');
    expect(n.defaultValue).toBe('column');
    expect(n.explicitValue).toBe('row');
    expect(n.nodeType).toBe('Box');
    expect(n.rationale).toMatch(/stacks to a column below md/);
  });

  it('explicit responsive.direction that EQUALS the default → NO notice', () => {
    // default at base = column; author also says column → not a real override
    const tree = box({ responsive: { direction: { base: 'column' } } }, twoKids);
    expect(detectOverrides(tree, { viewport: { width: 500 } })).toHaveLength(0);
  });

  it('no viewport fact → direction override NOT reported (honest-absent)', () => {
    const tree = box({ responsive: { direction: { base: 'row' } } }, twoKids);
    expect(detectOverrides(tree, {})).toHaveLength(0);
  });

  it('single-child container is not a stack-default site → NO notice', () => {
    const tree = box({ responsive: { direction: { base: 'row' } } }, [
      { id: 'only', type: 'Text', props: {} },
    ]);
    expect(detectOverrides(tree, { viewport: { width: 500 } })).toHaveLength(0);
  });

  it('at md the default is row, so an explicit row is NOT a deviation', () => {
    const tree = box({ responsive: { direction: { base: 'row', md: 'row' } } }, twoKids);
    expect(detectOverrides(tree, { viewport: { width: 900 } })).toHaveLength(0); // md → default row
  });
});

// ── padding / gap (ui_density_*_scale) ─────────────────────────────────────────
describe('detectOverrides — density padding/gap', () => {
  it('explicit responsive.gap differing from the density default → notice', () => {
    // compact gap default = 4px; author says 30px → deviation
    const tree = box({ responsive: { gap: { base: '30px' } } });
    const notices = detectOverrides(tree, { density: { level: 'compact' }, viewport: { width: 500 } });
    expect(notices).toHaveLength(1);
    expect(notices[0].attr).toBe('gap');
    expect(notices[0].practiceName).toBe('ui_density_gap_scale');
    expect(notices[0].defaultValue).toBe('4px');
    expect(notices[0].explicitValue).toBe('30px');
    expect(notices[0].rationale).toMatch(/Gap scales with the active display density/);
  });

  it('explicit responsive.gap that EQUALS the density default → NO notice', () => {
    // compact gap default = 4px; author also says 4px → not a real override
    const tree = box({ responsive: { gap: { base: '4px' } } });
    expect(detectOverrides(tree, { density: { level: 'compact' }, viewport: { width: 500 } })).toHaveLength(0);
  });

  it('explicit responsive.padding differing from the density default → notice', () => {
    // spacious padding default = 16px; author says 20px at md → deviation
    const tree = box({ responsive: { padding: { base: '2px', md: '20px' } } });
    const notices = detectOverrides(tree, { density: { level: 'spacious' }, viewport: { width: 900 } });
    expect(notices).toHaveLength(1);
    expect(notices[0].attr).toBe('padding');
    expect(notices[0].practiceName).toBe('ui_density_padding_scale');
    expect(notices[0].defaultValue).toBe('16px');
    expect(notices[0].explicitValue).toBe('20px');
  });

  it('no density fact → padding/gap overrides NOT reported (honest-absent)', () => {
    const tree = box({ responsive: { gap: { base: '30px' }, padding: { base: '99px' } } });
    expect(detectOverrides(tree, { viewport: { width: 500 } })).toHaveLength(0);
  });

  it('reports BOTH padding and gap when both deviate', () => {
    const tree = box({ responsive: { gap: { base: '30px' }, padding: { base: '99px' } } });
    const notices = detectOverrides(tree, { density: { level: 'comfortable' }, viewport: { width: 500 } });
    expect(notices.map((n) => n.attr).sort()).toEqual(['gap', 'padding']);
  });
});

// ── color (ui_theme_text_color) ────────────────────────────────────────────────
describe('detectOverrides — theme color', () => {
  const text = (extra: Partial<CanvasNodeLike> = {}): CanvasNodeLike => ({
    id: 't', type: 'Text', props: {}, ...extra,
  });

  it('explicit props.color differing from the token color → notice', () => {
    // fg light = #111111; author forces #123456 → deviation
    const tree = text({ themeToken: 'fg', props: { color: '#123456' } });
    const notices = detectOverrides(tree, { theme: { mode: 'light' } });
    expect(notices).toHaveLength(1);
    expect(notices[0].attr).toBe('color');
    expect(notices[0].practiceName).toBe('ui_theme_text_color');
    expect(notices[0].defaultValue).toBe('#111111');
    expect(notices[0].explicitValue).toBe('#123456');
    expect(notices[0].rationale).toMatch(/derived from your semantic theme token/);
  });

  it('explicit props.color that EQUALS the token color → NO notice', () => {
    const tree = text({ themeToken: 'fg', props: { color: '#111111' } });
    expect(detectOverrides(tree, { theme: { mode: 'light' } })).toHaveLength(0);
  });

  it('no theme fact → color override NOT reported (honest-absent)', () => {
    const tree = text({ themeToken: 'fg', props: { color: '#123456' } });
    expect(detectOverrides(tree, {})).toHaveLength(0);
  });

  it('explicit color but NO themeToken → no default would apply → NO notice', () => {
    const tree = text({ props: { color: '#123456' } });
    expect(detectOverrides(tree, { theme: { mode: 'light' } })).toHaveLength(0);
  });

  it('unknown token → default not determinable → NO notice', () => {
    const tree = text({ themeToken: 'no-such-token', props: { color: '#123456' } });
    expect(detectOverrides(tree, { theme: { mode: 'light' } })).toHaveLength(0);
  });

  it('honours facts.theme.tokens override when computing the default', () => {
    // token override makes the default #abcdef; author's #123456 still deviates
    const tree = text({ themeToken: 'accent', props: { color: '#123456' } });
    const notices = detectOverrides(tree, { theme: { mode: 'light', tokens: { accent: { light: '#abcdef' } } } });
    expect(notices).toHaveLength(1);
    expect(notices[0].defaultValue).toBe('#abcdef');
  });

  it('explicit color equal to the OVERRIDDEN token color → NO notice', () => {
    const tree = text({ themeToken: 'accent', props: { color: '#abcdef' } });
    expect(detectOverrides(tree, { theme: { mode: 'light', tokens: { accent: { light: '#abcdef' } } } })).toHaveLength(0);
  });
});

// ── nested children + nodeId scheme ────────────────────────────────────────────
describe('detectOverrides — nesting & nodeId', () => {
  it('detects a deviating child deep in the tree with its own id', () => {
    const tree = box({ id: 'outer' }, [
      box({ id: 'inner' }, [
        { id: 'deepText', type: 'Text', props: { color: '#123456' }, themeToken: 'fg' },
      ]),
    ]);
    const notices = detectOverrides(tree, { theme: { mode: 'light' } });
    expect(notices).toHaveLength(1);
    expect(notices[0].nodeId).toBe('deepText'); // node.id wins
    expect(notices[0].attr).toBe('color');
  });

  it('falls back to a structural path when a node has no id', () => {
    const child: CanvasNodeLike = { type: 'Text', props: { color: '#123456' }, themeToken: 'fg' };
    // root has an id, the deviating child does not → path from the child's position
    const tree = box({ id: 'root-box' }, [
      { id: 'sib', type: 'Text', props: {} },
      child,
    ]);
    const notices = detectOverrides(tree, { theme: { mode: 'light' } });
    expect(notices).toHaveLength(1);
    expect(notices[0].nodeId).toBe('root/children/1'); // index 1, no id → structural path
  });
});

// ── purity ─────────────────────────────────────────────────────────────────────
describe('detectOverrides — purity', () => {
  it('does NOT mutate the input tree (deep-equal before/after)', () => {
    const tree = box({ responsive: { gap: { base: '30px' } } }, [
      { id: 't', type: 'Text', props: { color: '#123456' }, themeToken: 'fg' },
    ]);
    const snapshot = JSON.parse(JSON.stringify(tree));
    detectOverrides(tree, {
      density: { level: 'compact' },
      viewport: { width: 500 },
      theme: { mode: 'light' },
    });
    expect(tree).toEqual(snapshot);
  });

  it('returns [] for an unregistered node type (nothing to override)', () => {
    const tree: CanvasNodeLike = {
      id: 'x', type: 'NotRegistered', props: { color: '#123456' }, themeToken: 'fg',
    };
    expect(detectOverrides(tree, { theme: { mode: 'light' } })).toEqual([]);
  });
});
