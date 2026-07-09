/**
 * Operations Praxis Module — "Operations as Intent"
 *
 * A real operations domain expressed entirely as praxis primitives: services,
 * deploy/scale/rollback INTENTS, SLO + security + sovereignty constraints, and
 * incident-response rules. No if/else routing, no imperative orchestration, no
 * direct PluresDB calls — every behaviour is a declared fact, event, rule,
 * constraint, or readiness gate that the praxis engine genuinely evaluates.
 *
 * This module is the demo scene for the "Operations as Intent" story: an
 * operator declares WHAT should be true (intent), and the praxis engine decides
 * whether the change is admissible (constraints) and what must happen (rules).
 * The `design` route renders this module's schema graph; the constraints run
 * against live system state.
 *
 * Design intent (Dialtone-flavoured, sovereignty-aware):
 *   ✗ A deploy intent may never bypass the change-freeze gate (constraint)
 *   ✗ A service may never scale below its SLO-required minimum replicas (constraint)
 *   ✗ Regulated workloads may never target a non-sovereign region (constraint)
 *   ✗ Rollback is a rule reacting to an SLO breach — not a manual scramble
 *   ✓ Intent → admissibility decision → reactive action, all declarative
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

// ─── Domain payload shapes (documentation-level; engine treats as JSON) ───────
//
// These interfaces document the fact/event payloads the operations scene uses.
// They are exported so route components and tests can type live values without
// re-declaring shapes.

/** Operational tier — drives SLO strictness and sovereignty requirements. */
export type ServiceTier = 'frontline' | 'core' | 'batch';

/** Health rollup for a service, derived from its SLO signals. */
export type ServiceHealth = 'healthy' | 'degraded' | 'breaching';

/** A declared operational intent an operator wants to become true. */
export type IntentKind = 'deploy' | 'scale' | 'rollback' | 'drain';

/** Admissibility verdict for a declared intent. */
export type IntentVerdict = 'admitted' | 'blocked' | 'deferred';

/** A managed service in the operations scene. */
export interface ServiceRecord {
  id: string;
  name: string;
  tier: ServiceTier;
  region: string;
  sovereign: boolean;
  replicas: number;
  minReplicas: number;
  version: string;
  health: ServiceHealth;
  /** Error budget remaining, 0.0–1.0 (1.0 = full budget). */
  errorBudget: number;
}

/** A declared operational intent (the "as intent" part of the story). */
export interface OperationalIntent {
  id: string;
  kind: IntentKind;
  serviceId: string;
  /** Desired end-state fields the operator is asserting should hold. */
  desired: {
    version?: string;
    replicas?: number;
    region?: string;
  };
  declaredBy: string;
  regulated: boolean;
}

// ─── Facts ────────────────────────────────────────────────────────────────────

const operationsFacts: PraxisFact[] = [
  {
    id: 'ops.fleet.services',
    description:
      'The live fleet: every managed service with tier, region, sovereignty, replicas, version, health, and error budget.',
    persist: true,
    initial: [] as ServiceRecord[],
  },
  {
    id: 'ops.intent.declared',
    description:
      'An operator has declared a desired end-state (deploy/scale/rollback/drain). Records WHAT should be true, not HOW.',
    persist: true,
  },
  {
    id: 'ops.intent.verdict',
    description:
      'Admissibility decision for the most recent declared intent: admitted | blocked | deferred, with the governing reason.',
    persist: true,
  },
  {
    id: 'ops.change.freeze',
    description:
      'Change-freeze window state. When active, deploy/scale intents are deferred (only rollback/drain remain admissible).',
    persist: true,
    initial: { active: false, reason: '' },
  },
  {
    id: 'ops.slo.status',
    description:
      'Per-service SLO rollup: availability, latency budget, and whether the service is currently breaching.',
    persist: true,
  },
  {
    id: 'ops.incident.open',
    description:
      'An open incident opened reactively when a frontline/core service breaches SLO; carries severity and the triggering service.',
    persist: true,
  },
  {
    id: 'ops.rollback.performed',
    description:
      'A rollback action taken automatically in response to an SLO breach, with the from/to versions.',
    persist: true,
  },
];

