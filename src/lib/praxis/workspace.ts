/**
 * Workspace praxis module — the executable twin of workspace-layout.px
 * (C-DEV-001) and the fact-schema declaration for the dock manager (C-PLURES-003).
 *
 * Declares the persisted layout facts, the pane_visibility constraint, and pure
 * dock-decision helpers that DELEGATE to src/lib/workspace/dock-resolution.ts
 * (one source of truth — no duplicated twin). The Svelte layer reads these facts;
 * components never decide docks.
 */

import type {
	PraxisConstraint,
	PraxisEvent,
	PraxisFact,
	PraxisGate,
	PraxisModule,
	PraxisRule,
	PraxisSystemState,
} from '../types/praxis.js';
import type { DockId, DockState } from '../workspace/types.js';
import type { PaneInstanceFact } from '../workspace/persistence.js';
import { resolvePaneDock, resolveVisibility } from '../workspace/dock-resolution.js';
import { defineContract } from './shell.js';

// Re-export the pure twins so the module + its tests share one implementation.
export { resolvePaneDock, resolveVisibility };

// ─── Facts (C-PLURES-003) ─────────────────────────────────────────────────────

const workspaceFacts: PraxisFact[] = [
	{
		id: 'workspace.layout',
		description:
			'Per-dock layout: Record<DockId,{visible,size,tabs:[instanceId],activeTab}>. ' +
			'Single source of truth for dock geometry/visibility/tab order; survives reload.',
		persist: true,
	},
	{
		id: 'workspace.paneInstances',
		description:
			'Index of live instance ids: string[]. Per-instance detail stored under ' +
			'workspace.paneInstances.<instanceId> facts (also persist:true).',
		persist: true,
		// Dynamic per-instance facts (workspace.paneInstances.<instanceId>) carry the
		// pane's pluginId/dockId/title/state. They are minted at runtime with ids the
		// static registry cannot know, so declare the namespace persistent: any
		// `workspace.paneInstances.*` fact is persisted + hydrated. Without this the
		// per-instance title is dropped on reload and the tab falls back to the id.
		persistPrefix: true,
	},
];

// ─── Events ────────────────────────────────────────────────────────────────

const workspaceEvents: PraxisEvent[] = [
	{
		id: 'workspace.dock.resolve.requested',
		description: 'A pane contribution asked for its resolved dock from preferred + override.',
		schema: '{ instanceId: string; preferred: DockId; override?: DockId; allowed: DockId[] }',
	},
];

// ─── Rules ─────────────────────────────────────────────────────────────────

const workspaceRules: PraxisRule[] = [
	{
		id: 'rule.resolve-pane-dock',
		description:
			'Resolve a pane instance to its actual dock (override if allowed, else preferred, else right).',
		trigger: 'workspace.dock.resolve.requested',
		emits: ['workspace.resolvedDock'],
		contract: defineContract({
			examples: [
				{
					given: { instanceId: 'agens#1', preferred: 'right', override: 'bottom', allowed: ['right', 'bottom', 'left'] },
					expect: { 'workspace.resolvedDock': { instanceId: 'agens#1', dock: 'bottom' } },
					description: 'an allowed override wins over the preferred dock',
				},
			],
			invariants: [
				{
					description: 'resolved dock is one of the allowed docks or the right fallback',
					check: (out) => {
						const r = (out as Record<string, { dock: string }>)['workspace.resolvedDock'];
						return ['center', 'right', 'bottom', 'left'].includes(r?.dock);
					},
				},
			],
		}),
		evaluate: async (event, ctx) => {
			const ev = event as {
				instanceId: string;
				preferred: DockId;
				override?: DockId;
				allowed: DockId[];
			};
			const dock = resolvePaneDock(ev.preferred, ev.override ?? null, ev.allowed);
			const resolved = { instanceId: ev.instanceId, dock };
			ctx.emitFact('workspace.resolvedDock', resolved);
			return { 'workspace.resolvedDock': resolved };
		},
	},
];

// ─── Constraints (twin of pane_visibility) ────────────────────────────────────

const workspaceConstraints: PraxisConstraint[] = [
	{
		id: 'constraint.pane-visibility',
		description:
			'A pane instance flagged defaultVisible and not explicitly hidden by the user must be ' +
			'present in some dock. Twin of workspace-layout.px pane_visibility.',
		check: (state: PraxisSystemState) => {
			const layout = state.facts.get('workspace.layout') as
				| Record<DockId, DockState>
				| undefined;
			const index = (state.facts.get('workspace.paneInstances') as string[] | undefined) ?? [];
			if (!layout) return true; // no layout yet — vacuously satisfied
			const present = (id: string) =>
				(Object.keys(layout) as DockId[]).some((d) => layout[d].tabs.includes(id));
			return index.every((id) => {
				const fact = state.facts.get('workspace.paneInstances.' + id) as
					| (PaneInstanceFact & { defaultVisible?: boolean; userHidden?: boolean })
					| undefined;
				if (!fact) return true;
				const mustBeVisible = resolveVisibility(
					fact.defaultVisible ?? false,
					fact.userHidden ?? false,
				);
				return mustBeVisible ? present(id) : true;
			});
		},
		message:
			'a defaultVisible, non-hidden pane instance is absent from every dock — pane_visibility violated',
	},
	{
		id: 'constraint.center-always-visible',
		description: 'The center dock (which hosts routing) must never be hidden.',
		check: (state: PraxisSystemState) => {
			const layout = state.facts.get('workspace.layout') as
				| Record<DockId, DockState>
				| undefined;
			if (!layout) return true;
			return layout.center.visible === true;
		},
		message: 'center dock is hidden — routing surface would be unreachable',
	},
];

// ─── Gate ─────────────────────────────────────────────────────────────────

const workspaceGates: PraxisGate[] = [
	{
		id: 'gate.workspace-ready',
		description: 'The workspace is ready once its layout fact is present.',
		conditions: ['workspace.layout'],
		check: (state: PraxisSystemState) => state.facts.get('workspace.layout') != null,
	},
];

// ─── Module ─────────────────────────────────────────────────────────────────

export const workspaceModule: PraxisModule = {
	id: 'workspace',
	description:
		'Multi-pane workspace dock manager: persisted per-dock layout, .px-resolved dock placement, ' +
		'and pane visibility — the Radix workbench (VS Code Panel / Secondary Sidebar model).',
	facts: workspaceFacts,
	events: workspaceEvents,
	rules: workspaceRules,
	constraints: workspaceConstraints,
	gates: workspaceGates,
};

export default workspaceModule;
