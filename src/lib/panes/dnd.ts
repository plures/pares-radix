/**
 * Drag-and-drop transport for moving pane instances between docks — pure,
 * framework-free. This layer holds NO dock state: it only tracks the in-flight
 * pointer/keyboard gesture and, on completion, emits a MoveCommand describing
 * "put item into dockId at index". Run B applies that against real dock state.
 */

import type { DragItem, DndSession, DropTarget, MoveCommand } from './types.js';

/** A hit-test resolves a pointer position to a drop target (or null). */
export type HitTest = (x: number, y: number) => DropTarget | null;

/** Begin a drag gesture at the given pointer position. */
export function beginDrag(item: DragItem, x: number, y: number): DndSession {
	return {
		item,
		startX: x,
		startY: y,
		x,
		y,
		target: null,
		active: true,
	};
}

/**
 * Update an in-flight drag with a new pointer position, re-running the hit test
 * to resolve the current drop target. Returns a new session (immutable update).
 * Updating an inactive session is a no-op that returns it unchanged.
 */
export function updateDrag(
	session: DndSession,
	x: number,
	y: number,
	hitTest: HitTest
): DndSession {
	if (!session.active) return session;
	return {
		...session,
		x,
		y,
		target: hitTest(x, y),
	};
}

/**
 * End a drag gesture. If the session resolved to a valid drop target, returns a
 * MoveCommand; otherwise returns null (dropped over nothing). A no-op move
 * (same dock + same effective index) still produces a command — the consumer
 * decides whether it changes anything.
 */
export function endDrag(session: DndSession): MoveCommand | null {
	if (!session.active || session.target === null) return null;
	const { item, target } = session;
	return {
		itemId: item.id,
		fromDock: item.fromDock,
		toDock: target.dockId,
		toIndex: target.index,
	};
}

export type KeyboardMoveKey =
	| 'ArrowLeft'
	| 'ArrowRight'
	| 'ArrowUp'
	| 'ArrowDown';

/**
 * Accessibility path: move a pane between docks via keyboard. `docks` is the
 * ordered ring of dock ids; `curDock` is where the item currently lives.
 * ArrowRight/ArrowDown move to the next dock, ArrowLeft/ArrowUp to the previous
 * (with wrap-around). The item is appended (index -1 => end) to the target dock.
 * Returns null when no move applies (unknown key, empty ring, single dock, or
 * curDock not found).
 */
export function keyboardMove(
	item: DragItem,
	key: KeyboardMoveKey | string,
	docks: string[],
	curDock: string
): MoveCommand | null {
	const n = docks.length;
	if (n < 2) return null;
	const cur = docks.indexOf(curDock);
	if (cur === -1) return null;

	let toIdx: number;
	switch (key) {
		case 'ArrowRight':
		case 'ArrowDown':
			toIdx = (cur + 1) % n;
			break;
		case 'ArrowLeft':
		case 'ArrowUp':
			toIdx = (cur - 1 + n) % n;
			break;
		default:
			return null;
	}

	return {
		itemId: item.id,
		fromDock: curDock,
		toDock: docks[toIdx],
		toIndex: -1,
	};
}
