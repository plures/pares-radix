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

describe('resolveUiTree — density (ui:density) scales container spacing', () => {
  it('compact tightens padding + gap on a container', () => {
    const tree = box({});
    const out = resolveUiTree(tree, { density: { level: 'compact' } });
    expect(out.props?.padding).toBe('4px');
    expect(out.props?.gap).toBe('4px');
  });

  it('comfortable is the baseline spacing', () => {
    const out = resolveUiTree(box({}), { density: { level: 'comfortable' } });
    expect(out.props?.padding).toBe('8px');
    expect(out.props?.gap).toBe('8px');
  });

  it('spacious loosens padding + gap', () => {
    const out = resolveUiTree(box({}), { density: { level: 'spacious' } });
    expect(out.props?.padding).toBe('16px');
    expect(out.props?.gap).toBe('12px');
  });

  it('does nothing when no ui:density fact is present', () => {
    const out = resolveUiTree(box({}), {});
    expect(out.props?.padding).toBeUndefined();
    expect(out.props?.gap).toBeUndefined();
  });

  it('an unknown/missing level falls back to comfortable', () => {
    const out = resolveUiTree(box({}), { density: { level: 'cozy' as unknown as 'compact' } });
    expect(out.props?.padding).toBe('8px');
    expect(out.props?.gap).toBe('8px');
  });

  it('does NOT scale text nodes (density is a container concern)', () => {
    const tree: CanvasNodeLike = { id: 't', type: 'Text', props: {} };
    const out = resolveUiTree(tree, { density: { level: 'compact' } });
    expect(out.props?.padding).toBeUndefined();
    expect(out.props?.gap).toBeUndefined();
  });

  it('applies density to nested containers recursively', () => {
    const tree = box({}, [box({ id: 'inner' }, [{ id: 'c', type: 'Button', props: { label: 'A' } }])]);
    const out = resolveUiTree(tree, { density: { level: 'spacious' } });
    expect(out.props?.padding).toBe('16px');
    expect(out.children?.[0].props?.padding).toBe('16px');
  });
});

describe('resolveUiTree — density precedence vs explicit responsive', () => {
  it('explicit responsive.padding WINS over the density default', () => {
    const tree = box({ responsive: { padding: { base: '2px', md: '20px' } } });
    // density would say 16px (spacious); explicit responsive says 20px at md → responsive wins
    const out = resolveUiTree(tree, {
      density: { level: 'spacious' },
      viewport: { width: 900 }, // md
    });
    expect(out.props?.padding).toBe('20px');
  });

  it('explicit responsive.gap WINS but density still fills padding', () => {
    const tree = box({ responsive: { gap: { base: '30px' } } });
    const out = resolveUiTree(tree, {
      density: { level: 'compact' },
      viewport: { width: 500 },
    });
    expect(out.props?.gap).toBe('30px'); // explicit responsive gap wins
    expect(out.props?.padding).toBe('4px'); // density still supplies padding
  });

  it('density default applies when responsive declares a DIFFERENT attribute', () => {
    // responsive.direction present, but no responsive.padding/gap → density fills both
    const tree = box({ responsive: { direction: { base: 'column', md: 'row' } } }, [
      { id: 'c1', type: 'Button', props: { label: 'A' } },
      { id: 'c2', type: 'Button', props: { label: 'B' } },
    ]);
    const out = resolveUiTree(tree, { density: { level: 'compact' }, viewport: { width: 900 } });
    expect(out.props?.direction).toBe('row'); // explicit responsive direction
    expect(out.props?.padding).toBe('4px'); // density default
    expect(out.props?.gap).toBe('4px');
  });
});