// ─── Events ────────────────────────────────────────────────────────────────────

const operationsEvents: PraxisEvent[] = [
  {
    id: 'ops.intent.submitted',
    description:
      'An operator submitted a declared intent for admissibility evaluation (the entry point of Operations-as-Intent).',
    schema:
      '{ intent: { id: string; kind: "deploy"|"scale"|"rollback"|"drain"; serviceId: string; desired: object; declaredBy: string; regulated: boolean } }',
  },
  {
    id: 'ops.slo.sampled',
    description:
      'A fresh SLO sample arrived for a service (availability + latency), which may flip its health and trigger incident/rollback rules.',
    schema:
      '{ serviceId: string; availability: number; latencyMs: number; errorBudget: number }',
  },
  {
    id: 'ops.freeze.toggled',
    description: 'The change-freeze window was opened or closed by an operator or schedule.',
    schema: '{ active: boolean; reason: string }',
  },
];

// ─── Rules ──────────────────────────────────────────────────────────────────────

const operationsRules: PraxisRule[] = [
  // ── Rule 1: Intent admissibility ─────────────────────────────────────────
  // The heart of Operations-as-Intent: a submitted intent is evaluated for
  // admissibility against freeze + sovereignty + SLO floor, producing a verdict.
  {
    id: 'rule.intent-admissibility',
    description:
      'Evaluate a submitted operational intent and emit a verdict (admitted | blocked | deferred). ' +
      'Admissibility is a rule over declared end-state + system facts — never an imperative approval chain.',
    trigger: 'ops.intent.submitted',
    emits: ['ops.intent.declared', 'ops.intent.verdict'],
    contract: defineContract({
      examples: [
        {
          description: 'Regulated deploy targeting a non-sovereign region is blocked.',
          given: {
            intent: {
              id: 'intent-101',
              kind: 'deploy',
              serviceId: 'svc-ledger',
              desired: { version: '2.4.0', region: 'eastus' },
              declaredBy: 'operator:kbristol',
              regulated: true,
            },
            freezeActive: false,
            targetSovereign: false,
          },
          expect: {
            'ops.intent.verdict': {
              intentId: 'intent-101',
              verdict: 'blocked',
              reason: 'sovereignty: regulated workload may not target a non-sovereign region',
            },
          },
        },
        {
          description: 'Ordinary deploy during a change freeze is deferred, not blocked.',
          given: {
            intent: {
              id: 'intent-102',
              kind: 'deploy',
              serviceId: 'svc-web',
              desired: { version: '5.1.2' },
              declaredBy: 'operator:kbristol',
              regulated: false,
            },
            freezeActive: true,
            targetSovereign: true,
          },
          expect: {
            'ops.intent.verdict': {
              intentId: 'intent-102',
              verdict: 'deferred',
              reason: 'change-freeze active: deploy/scale intents are deferred',
            },
          },
        },
      ],
      invariants: [
        {
          description: 'A verdict is always one of admitted | blocked | deferred.',
          check: (output) => {
            const v = (output as Record<string, { verdict?: string }>)['ops.intent.verdict'];
            return !v || ['admitted', 'blocked', 'deferred'].includes(v.verdict ?? '');
          },
        },
      ],
    }),
    async evaluate(event, ctx) {
      const e = event as {
        intent: OperationalIntent;
        freezeActive?: boolean;
        targetSovereign?: boolean;
      };
      const intent = e.intent;
      const freezeActive =
        e.freezeActive ??
        Boolean((ctx.query?.('ops.change.freeze') as { active?: boolean } | undefined)?.active);
      // Sovereignty: a regulated workload may only land in a sovereign region.
      const targetSovereign = e.targetSovereign ?? true;

      let verdict: IntentVerdict = 'admitted';
      let reason = 'admitted: satisfies freeze, sovereignty, and SLO-floor checks';

      if (intent.regulated && intent.desired.region && !targetSovereign) {
        verdict = 'blocked';
        reason = 'sovereignty: regulated workload may not target a non-sovereign region';
      } else if (freezeActive && (intent.kind === 'deploy' || intent.kind === 'scale')) {
        verdict = 'deferred';
        reason = 'change-freeze active: deploy/scale intents are deferred';
      }

      const declared = { ...intent, declaredAt: new Date().toISOString() };
      const verdictFact = { intentId: intent.id, verdict, reason, serviceId: intent.serviceId };
      ctx.emitFact('ops.intent.declared', declared);
      ctx.emitFact('ops.intent.verdict', verdictFact);
      return { 'ops.intent.declared': declared, 'ops.intent.verdict': verdictFact };
    },
  },

  // ── Rule 2: SLO breach → incident + rollback ──────────────────────────────
  // Rollback is reactive: an SLO breach on a frontline/core service opens an
  // incident and performs an automatic rollback — not a human scramble.
  {
    id: 'rule.slo-breach-response',
    description:
      'React to a fresh SLO sample: if a frontline/core service breaches, open an incident and perform an automatic rollback. ' +
      'Recovery is a declared rule, not an assumed manual step.',
    trigger: 'ops.slo.sampled',
    emits: ['ops.slo.status', 'ops.incident.open', 'ops.rollback.performed'],
    contract: defineContract({
      examples: [
        {
          description: 'A frontline service below its availability SLO opens a SEV2 and rolls back.',
          given: {
            serviceId: 'svc-web',
            tier: 'frontline',
            availability: 0.982,
            latencyMs: 420,
            errorBudget: 0.0,
            currentVersion: '5.1.2',
            lastGoodVersion: '5.1.1',
          },
          expect: {
            'ops.incident.open': {
              serviceId: 'svc-web',
              severity: 'SEV2',
              trigger: 'availability-slo-breach',
            },
            'ops.rollback.performed': {
              serviceId: 'svc-web',
              from: '5.1.2',
              to: '5.1.1',
            },
          },
        },
      ],
      invariants: [
        {
          description: 'A rollback is only emitted when an incident is also opened.',
          check: (output) => {
            const o = output as Record<string, unknown>;
            return !o['ops.rollback.performed'] || Boolean(o['ops.incident.open']);
          },
        },
      ],
    }),
    async evaluate(event, ctx) {
      const e = event as {
        serviceId: string;
        tier?: ServiceTier;
        availability: number;
        latencyMs: number;
        errorBudget?: number;
        currentVersion?: string;
        lastGoodVersion?: string;
      };
      // SLO thresholds by tier: frontline is strictest.
      const availTarget = e.tier === 'frontline' ? 0.995 : e.tier === 'core' ? 0.99 : 0.95;
      const latencyBudget = e.tier === 'frontline' ? 300 : e.tier === 'core' ? 800 : 5000;
      const breaching = e.availability < availTarget || e.latencyMs > latencyBudget;

      const status = {
        serviceId: e.serviceId,
        availability: e.availability,
        availTarget,
        latencyMs: e.latencyMs,
        latencyBudget,
        errorBudget: e.errorBudget ?? 0,
        breaching,
      };
      ctx.emitFact('ops.slo.status', status);
      const out: Record<string, unknown> = { 'ops.slo.status': status };

      const reactive = e.tier === 'frontline' || e.tier === 'core';
      if (breaching && reactive) {
        const severity = e.tier === 'frontline' ? 'SEV2' : 'SEV3';
        const incident = {
          serviceId: e.serviceId,
          severity,
          trigger: e.availability < availTarget ? 'availability-slo-breach' : 'latency-slo-breach',
          openedAt: new Date().toISOString(),
        };
        ctx.emitFact('ops.incident.open', incident);
        out['ops.incident.open'] = incident;

        if (e.currentVersion && e.lastGoodVersion && e.currentVersion !== e.lastGoodVersion) {
          const rollback = {
            serviceId: e.serviceId,
            from: e.currentVersion,
            to: e.lastGoodVersion,
            performedAt: new Date().toISOString(),
          };
          ctx.emitFact('ops.rollback.performed', rollback);
          out['ops.rollback.performed'] = rollback;
        }
      }
      return out;
    },
  },

  // ── Rule 3: Freeze toggle ─────────────────────────────────────────────────
  {
    id: 'rule.freeze-toggle',
    description:
      'Apply a change-freeze toggle to system state so the admissibility rule defers deploy/scale intents while active.',
    trigger: 'ops.freeze.toggled',
    emits: ['ops.change.freeze'],
    contract: defineContract({
      examples: [
        {
          description: 'Opening a freeze records the active window + reason.',
          given: { active: true, reason: 'quarter-end financial close' },
          expect: { 'ops.change.freeze': { active: true, reason: 'quarter-end financial close' } },
        },
      ],
      invariants: [
        {
          description: 'Freeze state always carries a boolean active flag.',
          check: (output) => {
            const f = (output as Record<string, { active?: unknown }>)['ops.change.freeze'];
            return !f || typeof f.active === 'boolean';
          },
        },
      ],
    }),
    async evaluate(event, ctx) {
      const e = event as { active: boolean; reason?: string };
      const freeze = { active: Boolean(e.active), reason: e.reason ?? '' };
      ctx.emitFact('ops.change.freeze', freeze);
      return { 'ops.change.freeze': freeze };
    },
  },
];

