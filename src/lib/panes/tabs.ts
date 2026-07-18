/**
 * Tab-strip logic — pure, framework-free.
 *
 * Handles reordering, closing (with active-tab follow), and roving-tabindex
 * keyboard navigation over an ordered list of TabDescriptors.
 */

import type { TabDescriptor } from './types.js';

/**
 * Move the tab at `from` to position `to`, preserving order of the rest.
 * Indices are clamped into range; a no-op returns a shallow copy.
 */
export function reorder(tabs: TabDescriptor[], from: number, to: number): TabDescriptor[] {
	const n = tabs.length;
	if (n === 0) return [];
	const f = Math.max(0, Math.min(n - 1, from));
	const t = Math.max(0, Math.min(n - 1, to));
	const next = tabs.slice();
	const [moved] = next.splice(f, 1);
	next.splice(t, 0, moved);
	return next;
}

/**
 * Close the tab with `id`. If the closed tab was active, the active id follows
 * to a neighbor (prefer the next tab, else the previous). Returns the new list
 * and the resolved active id (null when no tabs remain).
 */
export function closeTab(
	tabs: TabDescriptor[],
	id: string,
	active: string | null
): { tabs: TabDescriptor[]; active: string | null } {
	const idx = tabs.findIndex((t) => t.id === id);
	if (idx === -1) return { tabs: tabs.slice(), active };

	const next = tabs.slice();
	next.splice(idx, 1);

	if (next.length === 0) return { tabs: next, active: null };

	if (active !== id) {
		// Active tab wasn't the one closed — keep it (it still exists).
		return { tabs: next, active };
	}

	// Active followed: prefer the tab now at the closed index (the old "next"),
	// else fall back to the previous tab.
	const followIdx = idx < next.length ? idx : next.length - 1;
	return { tabs: next, active: next[followIdx].id };
}

export type RovingKey = 'ArrowLeft' | 'ArrowRight' | 'Home' | 'End';

/**
 * Roving-tabindex navigation. ArrowLeft/ArrowRight move focus one tab (with
 * wrap-around); Home focuses the first tab, End the last. Returns the id that
 * should receive focus. Unknown keys or empty lists return `current`.
 */
export function rovingNext(
	tabs: TabDescriptor[],
	current: string | null,
	key: RovingKey | string
): string | null {
	const n = tabs.length;
	if (n === 0) return current;

	const curIdx = current === null ? -1 : tabs.findIndex((t) => t.id === current);

	switch (key) {
		case 'ArrowRight': {
			const i = curIdx === -1 ? 0 : (curIdx + 1) % n;
			return tabs[i].id;
		}
		case 'ArrowLeft': {
			const i = curIdx === -1 ? n - 1 : (curIdx - 1 + n) % n;
			return tabs[i].id;
		}
		case 'Home':
			return tabs[0].id;
		case 'End':
			return tabs[n - 1].id;
		default:
			return current;
	}
}
