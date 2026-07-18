/**
 * Reactivity regression guard for the praxis-svelte fact store.
 *
 * WHY THIS EXISTS: the fact store originally used a plain `$state<Map>`. In
 * Svelte 5 a plain $state wrapping a native Map does NOT track `.get()`/`.set()`
 * mutations, so `emitFact()` writes never notified `query()` reads made inside a
 * `$derived`/`$effect`. The Operations scene therefore stayed stuck on its
 * empty-state even though `wireOperationsScene` had seeded the fleet. Crucially,
 * a synchronous `expect(query(id)).toEqual(value)` assertion PASSES against that
 * broken store (`.get()` still returns synchronously) — so the pre-existing unit
 * tests could not catch it. The reactive fix is to back the store with SvelteMap
 * (svelte/reactivity), whose per-key `.get()`/`.set()` participate in Svelte 5
 * reactivity.
 *
 * This test asserts the STRUCTURAL invariant that guarantees the fix: the
 * internal fact map is a SvelteMap, not a plain Map. That is deterministic and
 * cannot flake on the effect scheduler. (End-to-end reactivity is additionally
 * proven by the live browser check that the seeded fleet renders its racks.)
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SvelteMap } from 'svelte/reactivity';

vi.mock('./plures-db-adapter.js', () => ({
	getSharedAdapter: () => ({ persistFact: vi.fn() }),
}));
vi.mock('$lib/platform/plugin-loader.js', () => ({
	getAllNavItems: () => [],
}));

describe('praxis-svelte store — reactivity invariant', () => {
	let query: (id: string) => unknown;
	let emitFact: (id: string, value: unknown) => void;
	let __factsForTest: Map<string, unknown> | undefined;

	beforeEach(async () => {
		const mod = (await import('./praxis-svelte.svelte.js')) as typeof import('./praxis-svelte.svelte.js') & {
			__factsForTest?: Map<string, unknown>;
		};
		query = mod.query;
		emitFact = mod.emitFact;
		__factsForTest = mod.__factsForTest;
	});

	it('the internal fact map is a SvelteMap (per-key reactive), not a plain Map', () => {
		expect(__factsForTest).toBeInstanceOf(SvelteMap);
		// A plain `new Map()` is NOT an instanceof SvelteMap, so this fails loudly
		// if the store ever regresses back to `$state<Map>` / a native Map.
	});

	it('emitFact writes are retrievable through the SvelteMap-backed query', () => {
		emitFact('reactive.fleet', [{ id: 'svc-web' }, { id: 'svc-api' }]);
		expect(query('reactive.fleet')).toEqual([{ id: 'svc-web' }, { id: 'svc-api' }]);
		expect(__factsForTest?.get('reactive.fleet')).toEqual([{ id: 'svc-web' }, { id: 'svc-api' }]);
	});
});
