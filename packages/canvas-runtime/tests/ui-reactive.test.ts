import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { createReactiveGraph, type PluresDBGraph } from '../src/reactive-graph.js';
import { wireResolvedTree } from '../src/ui-reactive.js';
import { attachViewportBridge, readViewport, type WindowLike } from '../src/ui-viewport-bridge.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

beforeAll(() => {
  const base = { component: null as unknown as never, props: [], hasChildren: true, description: 'stub' };
  registerComponent('Box', { ...base, name: 'Box', category: 'layout' });
  registerComponent('Button', { ...base, name: 'Button', category: 'input', hasChildren: false });
});

/** Simple in-memory PluresDBGraph for tests. */
function memGraph(): PluresDBGraph {
  const m = new Map<string, unknown>();
  return {
    put: (k, v) => { m.set(k, v); },
    get: (k) => m.get(k),
    keys: (prefix) => [...m.keys()].filter((k) => !prefix || k.startsWith(prefix)),
    delete: (k) => { m.delete(k); },
  };
}

const authoredTree = (): CanvasNodeLike => ({
  id: 'root', type: 'Box', props: {},
  responsive: { direction: { base: 'column', md: 'row' } },
  children: [
    { id: 'c1', type: 'Button', props: { label: 'A' } },
    { id: 'c2', type: 'Button', props: { label: 'B' } },
  ],
});

describe('wireResolvedTree — reactive resolution', () => {
  it('writes a resolved tree when authored + viewport are present', () => {
    const g = createReactiveGraph(memGraph());
    const detach = wireResolvedTree(g);

    g.put('canvas:tree', authoredTree());
    g.put('ui:viewport', { width: 500, height: 800, breakpoint: 'base' });

    const resolved = g.get('canvas:tree:resolved') as CanvasNodeLike;
    expect(resolved).toBeDefined();
    expect(resolved.props?.direction).toBe('column'); // base → column
    detach();
  });

  it('re-resolves when the viewport changes (column → row)', () => {
    const g = createReactiveGraph(memGraph());
    const detach = wireResolvedTree(g);
    g.put('canvas:tree', authoredTree());

    g.put('ui:viewport', { width: 500 });
    expect((g.get('canvas:tree:resolved') as CanvasNodeLike).props?.direction).toBe('column');

    g.put('ui:viewport', { width: 1000 }); // md+
    expect((g.get('canvas:tree:resolved') as CanvasNodeLike).props?.direction).toBe('row');
    detach();
  });

  it('NEVER mutates the authored tree key', () => {
    const g = createReactiveGraph(memGraph());
    const detach = wireResolvedTree(g);
    const original = authoredTree();
    g.put('canvas:tree', original);
    g.put('ui:viewport', { width: 1000 });

    const authoredAfter = g.get('canvas:tree') as CanvasNodeLike;
    // authored direction intent is still the responsive map, props untouched
    expect(authoredAfter.props?.direction).toBeUndefined();
    expect(authoredAfter.responsive?.direction).toEqual({ base: 'column', md: 'row' });
    // and the resolved copy is a different object
    expect(g.get('canvas:tree:resolved')).not.toBe(authoredAfter);
    detach();
  });

  it('does nothing when there is no authored tree yet', () => {
    const g = createReactiveGraph(memGraph());
    const detach = wireResolvedTree(g);
    g.put('ui:viewport', { width: 1000 });
    expect(g.get('canvas:tree:resolved')).toBeUndefined();
    detach();
  });

  it('detach stops further resolution', () => {
    const g = createReactiveGraph(memGraph());
    const detach = wireResolvedTree(g);
    g.put('canvas:tree', authoredTree());
    g.put('ui:viewport', { width: 500 });
    detach();
    g.put('ui:viewport', { width: 1000 }); // after detach — should NOT update
    expect((g.get('canvas:tree:resolved') as CanvasNodeLike).props?.direction).toBe('column');
  });
});

describe('attachViewportBridge — edge IO', () => {
  function fakeWindow(initialWidth: number): WindowLike & { fire: () => void; setWidth: (w: number) => void } {
    let width = initialWidth;
    const listeners: Array<() => void> = [];
    return {
      get innerWidth() { return width; },
      innerHeight: 720,
      addEventListener: (_t, l) => { listeners.push(l); },
      removeEventListener: (_t, l) => {
        const i = listeners.indexOf(l);
        if (i >= 0) listeners.splice(i, 1);
      },
      fire: () => { for (const l of [...listeners]) l(); },
      setWidth: (w: number) => { width = w; },
    };
  }

  it('seeds ui:viewport immediately with the correct breakpoint', () => {
    const g = memGraph();
    const win = fakeWindow(500);
    const detach = attachViewportBridge(g, { win });
    const vp = g.get('ui:viewport') as { width: number; breakpoint: string };
    expect(vp.width).toBe(500);
    expect(vp.breakpoint).toBe('base');
    detach();
  });

  it('updates ui:viewport on resize', () => {
    const g = memGraph();
    const win = fakeWindow(500);
    const detach = attachViewportBridge(g, { win });
    win.setWidth(1100);
    win.fire();
    const vp = g.get('ui:viewport') as { width: number; breakpoint: string };
    expect(vp.width).toBe(1100);
    expect(vp.breakpoint).toBe('lg');
    detach();
  });

  it('detach removes the resize listener', () => {
    const g = memGraph();
    const win = fakeWindow(500);
    const detach = attachViewportBridge(g, { win });
    detach();
    win.setWidth(1100);
    win.fire(); // no listener now
    expect((g.get('ui:viewport') as { width: number }).width).toBe(500); // unchanged
  });

  it('no-ops safely when there is no window', () => {
    const g = memGraph();
    // pass an options.win that is undefined and ensure globalThis has no innerWidth
    const detach = attachViewportBridge(g, { win: undefined });
    expect(g.get('ui:viewport')).toBeUndefined();
    expect(() => detach()).not.toThrow();
  });

  it('readViewport derives breakpoint from width', () => {
    const win = fakeWindow(800);
    expect(readViewport(win)).toEqual({ width: 800, height: 720, breakpoint: 'md' });
  });

  it('bridge + wiring end-to-end: resize drives resolved tree', () => {
    const g = createReactiveGraph(memGraph());
    const detachWire = wireResolvedTree(g);
    g.put('canvas:tree', authoredTree());
    const win = fakeWindow(500);
    const detachBridge = attachViewportBridge(g, { win });

    // seeded at 500 → column
    expect((g.get('canvas:tree:resolved') as CanvasNodeLike).props?.direction).toBe('column');
    // resize to desktop → row
    win.setWidth(1000);
    win.fire();
    expect((g.get('canvas:tree:resolved') as CanvasNodeLike).props?.direction).toBe('row');

    detachBridge();
    detachWire();
  });
});
