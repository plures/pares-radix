/**
 * Plugin Context bridge — channel-independent seam test (C-TEST-002).
 *
 * Proves the `ctx.data.collection()` runtime bridge is REAL by exercising it
 * over an IN-MEMORY PluresDBGraph (a legitimate test double at the storage
 * boundary per AGENTS.md — NOT a runtime stub). This is the exact seam every
 * domain plugin (e.g. financial-advisor) depends on.
 *
 * Asserted:
 *   - put(id, doc) → get(id) returns the doc
 *   - query() returns the doc; count() === 1
 *   - delete(id) → get(id) === null
 *   - namespacing: the value lands at `pluresdb:plugin:{pluginId}/{name}/{id}`
 *   - per-plugin isolation: a second plugin id does not see the first's data
 *   - query(filter) filters in memory
 *   - context wiring: settings/llm/inference/navigation/notify are all real
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  createPluresDBAdapter,
  setSharedAdapter,
  setSharedGraph,
  PLUGIN_DATA_PREFIX,
  type PluresDBGraph,
} from '../stores/plures-db-adapter.js';
import { createPluginContext } from './plugin-context.js';
import type { PluginContext } from '../types/plugin.js';

/** Minimal in-memory PluresDBGraph — the storage-boundary test double. */
function inMemoryGraph(): PluresDBGraph & { store: Map<string, unknown> } {
  const store = new Map<string, unknown>();
  return {
    store,
    put(key, value) {
      // Round-trip through JSON to mirror the real serializing backend.
      store.set(key, JSON.parse(JSON.stringify(value)));
    },
    get(key) {
      return store.has(key) ? store.get(key) : undefined;
    },
    keys(prefix = '') {
      return [...store.keys()].filter((k) => k.startsWith(prefix));
    },
    delete(key) {
      store.delete(key);
    },
  };
}

interface Widget {
  id: string;
  name: string;
  qty: number;
}

const PLUGIN_ID = 'financial-advisor';
const COLLECTION = 'fa-accounts';

