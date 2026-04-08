/**
 * praxis-svelte — reactive bindings for praxis facts
 *
 * Bridges the praxis shell module (facts, events) to Svelte 5 runes so that
 * UI components can reactively bind to domain state without imperative logic.
 *
 * Usage:
 *   import { query, emitFact } from '$lib/stores/praxis-svelte.js';
 *
 *   // Read a fact reactively (returns a getter — use inside $derived or template)
 *   const themeValue = $derived(query('theme.applied'));
 *
 *   // Write a fact (e.g. after a user action)
 *   emitFact('theme.applied', { value: 'dark' });
 */

import { browser } from '$app/environment';
import { getAllNavItems } from '$lib/platform/plugin-loader.js';
import type { NavItem } from '$lib/types/plugin.js';
import { getSharedAdapter } from './plures-db-adapter.js';

// ─── Reactive Fact Store ─────────────────────────────────────────────────────

/**
 * Internal reactive map of fact values keyed by fact ID.
 * Uses Svelte 5 $state so that reads inside $derived or Svelte templates
 * are automatically tracked.
 */
let facts = $state<Map<string, unknown>>(new Map());

/**
 * Read a praxis fact reactively.
 * Must be called inside a Svelte reactive context ($derived, $effect, or a
 * Svelte template) for the subscription to take effect.
 */
export function query<T = unknown>(factId: string): T | undefined {
	return facts.get(factId) as T | undefined;
}

/**
 * Write a praxis fact, notifying all reactive subscribers.
 * Facts with persist: true are automatically written to PluresDB via the
 * shared adapter (set via setSharedAdapter before initPraxisFacts).
 */
export function emitFact(factId: string, value: unknown): void {
	facts.set(factId, value);
	getSharedAdapter()?.persistFact(factId, value);
}

// ─── Theme Fact Bridge ────────────────────────────────────────────────────────

/**
 * Persistent theme fact key used in localStorage (mirrors settings store).
 * Kept separate from 'pluresdb:setting:radix.theme' so that the theme is
 * immediately available before settings hydrate.
 */
const THEME_KEY = 'radix-theme';

function loadPersistedTheme(): 'light' | 'dark' {
	if (!browser) return 'dark';
	const stored = localStorage.getItem(THEME_KEY);
	return stored === 'light' || stored === 'dark' ? stored : 'dark';
}

function persistTheme(value: 'light' | 'dark'): void {
	if (!browser) return;
	// Keep the legacy 'radix-theme' key for backward compatibility — it is read
	// by loadPersistedTheme() as a fallback on first boot before the adapter
	// has persisted 'theme.applied'. The adapter is the canonical store.
	localStorage.setItem(THEME_KEY, value);
	document.documentElement.setAttribute('data-theme', value);
}

// ─── Nav Fact Bridge ──────────────────────────────────────────────────────────

/**
 * Convert plugin NavItem[] to the SidebarNavItem shape expected by design-dojo.
 * Keeps only the fields the Sidebar component understands.
 */
function toSidebarItems(navItems: NavItem[]) {
	return navItems.map((n) => ({
		href: n.href,
		label: n.label,
		icon: n.icon,
		badge: n.badge ? n.badge() : undefined,
	}));
}

// ─── Initialisation ───────────────────────────────────────────────────────────

/**
 * Boot the praxis-svelte bridge: hydrate persisted facts from PluresDB, then
 * seed any facts that were not stored (theme, nav, app.ready).
 *
 * Call once from the root layout's `$effect` or `onMount`.
 */
export function initPraxisFacts(): void {
	// 1. Restore all persist:true facts from PluresDB (adapter writes, adapter reads)
	const adapter = getSharedAdapter();
	if (adapter) {
		for (const [factId, value] of adapter.hydrateAll()) {
			facts.set(factId, value);
		}
	}

	// 2. Seed theme.applied — use persisted value from adapter if available,
	//    otherwise fall back to legacy localStorage key or default 'dark'.
	if (!facts.has('theme.applied')) {
		const initialTheme = loadPersistedTheme();
		emitFact('theme.applied', { value: initialTheme });
	} else {
		// Ensure DOM attribute is applied from the hydrated value
		const persisted = query<{ value: 'light' | 'dark' }>('theme.applied');
		if (persisted?.value) persistTheme(persisted.value);
	}

	// 3. Seed nav.visible from currently registered plugins (always ephemeral)
	emitFact('nav.visible', { items: toSidebarItems(getAllNavItems()) });

	// 4. Seed app.ready (always true for now — gates not yet wired to real checks)
	emitFact('app.ready', { ready: true });
}

// ─── Theme Helpers ────────────────────────────────────────────────────────────

/** Read the current theme value from the praxis fact store. */
export function getTheme(): 'light' | 'dark' {
	const fact = query<{ value: 'light' | 'dark' }>('theme.applied');
	return fact?.value ?? 'dark';
}

/** Apply a new theme, persist it, and emit the updated fact. */
export function applyTheme(value: 'light' | 'dark'): void {
	persistTheme(value);
	emitFact('theme.applied', { value });
}

/** Toggle between light and dark themes. */
export function toggleTheme(): void {
	applyTheme(getTheme() === 'dark' ? 'light' : 'dark');
}
