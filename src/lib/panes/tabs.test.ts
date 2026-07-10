import { describe, it, expect } from 'vitest';
import { reorder, closeTab, rovingNext } from './tabs.js';
import type { TabDescriptor } from './types.js';

const make = (ids: string[]): TabDescriptor[] =>
	ids.map((id) => ({ id, title: id.toUpperCase() }));

describe('reorder', () => {
	it('moves a tab forward', () => {
		const r = reorder(make(['a', 'b', 'c', 'd']), 0, 2);
		expect(r.map((t) => t.id)).toEqual(['b', 'c', 'a', 'd']);
	});
	it('moves a tab backward', () => {
		const r = reorder(make(['a', 'b', 'c', 'd']), 3, 1);
		expect(r.map((t) => t.id)).toEqual(['a', 'd', 'b', 'c']);
	});
	it('handles first->last edge', () => {
		const r = reorder(make(['a', 'b', 'c']), 0, 2);
		expect(r.map((t) => t.id)).toEqual(['b', 'c', 'a']);
	});
	it('handles last->first edge', () => {
		const r = reorder(make(['a', 'b', 'c']), 2, 0);
		expect(r.map((t) => t.id)).toEqual(['c', 'a', 'b']);
	});
	it('clamps out-of-range indices', () => {
		const r = reorder(make(['a', 'b', 'c']), 5, -3);
		expect(r.map((t) => t.id)).toEqual(['c', 'a', 'b']);
	});
	it('is a no-op copy for same index', () => {
		const src = make(['a', 'b', 'c']);
		const r = reorder(src, 1, 1);
		expect(r.map((t) => t.id)).toEqual(['a', 'b', 'c']);
		expect(r).not.toBe(src);
	});
	it('returns [] for empty', () => {
		expect(reorder([], 0, 1)).toEqual([]);
	});
});

describe('closeTab', () => {
	it('closing a non-active tab keeps active', () => {
		const { tabs, active } = closeTab(make(['a', 'b', 'c']), 'a', 'b');
		expect(tabs.map((t) => t.id)).toEqual(['b', 'c']);
		expect(active).toBe('b');
	});
	it('closing the active tab follows to the next', () => {
		const { tabs, active } = closeTab(make(['a', 'b', 'c']), 'b', 'b');
		expect(tabs.map((t) => t.id)).toEqual(['a', 'c']);
		expect(active).toBe('c');
	});
	it('closing the active last tab follows to previous', () => {
		const { tabs, active } = closeTab(make(['a', 'b', 'c']), 'c', 'c');
		expect(tabs.map((t) => t.id)).toEqual(['a', 'b']);
		expect(active).toBe('b');
	});
	it('closing the only tab yields null active', () => {
		const { tabs, active } = closeTab(make(['a']), 'a', 'a');
		expect(tabs).toEqual([]);
		expect(active).toBeNull();
	});
	it('closing an unknown id is a no-op', () => {
		const { tabs, active } = closeTab(make(['a', 'b']), 'z', 'a');
		expect(tabs.map((t) => t.id)).toEqual(['a', 'b']);
		expect(active).toBe('a');
	});
});

describe('rovingNext', () => {
	const tabs = make(['a', 'b', 'c']);
	it('ArrowRight advances', () => {
		expect(rovingNext(tabs, 'a', 'ArrowRight')).toBe('b');
	});
	it('ArrowRight wraps', () => {
		expect(rovingNext(tabs, 'c', 'ArrowRight')).toBe('a');
	});
	it('ArrowLeft retreats', () => {
		expect(rovingNext(tabs, 'b', 'ArrowLeft')).toBe('a');
	});
	it('ArrowLeft wraps', () => {
		expect(rovingNext(tabs, 'a', 'ArrowLeft')).toBe('c');
	});
	it('Home goes first', () => {
		expect(rovingNext(tabs, 'c', 'Home')).toBe('a');
	});
	it('End goes last', () => {
		expect(rovingNext(tabs, 'a', 'End')).toBe('c');
	});
	it('unknown key returns current', () => {
		expect(rovingNext(tabs, 'b', 'Enter')).toBe('b');
	});
	it('empty list returns current', () => {
		expect(rovingNext([], 'x', 'ArrowRight')).toBe('x');
	});
});
