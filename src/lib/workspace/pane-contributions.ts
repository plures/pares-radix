/**
 * Pane contribution seeding — pure TS, framework-free (root vitest covers this).
 *
 * Maps a plugin's declared PaneContributions into real PaneInstances docked via
 * the Run B reducer (`addInstance`) + dock resolution (`resolvePaneDock`, the
 * executable twin of praxis/procedures/workspace-layout.px, C-DEV-001). All dock
 * decisions delegate to dock-resolution.ts — NO new dock logic lives here.
 *
 * Seeding is IDEMPOTENT + hydration-safe (C-PLURES-003: instances = workspace
 * facts): re-seeding a layout that already carries an instance of the plugin is
 * a no-op, so a user who closed the pane is not re-seeded on reload and a
 * defaultVisible singleton-by-default contribution never spawns agens#2.
 */

import type { PaneContribution } from '$lib/types/plugin.js';
import { resolvePaneDock } from './dock-resolution.js';
import { applyAction } from './reducer.js';
import type { DockId, PaneInstance, WorkspaceLayoutState } from './types.js';
import { DOCKABLE } from './types.js';

/** A plugin-scoped contribution (as returned by getAllPaneContributions). */
export interface ScopedPaneContribution extends PaneContribution {
	pluginId: string;
}

/**
 * Next free instance id for a plugin, using the `<pluginId>#<n>` convention.
 * Skips any ordinal already present in the layout (collision avoidance).
 */
export function nextInstanceId(state: WorkspaceLayoutState, pluginId: string): string {
	let n = 1;
	while (state.instances[`${pluginId}#${n}`]) n++;
	return `${pluginId}#${n}`;
}

/**
 * Map one scoped contribution to a PaneInstance + its resolved dock (pure).
 * The dock is decided ONLY by resolvePaneDock (Run B twin of the .px); an
 * `override` dock wins when allowed, else the contribution's preferredDock,
 * else 'right'.
 */
export function contributionToInstance(
	state: WorkspaceLayoutState,
	c: ScopedPaneContribution,
	override: DockId | null = null,
): { instance: PaneInstance; dock: DockId } {
	const dock = resolvePaneDock(c.preferredDock, override, DOCKABLE);
	return {
		instance: {
			instanceId: nextInstanceId(state, c.pluginId),
			pluginId: c.pluginId,
			title: c.title,
		},
		dock,
	};
}

/**
 * Seed instances from contributions into a layout. A contribution is seeded
 * ONLY when:
 *   - it is defaultVisible, AND
 *   - no instance of that plugin already exists (idempotent / hydration-safe).
 * Returns the (possibly new) state — pure; the caller persists via the bridge.
 * When nothing is seeded the SAME state reference is returned so callers can
 * skip a redundant write.
 */
export function seedInstancesFromContributions(
	state: WorkspaceLayoutState,
	contributions: ScopedPaneContribution[],
): WorkspaceLayoutState {
	let next = state;
	for (const c of contributions) {
		if (!c.defaultVisible) continue;
		const alreadyPresent = Object.values(next.instances).some(
			(i) => i.pluginId === c.pluginId,
		);
		if (alreadyPresent) continue;
		const { instance, dock } = contributionToInstance(next, c);
		next = applyAction(next, { type: 'addInstance', instance, dock, index: -1 });
	}
	return next;
}
