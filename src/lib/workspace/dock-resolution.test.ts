/**
 * Dock-resolution tests — twins of workspace-layout.px (C-DEV-001).
 */

import { describe, it, expect } from 'vitest';
import { resolvePaneDock, resolveVisibility } from './dock-resolution.js';
import type { DockId } from './types.js';

const ALLOWED: DockId[] = ['right', 'bottom', 'left'];

describe('resolvePaneDock', () => {
	it('override wins when set & allowed', () => {
		expect(resolvePaneDock('right', 'bottom', ALLOWED)).toBe('bottom');
	});
	it('falls back to preferred when override is not allowed', () => {
		expect(resolvePaneDock('right', 'center' as DockId, ALLOWED)).toBe('right');
	});
	it('falls back to preferred when no override', () => {
		expect(resolvePaneDock('left', null, ALLOWED)).toBe('left');
	});
	it("falls back to 'right' when neither is allowed", () => {
		expect(resolvePaneDock('center' as DockId, null, ['bottom'])).toBe('right');
	});
});

describe('resolveVisibility', () => {
	it('defaultVisible & !userHid -> true', () => {
		expect(resolveVisibility(true, false)).toBe(true);
	});
	it('userHid -> false', () => {
		expect(resolveVisibility(true, true)).toBe(false);
	});
	it('not defaultVisible -> false', () => {
		expect(resolveVisibility(false, false)).toBe(false);
	});
});
