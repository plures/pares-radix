/**
 * Workspace reducer tests (C-TEST-002: real logic, no fixture-faking).
 * Exercises every action + applyMoveCommand against real dnd.ts output.
 */

import { describe, it, expect } from 'vitest';
import { applyAction, applyMoveCommand, initialLayout } from './reducer.js';
import { reorder } from '../panes/tabs.js';
import { beginDrag, updateDrag, endDrag } from '../panes/dnd.js';
import type { PaneInstance, WorkspaceLayoutState } from './types.js';

function inst(id: string, pluginId = 'agens'): PaneInstance {
	return { instanceId: id, pluginId, title: id };
}

/** Build a state with two instances in `right` and one in `bottom`. */
function seeded(): WorkspaceLayoutState {
	let s = initialLayout();
	s = applyAction(s, { type: 'addInstance', instance: inst('agens#1'), dock: 'right' });
	s = applyAction(s, { type: 'addInstance', instance: inst('agens#2'), dock: 'right' });
	s = applyAction(s, { type: 'addInstance', instance: inst('term#1', 'terminal'), dock: 'bottom' });
	return s;
}

describe('addInstance', () => {
	it('registers the instance and appends it to the dock, becoming active', () => {
		const s = applyAction(initialLayout(), { type: 'addInstance', instance: inst('agens#1'), dock: 'right' });
		expect(s.instances['agens#1'].pluginId).toBe('agens');
		expect(s.docks.right.tabs).toEqual(['agens#1']);
		expect(s.docks.right.activeTab).toBe('agens#1');
	});

	it('inserts at a given index', () => {
		let s = seeded();
		s = applyAction(s, { type: 'addInstance', instance: inst('agens#3'), dock: 'right', index: 1 });
		expect(s.docks.right.tabs).toEqual(['agens#1', 'agens#3', 'agens#2']);
	});
});

describe('moveInstance dock->dock', () => {
	it('moves an instance from right to bottom, updating both docks and active', () => {
		let s = seeded();
		s = applyAction(s, { type: 'moveInstance', instanceId: 'agens#1', toDock: 'bottom', toIndex: -1 });
		expect(s.docks.right.tabs).toEqual(['agens#2']);
		expect(s.docks.bottom.tabs).toEqual(['term#1', 'agens#1']);
		expect(s.docks.bottom.activeTab).toBe('agens#1');
	});

	it('source activeTab follows a neighbor when the active instance moves out', () => {
		let s = seeded(); // right active = agens#2 (last added)
		expect(s.docks.right.activeTab).toBe('agens#2');
		s = applyAction(s, { type: 'moveInstance', instanceId: 'agens#2', toDock: 'bottom', toIndex: -1 });
		expect(s.docks.right.activeTab).toBe('agens#1');
	});

	it('toIndex=-1 appends; a mid index splices at position', () => {
		let s = seeded();
		s = applyAction(s, { type: 'moveInstance', instanceId: 'agens#1', toDock: 'bottom', toIndex: 0 });
		expect(s.docks.bottom.tabs).toEqual(['agens#1', 'term#1']);
	});

	it('no-op move (same dock + same index) returns structurally-equal state', () => {
		const s = seeded();
		const curIdx = s.docks.right.tabs.indexOf('agens#1');
		const next = applyAction(s, { type: 'moveInstance', instanceId: 'agens#1', toDock: 'right', toIndex: curIdx });
		expect(next).toBe(s);
	});

	it('unknown instance is a no-op', () => {
		const s = seeded();
		expect(applyAction(s, { type: 'moveInstance', instanceId: 'nope', toDock: 'bottom', toIndex: 0 })).toBe(s);
	});
});

describe('applyMoveCommand (dnd.ts seam)', () => {
	it('lands an instance from a real endDrag() output', () => {
		let s = seeded();
		const session0 = beginDrag({ id: 'agens#1', fromDock: 'right' }, 10, 10);
		const session1 = updateDrag(session0, 400, 300, () => ({ dockId: 'bottom', index: 0 }));
		const cmd = endDrag(session1);
		expect(cmd).not.toBeNull();
		s = applyMoveCommand(s, cmd!);
		expect(s.docks.bottom.tabs).toEqual(['agens#1', 'term#1']);
		expect(s.docks.right.tabs).toEqual(['agens#2']);
	});
});

describe('reorderInDock', () => {
	it('matches panes/tabs.reorder on the dock tabs', () => {
		let s = seeded();
		s = applyAction(s, { type: 'reorderInDock', dock: 'right', from: 0, to: 1 });
		const expected = reorder(['agens#1', 'agens#2'].map((id) => ({ id, title: id })), 0, 1).map((t) => t.id);
		expect(s.docks.right.tabs).toEqual(expected);
	});
});

describe('setActive', () => {
	it('sets active only when the instance is present in the dock', () => {
		let s = seeded();
		s = applyAction(s, { type: 'setActive', dock: 'right', instanceId: 'agens#1' });
		expect(s.docks.right.activeTab).toBe('agens#1');
	});
	it('is ignored when the instance is not in the dock', () => {
		const s = seeded();
		expect(applyAction(s, { type: 'setActive', dock: 'right', instanceId: 'term#1' })).toBe(s);
	});
});

describe('toggleDock', () => {
	it('flips visibility', () => {
		let s = initialLayout();
		const before = s.docks.right.visible;
		s = applyAction(s, { type: 'toggleDock', dock: 'right' });
		expect(s.docks.right.visible).toBe(!before);
	});
	it('center refuses to hide', () => {
		const s = initialLayout();
		const next = applyAction(s, { type: 'toggleDock', dock: 'center', visible: false });
		expect(next).toBe(s);
		expect(next.docks.center.visible).toBe(true);
	});
});

describe('resizeDock', () => {
	it('sets size; negative clamps to 0', () => {
		let s = initialLayout();
		s = applyAction(s, { type: 'resizeDock', dock: 'right', size: 500 });
		expect(s.docks.right.size).toBe(500);
		s = applyAction(s, { type: 'resizeDock', dock: 'right', size: -20 });
		expect(s.docks.right.size).toBe(0);
	});
});

describe('removeInstance', () => {
	it('unregisters and removes from its dock, active follows', () => {
		let s = seeded(); // right = [agens#1, agens#2], active agens#2
		s = applyAction(s, { type: 'removeInstance', instanceId: 'agens#2' });
		expect(s.instances['agens#2']).toBeUndefined();
		expect(s.docks.right.tabs).toEqual(['agens#1']);
		expect(s.docks.right.activeTab).toBe('agens#1');
	});
	it('removing the last instance in a dock nulls activeTab', () => {
		let s = seeded();
		s = applyAction(s, { type: 'removeInstance', instanceId: 'term#1' });
		expect(s.docks.bottom.tabs).toEqual([]);
		expect(s.docks.bottom.activeTab).toBeNull();
	});
});
