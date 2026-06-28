import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { resolveUiTree } from '../src/ui-resolve.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

// Register stand-ins with the right CATEGORY so schemaKind inference works:
//   Box → layout → container ; Text → display → text ; Button → input → control.
beforeAll(() => {
  const base = {
    component: null as unknown as never,
    props: [],
    hasChildren: true,
    description: 'stub',
  };
  registerComponent('Box', { ...base, name: 'Box', category: 'layout' });
  registerComponent('DashboardGrid', { ...base, name: 'DashboardGrid', category: 'layout' });
  registerComponent('Text', { ...base, name: 'Text', category: 'display' });
  registerComponent('Button', { ...base, name: 'Button', category: 'input', hasChildren: false });
});

const box = (extra: Partial<CanvasNodeLike> = {}, children: CanvasNodeLike[] = []): CanvasNodeLike => ({
  id: 'b', type: 'Box', props: {}, children, ...extra,
});

describe('resolveUiTree — purity / identity', () => {
  it('returns a structurally equal clone when there are no facts', () => {
    const tree = box({ props: { direction: 'row' } }, [
      { id: 'c1', type: 'Button', props: { label: 'A' } },
    ]);
    const out = resolveUiTree(tree);
    expect(out).toEqual(tree);
    expect(out).not.toBe(tree); // it's a clone
  });

  it('NEVER mutates the authored tree (deep-equal before/after)', () => {
    const tree = box(
      { responsive: { direction: { base: 'column', md: 'row' } } },
      [
        { id: 'c1', type: 'Button', props: { label: 'A' } },
        { id: 'c2', type: 'Button', props: { label: 'B' } },
      ],
    );
    const snapshot = JSON.parse(JSON.stringify(tree));
    resolveUiTree(tree, { viewport: { width: 1200 } });
    expect(tree).toEqual(snapshot); // authored intent pristine
  });
});

describe('resolveUiTree — responsive collapse', () => {
  it('responsive.direction → column below md, row at md+', () => {
    const tree = box({ responsive: { direction: { base: 'column', md: 'row' } } });

    const narrow = resolveUiTree(tree, { viewport: { width: 500 } }); // base
    expect(narrow.props?.direction).toBe('column');

    const wide = resolveUiTree(tree, { viewport: { width: 900 } }); // md
    expect(wide.props?.direction).toBe('row');
  });

  it('responsive.gap collapses with mobile-first fallback', () => {
    const tree = box({ responsive: { gap: { base: '8px', lg: '24px' } } });
    expect(resolveUiTree(tree, { viewport: { width: 800 } }).props?.gap).toBe('8px'); // md falls back to base
    expect(resolveUiTree(tree, { viewport: { width: 1100 } }).props?.gap).toBe('24px'); // lg
  });

  it('responsive.hidden resolves a boolean per breakpoint', () => {
    const tree = box({ responsive: { hidden: { base: true, lg: false } } });
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.hidden).toBe(true);
    expect(resolveUiTree(tree, { viewport: { width: 1300 } }).props?.hidden).toBe(false);
  });

  it('responsive.columns reflows a grid container', () => {
    const tree: CanvasNodeLike = {
      id: 'g', type: 'DashboardGrid', props: {},
      responsive: { columns: { base: 1, md: 2, xl: 4 } },
    };
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.columns).toBe(1);
    expect(resolveUiTree(tree, { viewport: { width: 800 } }).props?.columns).toBe(2);
    expect(resolveUiTree(tree, { viewport: { width: 1400 } }).props?.columns).toBe(4);
  });

  it('responsive.size resolves on text kind', () => {
    const tree: CanvasNodeLike = {
      id: 't', type: 'Text', props: {},
      responsive: { size: { base: '14px', lg: '18px' } },
    };
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.size).toBe('14px');
    expect(resolveUiTree(tree, { viewport: { width: 1100 } }).props?.size).toBe('18px');
  });

  it('does nothing for a responsive attr when no viewport is given', () => {
    const tree = box({ responsive: { direction: { base: 'column', md: 'row' } } });
    const out = resolveUiTree(tree, {}); // no viewport
    expect(out.props?.direction).toBeUndefined();
  });
});

describe('resolveUiTree — type-based default stacking', () => {
  it('multi-child container with no explicit direction stacks below md', () => {
    const tree = box({}, [
      { id: 'c1', type: 'Button', props: { label: 'A' } },
      { id: 'c2', type: 'Button', props: { label: 'B' } },
    ]);
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.direction).toBe('column'); // base → column
    expect(resolveUiTree(tree, { viewport: { width: 900 } }).props?.direction).toBe('row'); // md → row
  });

  it('explicit responsive.direction WINS over the default', () => {
    const tree = box(
      { responsive: { direction: { base: 'row' } } }, // author insists on row even on mobile
      [
        { id: 'c1', type: 'Button', props: { label: 'A' } },
        { id: 'c2', type: 'Button', props: { label: 'B' } },
      ],
    );
    // default would say column at 500; explicit responsive says row → row wins
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.direction).toBe('row');
  });

  it('single-child container is NOT stacked by the default', () => {
    const tree = box({}, [{ id: 'c1', type: 'Button', props: { label: 'A' } }]);
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.direction).toBeUndefined();
  });
});

describe('resolveUiTree — nesting & unknown nodes', () => {
  it('resolves nested containers recursively', () => {
    const tree = box({ responsive: { gap: { base: '4px', md: '12px' } } }, [
      box({ id: 'inner', responsive: { direction: { base: 'column', md: 'row' } } } as Partial<CanvasNodeLike>, [
        { id: 'c1', type: 'Button', props: { label: 'A' } },
      ]),
    ]);
    const out = resolveUiTree(tree, { viewport: { width: 900 } });
    expect(out.props?.gap).toBe('12px');
    expect(out.children?.[0].props?.direction).toBe('row');
  });

  it('leaves unregistered node types untouched', () => {
    const tree: CanvasNodeLike = {
      id: 'x', type: 'NotRegistered', props: {},
      responsive: { direction: { base: 'column', md: 'row' } },
    };
    const out = resolveUiTree(tree, { viewport: { width: 900 } });
    expect(out.props?.direction).toBeUndefined(); // no kind → no resolution
  });
});
