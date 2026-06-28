import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { resolveUiTree } from '../src/ui-resolve.js';
import { THEME_TOKENS } from '../src/ui-practices.js';
import { DEMO_CANVAS_TREE } from '../src/demo-canvas.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

/**
 * Behavior test for the SHARED demo canvas tree (src/demo-canvas.ts) — the SAME
 * tree the /canvas app surface loads. This proves the wiring produces the right
 * RESOLVED tree at two viewports (and a theme), so the living example stays a
 * faithful exercise of the engine rather than "open the app and look".
 *
 * Registry: register the real categories so schemaKind inference matches the app
 *   Box → layout → container ; Text/Heading → display → text.
 *
 * NB: resolveUiTree reads the registry, so every resolve call lives INSIDE an
 * `it`/helper that runs AFTER beforeAll registers the components.
 */
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

/** Depth-first find a node by id in a resolved tree. */
function findById(node: CanvasNodeLike, id: string): CanvasNodeLike | undefined {
  if (node.id === id) return node;
  for (const child of node.children ?? []) {
    const hit = findById(child, id);
    if (hit) return hit;
  }
  return undefined;
}

const resolveNarrow = () =>
  resolveUiTree(DEMO_CANVAS_TREE as CanvasNodeLike, { viewport: { width: 375 } });

const resolveWideDark = () =>
  resolveUiTree(DEMO_CANVAS_TREE as CanvasNodeLike, {
    viewport: { width: 1280 },
    theme: { mode: 'dark' },
  });

describe('demo canvas — narrow viewport (375px, base breakpoint)', () => {
  it('root stacks as a column on mobile', () => {
    expect(resolveNarrow().props?.direction).toBe('column');
  });

  it('root uses the small (base) gap on mobile', () => {
    expect(resolveNarrow().props?.gap).toBe('8px');
  });

  it('root uses the base padding on mobile (lg padding NOT yet applied)', () => {
    expect(resolveNarrow().props?.padding).toBe('8px');
  });

  it('the sidebar is hidden on mobile', () => {
    const sidebar = findById(resolveNarrow(), 'demo-sidebar');
    expect(sidebar?.props?.hidden).toBe(true);
  });

  it('does NOT mutate the authored tree', () => {
    resolveNarrow();
    // Authored intent stays pristine — no concrete props leaked back onto source.
    expect((DEMO_CANVAS_TREE.props as Record<string, unknown>)?.direction).toBeUndefined();
  });
});

describe('demo canvas — wide viewport (1280px, xl) + dark theme', () => {
  it('root becomes a row at md+ (md value cascades up to xl)', () => {
    expect(resolveWideDark().props?.direction).toBe('row');
  });

  it('root uses the wide (md) gap when wide', () => {
    expect(resolveWideDark().props?.gap).toBe('24px');
  });

  it('root uses the lg padding when wide', () => {
    expect(resolveWideDark().props?.padding).toBe('32px');
  });

  it('the sidebar is visible at md+ (hidden resolves false)', () => {
    const sidebar = findById(resolveWideDark(), 'demo-sidebar');
    expect(sidebar?.props?.hidden).toBe(false);
  });

  it('a themed (fg) Text resolves to the dark-mode color from THEME_TOKENS', () => {
    const body = findById(resolveWideDark(), 'demo-body');
    expect(body?.props?.color).toBe(THEME_TOKENS.fg.dark);
  });

  it('an accent-themed Text resolves to the dark-mode accent color', () => {
    const accent = findById(resolveWideDark(), 'demo-accent');
    expect(accent?.props?.color).toBe(THEME_TOKENS.accent.dark);
  });
});

describe('demo canvas — theme reaction (light vs dark on the same node)', () => {
  it('the fg body text flips between the two THEME_TOKENS.fg colors', () => {
    const light = resolveUiTree(DEMO_CANVAS_TREE as CanvasNodeLike, {
      viewport: { width: 1280 },
      theme: { mode: 'light' },
    });
    const dark = resolveUiTree(DEMO_CANVAS_TREE as CanvasNodeLike, {
      viewport: { width: 1280 },
      theme: { mode: 'dark' },
    });
    expect(findById(light, 'demo-body')?.props?.color).toBe(THEME_TOKENS.fg.light);
    expect(findById(dark, 'demo-body')?.props?.color).toBe(THEME_TOKENS.fg.dark);
  });
});