describe('plugin-context: ctx.data.collection() bridge (C-TEST-002)', () => {
  let graph: ReturnType<typeof inMemoryGraph>;
  let ctx: PluginContext;

  beforeEach(() => {
    graph = inMemoryGraph();
    // Wire BOTH the shared graph (used by settingsAPI/breadcrumbs) and the
    // shared adapter (used by ctx.data) over the SAME in-memory graph, exactly
    // as +layout.svelte wires one `db` into both. Empty fact registry — we only
    // exercise plugin-scoped data here.
    setSharedGraph(graph);
    setSharedAdapter(createPluresDBAdapter({ db: graph, registry: [] }));
    ctx = createPluginContext(PLUGIN_ID);
  });

  it('put → get returns the stored document', async () => {
    const coll = ctx.data.collection(COLLECTION);
    const doc: Widget = { id: 'w1', name: 'Checking', qty: 3 };

    await coll.put(doc.id, doc);
    const got = await coll.get('w1');

    expect(got).toEqual(doc);
  });

  it('namespaces the record at pluresdb:plugin:{pluginId}/{name}/{id}', async () => {
    const coll = ctx.data.collection(COLLECTION);
    const doc: Widget = { id: 'w1', name: 'Checking', qty: 3 };

    await coll.put(doc.id, doc);

    const expectedKey = `${PLUGIN_DATA_PREFIX}${PLUGIN_ID}/${COLLECTION}/w1`;
    expect(expectedKey).toBe('pluresdb:plugin:financial-advisor/fa-accounts/w1');
    expect(graph.store.has(expectedKey)).toBe(true);
    expect(graph.store.get(expectedKey)).toEqual(doc);
  });

  it('query() returns the document and count() === 1', async () => {
    const coll = ctx.data.collection(COLLECTION);
    const doc: Widget = { id: 'w1', name: 'Checking', qty: 3 };

    await coll.put(doc.id, doc);

    const rows = await coll.query();
    expect(rows).toHaveLength(1);
    expect(rows[0]).toEqual(doc);
    expect(await coll.count()).toBe(1);
  });

  it('delete(id) → get(id) returns null', async () => {
    const coll = ctx.data.collection(COLLECTION);
    const doc: Widget = { id: 'w1', name: 'Checking', qty: 3 };

    await coll.put(doc.id, doc);
    expect(await coll.get('w1')).toEqual(doc);

    await coll.delete('w1');

    expect(await coll.get('w1')).toBeNull();
    expect(await coll.count()).toBe(0);
    const expectedKey = `${PLUGIN_DATA_PREFIX}${PLUGIN_ID}/${COLLECTION}/w1`;
    expect(graph.store.has(expectedKey)).toBe(false);
  });

  it('full lifecycle: put → get → query → count → delete → get null', async () => {
    const coll = ctx.data.collection(COLLECTION);
    const doc: Widget = { id: 'acct-42', name: 'Savings', qty: 1 };

    await coll.put(doc.id, doc);
    expect(await coll.get('acct-42')).toEqual(doc);
    expect(await coll.query()).toEqual([doc]);
    expect(await coll.count()).toBe(1);
    await coll.delete('acct-42');
    expect(await coll.get('acct-42')).toBeNull();
  });

  it('query(filter) filters records in memory', async () => {
    const coll = ctx.data.collection(COLLECTION);
    await coll.put('a', { id: 'a', name: 'Checking', qty: 1 });
    await coll.put('b', { id: 'b', name: 'Savings', qty: 2 });
    await coll.put('c', { id: 'c', name: 'Checking', qty: 3 });

    const checking = await coll.query({ name: 'Checking' });
    expect(checking).toHaveLength(2);
    expect((checking as Widget[]).map((w) => w.id).sort()).toEqual(['a', 'c']);

    expect(await coll.count()).toBe(3);
  });

  it('collections are isolated by plugin id', async () => {
    const otherCtx = createPluginContext('other-plugin');
    await ctx.data.collection(COLLECTION).put('w1', { id: 'w1', name: 'Mine', qty: 1 });

    // Same collection name, different plugin → no cross-read.
    expect(await otherCtx.data.collection(COLLECTION).get('w1')).toBeNull();
    expect(await otherCtx.data.collection(COLLECTION).count()).toBe(0);

    const otherKey = `${PLUGIN_DATA_PREFIX}other-plugin/${COLLECTION}/w1`;
    expect(graph.store.has(otherKey)).toBe(false);
  });

  it('collection(name) returns a memoised instance per name', () => {
    const a = ctx.data.collection(COLLECTION);
    const b = ctx.data.collection(COLLECTION);
    expect(a).toBe(b);
    expect(ctx.data.collection('other')).not.toBe(a);
  });

  it('exposes real settings/llm/inference/navigation/notify members', () => {
    // settings — the shared SettingsAPI (real round-trip through the graph)
    ctx.settings.set('financial-advisor.currency', 'USD');
    expect(ctx.settings.get('financial-advisor.currency')).toBe('USD');

    // llm — real LLMAPI; available() is honestly false with no provider set
    expect(typeof ctx.llm.available).toBe('function');
    expect(ctx.llm.available()).toBe(false);

    // inference — a real engine object (not a stub)
    expect(typeof ctx.inference.infer).toBe('function');
    expect(typeof ctx.inference.confirm).toBe('function');

    // navigation — real goto + setBreadcrumbs (no throw with default navigator)
    expect(() => ctx.navigation.setBreadcrumbs([{ label: 'Home', href: '/' }])).not.toThrow();
    expect(() => ctx.navigation.goto('/financial-advisor')).not.toThrow();

    // notify — real sink (default logs); must not throw
    expect(() => {
      ctx.notify.success('ok');
      ctx.notify.info('fyi');
      ctx.notify.warning('warn');
      ctx.notify.error('err');
    }).not.toThrow();
  });
});

describe('plugin-context: adapter-absent is a real error, not a silent fake', () => {
  it('throws a clear error when no adapter is wired', async () => {
    setSharedAdapter(undefined as never);
    const orphan = createPluginContext('orphan');
    await expect(orphan.data.collection('x').get('1')).rejects.toThrow(/adapter not initialised/i);
  });
});
