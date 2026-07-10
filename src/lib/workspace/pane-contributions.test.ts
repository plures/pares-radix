/**
 * Pane-contribution seeder tests (C-TEST-002: real assertions, no fixtures).
 * Exercises seeding into the right dock, idempotency/hydration-safety, the
 * defaultVisible skip, dock resolution via resolve_pane_dock, and the
 * `<pluginId>#<n>` id collision-avoidance.
 */

import { describe, it, expect } from 'vitest';
import {
	seedInstancesFromContributions,
	contributionToInstance,
	nextInstanceId,
	type ScopedPaneContribution,
} from './pane-contributions.js';
import { defaultLayout } from './types.js';
import type { WorkspaceLayoutState } from './types.js';
import { serializeLayout, deserializeLayout } from './persistence.js';
import {
	createPluresDBAdapter,
	type PluresDBGraph,
} from '$lib/stores/plures-db-adapter.js';
import { workspaceModule } from '$lib/praxis/workspace.js';

/** In-memory graph implementing the PluresDBGraph contract (env-agnostic). */
function memoryGraph(): PluresDBGraph {
	const store = new Map<string, unknown>();
	return {
		put: (k, v) => void store.set(k, structuredClone(v)),
		get: (k) => store.get(k),
		keys: (prefix = '') => [...store.keys()].filter((k) => k.startsWith(prefix)),
		delete: (k) => void store.delete(k),
	};
}

const agens: ScopedPaneContribution = {
	pluginId: 'agens',
	id: 'agens',
	title: 'Agens',
	icon: '💬',
	preferredDock: 'right',
	defaultVisible: true,
	allowMultiple: true,
};

describe('seedInstancesFromContributions', () => {
	it('places a defaultVisible agens contribution into the right dock', () => {
		const state = seedInstancesFromContributions(defaultLayout(), [agens]);
		expect(state.docks.right.tabs).toEqual(['agens#1']);
		expect(state.docks.right.activeTab).toBe('agens#1');
		expect(state.instances['agens#1'].pluginId).toBe('agens');
		expect(state.instances['agens#1'].title).toBe('Agens');
	});

	it('is idempotent — an already-present plugin instance is not duplicated on re-seed', () => {
		const once = seedInstancesFromContributions(defaultLayout(), [agens]);
		const twice = seedInstancesFromContributions(once, [agens]);
		expect(twice.docks.right.tabs).toEqual(['agens#1']);
		expect(twice.instances['agens#2']).toBeUndefined();
		// No change → same state reference is returned (caller skips a write).
		expect(twice).toBe(once);
	});

	it('does not seed a non-defaultVisible contribution', () => {
		const hidden: ScopedPaneContribution = { ...agens, defaultVisible: false };
		const state = seedInstancesFromContributions(defaultLayout(), [hidden]);
		expect(state.docks.right.tabs).toEqual([]);
		expect(Object.keys(state.instances)).toHaveLength(0);
		// Omitted defaultVisible is likewise skipped.
		const omitted: ScopedPaneContribution = {
			pluginId: 'agens',
			id: 'agens',
			title: 'Agens',
			preferredDock: 'right',
		};
		const state2 = seedInstancesFromContributions(defaultLayout(), [omitted]);
		expect(Object.keys(state2.instances)).toHaveLength(0);
	});
});

describe('contributionToInstance resolves the dock through resolve_pane_dock', () => {
	it('honours preferredDock=bottom', () => {
		const { dock } = contributionToInstance(defaultLayout(), { ...agens, preferredDock: 'bottom' });
		expect(dock).toBe('bottom');
	});

	it('honours preferredDock=left', () => {
		const { dock } = contributionToInstance(defaultLayout(), { ...agens, preferredDock: 'left' });
		expect(dock).toBe('left');
	});

	it('lets an allowed override win over preferredDock', () => {
		const { dock } = contributionToInstance(defaultLayout(), agens, 'bottom');
		expect(dock).toBe('bottom');
	});

	it('falls back to right when the preferred dock is not dockable (center)', () => {
		const { dock } = contributionToInstance(defaultLayout(), { ...agens, preferredDock: 'center' });
		expect(dock).toBe('right');
	});
});

