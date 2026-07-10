import { describe, it, expect } from 'vitest';
import { clampSize, applyDelta, keyResize, type ResizeParams } from './resize.js';

const params: ResizeParams = { total: 1000, minA: 100, minB: 200 };

describe('clampSize', () => {
	it('returns proposed when within range', () => {
		expect(clampSize(500, params)).toBe(500);
	});
	it('clamps to minA when below', () => {
		expect(clampSize(50, params)).toBe(100);
	});
	it('clamps to total - minB when above', () => {
		expect(clampSize(950, params)).toBe(800);
	});
	it('accepts exact boundaries', () => {
		expect(clampSize(100, params)).toBe(100);
		expect(clampSize(800, params)).toBe(800);
	});
	it('falls back to midpoint on infeasible range', () => {
		expect(clampSize(700, { total: 100, minA: 80, minB: 80 })).toBe(50);
	});
});

describe('applyDelta', () => {
	it('grows the primary on positive delta', () => {
		expect(applyDelta(500, 100, params)).toBe(600);
	});
	it('shrinks the primary on negative delta', () => {
		expect(applyDelta(500, -100, params)).toBe(400);
	});
	it('clamps the result', () => {
		expect(applyDelta(500, 1000, params)).toBe(800);
		expect(applyDelta(500, -1000, params)).toBe(100);
	});
});

describe('keyResize', () => {
	const step = 20;
	it('ArrowLeft shrinks primary', () => {
		expect(keyResize(500, 'ArrowLeft', step, params)).toBe(480);
	});
	it('ArrowUp shrinks primary', () => {
		expect(keyResize(500, 'ArrowUp', step, params)).toBe(480);
	});
	it('ArrowRight grows primary', () => {
		expect(keyResize(500, 'ArrowRight', step, params)).toBe(520);
	});
	it('ArrowDown grows primary', () => {
		expect(keyResize(500, 'ArrowDown', step, params)).toBe(520);
	});
	it('Home clamps to minA', () => {
		expect(keyResize(500, 'Home', step, params)).toBe(100);
	});
	it('End clamps to total - minB', () => {
		expect(keyResize(500, 'End', step, params)).toBe(800);
	});
	it('respects clamp at the low edge', () => {
		expect(keyResize(110, 'ArrowLeft', step, params)).toBe(100);
	});
	it('respects clamp at the high edge', () => {
		expect(keyResize(790, 'ArrowRight', step, params)).toBe(800);
	});
	it('returns current for unknown keys', () => {
		expect(keyResize(500, 'Enter', step, params)).toBe(500);
	});
});
