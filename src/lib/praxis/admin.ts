/**
 * Admin Console praxis module — the executable twin of admin-console.px.
 *
 * The operator's window into the running Radix platform, expressed entirely as
 * praxis primitives (facts / events / rules / constraints / gate). No if/else
 * business logic leaks into the Svelte view — the route is a pure projection of
 * the facts these rules emit. No direct db.put; all state flows through emitFact
 * / the praxis adapter (C-PLURES-003).
 *
 * Answers four operator questions, each a faithful twin of a .px procedure:
 *   1. derive_system_readiness   — is the platform operable? (gates + violations)
 *   2. assess_plugin_health      — healthy | degraded | failed | inactive
 *   3. collect_violations        — which constraints do not hold right now
 *   4. authorize_admin_action    — may this admin action proceed? (guard)
 *
 * The pure decision helpers (computeReadiness / classifyPluginHealth /
 * decideAdminAction) are exported so the module's tests exercise the SAME logic
 * the rules run — no duplicated twin, one source of truth.
 */

import type {
	PraxisFact,
	PraxisEvent,
	PraxisRule,
	PraxisConstraint,
	PraxisGate,
	PraxisModule,
	PraxisSystemState,
} from '../types/praxis.js';
import { defineContract } from './shell.js';

// ─── Domain types ─────────────────────────────────────────────────────────────

export type PluginHealthStatus = 'healthy' | 'degraded' | 'failed' | 'inactive';

export interface PluginHealth {
	pluginId: string;
	name: string;
	version: string;
	status: PluginHealthStatus;
	active: boolean;
	/** number of routes + nav items this plugin contributes (its UI surface) */
	surface: number;
	lastError?: string;
}

export interface SystemReadiness {
	operable: boolean;
	openGates: number;
	totalGates: number;
	violations: number;
}

export type AdminAction = 'activate' | 'deactivate' | 'reload' | 'toggle-flag';

export interface AdminActionVerdict {
	action: AdminAction;
	target: string;
	verdict: 'allowed' | 'blocked';
	reason: string;
}

export interface FeatureFlag {
	key: string;
	label: string;
	enabled: boolean;
	description: string;
}

// ─── Pure decision helpers (twins of the .px procedures) ──────────────────────

/**
 * derive_system_readiness — operable iff every required gate is open AND no
 * error-severity constraint is violated. Warn-severity advisories do not block.
 */
export function computeReadiness(
	openGates: number,
	totalGates: number,
	errorViolations: number,
): boolean {
	return openGates >= totalGates && errorViolations === 0;
}

/**
 * assess_plugin_health — map observable plugin state to a health status.
 *   inactive ⇐ not active
 *   failed   ⇐ active but lastError present
 *   degraded ⇐ active, no error, but contributes zero surface (dead weight)
 *   healthy  ⇐ active, no error, contributes surface
 */
export function classifyPluginHealth(
	active: boolean,
	lastError: string | undefined,
	surface: number,
): PluginHealthStatus {
	if (!active) return 'inactive';
	if (lastError) return 'failed';
	if (surface <= 0) return 'degraded';
	return 'healthy';
}

/**
 * authorize_admin_action — block only actions that would break a safe posture:
 *   deactivate a plugin with an active dependent → blocked (would break it).
 * Everything reversible (activate / reload / toggle-flag) is allowed; reload &
 * activate are recovery paths and remain allowed even when a gate is closed.
 */
export function decideAdminAction(
	action: AdminAction,
	target: string,
	activeDependents: string[],
): { decision: 'allowed' | 'blocked'; reason: string } {
	if (action === 'deactivate' && activeDependents.length > 0) {
		return {
			decision: 'blocked',
			reason: `cannot deactivate "${target}": active plugin(s) depend on it (${activeDependents.join(', ')})`,
		};
	}
	if (action === 'toggle-flag') {
		return { decision: 'allowed', reason: 'feature flags are reversible and non-destructive' };
	}
	return { decision: 'allowed', reason: `${action} on "${target}" is permitted` };
}

