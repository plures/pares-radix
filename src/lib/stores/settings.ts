// Settings store — PluresDB-backed settings API
//
// Implements the SettingsAPI interface with reactive pub/sub.
// All persistence is routed through the shared PluresDBGraph so that
// swapping the backend (localStorage → real PluresDB) only requires
// calling setSharedGraph() at startup — no other changes needed.

import type { SettingsAPI } from '$lib/types/plugin.js';
import { getSharedGraph, SETTING_PREFIX } from './plures-db-adapter.js';

const subscribers = new Map<string, Set<(value: unknown) => void>>();

function load(key: string): unknown {
	return getSharedGraph().get(`${SETTING_PREFIX}${key}`);
}

function persist(key: string, value: unknown): void {
	getSharedGraph().put(`${SETTING_PREFIX}${key}`, value);
}

export const settingsAPI: SettingsAPI = {
	get<T = unknown>(key: string): T | undefined {
		return load(key) as T | undefined;
	},

	set(key: string, value: unknown): void {
		persist(key, value);
		const subs = subscribers.get(key);
		if (subs) {
			for (const cb of subs) cb(value);
		}
	},

	subscribe(key: string, callback: (value: unknown) => void): () => void {
		if (!subscribers.has(key)) subscribers.set(key, new Set());
		subscribers.get(key)!.add(callback);
		return () => {
			const subs = subscribers.get(key);
			if (subs) {
				subs.delete(callback);
				if (subs.size === 0) subscribers.delete(key);
			}
		};
	},
};

/** Remove all persisted settings from the PluresDB namespace. */
export function clearAllSettings(): void {
	const graph = getSharedGraph();
	for (const k of graph.keys(SETTING_PREFIX)) {
		graph.delete(k);
	}
	subscribers.clear();
}

/** Snapshot all persisted settings as a plain key→value record. */
export function exportSettings(): Record<string, unknown> {
	const graph = getSharedGraph();
	const data: Record<string, unknown> = {};
	for (const k of graph.keys(SETTING_PREFIX)) {
		data[k.slice(SETTING_PREFIX.length)] = graph.get(k);
	}
	return data;
}

/** Restore settings from a plain key→value snapshot. */
export function importSettings(data: Record<string, unknown>): void {
	for (const [key, value] of Object.entries(data)) {
		settingsAPI.set(key, value);
	}
}
