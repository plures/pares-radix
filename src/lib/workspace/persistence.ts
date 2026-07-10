/**
 * Workspace persistence — pure mapping WorkspaceLayoutState <-> PraxisFact shapes.
 *
 * Kept free of emitFact/query so the mapping is unit-tested away from Svelte
 * (C-PLURES-003: layout state = PluresDB facts). Fact shapes:
 *   - workspace.layout                        -> Record<DockId, DockState>
 *   - workspace.paneInstances                 -> InstanceId[] (the index)
 *   - workspace.paneInstances.<instanceId>    -> { pluginId, dockId, title, state }
 * The per-instance `dockId` is a denormalized convenience; workspace.layout tabs
 * remain authoritative on conflict.
 */

import type {
	DockId,
	DockState,
	InstanceId,
	PaneInstance,
	WorkspaceLayoutState,
} from './types.js';
import { defaultLayout } from './types.js';

/** Per-instance persisted fact shape. */
export interface PaneInstanceFact {
	pluginId: string;
	dockId: DockId;
	title: string;
	state?: Record<string, unknown>;
}

/** The serialized bundle: the layout fact, the index fact, and per-instance facts. */
export interface SerializedLayout {
	layout: Record<DockId, DockState>;
	instanceIndex: InstanceId[];
	instanceFacts: Record<InstanceId, PaneInstanceFact>;
}

/** Find the authoritative dock of an instance from the layout tabs. */
function dockOf(layout: Record<DockId, DockState>, id: InstanceId): DockId {
	for (const dock of Object.keys(layout) as DockId[]) {
		if (layout[dock].tabs.includes(id)) return dock;
	}
	// Not in any dock's tabs: default to 'right' (denormalized hint only).
	return 'right';
}

/** toFacts / serializeLayout — WorkspaceLayoutState -> the three fact shapes. */
export function serializeLayout(state: WorkspaceLayoutState): SerializedLayout {
	const layout = state.docks;
	const instanceIndex = Object.keys(state.instances);
	const instanceFacts: Record<InstanceId, PaneInstanceFact> = {};
	for (const id of instanceIndex) {
		const inst = state.instances[id];
		instanceFacts[id] = {
			pluginId: inst.pluginId,
			dockId: dockOf(layout, id),
			title: inst.title,
			state: inst.state,
		};
	}
	return { layout, instanceIndex, instanceFacts };
}

/** Alias matching the plan's naming. */
export const toFacts = serializeLayout;

/**
 * deserializeLayout / fromFacts — rebuild WorkspaceLayoutState from the facts.
 * A missing/absent layout fact falls back to defaultLayout() (hydration tolerance).
 * Instances are rebuilt from the per-instance facts; `state` survives verbatim.
 */
export function deserializeLayout(
	layoutFact: Record<DockId, DockState> | null | undefined,
	instanceFacts: Array<PaneInstanceFact & { instanceId: InstanceId }> | null | undefined,
): WorkspaceLayoutState {
	const base = defaultLayout();
	const docks = layoutFact ?? base.docks;
	const instances: Record<InstanceId, PaneInstance> = {};
	for (const f of instanceFacts ?? []) {
		instances[f.instanceId] = {
			instanceId: f.instanceId,
			pluginId: f.pluginId,
			title: f.title,
			state: f.state,
		};
	}
	return { docks, instances };
}

/** Alias matching the plan's naming: fromFacts(serialized) -> state. */
export function fromFacts(s: SerializedLayout): WorkspaceLayoutState {
	const instanceFacts = s.instanceIndex.map((id) => ({ instanceId: id, ...s.instanceFacts[id] }));
	return deserializeLayout(s.layout, instanceFacts);
}