/** Stable-ordered constraint violations against a live fact state. */
export function collectViolations(
	constraints: PraxisConstraint[],
	state: PraxisSystemState,
): Array<{ id: string; message: string }> {
	return constraints
		.filter((c) => {
			try {
				return !c.check(state);
			} catch {
				// A constraint that throws on the current state is itself a violation.
				return true;
			}
		})
		.map((c) => ({ id: c.id, message: c.message }))
		.sort((a, b) => a.id.localeCompare(b.id));
}

// ─── Facts ─────────────────────────────────────────────────────────────────

const adminFacts: PraxisFact[] = [
	{
		id: 'admin.plugins.health',
		description: 'Per-plugin health roster (healthy | degraded | failed | inactive) for the console',
		persist: false,
	},
	{
		id: 'admin.system.readiness',
		description: 'Derived platform operability: open gates vs total, and error-violation count',
		persist: false,
	},
	{
		id: 'admin.constraint.violations',
		description: 'Constraints that do not currently hold, stable-ordered by id',
		persist: false,
	},
	{
		id: 'admin.feature.flags',
		description: 'Operator-toggleable feature flags (reversible, non-destructive)',
		persist: true,
	},
	{
		id: 'admin.action.verdict',
		description: 'Verdict for the last requested admin action (allowed | blocked) with reason',
		persist: false,
	},
	{
		id: 'admin.audit.log',
		description: 'Append-only log of admin actions taken through the console',
		persist: true,
	},
];

// ─── Events ────────────────────────────────────────────────────────────────

const adminEvents: PraxisEvent[] = [
	{
		id: 'admin.readiness.requested',
		description: 'Console asked to recompute system readiness from current gate/violation state',
		schema: '{ openGates: number; totalGates: number; errorViolations: number }',
	},
	{
		id: 'admin.plugin.health.requested',
		description: 'Console asked to assess a plugin roster into health statuses',
		schema: '{ plugins: Array<{ pluginId: string; name: string; version: string; active: boolean; surface: number; lastError?: string }> }',
	},
	{
		id: 'admin.action.requested',
		description: 'Operator requested an admin action against a plugin or flag — must be authorized',
		schema: '{ action: "activate"|"deactivate"|"reload"|"toggle-flag"; target: string; activeDependents?: string[] }',
	},
];

// ─── Rules ─────────────────────────────────────────────────────────────────

