/**
 * Workspace reducer — pure, immutable, framework-free.
 *
 * `applyAction(state, action)` returns a NEW WorkspaceLayoutState for every user
 * layout mutation; it never mutates its input. Dock membership is authoritative
 * via each DockState.tabs; the PaneInstance registry carries no dockId.
 *
 * Run A logic is REUSED, not duplicated (anti-dup gate): tab reordering delegates
 * to panes/tabs.reorder and active-follow-on-remove delegates to panes/tabs.closeTab.
 * MoveCommand (from panes/dnd) is the seam that consumes drag output.
 */

import type { MoveCommand, TabDescriptor } from '../panes/types.js';
import { reorder, closeTab } from '../panes/tabs.js';
import type { DockId, InstanceId, WorkspaceAction, WorkspaceLayoutState, DockState } from './types.js';
import { defaultLayout } from './types.js';

/** initialLayout — alias for the plan's naming; center + empty dockable docks. */
export function initialLayout(): WorkspaceLayoutState {
	return defaultLayout();
}

/** Find the dock currently holding an instance (tabs are authoritative). */
function dockOf(state: WorkspaceLayoutState, id: InstanceId): DockId | null {
	for (const dock of Object.keys(state.docks) as DockId[]) {
		if (state.docks[dock].tabs.includes(id)) return dock;
	}
	return null;
}

/** Shallow-clone a dock with overrides. */
function withDock(state: WorkspaceLayoutState, dock: DockId, patch: Partial<DockState>): WorkspaceLayoutState {
	return {
		...state,
		docks: { ...state.docks, [dock]: { ...state.docks[dock], ...patch } },
	};
}

/** Model an instance id as a TabDescriptor so we can reuse Run A tab logic. */
function asTab(id: InstanceId): TabDescriptor {
	return { id, title: id };
}

/** Remove an id from a dock, resolving active-follow via Run A closeTab. */
function removeFromDock(
	dockState: DockState,
	id: InstanceId,
): DockState {
	if (!dockState.tabs.includes(id)) return dockState;
	const res = closeTab(dockState.tabs.map(asTab), id, dockState.activeTab);
	return { ...dockState, tabs: res.tabs.map((t) => t.id), activeTab: res.active };
}

/** Insert an id into a dock at a clamped index (-1 or out-of-range = append). */
function insertIntoDock(dockState: DockState, id: InstanceId, index: number): DockState {
	const tabs = dockState.tabs.slice();
	const at = index < 0 || index > tabs.length ? tabs.length : index;
	tabs.splice(at, 0, id);
	// Moving/adding an instance makes it the active tab of its new dock.
	return { ...dockState, tabs, activeTab: id };
}

/** moveInstance — the core dock->dock (and intra-dock reposition) transition. */
function moveInstance(
	state: WorkspaceLayoutState,
	id: InstanceId,
	toDock: DockId,
	toIndex: number,
): WorkspaceLayoutState {
	const from = dockOf(state, id);
	if (from === null) return state; // unknown instance — no-op

	// No-op: same dock and the same effective index.
	if (from === toDock) {
		const cur = state.docks[from].tabs.indexOf(id);
		const target = toIndex < 0 || toIndex >= state.docks[from].tabs.length
			? state.docks[from].tabs.length - 1
			: toIndex;
		if (cur === target) return state;
		// Intra-dock reposition via reorder.
		const reordered = reorder(state.docks[from].tabs.map(asTab), cur, target).map((t) => t.id);
		return withDock(state, from, { tabs: reordered, activeTab: id });
	}

	// Cross-dock: remove from source (active follows a neighbor), insert into target.
	const srcDock = removeFromDock(state.docks[from], id);
	let next = withDock(state, from, srcDock);
	const dstDock = insertIntoDock(next.docks[toDock], id, toIndex);
	next = withDock(next, toDock, dstDock);
	return next;
}

/** applyMoveCommand — consume dnd.ts output; state is authoritative over fromDock. */
export function applyMoveCommand(state: WorkspaceLayoutState, cmd: MoveCommand): WorkspaceLayoutState {
	return moveInstance(state, cmd.itemId, cmd.toDock as DockId, cmd.toIndex);
}

/** applyAction — the pure reducer. */
export function applyAction(state: WorkspaceLayoutState, action: WorkspaceAction): WorkspaceLayoutState {
	switch (action.type) {
		case 'moveInstance':
			return moveInstance(state, action.instanceId, action.toDock, action.toIndex);

		case 'reorderInDock': {
			const dock = state.docks[action.dock];
			const tabs = reorder(dock.tabs.map(asTab), action.from, action.to).map((t) => t.id);
			return withDock(state, action.dock, { tabs });
		}

		case 'setActive': {
			const dock = state.docks[action.dock];
			if (!dock.tabs.includes(action.instanceId)) return state; // only if present
			return withDock(state, action.dock, { activeTab: action.instanceId });
		}

		case 'toggleDock': {
			if (action.dock === 'center') return state; // center can never be hidden
			const cur = state.docks[action.dock].visible;
			const next = action.visible ?? !cur;
			if (next === cur) return state;
			return withDock(state, action.dock, { visible: next });
		}

		case 'resizeDock':
			return withDock(state, action.dock, { size: Math.max(0, action.size) });

		case 'addInstance': {
			const { instance, dock, index } = action;
			const instances = { ...state.instances, [instance.instanceId]: instance };
			const dstDock = insertIntoDock(state.docks[dock], instance.instanceId, index ?? -1);
			return withDock({ ...state, instances }, dock, dstDock);
		}

		case 'removeInstance': {
			const from = dockOf(state, action.instanceId);
			const instances = { ...state.instances };
			delete instances[action.instanceId];
			if (from === null) return { ...state, instances };
			const srcDock = removeFromDock(state.docks[from], action.instanceId);
			return withDock({ ...state, instances }, from, srcDock);
		}

		default:
			return state;
	}
}