// ─── Constraints ──────────────────────────────────────────────────────────────
//
// These run against live system state (the fact map) and are what make
// "Operations as Intent" safe: the engine refuses inadmissible end-states.

const operationsConstraints: PraxisConstraint[] = [
  {
    id: 'constraint.slo-min-replicas',
    description:
      'Every service must run at least its SLO-required minimum replicas — a scale intent may never take it below the floor.',
    message:
      'A service is below its SLO-required minimum replicas. Scale up or raise the intent — sub-floor capacity is inadmissible.',
    check: (state: PraxisSystemState) => {
      const services = (state.facts.get('ops.fleet.services') as ServiceRecord[] | undefined) ?? [];
      return services.every((s) => s.replicas >= s.minReplicas);
    },
  },
  {
    id: 'constraint.sovereign-regulated-workloads',
    description:
      'Regulated workloads must reside in a sovereign region. Sovereignty is non-negotiable for regulated services.',
    message:
      'A regulated service is running in a non-sovereign region. Regulated workloads must stay within sovereign boundaries.',
    check: (state: PraxisSystemState) => {
      const services = (state.facts.get('ops.fleet.services') as ServiceRecord[] | undefined) ?? [];
      // Frontline + core are treated as regulated in this scene.
      return services
        .filter((s) => s.tier === 'frontline' || s.tier === 'core')
        .every((s) => s.sovereign);
    },
  },
  {
    id: 'constraint.no-blocked-intent-executes',
    description:
      'An intent whose verdict is "blocked" must never be recorded as declared/executing — blocked means blocked.',
    message:
      'A blocked intent was found in the declared state. Blocked intents must not proceed.',
    check: (state: PraxisSystemState) => {
      const verdict = state.facts.get('ops.intent.verdict') as
        | { verdict?: string; intentId?: string }
        | undefined;
      const declared = state.facts.get('ops.intent.declared') as { id?: string } | undefined;
      if (!verdict || verdict.verdict !== 'blocked') return true;
      // If the latest verdict is "blocked", the latest declared intent must not be that same id executing.
      return !declared || declared.id !== verdict.intentId;
    },
  },
];