const adminRules: PraxisRule[] = [
	// ── Rule 1: System readiness (twin of derive_system_readiness) ─────────────
	{
		id: 'rule.derive-system-readiness',
		description:
			'Compute platform operability from open-gate count and error-severity violation count. ' +
			'Operable iff all required gates open AND zero error violations.',
		trigger: 'admin.readiness.requested',
		emits: ['admin.system.readiness'],
		contract: defineContract({
			examples: [
				{
					given: { openGates: 3, totalGates: 3, errorViolations: 0 },
					expect: {
						'admin.system.readiness': {
							operable: true,
							openGates: 3,
							totalGates: 3,
							violations: 0,
						},
					},
					description: 'all gates open + no violations → operable',
				},
				{
					given: { openGates: 2, totalGates: 3, errorViolations: 0 },
					expect: {
						'admin.system.readiness': {
							operable: false,
							openGates: 2,
							totalGates: 3,
							violations: 0,
						},
					},
					description: 'a closed gate → not operable',
				},
			],
			invariants: [
				{
					description: 'operable is always a boolean',
					check: (out) =>
						typeof (out as Record<string, SystemReadiness>)['admin.system.readiness']
							?.operable === 'boolean',
				},
			],
		}),
		evaluate: async (event, ctx) => {
			const ev = event as { openGates: number; totalGates: number; errorViolations: number };
			const readiness: SystemReadiness = {
				operable: computeReadiness(ev.openGates, ev.totalGates, ev.errorViolations),
				openGates: ev.openGates,
				totalGates: ev.totalGates,
				violations: ev.errorViolations,
			};
			ctx.emitFact('admin.system.readiness', readiness);
			return { 'admin.system.readiness': readiness };
		},
	},

	// ── Rule 2: Plugin health (twin of assess_plugin_health) ───────────────────
	{
		id: 'rule.assess-plugin-health',
		description:
			'Classify each plugin as healthy | degraded | failed | inactive from its active flag, ' +
			'lastError, and contributed UI surface.',
		trigger: 'admin.plugin.health.requested',
		emits: ['admin.plugins.health'],
		contract: defineContract({
			examples: [
				{
					given: {
						plugins: [
							{ pluginId: 'canvas', name: 'AI Canvas', version: '0.1.0', active: true, surface: 2 },
							{ pluginId: 'x', name: 'X', version: '1.0.0', active: false, surface: 0 },
						],
					},
					expect: {
						'admin.plugins.health': [
							{
								pluginId: 'canvas',
								name: 'AI Canvas',
								version: '0.1.0',
								status: 'healthy',
								active: true,
								surface: 2,
							},
							{
								pluginId: 'x',
								name: 'X',
								version: '1.0.0',
								status: 'inactive',
								active: false,
								surface: 0,
							},
						],
					},
					description: 'active+surface → healthy; inactive → inactive',
				},
			],
			invariants: [
				{
					description: 'every roster entry has a valid status',
					check: (out) =>
						(out as Record<string, PluginHealth[]>)['admin.plugins.health'].every((p) =>
							['healthy', 'degraded', 'failed', 'inactive'].includes(p.status),
						),
				},
			],
		}),
		evaluate: async (event, ctx) => {
			const ev = event as {
				plugins: Array<{
					pluginId: string;
					name: string;
					version: string;
					active: boolean;
					surface: number;
					lastError?: string;
				}>;
			};
			const roster: PluginHealth[] = ev.plugins.map((p) => ({
				pluginId: p.pluginId,
				name: p.name,
				version: p.version,
				active: p.active,
				surface: p.surface,
				lastError: p.lastError,
				status: classifyPluginHealth(p.active, p.lastError, p.surface),
			}));
			ctx.emitFact('admin.plugins.health', roster);
			return { 'admin.plugins.health': roster };
		},
	},

	// ── Rule 3: Authorize admin action (twin of authorize_admin_action) ────────
	{
		id: 'rule.authorize-admin-action',
		description:
			'Guard operator actions: deactivating a plugin with an active dependent is blocked; ' +
			'reversible actions (activate/reload/toggle-flag) are allowed.',
		trigger: 'admin.action.requested',
		emits: ['admin.action.verdict', 'admin.audit.log'],
		contract: defineContract({
			examples: [
				{
					given: { action: 'deactivate', target: 'storage', activeDependents: ['commerce'] },
					expect: {
						'admin.action.verdict': {
							action: 'deactivate',
							target: 'storage',
							verdict: 'blocked',
						},
					},
					description: 'deactivating a depended-upon plugin is blocked',
				},
				{
					given: { action: 'toggle-flag', target: 'admin.experimental', activeDependents: [] },
					expect: {
						'admin.action.verdict': {
							action: 'toggle-flag',
							target: 'admin.experimental',
							verdict: 'allowed',
						},
					},
					description: 'toggling a flag is always allowed',
				},
			],
			invariants: [
				{
					description: 'verdict is allowed or blocked',
					check: (out) =>
						['allowed', 'blocked'].includes(
							(out as Record<string, AdminActionVerdict>)['admin.action.verdict']?.verdict,
						),
				},
			],
		}),
		evaluate: async (event, ctx) => {
			const ev = event as {
				action: AdminAction;
				target: string;
				activeDependents?: string[];
			};
			const decision = decideAdminAction(ev.action, ev.target, ev.activeDependents ?? []);
			const verdict: AdminActionVerdict = {
				action: ev.action,
				target: ev.target,
				verdict: decision.decision,
				reason: decision.reason,
			};
			ctx.emitFact('admin.action.verdict', verdict);
			// Append to the audit log (read-modify-write through the reactive fact).
			const priorLog =
				(ctx.query?.('admin.audit.log') as Array<Record<string, unknown>> | undefined) ?? [];
			const entry = { ...verdict, at: new Date().toISOString() };
			ctx.emitFact('admin.audit.log', [...priorLog, entry]);
			return { 'admin.action.verdict': verdict, 'admin.audit.log': [...priorLog, entry] };
		},
	},
];

