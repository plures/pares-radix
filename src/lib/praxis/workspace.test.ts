/**
 * Workspace praxis module tests (C-TEST-002).
 * Facts declared with correct persist flags; pane_visibility + center constraints
 * hold on well-formed state and fail on a real violation.
 */

import { describe, it, expect } from 'vitest';
import type { PraxisSystemState } from '../types/praxis.js';
import { workspaceModule, resolvePaneDock, resolveVisibility } from './workspace.js';
import { serializeLayout, type PaneInstanceFact } from '../workspace/persistence.js';
import { applyAction, initialLayout } from '../workspace/reducer.js';

function stateFrom(
	facts: Record<string, unknown>,
): PraxisSystemState {
	return { facts: new Map(Object.entries(facts)) };
}

describe('facts', () => {
	it('workspace.layout and workspace.paneInstances are persist:true', () => {
		const layout = workspaceModule.facts.find((f) => f.id === 'workspace.layout');
		const index = workspaceModule.facts.find((f) => f.id === 'workspace.paneInstances');
		expect(layout?.persist).toBe(true);
		expect(index?.persist).toBe(true);
	});
});

describe('pure twins', () => {
	it('resolvePaneDock / resolveVisibility are re-exported and functional', () => {
		expect(resolvePaneDock('right', 'bottom', ['right', 'bottom'])).toBe('bottom');
		expect(resolveVisibility(true, false)).toBe(true);
	});
});

const paneVisibility = workspaceModule.constraints.find((c) => c.id === 'constraint.pane-visibility')!;
const centerVisible = workspaceModule.constraints.find((c) => c.id === 'constraint.center-always-visible')!;

describe('pane_visibility constraint', () => {
	it('holds when a defaultVisible instance is present in a dock', () => {
		const s = applyAction(initialLayout(), {
			type: 'addInstance',
			instance: { instanceId: 'agens#1', pluginId: 'agens', title: 'Agens' },
			dock: 'right',
		});
		const facts = serializeLayout(s);
		const instFact: PaneInstanceFact & { defaultVisible: boolean; userHidden: boolean } = {
			...facts.instanceFacts['agens#1'],
			defaultVisible: true,
			userHidden: false,
		};
		const state = stateFrom({
			'workspace.layout': facts.layout,
			'workspace.paneInstances': facts.instanceIndex,
			'workspace.paneInstances.agens#1': instFact,
		});
		expect(paneVisibility.check(state)).toBe(true);
	});

	it('violates when a defaultVisible, non-hidden instance is absent from all docks', () => {
		const facts = serializeLayout(initialLayout()); // no instances docked
		const state = stateFrom({
			'workspace.layout': facts.layout,
			'workspace.paneInstances': ['agens#1'],
			'workspace.paneInstances.agens#1': {
				pluginId: 'agens',
				dockId: 'right',
				title: 'Agens',
				defaultVisible: true,
				userHidden: false,
			},
		});
		expect(paneVisibility.check(state)).toBe(false);
	});

	it('holds when the user explicitly hid the instance', () => {
		const facts = serializeLayout(initialLayout());
		const state = stateFrom({
			'workspace.layout': facts.layout,
			'workspace.paneInstances': ['agens#1'],
			'workspace.paneInstances.agens#1': {
				pluginId: 'agens',
				dockId: 'right',
				title: 'Agens',
				defaultVisible: true,
				userHidden: true,
			},
		});
		expect(paneVisibility.check(state)).toBe(true);
	});
});

describe('center-always-visible constraint', () => {
	it('holds for the default layout', () => {
		const facts = serializeLayout(initialLayout());
		expect(centerVisible.check(stateFrom({ 'workspace.layout': facts.layout }))).toBe(true);
	});
	it('violates if center is hidden', () => {
		const facts = serializeLayout(initialLayout());
		const layout = { ...facts.layout, center: { ...facts.layout.center, visible: false } };
		expect(centerVisible.check(stateFrom({ 'workspace.layout': layout }))).toBe(false);
	});
});