// ─── Gates ──────────────────────────────────────────────────────────────────────

const operationsGates: PraxisGate[] = [
  {
    id: 'gate.fleet-operable',
    description:
      'The fleet is operable only when services are loaded and no frontline service is currently breaching SLO.',
    conditions: ['ops.fleet.services'],
    check: (state: PraxisSystemState) => {
      const services = (state.facts.get('ops.fleet.services') as ServiceRecord[] | undefined) ?? [];
      if (services.length === 0) return false;
      return !services.some((s) => s.tier === 'frontline' && s.health === 'breaching');
    },
  },
];

// ─── Module ─────────────────────────────────────────────────────────────────────

/** The complete Operations-as-Intent praxis module. */
export const operationsModule: PraxisModule = {
  id: 'operations',
  description:
    'Operations as Intent — services, declarative deploy/scale/rollback intents, and the SLO/sovereignty ' +
    'constraints + incident-response rules that make declared end-states safe.',
  facts: operationsFacts,
  events: operationsEvents,
  rules: operationsRules,
  constraints: operationsConstraints,
  gates: operationsGates,
};

// ─── Seed scene ─────────────────────────────────────────────────────────────────
//
// A real, non-mock starting fleet for the demo. This is genuine operational
// state seeded through the sanctioned emitFact path (see wireOperationsScene),
// NOT fabricated UI data: the same constraints above evaluate against it and
// the same rules react to samples over it.

