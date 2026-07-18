/**
 * Workspace Svelte bridge — reactive projection of the workspace layout facts +
 * a dispatch() that reduces an action and writes the changed facts through the
 * sanctioned emitFact path (C-PLURES-003: no ad-hoc store). Thin: it holds NO
 * dock-decision logic — that lives in the reducer + dock-resolution twins.
 */

// The praxis query/emit facade is the sanctioned store; this bridge is its only consumer.
import { query, emitFact } from './praxis-svelte.svelte.js';
import { applyAction } from '$lib/workspace/reducer.js';
import { serializeLayout, deserializeLayout } from '$lib/workspace/persistence.js';
import { seedInstancesFromContributions } from '$lib/workspace/pane-contributions.js';
import type { ScopedPaneContribution } from '$lib/workspace/pane-contributions.js';
import type { PaneInstanceFact } from '$lib/workspace/persistence.js';
import { defaultLayout } from '$lib/workspace/types.js';
import type {
	DockId,
	DockState,
	WorkspaceAction,
	WorkspaceLayoutState,
} from '$lib/workspace/types.js';

/** Read the current layout from facts, falling back to defaultLayout(). */
export function readLayout(): WorkspaceLayoutState {
	const layoutFact = query<Record<DockId, DockState>>('workspace.layout');
	const index = query<string[]>('workspace.paneInstances') ?? [];
	const instanceFacts = index
		.map((id) => {
			const f = query<PaneInstanceFact>('workspace.paneInstances.' + id);
			return f ? { instanceId: id, ...f } : null;
		})
		.filter((f): f is PaneInstanceFact & { instanceId: string } => f !== null);
	return deserializeLayout(layoutFact ?? null, instanceFacts);
}

/**
 * Write a full layout state back to facts: the layout fact, the instance index,
 * and one fact per instance. Called after every reduce; persist:true makes the
 * adapter write PluresDB immediately so the layout survives reload.
 */
function writeLayout(state: WorkspaceLayoutState): void {
	const { layout, instanceIndex, instanceFacts } = serializeLayout(state);
	emitFact('workspace.layout', layout);
	emitFact('workspace.paneInstances', instanceIndex);
	for (const id of instanceIndex) {
		emitFact('workspace.paneInstances.' + id, instanceFacts[id]);
	}
}

/** Reduce an action against the current fact-backed layout and persist the result. */
export function dispatch(action: WorkspaceAction): void {
	const next = applyAction(readLayout(), action);
	writeLayout(next);
}

/**
 * Seed pane instances from plugin contributions through the sanctioned write
 * path. Idempotent + hydration-safe: seedInstancesFromContributions returns the
 * SAME state reference when nothing is added (an agens#1 already restored from
 * PluresDB, or a defaultVisible singleton-by-default already present), so a
 * reload is a no-op and a user who closed the pane is respected. Only writes
 * (and thus persists) when an instance was actually added.
 */
export function seedPaneInstances(contributions: ScopedPaneContribution[]): void {
	const current = readLayout();
	const seeded = seedInstancesFromContributions(current, contributions);
	if (seeded !== current) writeLayout(seeded);
}

/**
 * Seed the default workspace layout hydration-safely: only seed when the layout
 * fact was not already restored from PluresDB, mirroring wireAdminScene /
 * wireOperationsScene so an operator's dock changes survive a restart.
 */
export function wireWorkspaceScene(
	emit: (id: string, value: unknown) => void,
	q: (id: string) => unknown,
): void {
	if (q('workspace.layout') == null) {
		const { layout, instanceIndex } = serializeLayout(defaultLayout());
		emit('workspace.layout', layout);
		emit('workspace.paneInstances', instanceIndex);
	}
}
