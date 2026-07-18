/**
 * Persistence tests — round-trip toFacts -> fromFacts == identity (C-PLURES-003).
 */

import { describe, it, expect } from 'vitest';
import { serializeLayout, deserializeLayout, toFacts, fromFacts } from './persistence.js';
import { applyAction, initialLayout } from './reducer.js';
import type { PaneInstance, WorkspaceLayoutState } from './types.js';
import { defaultLayout } from './types.js';

function inst(id: string, pluginId: string, state?: Record<string, unknown>): PaneInstance {
	return { instanceId: id, pluginId, title: id, state };
}

function multiInstanceState(): WorkspaceLayoutState {
	let s = initialLayout();
	s = applyAction(s, { type: 'addInstance', instance: inst('agens#1', 'agens', { scroll: 42 }), dock: 'right' });
	s = applyAction(s, { type: 'addInstance', instance: inst('agens#2', 'agens'), dock: 'right' });
	s = applyAction(s, { type: 'addInstance', instance: inst('term#1', 'terminal', { cwd: '/tmp' }), dock: 'bottom' });
	s = applyAction(s, { type: 'resizeDock', dock: 'right', size: 400 });
	s = applyAction(s, { type: 'toggleDock', dock: 'left', visible: false });
	return s;
}

describe('round-trip identity', () => {
	it('fromFacts(toFacts(s)) === s for multi-dock, multi-instance state', () => {
		const s = multiInstanceState();
		const back = fromFacts(toFacts(s));
		expect(back).toEqual(s);
	});

	it('preserves per-instance opaque state verbatim', () => {
		const s = multiInstanceState();
		const facts = serializeLayout(s);
		expect(facts.instanceFacts['agens#1'].state).toEqual({ scroll: 42 });
		expect(facts.instanceFacts['agens#1'].dockId).toBe('right');
		const back = fromFacts(facts);
		expect(back.instances['agens#1'].state).toEqual({ scroll: 42 });
	});
});

describe('hydration tolerance', () => {
	it('missing workspace.layout fact falls back to defaultLayout()', () => {
		const back = deserializeLayout(null, null);
		expect(back).toEqual(defaultLayout());
	});

	it('deserializes per-instance facts into the instance registry', () => {
		const back = deserializeLayout(defaultLayout().docks, [
			{ instanceId: 'agens#1', pluginId: 'agens', dockId: 'right', title: 'Agens', state: { n: 1 } },
		]);
		expect(back.instances['agens#1']).toEqual({
			instanceId: 'agens#1',
			pluginId: 'agens',
			title: 'Agens',
			state: { n: 1 },
		});
	});
});