describe('nextInstanceId allocates <pluginId>#<n> avoiding collisions', () => {
	it('returns agens#1 on an empty layout', () => {
		expect(nextInstanceId(defaultLayout(), 'agens')).toBe('agens#1');
	});

	it('returns agens#2 when agens#1 is already present', () => {
		const seeded = seedInstancesFromContributions(defaultLayout(), [agens]);
		expect(nextInstanceId(seeded, 'agens')).toBe('agens#2');
	});

	it('skips occupied ordinals in the middle of the range', () => {
		const state: WorkspaceLayoutState = defaultLayout();
		state.instances['agens#1'] = { instanceId: 'agens#1', pluginId: 'agens', title: 'Agens' };
		state.instances['agens#3'] = { instanceId: 'agens#3', pluginId: 'agens', title: 'Agens' };
		expect(nextInstanceId(state, 'agens')).toBe('agens#2');
	});
});

/**
 * Boot-path regression (title lost on reload): the dynamic per-instance fact
 * `workspace.paneInstances.<id>` carries the pane title but is minted at runtime
 * with an id the static registry cannot know. Before persistPrefix, the adapter
 * silently dropped it, so a reload rebuilt the instance without a title and the
 * tab fell back to the instance id ('agens#1' instead of 'Agens'). This drives
 * the ACTUAL boot persistence path — seed → serializeLayout → adapter.persistFact
 * (per fact id, exactly as writeLayout does) → new adapter → hydrateAll →
 * deserializeLayout — and asserts the hydrated instance keeps title='Agens'.
 */
describe('boot-path round-trip retains per-instance title through PluresDB', () => {
	it('a hydrated agens instance keeps title="Agens" (not the id)', () => {
		const db = memoryGraph();
		const makeAdapter = () =>
			createPluresDBAdapter({ db, registry: [...workspaceModule.facts] });

		// 1. Seed agens into the layout (first boot).
		const seeded = seedInstancesFromContributions(defaultLayout(), [agens]);
		expect(seeded.instances['agens#1'].title).toBe('Agens');

		// 2. Persist EXACTLY as writeLayout does: layout fact, index fact, and one
		//    fact per instance under workspace.paneInstances.<id>.
		const writer = makeAdapter();
		const { layout, instanceIndex, instanceFacts } = serializeLayout(seeded);
		writer.persistFact('workspace.layout', layout);
		writer.persistFact('workspace.paneInstances', instanceIndex);
		for (const id of instanceIndex) {
			writer.persistFact('workspace.paneInstances.' + id, instanceFacts[id]);
		}

		// The dynamic per-instance fact MUST have been persisted (persistPrefix).
		expect(writer.isPersistent('workspace.paneInstances.agens#1')).toBe(true);

		// 3. Fresh boot: a new adapter hydrates ALL persisted facts from the graph.
		const booted = makeAdapter();
		const hydrated = booted.hydrateAll();
		expect(hydrated.has('workspace.paneInstances.agens#1')).toBe(true);

		// 4. Rebuild the layout the way readLayout() does, from hydrated facts.
		const layoutFact = hydrated.get('workspace.layout') as typeof layout;
		const index = hydrated.get('workspace.paneInstances') as string[];
		const rebuiltFacts = index
			.map((id) => {
				const f = hydrated.get('workspace.paneInstances.' + id);
				return f ? { instanceId: id, ...(f as object) } : null;
			})
			.filter((f): f is { instanceId: string } & Record<string, unknown> => f !== null);
		const state = deserializeLayout(layoutFact, rebuiltFacts as never);

		// 5. The hydrated instance retains its title — the tab renders 'Agens'.
		expect(state.instances['agens#1']).toBeDefined();
		expect(state.instances['agens#1'].title).toBe('Agens');
	});
});
