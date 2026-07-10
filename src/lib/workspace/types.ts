/**
 * Workspace dock-manager types — framework-free, PluresDB-facts-backed.
 *
 * A workspace is a set of named docks (center/right/bottom/left). The center
 * hosts the routed page; the other docks are tab groups of pane instances that
 * live orthogonally to routing (VS Code Panel / Secondary Sidebar model, per
 * vscode-pane-model-study §4.2-4.4). This layer holds NO Svelte / PluresDB
 * concerns — persistence.ts maps it to PraxisFact[], the Svelte bridge reduces.
 */

/** The four named dock regions. `center` is always visible (hosts routing). */
export type DockId = 'center' | 'right' | 'bottom' | 'left';

/** Stable dock ring order (used for keyboard move + iteration determinism). */
export const DOCK_RING: DockId[] = ['center', 'right', 'bottom', 'left'];

/** Non-center docks — the only ones that can be hidden or hold pane instances. */
export const DOCKABLE: DockId[] = ['right', 'bottom', 'left'];

/** Instance identity, conventionally `<pluginId>#<n>` (e.g. "agens#1"). */
export type InstanceId = string;

/** A live pane instance docked somewhere. Its `state` is opaque + persisted. */
export interface PaneInstance {
	instanceId: InstanceId;
	pluginId: string;
	title: string;
	/** Opaque, pane-owned state (persisted verbatim across reload). */
	state?: Record<string, unknown>;
}

/** Per-dock geometry + tab group. `tabs` is authoritative on dock membership. */
export interface DockState {
	visible: boolean;
	/** px extent of the dock along its split axis. */
	size: number;
	/** Ordered instance ids in this dock's tab group. */
	tabs: InstanceId[];
	/** Currently active tab, or null when the dock is empty. */
	activeTab: InstanceId | null;
}

/** The full workspace layout: dock geometry + the instance registry. */
export interface WorkspaceLayoutState {
	docks: Record<DockId, DockState>;
	instances: Record<InstanceId, PaneInstance>;
}

/** The reducer's action union — every user layout mutation. */
export type WorkspaceAction =
	| { type: 'moveInstance'; instanceId: InstanceId; toDock: DockId; toIndex: number }
	| { type: 'reorderInDock'; dock: DockId; from: number; to: number }
	| { type: 'setActive'; dock: DockId; instanceId: InstanceId }
	| { type: 'toggleDock'; dock: DockId; visible?: boolean }
	| { type: 'resizeDock'; dock: DockId; size: number }
	| { type: 'addInstance'; instance: PaneInstance; dock: DockId; index?: number }
	| { type: 'removeInstance'; instanceId: InstanceId };

/** Default (first-boot) layout: center visible, dockable docks empty-but-real. */
export function defaultLayout(): WorkspaceLayoutState {
	const empty = (visible: boolean, size: number): DockState => ({
		visible,
		size,
		tabs: [],
		activeTab: null,
	});
	return {
		docks: {
			center: { visible: true, size: 0, tabs: [], activeTab: null },
			right: empty(true, 320),
			bottom: empty(true, 220),
			left: empty(false, 240),
		},
		instances: {},
	};
}
