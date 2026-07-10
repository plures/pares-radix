/**
 * Dock resolution — pure TS twin of praxis/procedures/workspace-layout.px
 * (C-DEV-001). The `.px` is the source of truth; these functions are the
 * executable twin the Svelte layer and the constraint call. Components never
 * decide docks — they call dispatch and render the resolved facts.
 */

import type { DockId } from './types.js';

/**
 * resolve_pane_dock — a pane lands in the user override dock if set & allowed,
 * else its plugin preferredDock if allowed, else falls back to 'right'.
 */
export function resolvePaneDock(
	preferred: DockId,
	override: DockId | null,
	allowed: DockId[],
): DockId {
	if (override && allowed.includes(override)) return override;
	if (allowed.includes(preferred)) return preferred;
	return 'right';
}

/**
 * pane_visibility — a defaultVisible pane is present unless the user explicitly
 * hid it. Visibility = defaultVisible AND NOT userHidden.
 */
export function resolveVisibility(defaultVisible: boolean, userHid: boolean): boolean {
	return defaultVisible && !userHid;
}
