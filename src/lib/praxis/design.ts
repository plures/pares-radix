/**
 * Design Mode Praxis Module
 *
 * Enables self-modification of praxis rules, constraints, and UX contracts
 * from within the running application. Design mode is itself a praxis module
 * governed by the same primitives it enables editing of.
 *
 * Architecture: docs/DESIGN-MODE.md
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

// ─── Types ───────────────────────────────────────────────────────────────────

export type SchemaKind = 'fact' | 'event' | 'rule' | 'constraint' | 'gate' | 'route' | 'component';

export interface DesignSchema {
  /** Unique schema ID */
  id: string;
  /** Kind of praxis primitive */
  kind: SchemaKind;
  /** Source module ID */
  moduleId: string;
  /** Human-readable label */
  label: string;
  /** Description */
  description: string;
  /** The actual primitive definition (JSON-serializable) */
  definition: Record<string, unknown>;
  /** Whether this schema is user-created (vs built-in) */
  userCreated: boolean;
  /** Last modified timestamp */
  updatedAt: string;
}

export interface DesignDraft {
  /** Schema being edited */
  schemaId: string;
  /** Modified definition */
  definition: Record<string, unknown>;
  /** Validation errors (empty = valid) */
  errors: string[];
  /** Whether the draft differs from the saved schema */
  dirty: boolean;
}

// ─── Facts ───────────────────────────────────────────────────────────────────

const designFacts: PraxisFact[] = [
  {
    id: 'design.mode.active',
    description: 'Whether design mode is currently enabled',
    persist: true,
  },
  {
    id: 'design.schema.registry',
    description: 'All editable schemas indexed by id',
    persist: true,
  },
  {
    id: 'design.edit.active',
    description: 'Currently selected schema for editing (null if none)',
    persist: false,
  },
  {
    id: 'design.edit.draft',
    description: 'Current unsaved modifications to a schema',
    persist: false,
  },
  {
    id: 'design.edit.validation',
    description: 'Real-time validation state of the current draft',
    persist: false,
  },
  {
    id: 'design.history',
    description: 'Audit trail of all design changes (decision ledger)',
    persist: true,
  },
];

// ─── Events ──────────────────────────────────────────────────────────────────

const designEvents: PraxisEvent[] = [
  {
    id: 'design.mode.toggled',
    description: 'User toggled design mode on or off',
    schema: '{ active: boolean }',
  },
  {
    id: 'design.schema.selected',
    description: 'User selected a schema to view/edit',
    schema: '{ schemaId: string }',
  },
  {
    id: 'design.schema.draft.updated',
    description: 'User modified a field in the draft editor',
    schema: '{ schemaId: string, definition: Record<string, unknown> }',
  },
  {
    id: 'design.schema.saved',
    description: 'User committed a schema change',
    schema: '{ schemaId: string, definition: Record<string, unknown> }',
  },
  {
    id: 'design.schema.reverted',
    description: 'User discarded unsaved changes',
    schema: '{ schemaId: string }',
  },
  {
    id: 'design.schema.created',
    description: 'User created a new schema (rule, constraint, etc.)',
    schema: '{ kind: SchemaKind, definition: Record<string, unknown> }',
  },
  {
    id: 'design.schema.deleted',
    description: 'User deleted a user-created schema',
    schema: '{ schemaId: string }',
  },
];

// ─── Rules ───────────────────────────────────────────────────────────────────