// ─── Constraints ─────────────────────────────────────────────────────────────

const adminConstraints: PraxisConstraint[] = [
	{
		id: 'constraint.no-blocked-action-in-audit',
		description: 'A blocked action must never appear as an executed (allowed) entry in the audit log',
		check: (state: PraxisSystemState) => {
			const log =
				(state.facts.get('admin.audit.log') as Array<{ verdict?: string; executed?: boolean }>) ??
				[];
			return log.every((e) => !(e.verdict === 'blocked' && e.executed === true));
		},
		message: 'audit log contains a blocked action marked executed — a guard was bypassed',
	},
	{
		id: 'constraint.feature-flags-well-formed',
		description: 'Every feature flag has a string key and a boolean enabled state',
		check: (state: PraxisSystemState) => {
			const flags = (state.facts.get('admin.feature.flags') as FeatureFlag[] | undefined) ?? [];
			return flags.every((f) => typeof f.key === 'string' && typeof f.enabled === 'boolean');
		},
		message: 'a feature flag is malformed (missing key or non-boolean enabled)',
	},
];

// ─── Gate ─────────────────────────────────────────────────────────────────

const adminGates: PraxisGate[] = [
	{
		id: 'gate.admin-console-ready',
		description: 'The admin console is ready once the plugin roster and system readiness are computed',
		conditions: ['admin.plugins.health', 'admin.system.readiness'],
		check: (state: PraxisSystemState) =>
			Array.isArray(state.facts.get('admin.plugins.health')) &&
			state.facts.get('admin.system.readiness') != null,
	},
];

// ─── Module ─────────────────────────────────────────────────────────────────

export const adminModule: PraxisModule = {
	id: 'admin',
	description:
		'Admin console: plugin health, system readiness (gates + violations), feature flags, and ' +
		'guarded admin actions — the operator window into the running Radix platform.',
	facts: adminFacts,
	events: adminEvents,
	rules: adminRules,
	constraints: adminConstraints,
	gates: adminGates,
};

// ─── Default feature flags (real, reversible platform toggles) ────────────────

export const defaultFeatureFlags: FeatureFlag[] = [
	{
		key: 'admin.showAuditLog',
		label: 'Show audit log',
		enabled: true,
		description: 'Display the append-only admin action audit trail in the console',
	},
	{
		key: 'ops.autoRollback',
		label: 'Auto-rollback on SLO breach',
		enabled: true,
		description: 'Let rule.slo-breach-response perform automatic rollback on a frontline breach',
	},
	{
		key: 'admin.experimental',
		label: 'Experimental features',
		enabled: false,
		description: 'Surface in-development panels and controls in the console',
	},
];

/**
 * Seed the admin console scene through the sanctioned emitFact path.
 * Idempotent + hydration-safe: only seeds the feature flags if they were not
 * already restored from PluresDB, so operator toggles survive a restart. The
 * live health/readiness facts are (re)derived by the route on mount from the
 * real plugin loader + module state, so they are intentionally NOT seeded here.
 */
export function wireAdminScene(
	emitFact: (id: string, value: unknown) => void,
	query: (id: string) => unknown,
): void {
	if (query('admin.feature.flags') == null) {
		emitFact('admin.feature.flags', defaultFeatureFlags);
	}
	if (query('admin.audit.log') == null) {
		emitFact('admin.audit.log', []);
	}
}

export default adminModule;
