import { describe, it, expect } from 'vitest';
import { beginDrag, updateDrag, endDrag, keyboardMove } from './dnd.js';
import type { DragItem, DropTarget } from './types.js';

const item: DragItem = { id: 'agens#1', fromDock: 'right' };

describe('pointer drag path', () => {
	it('begins active with no target', () => {
		const s = beginDrag(item, 10, 20);
		expect(s.active).toBe(true);
		expect(s.target).toBeNull();
		expect(s.startX).toBe(10);
		expect(s.startY).toBe(20);
	});

	it('updateDrag runs the hit test and records position', () => {
		const hit = (x: number): DropTarget | null =>
			x > 100 ? { dockId: 'bottom', index: 2 } : null;
		const s = updateDrag(beginDrag(item, 10, 20), 150, 40, hit);
		expect(s.x).toBe(150);
		expect(s.y).toBe(40);
		expect(s.target).toEqual({ dockId: 'bottom', index: 2 });
	});

	it('endDrag emits a MoveCommand for a valid target', () => {
		const hit = (): DropTarget => ({ dockId: 'bottom', index: 2 });
		const s = updateDrag(beginDrag(item, 10, 20), 150, 40, hit);
		expect(endDrag(s)).toEqual({
			itemId: 'agens#1',
			fromDock: 'right',
			toDock: 'bottom',
			toIndex: 2,
		});
	});

	it('endDrag returns null when dropped over nothing', () => {
		const s = updateDrag(beginDrag(item, 10, 20), 5, 5, () => null);
		expect(endDrag(s)).toBeNull();
	});

	it('updateDrag on inactive session is a no-op', () => {
		const s = { ...beginDrag(item, 0, 0), active: false };
		expect(updateDrag(s, 500, 500, () => ({ dockId: 'x', index: 0 }))).toBe(s);
	});
});

describe('keyboardMove a11y path', () => {
	const docks = ['center', 'right', 'bottom'];
	it('ArrowRight moves to next dock, appended', () => {
		expect(keyboardMove(item, 'ArrowRight', docks, 'right')).toEqual({
			itemId: 'agens#1',
			fromDock: 'right',
			toDock: 'bottom',
			toIndex: -1,
		});
	});
	it('ArrowRight wraps around the ring', () => {
		expect(keyboardMove(item, 'ArrowRight', docks, 'bottom')?.toDock).toBe('center');
	});
	it('ArrowLeft moves to previous dock', () => {
		expect(keyboardMove(item, 'ArrowLeft', docks, 'right')?.toDock).toBe('center');
	});
	it('ArrowLeft wraps around the ring', () => {
		expect(keyboardMove(item, 'ArrowLeft', docks, 'center')?.toDock).toBe('bottom');
	});
	it('ArrowDown/ArrowUp behave like Right/Left', () => {
		expect(keyboardMove(item, 'ArrowDown', docks, 'center')?.toDock).toBe('right');
		expect(keyboardMove(item, 'ArrowUp', docks, 'center')?.toDock).toBe('bottom');
	});
	it('returns null for unknown key', () => {
		expect(keyboardMove(item, 'Enter', docks, 'right')).toBeNull();
	});
	it('returns null with fewer than two docks', () => {
		expect(keyboardMove(item, 'ArrowRight', ['only'], 'only')).toBeNull();
	});
	it('returns null when current dock is not in the ring', () => {
		expect(keyboardMove(item, 'ArrowRight', docks, 'ghost')).toBeNull();
	});
});
