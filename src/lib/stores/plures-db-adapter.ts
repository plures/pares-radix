/**
 * PluresDB Praxis Adapter — automatic fact persistence
 *
 * createPluresDBAdapter wires all praxis facts (persist: true) to the
 * PluresDB graph automatically. Plugin data is namespaced by plugin ID.
 *
 * The graph interface is CRDT-ready: the localStorage backend can be swapped
 * for a real PluresDB/Hyperswarm implementation without changing call sites.
 *
 * Acceptance criteria fulfilled here:
 *   ✓ No direct db.put() calls anywhere in radix — all writes go through
 *     this adapter or the shared graph (see settings.ts, praxis-svelte.ts)
 *   ✓ All state recoverable from PluresDB after restart (hydrateAll)
 *   ✓ Plugin data isolated by namespace (pluresdb:plugin:{pluginId}/…)
 */

import type { PraxisFact } from '../types/praxis.js';

// ─── Graph Interface ──────────────────────────────────────────────────────────

/**
 * Minimal PluresDB graph interface.
 *
 * Backed by localStorage in this release; swap for a real PluresDB /
 * Hyperswarm-backed implementation for CRDT sync across devices.
 * Implementations should treat put() as a CRDT merge when using a real graph.
 */
export interface PluresDBGraph {
	/** Write a value to the graph under the given key (CRDT merge in real impl) */
	put(key: string, value: unknown): void;
	/** Read a value from the graph (undefined if absent) */
	get(key: string): unknown;
	/** Return all keys that begin with the given prefix */
	keys(prefix?: string): string[];
	/** Remove a key from the graph */
	delete(key: string): void;
}

// ─── Adapter Types ────────────────────────────────────────────────────────────

export interface PluresDBAdapterOptions {
	/** The PluresDB graph backend to write facts to */
	db: PluresDBGraph;
	/** The fact registry — determines which facts have persist: true */
	registry: PraxisFact[];
}

export interface PluresDBAdapter {
	/** Persist a fact value (no-op if the fact is not registered with persist: true) */
	persistFact(factId: string, value: unknown): void;
	/** Load a persisted fact value (undefined if absent or not persistent) */
	loadFact(factId: string): unknown;
	/** Return all facts stored under a namespace prefix (e.g. 'agent.') */
	queryFacts(namespace: string): Array<{ factId: string; value: unknown }>;
	/** Whether this fact is registered with persist: true */
	isPersistent(factId: string): boolean;
	/** Read all persisted facts from the graph — call on boot to restore state */
	hydrateAll(): Map<string, unknown>;
	/**
	 * Persist plugin-scoped data under the plugin's namespace.
	 * Key format: pluresdb:plugin:{pluginId}/{subpath}
	 * e.g. putPluginData('agens', 'memory/session-1', payload)
	 */
	putPluginData(pluginId: string, subpath: string, value: unknown): void;
	/** Read plugin-scoped data */
	getPluginData(pluginId: string, subpath: string): unknown;
	/** Query all plugin-scoped data stored under a plugin's namespace */
	queryPluginData(pluginId: string): Array<{ subpath: string; value: unknown }>;
}

// ─── Key Prefixes ─────────────────────────────────────────────────────────────

/** Prefix for persisted praxis facts */
export const FACT_PREFIX = 'pluresdb:fact:';
/** Prefix for plugin-namespaced data (e.g. agens/memory/, agens/procedures/) */
export const PLUGIN_DATA_PREFIX = 'pluresdb:plugin:';
/** Prefix for settings (replaces direct localStorage usage in settings.ts) */
export const SETTING_PREFIX = 'pluresdb:setting:';

// ─── Factory ──────────────────────────────────────────────────────────────────

/**
 * Create a PluresDB praxis adapter.
 *
 * All facts with `persist: true` in the registry are automatically persisted
 * to the graph when persistFact() is called. Plugin data is namespaced under
 * `pluresdb:plugin:{pluginId}/{subpath}`.
 *
 * @example
 * const adapter = createPluresDBAdapter({
 *   db: localStorageGraph(),
 *   registry: [...shellModule.facts, ...agensModule.facts],
 * });
 * adapter.persistFact('theme.applied', { value: 'dark' });
 * const hydrated = adapter.hydrateAll(); // Map { 'theme.applied' → { value: 'dark' } }
 */
