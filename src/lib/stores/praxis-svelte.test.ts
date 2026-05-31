import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock plures-db-adapter before importing the store
vi.mock('./plures-db-adapter.js', () => ({
	getSharedAdapter: () => ({
		persistFact: vi.fn(),
	}),
}));

// Mock plugin-loader
vi.mock('$lib/platform/plugin-loader.js', () => ({
	getAllNavItems: () => [],
}));

describe('praxis-svelte store', () => {
	let query: (id: string) => unknown;
	let emitFact: (id: string, value: unknown) => void;

	beforeEach(async () => {
		const mod = await import('./praxis-svelte.svelte.js');
		query = mod.query;
		emitFact = mod.emitFact;
	});

	it('query returns undefined for unknown facts', () => {
		expect(query('nonexistent.fact')).toBeUndefined();
	});

	it('emitFact stores a value retrievable by query', () => {
		emitFact('test.fact', { value: 42 });
		expect(query('test.fact')).toEqual({ value: 42 });
	});

	it('emitFact overwrites previous values', () => {
		emitFact('test.overwrite', 'first');
		emitFact('test.overwrite', 'second');
		expect(query('test.overwrite')).toBe('second');
	});
});
