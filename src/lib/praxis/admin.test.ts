/**
 * Admin module — logic tests (C-TEST-002: channel-independent).
 *
 * Exercises the admin console's real decision logic directly — no adapter, no
 * Tauri, no transport. The pure helpers are the SAME ones the rules call, so
 * these tests pin the twin of admin-console.px:
 *   - computeReadiness / classifyPluginHealth / decideAdminAction / collectViolations
 *   - the rules emit the expected facts through emitFact
 *   - the constraints hold on well-formed state and fail on a bypass
 */

import { describe, it, expect } from 'vitest';
import type { PraxisContext, PraxisSystemState, PraxisConstraint } from '../types/praxis.js';
import {
	adminModule,
	wireAdminScene,
	computeReadiness,
	classifyPluginHealth,
	decideAdminAction,
	collectViolations,
	defaultFeatureFlags,
	type SystemReadiness,
	type PluginHealth,
	type AdminActionVerdict,
} from './admin.js';

function makeStore() {
	const facts = new Map<string, unknown>();
	const emitFact = (id: string, value: unknown) => facts.set(id, value);
	const query = (id: string) => facts.get(id);
	const state = (): PraxisSystemState => ({ facts });
	const ctx = (): PraxisContext => ({
		settings: {
			get: () => undefined,
			set: () => {},
			keys: () => [],
			delete: () => {},
		} as unknown as PraxisContext['settings'],
		emitFact,
		query,
	});
	return { facts, emitFact, query, state, ctx };
}

function rule(id: string) {
	const r = adminModule.rules.find((x) => x.id === id);
	if (!r) throw new Error(`rule ${id} not found`);
	return r;
}
function constraint(id: string): PraxisConstraint {
	const c = adminModule.constraints.find((x) => x.id === id);
	if (!c) throw new Error(`constraint ${id} not found`);
	return c;
}

describe('admin — pure decision helpers (twins of admin-console.px)', () => {
	it('computeReadiness: operable iff all gates open AND no error violations', () => {
		expect(computeReadiness(3, 3, 0)).toBe(true);
		expect(computeReadiness(2, 3, 0)).toBe(false); // closed gate
		expect(computeReadiness(3, 3, 1)).toBe(false); // error violation
		expect(computeReadiness(0, 0, 0)).toBe(true); // vacuously operable
	});

	it('classifyPluginHealth: inactive / failed / degraded / healthy', () => {
		expect(classifyPluginHealth(false, undefined, 5)).toBe('inactive');
		expect(classifyPluginHealth(true, 'boom', 5)).toBe('failed');
		expect(classifyPluginHealth(true, undefined, 0)).toBe('degraded');
		expect(classifyPluginHealth(true, undefined, 2)).toBe('healthy');
	});

	it('decideAdminAction: blocks deactivate w/ active dependent, allows reversible', () => {
		expect(decideAdminAction('deactivate', 'storage', ['commerce']).decision).toBe('blocked');
		expect(decideAdminAction('deactivate', 'storage', []).decision).toBe('allowed');
		expect(decideAdminAction('toggle-flag', 'admin.x', []).decision).toBe('allowed');
		expect(decideAdminAction('reload', 'canvas', []).decision).toBe('allowed');
	});

	it('collectViolations: returns only failing constraints, stable-ordered', () => {
		const facts = new Map<string, unknown>();
		const cs: PraxisConstraint[] = [
			{ id: 'z.fails', description: '', check: () => false, message: 'z' },
			{ id: 'a.holds', description: '', check: () => true, message: 'a' },
			{ id: 'b.throws', description: '', check: () => {
				throw new Error('x');
			}, message: 'b' },
		];
		const out = collectViolations(cs, { facts });
		expect(out.map((v) => v.id)).toEqual(['b.throws', 'z.fails']); // sorted, holds excluded
	});
});

describe('admin — rules emit the expected facts', () => {
	it('rule.derive-system-readiness computes operability', async () => {
		const { facts, ctx } = makeStore();
		await rule('rule.derive-system-readiness').evaluate(
			{ openGates: 2, totalGates: 3, errorViolations: 0 },
			ctx(),
		);
		expect((facts.get('admin.system.readiness') as SystemReadiness).operable).toBe(false);
	});

	it('rule.assess-plugin-health builds a status roster', async () => {
		const { facts, ctx } = makeStore();
		await rule('rule.assess-plugin-health').evaluate(
			{
				plugins: [
					{ pluginId: 'canvas', name: 'AI Canvas', version: '0.1.0', active: true, surface: 2 },
					{ pluginId: 'dead', name: 'Dead', version: '1.0.0', active: true, surface: 0 },
					{ pluginId: 'off', name: 'Off', version: '1.0.0', active: false, surface: 3 },
				],
			},
			ctx(),
		);
		const roster = facts.get('admin.plugins.health') as PluginHealth[];
		expect(roster.map((r) => r.status)).toEqual(['healthy', 'degraded', 'inactive']);
	});

	it('rule.authorize-admin-action emits verdict + appends audit log', async () => {
		const { facts, ctx } = makeStore();
		facts.set('admin.audit.log', []);
		await rule('rule.authorize-admin-action').evaluate(
			{ action: 'deactivate', target: 'storage', activeDependents: ['commerce'] },
			ctx(),
		);
		expect((facts.get('admin.action.verdict') as AdminActionVerdict).verdict).toBe('blocked');
		expect((facts.get('admin.audit.log') as unknown[]).length).toBe(1);
	});
});

describe('admin — constraints + scene seeding', () => {
	it('wireAdminScene seeds default flags + empty audit log (idempotent)', () => {
		const { facts, emitFact, query } = makeStore();
		wireAdminScene(emitFact, query);
		expect((facts.get('admin.feature.flags') as unknown[]).length).toBe(defaultFeatureFlags.length);
		expect(facts.get('admin.audit.log')).toEqual([]);
		// Second call must not clobber an operator change.
		facts.set('admin.feature.flags', [{ key: 'k', label: 'l', enabled: false, description: 'd' }]);
		wireAdminScene(emitFact, query);
		expect((facts.get('admin.feature.flags') as unknown[]).length).toBe(1);
	});

	it('constraint.feature-flags-well-formed holds on defaults, fails on malformed', () => {
		const { facts, state } = makeStore();
		facts.set('admin.feature.flags', defaultFeatureFlags);
		expect(constraint('constraint.feature-flags-well-formed').check(state())).toBe(true);
		facts.set('admin.feature.flags', [{ key: 123, enabled: 'yes' }]);
		expect(constraint('constraint.feature-flags-well-formed').check(state())).toBe(false);
	});

	it('constraint.no-blocked-action-in-audit fails when a blocked action is marked executed', () => {
		const { facts, state } = makeStore();
		facts.set('admin.audit.log', [{ verdict: 'blocked', executed: true }]);
		expect(constraint('constraint.no-blocked-action-in-audit').check(state())).toBe(false);
		facts.set('admin.audit.log', [{ verdict: 'blocked', executed: false }]);
		expect(constraint('constraint.no-blocked-action-in-audit').check(state())).toBe(true);
	});
});