const designRules: PraxisRule[] = [
  // ── Rule 1: Design Mode Toggle ─────────────────────────────────────────────
  {
    id: 'rule.design-mode-toggle',
    description: 'Toggle design mode and update UI affordances',
    trigger: 'design.mode.toggled',
    emits: ['design.mode.active'],
    contract: defineContract({
      examples: [
        {
          given: { active: true },
          expect: { fact: 'design.mode.active', payload: { active: true } },
          description: 'enabling design mode sets the active fact to true',
        },
        {
          given: { active: false },
          expect: { fact: 'design.mode.active', payload: { active: false } },
          description: 'disabling design mode sets the active fact to false and clears editor state',
        },
      ],
      invariants: [
        {
          description: 'design.mode.active must always be emitted',
          check: (output) => (output as { fact: string }).fact === 'design.mode.active',
        },
        {
          description: 'payload.active must be a boolean',
          check: (output) => typeof (output as { payload: { active: unknown } }).payload.active === 'boolean',
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as { active: boolean };
      const payload = { active: ev.active };
      ctx.emitFact('design.mode.active', payload);

      // Clear editor state when exiting design mode
      if (!ev.active) {
        ctx.emitFact('design.edit.active', null);
        ctx.emitFact('design.edit.draft', null);
        ctx.emitFact('design.edit.validation', null);
      }

      return { fact: 'design.mode.active', payload };
    },
  },

  // ── Rule 2: Schema Selection ───────────────────────────────────────────────
  {
    id: 'rule.design-schema-select',
    description: 'Select a schema for editing and populate the draft',
    trigger: 'design.schema.selected',
    emits: ['design.edit.active', 'design.edit.draft'],
    contract: defineContract({
      examples: [
        {
          given: {
            schemaId: 'rule.plugin-registration',
            registry: {
              'rule.plugin-registration': {
                id: 'rule.plugin-registration',
                kind: 'rule',
                definition: { trigger: 'plugin.install.requested' },
              },
            },
          },
          expect: {
            fact: 'design.edit.active',
            payload: { schemaId: 'rule.plugin-registration' },
          },
          description: 'selecting a schema populates the editor with its definition',
        },
      ],
      invariants: [
        {
          description: 'design.edit.active must be emitted',
          check: (output) => (output as { fact: string }).fact === 'design.edit.active',
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as { schemaId: string; registry?: Record<string, DesignSchema> };
      const schema = ev.registry?.[ev.schemaId];

      ctx.emitFact('design.edit.active', { schemaId: ev.schemaId });

      if (schema) {
        ctx.emitFact('design.edit.draft', {
          schemaId: ev.schemaId,
          definition: { ...schema.definition },
          errors: [],
          dirty: false,
        });
      }

      return { fact: 'design.edit.active', payload: { schemaId: ev.schemaId } };
    },
  },

  // ── Rule 3: Draft Validation ───────────────────────────────────────────────
  {
    id: 'rule.design-draft-validate',
    description: 'Validate a draft schema change in real-time',
    trigger: 'design.schema.draft.updated',
    emits: ['design.edit.draft', 'design.edit.validation'],
    contract: defineContract({
      examples: [
        {
          given: {
            schemaId: 'rule.example',
            definition: { trigger: 'some.event', emits: ['some.fact'] },
          },
          expect: {
            fact: 'design.edit.validation',
            payload: { valid: true, errors: [] },
          },
          description: 'valid draft produces empty error list',
        },
        {
          given: {
            schemaId: 'rule.example',
            definition: { trigger: '', emits: [] },
          },
          expect: {
            fact: 'design.edit.validation',
            payload: { valid: false, errors: ['Rule must have a trigger event'] },
          },
          description: 'rule without trigger fails validation',
        },
      ],
      invariants: [
        {
          description: 'design.edit.validation must always be emitted',
          check: (output) => (output as { fact: string }).fact === 'design.edit.validation',
        },
        {
          description: 'validation payload must include valid boolean and errors array',
          check: (output) => {
            const p = (output as { payload: { valid: unknown; errors: unknown } }).payload;
            return typeof p.valid === 'boolean' && Array.isArray(p.errors);
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as { schemaId: string; definition: Record<string, unknown> };
      const errors: string[] = [];

      // Basic validation — kind-specific validators would be registered via plugins
      const def = ev.definition;
      if (def.trigger !== undefined && !def.trigger) {
        errors.push('Rule must have a trigger event');
      }
      if (def.description !== undefined && !def.description) {
        errors.push('Description is required');
      }
      if (def.id !== undefined && !def.id) {
        errors.push('ID is required');
      }

      const validation = { valid: errors.length === 0, errors };
      ctx.emitFact('design.edit.validation', validation);
      ctx.emitFact('design.edit.draft', {
        schemaId: ev.schemaId,
        definition: def,
        errors,
        dirty: true,
      });

      return { fact: 'design.edit.validation', payload: validation };
    },
  },

  // ── Rule 4: Schema Save with Audit ─────────────────────────────────────────
  {
    id: 'rule.design-schema-save',
    description: 'Persist schema changes to PluresDB and log to decision ledger',
    trigger: 'design.schema.saved',
    emits: ['design.schema.registry', 'design.history'],
    contract: defineContract({
      examples: [
        {
          given: {
            schemaId: 'rule.custom-greeting',
            definition: { trigger: 'user.message', emits: ['greeting.sent'] },
          },
          expect: {
            fact: 'design.schema.registry',
          },
          description: 'saving a schema updates the registry and logs the change',
        },
      ],
      invariants: [
        {
          description: 'design.schema.registry must be emitted on save',
          check: (output) => (output as { fact: string }).fact === 'design.schema.registry',
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as { schemaId: string; definition: Record<string, unknown> };

      // Get current registry
      const registry = (ctx.query?.('design.schema.registry') as Record<string, DesignSchema>) ?? {};

      // Update the schema
      const existing = registry[ev.schemaId];
      registry[ev.schemaId] = {
        ...(existing ?? { id: ev.schemaId, kind: 'rule', moduleId: 'user', label: ev.schemaId, userCreated: true }),
        definition: ev.definition,
        description: (ev.definition.description as string) ?? existing?.description ?? '',
        updatedAt: new Date().toISOString(),
      };

      ctx.emitFact('design.schema.registry', registry);

      // Audit trail
      const historyEntry = {
        action: 'save',
        schemaId: ev.schemaId,
        definition: ev.definition,
        timestamp: new Date().toISOString(),
      };
      ctx.emitFact('design.history', historyEntry);

      // Clear editor state
      ctx.emitFact('design.edit.draft', null);
      ctx.emitFact('design.edit.active', null);

      return { fact: 'design.schema.registry', payload: registry };
    },
  },
];

// ─── Constraints ─────────────────────────────────────────────────────────────

const designConstraints: PraxisConstraint[] = [
  {
    id: 'constraint.schema-validity',
    description: 'Saved schemas must pass validation — no invalid schemas in the registry',
    message: 'Schema registry contains invalid entries — all schemas must pass validation',
    check: (state: PraxisSystemState) => {
      const registry = state.facts.get('design.schema.registry') as Record<string, DesignSchema> | undefined;
      if (!registry) return true;

      for (const schema of Object.values(registry)) {
        if (schema.kind === 'rule') {
          if (!schema.definition.trigger) return false;
        }
        if (!schema.id || !schema.description) return false;
      }
      return true;
    },
  },
  {
    id: 'constraint.design-mode-required',
    description: 'Schema edits can only occur when design mode is active',
    message: 'Cannot edit schemas outside of design mode',
    check: (state: PraxisSystemState) => {
      const draft = state.facts.get('design.edit.draft') as DesignDraft | undefined;
      if (!draft || !draft.dirty) return true;

      const designMode = state.facts.get('design.mode.active') as { active: boolean } | undefined;
      return designMode?.active === true;
    },
  },
];

// ─── Gates ───────────────────────────────────────────────────────────────────

const designGates: PraxisGate[] = [
  {
    id: 'design-ready',
    description: 'Design mode infrastructure initialized — schema registry populated',
    conditions: ['design.schema.registry'],
    check: (state: PraxisSystemState) => {
      return state.facts.has('design.schema.registry');
    },
  },
];

// ─── Module Export ───────────────────────────────────────────────────────────

export const designModule: PraxisModule = {
  id: 'radix.design',
  description: 'Self-modification capability — edit praxis rules, constraints, and UX contracts from within the app',
  facts: designFacts,
  events: designEvents,
  rules: designRules,
  constraints: designConstraints,
  gates: designGates,
};

// ─── Schema Registry Builder ─────────────────────────────────────────────────

/**
 * Build a schema registry from one or more praxis modules.
 * This converts all praxis primitives into editable DesignSchema objects.
 */
export function buildSchemaRegistry(...modules: PraxisModule[]): Record<string, DesignSchema> {
  const registry: Record<string, DesignSchema> = {};

  for (const module of modules) {
    for (const fact of module.facts) {
      registry[fact.id] = {
        id: fact.id,
        kind: 'fact',
        moduleId: module.id,
        label: fact.id,
        description: fact.description,
        definition: { id: fact.id, description: fact.description, persist: fact.persist },
        userCreated: false,
        updatedAt: new Date().toISOString(),
      };
    }

    for (const event of module.events) {
      registry[event.id] = {
        id: event.id,
        kind: 'event',
        moduleId: module.id,
        label: event.id,
        description: event.description,
        definition: { id: event.id, description: event.description, schema: event.schema },
        userCreated: false,
        updatedAt: new Date().toISOString(),
      };
    }

    for (const rule of module.rules) {
      registry[rule.id] = {
        id: rule.id,
        kind: 'rule',
        moduleId: module.id,
        label: rule.id,
        description: rule.description,
        definition: {
          id: rule.id,
          description: rule.description,
          trigger: rule.trigger,
          emits: rule.emits,
          contractExamples: rule.contract.examples.length,
          contractInvariants: rule.contract.invariants.length,
        },
        userCreated: false,
        updatedAt: new Date().toISOString(),
      };
    }

    for (const constraint of module.constraints) {
      registry[constraint.id] = {
        id: constraint.id,
        kind: 'constraint',
        moduleId: module.id,
        label: constraint.id,
        description: constraint.description,
        definition: { id: constraint.id, description: constraint.description, message: constraint.message },
        userCreated: false,
        updatedAt: new Date().toISOString(),
      };
    }

    for (const gate of module.gates) {
      registry[gate.id] = {
        id: gate.id,
        kind: 'gate',
        moduleId: module.id,
        label: gate.id,
        description: gate.description,
        definition: { id: gate.id, description: gate.description, conditions: gate.conditions },
        userCreated: false,
        updatedAt: new Date().toISOString(),
      };
    }
  }

  return registry;
}