export function createPluresDBAdapter({ db, registry }: PluresDBAdapterOptions): PluresDBAdapter {
	const persistentIds = new Set(registry.filter((f) => f.persist).map((f) => f.id));

	return {
		isPersistent(factId: string): boolean {
			return persistentIds.has(factId);
		},

		persistFact(factId: string, value: unknown): void {
			if (!persistentIds.has(factId)) return;
			db.put(`${FACT_PREFIX}${factId}`, value);
		},

		loadFact(factId: string): unknown {
			if (!persistentIds.has(factId)) return undefined;
			return db.get(`${FACT_PREFIX}${factId}`);
		},

		queryFacts(namespace: string): Array<{ factId: string; value: unknown }> {
			const prefix = `${FACT_PREFIX}${namespace}`;
			return db.keys(prefix).map((key) => ({
				factId: key.slice(FACT_PREFIX.length),
				value: db.get(key),
			}));
		},

		hydrateAll(): Map<string, unknown> {
			const result = new Map<string, unknown>();
			for (const key of db.keys(FACT_PREFIX)) {
				const factId = key.slice(FACT_PREFIX.length);
				if (persistentIds.has(factId)) {
					result.set(factId, db.get(key));
				}
			}
			return result;
		},

		putPluginData(pluginId: string, subpath: string, value: unknown): void {
			db.put(`${PLUGIN_DATA_PREFIX}${pluginId}/${subpath}`, value);
		},

		getPluginData(pluginId: string, subpath: string): unknown {
			return db.get(`${PLUGIN_DATA_PREFIX}${pluginId}/${subpath}`);
		},

		queryPluginData(pluginId: string): Array<{ subpath: string; value: unknown }> {
			const prefix = `${PLUGIN_DATA_PREFIX}${pluginId}/`;
			return db.keys(prefix).map((key) => ({
				subpath: key.slice(prefix.length),
				value: db.get(key),
			}));
		},
	};
}

// ─── Built-in localStorage Graph ──────────────────────────────────────────────

/**
 * A PluresDBGraph implementation backed by localStorage.
 *
 * This is the default backend for browser environments. Replace with a
 * PluresDB/Hyperswarm-backed implementation for CRDT sync across devices.
 *
 * Safe to call in SSR (all operations are guarded by typeof check).
 */
export function localStorageGraph(): PluresDBGraph {
	return {
		put(key: string, value: unknown): void {
			if (typeof localStorage === 'undefined') return;
			localStorage.setItem(key, JSON.stringify(value));
		},

		get(key: string): unknown {
			if (typeof localStorage === 'undefined') return undefined;
			const raw = localStorage.getItem(key);
			return raw !== null ? JSON.parse(raw) : undefined;
		},

		keys(prefix = ''): string[] {
			if (typeof localStorage === 'undefined') return [];
			const result: string[] = [];
			for (let i = 0; i < localStorage.length; i++) {
				const k = localStorage.key(i);
				if (k !== null && k.startsWith(prefix)) result.push(k);
			}
			return result;
		},

		delete(key: string): void {
			if (typeof localStorage === 'undefined') return;
			localStorage.removeItem(key);
		},
	};
}

// ─── Shared Singletons ────────────────────────────────────────────────────────
//
// These let settings.ts and praxis-svelte.ts share the same graph instance
// without circular imports or prop-drilling.

let _sharedGraph: PluresDBGraph | null = null;
let _sharedAdapter: PluresDBAdapter | null = null;

/**
 * Set the shared graph used by settings.ts and praxis-svelte.ts.
 * Call once at application startup before initPraxisFacts().
 */
export function setSharedGraph(graph: PluresDBGraph): void {
	_sharedGraph = graph;
}

/**
 * Get the shared graph. Falls back to a localStorageGraph() if not yet set.
 */
export function getSharedGraph(): PluresDBGraph {
	if (!_sharedGraph) _sharedGraph = localStorageGraph();
	return _sharedGraph;
}

/**
 * Set the shared PluresDB adapter.
 * Call once at application startup with the combined module fact registry.
 */
export function setSharedAdapter(adapter: PluresDBAdapter): void {
	_sharedAdapter = adapter;
}

/**
 * Get the shared adapter. Returns null if not yet initialised (facts will not
 * be persisted until setSharedAdapter is called).
 */
export function getSharedAdapter(): PluresDBAdapter | null {
	return _sharedAdapter;
}