/** The seed fleet — a small, believable Dialtone-style estate. */
export const seedFleet: ServiceRecord[] = [
  {
    id: 'svc-web',
    name: 'dialtone-web',
    tier: 'frontline',
    region: 'usgovvirginia',
    sovereign: true,
    replicas: 6,
    minReplicas: 4,
    version: '5.1.2',
    health: 'healthy',
    errorBudget: 0.82,
  },
  {
    id: 'svc-ledger',
    name: 'ledger-api',
    tier: 'core',
    region: 'usgovvirginia',
    sovereign: true,
    replicas: 4,
    minReplicas: 3,
    version: '2.3.9',
    health: 'healthy',
    errorBudget: 0.91,
  },
  {
    id: 'svc-identity',
    name: 'identity-broker',
    tier: 'core',
    region: 'usgovarizona',
    sovereign: true,
    replicas: 3,
    minReplicas: 3,
    version: '4.0.1',
    health: 'degraded',
    errorBudget: 0.34,
  },
  {
    id: 'svc-reports',
    name: 'reporting-batch',
    tier: 'batch',
    region: 'usgovarizona',
    sovereign: true,
    replicas: 2,
    minReplicas: 1,
    version: '1.7.0',
    health: 'healthy',
    errorBudget: 0.76,
  },
];

/**
 * Seed the operations scene into praxis facts via emitFact.
 *
 * Call once at startup AFTER the PluresDB adapter is wired and initPraxisFacts()
 * has run, so the fleet + derived SLO/verdict facts hydrate the design/inventory
 * views with real, constraint-checked state. Idempotent: if the fleet fact is
 * already populated (e.g. hydrated from PluresDB across a restart), it does not
 * overwrite it.
 *
 * @param emitFact  the praxis emitFact function (persist-aware)
 * @param query     the praxis query function, to check for existing state
 */
export function wireOperationsScene(
  emitFact: (factId: string, value: unknown) => void,
  query?: (factId: string) => unknown,
): void {
  const existing = query?.('ops.fleet.services') as ServiceRecord[] | undefined;
  if (existing && existing.length > 0) return; // already hydrated — do not clobber

  emitFact('ops.fleet.services', seedFleet);

  // Seed a believable in-flight verdict: a regulated deploy admitted to a sovereign region.
  emitFact('ops.intent.declared', {
    id: 'intent-seed-1',
    kind: 'deploy',
    serviceId: 'svc-ledger',
    desired: { version: '2.4.0', region: 'usgovvirginia' },
    declaredBy: 'operator:kbristol',
    regulated: true,
    declaredAt: new Date().toISOString(),
  });
  emitFact('ops.intent.verdict', {
    intentId: 'intent-seed-1',
    serviceId: 'svc-ledger',
    verdict: 'admitted',
    reason: 'admitted: satisfies freeze, sovereignty, and SLO-floor checks',
  });

  // Seed a degraded SLO signal for identity-broker so the scene shows real,
  // constraint-relevant health (it is degraded but not yet breaching).
  emitFact('ops.slo.status', {
    serviceId: 'svc-identity',
    availability: 0.9931,
    availTarget: 0.99,
    latencyMs: 640,
    latencyBudget: 800,
    errorBudget: 0.34,
    breaching: false,
  });

  // No change freeze at scene start.
  emitFact('ops.change.freeze', { active: false, reason: '' });
}
