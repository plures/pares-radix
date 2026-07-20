/**
 * Operations module — logic tests (C-TEST-002: channel-independent).
 *
 * Proves the "Operations as Intent" scene is REAL by exercising the module's
 * own primitives directly — no adapter, no Tauri, no transport:
 *   - wireOperationsScene seeds the fleet + verdict facts through emitFact
 *   - the module's constraints actually hold on the seeded fleet
 *   - the module's constraints actually FAIL on an inadmissible fleet
 *   - rule.intent-admissibility computes real verdicts (block/defer/admit)
 *   - rule.slo-breach-response opens an incident + rolls back on a frontline breach
 *
 * The only "double" here is an in-memory fact Map standing in for the reactive
 * store — a legitimate test seam per AGENTS.md, never a runtime stub.
 */

import { describe, it, expect } from 'vitest';
import type { PraxisContext, PraxisSystemState } from '../types/praxis.js';
import {
	operationsModule,
	wireOperationsScene,
	seedFleet,
	type ServiceRecord,
} from './operations.js';

/** Build an in-memory emitFact/query pair backed by a plain Map. */
function makeStore() {
	const facts = new Map<string, unknown>();
	const emitFact = (id: string, value: unknown) => facts.set(id, value);
	const query = (id: string) => facts.get(id);
	const state = (): PraxisSystemState => ({ facts });
	const ctx = (): PraxisContext => ({
		// settings is unused by the operations rules; a minimal stub satisfies the type.
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

describe('operations module — scene seeding', () => {
	it('wireOperationsScene seeds a real fleet through emitFact', () => {
		const { facts, emitFact, query } = makeStore();
		wireOperationsScene(emitFact, query);

		const fleet = facts.get('ops.fleet.services') as ServiceRecord[];
		expect(Array.isArray(fleet)).toBe(true);
		expect(fleet.length).toBe(seedFleet.length);
		expect(fleet.map((s) => s.id)).toContain('svc-web');
		// A seeded, admitted verdict is present for the demo.
		expect((facts.get('ops.intent.verdict') as { verdict?: string }).verdict).toBe('admitted');
	});

	it('is idempotent / hydration-safe: does not clobber an existing fleet', () => {
		const { facts, emitFact, query } = makeStore();
		facts.set('ops.fleet.services', [{ id: 'pre-existing' }]);
		wireOperationsScene(emitFact, query);
		expect((facts.get('ops.fleet.services') as Array<{ id: string }>)[0].id).toBe('pre-existing');
	});
});

describe('operations module — constraints run on live state', () => {
	function findConstraint(id: string) {
		const c = operationsModule.constraints.find((x) => x.id === id);
		if (!c) throw new Error(`constraint ${id} not found`);
		return c;
	}

	it('all constraints hold on the seeded fleet', () => {
		const { emitFact, query, state } = makeStore();
		wireOperationsScene(emitFact, query);
		for (const c of operationsModule.constraints) {
			expect(c.check(state()), `${c.id} should hold on seed`).toBe(true);
		}
	});

	it('slo-min-replicas FAILS when a service drops below its floor', () => {
		const { facts, state } = makeStore();
		facts.set('ops.fleet.services', [
			{ ...seedFleet[0], replicas: 2, minReplicas: 4 }, // below floor
		]);
		expect(findConstraint('constraint.slo-min-replicas').check(state())).toBe(false);
	});

	it('sovereign-regulated-workloads FAILS when a core service is non-sovereign', () => {
		const { facts, state } = makeStore();
		facts.set('ops.fleet.services', [
			{ ...seedFleet[1], sovereign: false }, // core, non-sovereign
		]);
		expect(findConstraint('constraint.sovereign-regulated-workloads').check(state())).toBe(false);
	});
});

describe('operations module — rules compute real verdicts', () => {
	function rule(id: string) {
		const r = operationsModule.rules.find((x) => x.id === id);
		if (!r) throw new Error(`rule ${id} not found`);
		return r;
	}

	it('blocks a regulated deploy targeting a non-sovereign region', async () => {
		const { ctx } = makeStore();
		const out = await rule('rule.intent-admissibility').evaluate(
			{
				intent: {
					id: 'i1',
					kind: 'deploy',
					serviceId: 'svc-ledger',
					desired: { version: '2.4.0', region: 'eastus' },
					declaredBy: 'op',
					regulated: true,
				},
				freezeActive: false,
				targetSovereign: false,
			},
			ctx(),
		);
		expect((out['ops.intent.verdict'] as { verdict: string }).verdict).toBe('blocked');
	});

	it('defers an ordinary deploy during a change freeze', async () => {
		const { ctx } = makeStore();
		const out = await rule('rule.intent-admissibility').evaluate(
			{
				intent: {
					id: 'i2',
					kind: 'deploy',
					serviceId: 'svc-web',
					desired: { version: '5.2.0' },
					declaredBy: 'op',
					regulated: false,
				},
				freezeActive: true,
				targetSovereign: true,
			},
			ctx(),
		);
		expect((out['ops.intent.verdict'] as { verdict: string }).verdict).toBe('deferred');
	});

	it('opens an incident and rolls back on a frontline SLO breach', async () => {
		const { ctx } = makeStore();
		const out = await rule('rule.slo-breach-response').evaluate(
			{
				serviceId: 'svc-web',
				tier: 'frontline',
				availability: 0.982, // below the 0.995 frontline target
				latencyMs: 420,
				errorBudget: 0.0,
				currentVersion: '5.1.2',
				lastGoodVersion: '5.1.1',
			},
			ctx(),
		);
		expect(out['ops.incident.open']).toBeDefined();
		const rb = out['ops.rollback.performed'] as { from: string; to: string };
		expect(rb.from).toBe('5.1.2');
		expect(rb.to).toBe('5.1.1');
	});

	it('does NOT roll back a healthy frontline sample', async () => {
		const { ctx } = makeStore();
		const out = await rule('rule.slo-breach-response').evaluate(
			{
				serviceId: 'svc-web',
				tier: 'frontline',
				availability: 0.999,
				latencyMs: 120,
				errorBudget: 0.9,
				currentVersion: '5.1.2',
				lastGoodVersion: '5.1.1',
			},
			ctx(),
		);
		expect(out['ops.incident.open']).toBeUndefined();
		expect(out['ops.rollback.performed']).toBeUndefined();
	});
});