describe('resolveUiTree — theme (ui:theme) maps token → color on text', () => {
  const text = (extra: Partial<CanvasNodeLike> = {}): CanvasNodeLike => ({
    id: 't', type: 'Text', props: {}, ...extra,
  });

  it('resolves a built-in token to the light-mode color', () => {
    const out = resolveUiTree(text({ themeToken: 'fg' }), { theme: { mode: 'light' } });
    expect(out.props?.color).toBe('#111111');
  });

  it('resolves the same token to the dark-mode color', () => {
    const out = resolveUiTree(text({ themeToken: 'fg' }), { theme: { mode: 'dark' } });
    expect(out.props?.color).toBe('#f5f5f5');
  });

  it('resolves the accent token per mode', () => {
    expect(resolveUiTree(text({ themeToken: 'accent' }), { theme: { mode: 'light' } }).props?.color).toBe('#1d4ed8');
    expect(resolveUiTree(text({ themeToken: 'accent' }), { theme: { mode: 'dark' } }).props?.color).toBe('#60a5fa');
  });

  it('does nothing when the node has no themeToken', () => {
    const out = resolveUiTree(text({}), { theme: { mode: 'light' } });
    expect(out.props?.color).toBeUndefined();
  });

  it('does nothing when no ui:theme fact is present', () => {
    const out = resolveUiTree(text({ themeToken: 'fg' }), {});
    expect(out.props?.color).toBeUndefined();
  });

  it('leaves an unknown token honestly absent (no fake color)', () => {
    const out = resolveUiTree(text({ themeToken: 'no-such-token' }), { theme: { mode: 'light' } });
    expect(out.props?.color).toBeUndefined();
  });

  it('an unknown mode falls back to light', () => {
    const out = resolveUiTree(text({ themeToken: 'fg' }), {
      theme: { mode: 'sepia' as unknown as 'light' },
    });
    expect(out.props?.color).toBe('#111111');
  });

  it('facts.theme.tokens override a built-in token for the active mode', () => {
    const out = resolveUiTree(text({ themeToken: 'accent' }), {
      theme: { mode: 'light', tokens: { accent: { light: '#abcdef' } } },
    });
    expect(out.props?.color).toBe('#abcdef');
  });

  it('facts.theme.tokens can define a brand-new token', () => {
    const out = resolveUiTree(text({ themeToken: 'brand' }), {
      theme: { mode: 'dark', tokens: { brand: { light: '#000000', dark: '#00ffaa' } } },
    });
    expect(out.props?.color).toBe('#00ffaa');
  });

  it('does NOT theme container nodes (color is a text concern here)', () => {
    const out = resolveUiTree(box({ themeToken: 'fg' } as Partial<CanvasNodeLike>), { theme: { mode: 'light' } });
    expect(out.props?.color).toBeUndefined();
  });
});

describe('resolveUiTree — theme precedence vs explicit color', () => {
  it('an explicit literal props.color WINS over the theme token', () => {
    const tree: CanvasNodeLike = {
      id: 't', type: 'Text', props: { color: '#123456' }, themeToken: 'fg',
    };
    const out = resolveUiTree(tree, { theme: { mode: 'light' } });
    expect(out.props?.color).toBe('#123456'); // author override wins
  });
});

describe('resolveUiTree — density + theme + viewport compose', () => {
  it('all three facts resolve independently on the right kinds', () => {
    const tree = box(
      { responsive: { direction: { base: 'column', md: 'row' } } },
      [
        { id: 'h', type: 'Text', props: {}, themeToken: 'fg' },
        { id: 'c2', type: 'Button', props: { label: 'B' } },
      ],
    );
    const out = resolveUiTree(tree, {
      viewport: { width: 900 }, // md
      density: { level: 'compact' },
      theme: { mode: 'dark' },
    });
    // container: explicit responsive direction + density-filled spacing
    expect(out.props?.direction).toBe('row');
    expect(out.props?.padding).toBe('4px');
    expect(out.props?.gap).toBe('4px');
    // text child: theme color
    expect(out.children?.[0].props?.color).toBe('#f5f5f5');
  });

  it('NEVER mutates the authored tree when density+theme facts are present', () => {
    const tree = box({ themeToken: 'fg' } as Partial<CanvasNodeLike>, [
      { id: 't', type: 'Text', props: {}, themeToken: 'accent' },
    ]);
    const snapshot = JSON.parse(JSON.stringify(tree));
    resolveUiTree(tree, { density: { level: 'spacious' }, theme: { mode: 'dark' } });
    expect(tree).toEqual(snapshot); // authored intent pristine
  });
});
