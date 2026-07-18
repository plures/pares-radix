/**
 * Pane primitive types — framework-free transport/logic types.
 *
 * These describe the pure data shapes used by resize/tabs/dnd logic and by the
 * Svelte adapter components in packages/design-dojo/src/panes/. They hold NO
 * dock/PluresDB state — Run B projects dock placement to PluresDB facts.
 */

export type Orientation = 'horizontal' | 'vertical';

/** A single tab in a tab strip. */
export interface TabDescriptor {
	id: string;
	title: string;
	icon?: string;
	closable?: boolean;
}

/** An item being dragged (a pane instance / tab). Transport-only identity. */
export interface DragItem {
	id: string;
	/** Originating dock/region id, if any. Opaque to this layer. */
	fromDock?: string;
}

/** A resolved drop location produced by a hit test. */
export interface DropTarget {
	/** Target dock/region id. */
	dockId: string;
	/** Insertion index within the target dock's tab list. */
	index: number;
}

/** In-flight drag session. Pure transport — no dock state stored here. */
export interface DndSession {
	item: DragItem;
	/** Pointer start position (px). */
	startX: number;
	startY: number;
	/** Current pointer position (px). */
	x: number;
	y: number;
	/** Current resolved drop target, or null when over no valid target. */
	target: DropTarget | null;
	active: boolean;
}

/**
 * The result of a completed move: "put item into dockId at index".
 * The consumer (Run B) applies this against real dock state / PluresDB.
 */
export interface MoveCommand {
	itemId: string;
	fromDock?: string;
	toDock: string;
	toIndex: number;
}
