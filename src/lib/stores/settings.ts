// Settings store — PluresDB-backed settings API
//
// Implements the SettingsAPI interface with reactive pub/sub.
// Storage is proxied through a PluresDB-scoped key namespace so that
// migration to real PluresDB only requires swapping the persistence layer.

import { browser } from '$app/environment';
import type { SettingsAPI } from '$lib/types/plugin.js';

const DB_PREFIX = 'pluresdb:setting:';

const subscribers = new Map<string, Set<(value: unknown) => void>>();

function load(key: string): unknown {
	if (!browser) return undefined;
	const raw = localStorage.getItem(`${DB_PREFIX}${key}`);
	return raw !== null ? JSON.parse(raw) : undefined;
}

function persist(key: string, value: unknown): void {
	if (!browser) return;
	localStorage.setItem(`${DB_PREFIX}${key}`, JSON.stringify(value));
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
	if (!browser) return;
	const keys: string[] = [];
	for (let i = 0; i < localStorage.length; i++) {
		const k = localStorage.key(i);
		if (k?.startsWith(DB_PREFIX)) keys.push(k);
	}
	keys.forEach((k) => localStorage.removeItem(k));
	subscribers.clear();
}

/** Snapshot all persisted settings as a plain key→value record. */
export function exportSettings(): Record<string, unknown> {
	if (!browser) return {};
	const data: Record<string, unknown> = {};
	for (let i = 0; i < localStorage.length; i++) {
		const k = localStorage.key(i);
		if (k?.startsWith(DB_PREFIX)) {
			const raw = localStorage.getItem(k);
			if (raw !== null) data[k.slice(DB_PREFIX.length)] = JSON.parse(raw);
		}
	}
	return data;
}

/** Restore settings from a plain key→value snapshot. */
export function importSettings(data: Record<string, unknown>): void {
	for (const [key, value] of Object.entries(data)) {
		settingsAPI.set(key, value);
	}
}
