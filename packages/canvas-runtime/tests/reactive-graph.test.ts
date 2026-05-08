import { describe, it, expect } from 'vitest';
import { createReactiveGraph } from '../src/reactive-graph.js';
import type { PluresDBGraph } from '../src/reactive-graph.js';

function createMemoryGraph(): PluresDBGraph {
  const store = new Map<string, unknown>();
  return {
    put(key, value) { store.set(key, value); },
    get(key) { return store.get(key); },
    keys(prefix = '') { return [...store.keys()].filter((k) => k.startsWith(prefix)); },
    delete(key) { store.delete(key); },
  };
}

describe('ReactiveGraph', () => {
  it('put/get works like base graph', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    graph.put('k1', 'hello');
    expect(graph.get('k1')).toBe('hello');
  });

  it('keys returns matching prefixes', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    graph.put('canvas:a', 1);
    graph.put('canvas:b', 2);
    graph.put('other:c', 3);
    expect(graph.keys('canvas:')).toEqual(['canvas:a', 'canvas:b']);
  });

  it('delete removes key and notifies', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    graph.put('k1', 'hello');

    let notified = false;
    graph.subscribe('k1', (value) => {
      if (value === undefined) notified = true;
    });

    graph.delete('k1');
    expect(graph.get('k1')).toBeUndefined();
    expect(notified).toBe(true);
  });

  it('subscribe fires immediately with current value', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    graph.put('k1', 'initial');

    let received: unknown = null;
    graph.subscribe('k1', (value) => { received = value; });
    expect(received).toBe('initial');
  });

  it('subscribe fires on put', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    const values: unknown[] = [];

    graph.subscribe('k1', (value) => { values.push(value); });
    graph.put('k1', 'first');
    graph.put('k1', 'second');

    expect(values).toEqual(['first', 'second']);
  });

  it('unsubscribe stops notifications', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    const values: unknown[] = [];

    const unsub = graph.subscribe('k1', (value) => { values.push(value); });
    graph.put('k1', 'first');
    unsub();
    graph.put('k1', 'second');

    expect(values).toEqual(['first']);
  });

  it('subscribePrefix fires for matching keys', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    const notifications: Array<[string, unknown]> = [];

    graph.subscribePrefix('canvas:', (key, value) => {
      notifications.push([key, value]);
    });

    graph.put('canvas:tree', { type: 'root' });
    graph.put('canvas:data:items', [1, 2, 3]);
    graph.put('other:key', 'ignored');

    expect(notifications).toHaveLength(2);
    expect(notifications[0][0]).toBe('canvas:tree');
    expect(notifications[1][0]).toBe('canvas:data:items');
  });

  it('subscribePrefix unsubscribe works', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    let count = 0;

    const unsub = graph.subscribePrefix('c:', (_k, _v) => { count++; });
    graph.put('c:1', 'a');
    unsub();
    graph.put('c:2', 'b');

    expect(count).toBe(1);
  });

  it('multiple subscribers on same key all fire', () => {
    const graph = createReactiveGraph(createMemoryGraph());
    let a = 0, b = 0;

    graph.subscribe('k', () => { a++; });
    graph.subscribe('k', () => { b++; });
    graph.put('k', 'val');

    expect(a).toBe(1);
    expect(b).toBe(1);
  });
});
