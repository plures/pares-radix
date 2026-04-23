/**
 * Design Mode Hot-Reload Engine
 *
 * When a schema is saved in design mode, this module applies the change
 * to the live praxis modules without requiring a page reload.
 *
 * The hot-reload flow:
 * 1. User edits a schema in the Rule Editor
 * 2. On save, design.schema.saved event fires
 * 3. rule.design-schema-save (design.ts) persists to registry + audit trail
 * 4. This module detects the registry change and patches the live modules
 * 5. UI re-renders via praxis-svelte reactive bindings
 */

import type { PraxisModule, PraxisRule, PraxisConstraint, PraxisFact } from '../types/praxis.js';
import type { DesignSchema } from './design.js';
import { defineContract } from './shell.js';

/** Registered live modules that can be hot-patched */
const liveModules = new Map<string, PraxisModule>();

/** Register a module for hot-reload */
export function registerForHotReload(module: PraxisModule): void {
  liveModules.set(module.id, module);
}

/** Get all registered live modules */
export function getLiveModules(): PraxisModule[] {
  return [...liveModules.values()];
}

/**
 * Apply a schema change to the live praxis modules.
 *
 * For built-in schemas: updates the existing primitive in-place.
 * For user-created schemas: adds to the 'radix.design' module (or creates if needed).
 */
export function applySchemaChange(schema: DesignSchema): { applied: boolean; error?: string } {
  const module = liveModules.get(schema.moduleId);

  if (schema.userCreated) {
    // User-created schemas go into a dynamic user module
    return applyUserSchema(schema);
  }

  if (!module) {
    return { applied: false, error: `Module '${schema.moduleId}' not registered for hot-reload` };
  }

  switch (schema.kind) {
    case 'fact':
      return applyFactChange(module, schema);
    case 'rule':
      return applyRuleChange(module, schema);
    case 'constraint':
      return applyConstraintChange(module, schema);
    default:
      return { applied: false, error: `Hot-reload not supported for kind '${schema.kind}'` };
  }
}

function applyFactChange(module: PraxisModule, schema: DesignSchema): { applied: boolean; error?: string } {
  const idx = module.facts.findIndex(f => f.id === schema.id);
  if (idx === -1) return { applied: false, error: `Fact '${schema.id}' not found in module` };

  const def = schema.definition;
  module.facts[idx] = {
    id: def.id as string,
    description: def.description as string,
    persist: def.persist as boolean ?? false,
  };

  return { applied: true };
}

function applyRuleChange(module: PraxisModule, schema: DesignSchema): { applied: boolean; error?: string } {
  const idx = module.rules.findIndex(r => r.id === schema.id);
  if (idx === -1) return { applied: false, error: `Rule '${schema.id}' not found in module` };

  const def = schema.definition;
  const existing = module.rules[idx];

  // Update metadata — preserve the evaluate function and contract (those require code)
  module.rules[idx] = {
    ...existing,
    description: (def.description as string) ?? existing.description,
    trigger: (def.trigger as string) ?? existing.trigger,
    emits: Array.isArray(def.emits) ? def.emits as string[] :
           typeof def.emits === 'string' ? (def.emits as string).split(',').map(s => s.trim()).filter(Boolean) :
           existing.emits,
  };

  return { applied: true };
}

function applyConstraintChange(module: PraxisModule, schema: DesignSchema): { applied: boolean; error?: string } {
  const idx = module.constraints.findIndex(c => c.id === schema.id);
  if (idx === -1) return { applied: false, error: `Constraint '${schema.id}' not found in module` };

  const def = schema.definition;
  const existing = module.constraints[idx];

  module.constraints[idx] = {
    ...existing,
    description: (def.description as string) ?? existing.description,
    message: (def.message as string) ?? existing.message,
  };

  return { applied: true };
}

function applyUserSchema(schema: DesignSchema): { applied: boolean; error?: string } {
  // User schemas get added to a dedicated dynamic module
  let userModule = liveModules.get('radix.user');
  if (!userModule) {
    userModule = {
      id: 'radix.user',
      description: 'User-created praxis primitives from design mode',
      facts: [],
      events: [],
      rules: [],
      constraints: [],
      gates: [],
    };
    liveModules.set('radix.user', userModule);
  }

  switch (schema.kind) {
    case 'fact': {
      const def = schema.definition;
      const existing = userModule.facts.findIndex(f => f.id === schema.id);
      const fact: PraxisFact = {
        id: def.id as string,
        description: def.description as string,
        persist: def.persist as boolean ?? false,
      };
      if (existing >= 0) userModule.facts[existing] = fact;
      else userModule.facts.push(fact);
      return { applied: true };
    }
    case 'constraint': {
      const def = schema.definition;
      const existing = userModule.constraints.findIndex(c => c.id === schema.id);
      const constraint: PraxisConstraint = {
        id: def.id as string,
        description: def.description as string,
        message: def.message as string,
        check: () => true, // User constraints need a code editor — placeholder for now
      };
      if (existing >= 0) userModule.constraints[existing] = constraint;
      else userModule.constraints.push(constraint);
      return { applied: true };
    }
    case 'rule': {
      const def = schema.definition;
      const existing = userModule.rules.findIndex(r => r.id === schema.id);
      const rule: PraxisRule = {
        id: def.id as string,
        description: def.description as string,
        trigger: def.trigger as string,
        emits: Array.isArray(def.emits) ? def.emits as string[] :
               typeof def.emits === 'string' ? (def.emits as string).split(',').map(s => s.trim()).filter(Boolean) :
               [],
        contract: defineContract({ examples: [], invariants: [] }),
        evaluate: async (event, ctx) => {
          // User rules start as pass-through — LLM-assisted code gen in Phase 4
          ctx.emitFact(def.id as string, event);
          return { fact: def.id as string, payload: event };
        },
      };
      if (existing >= 0) userModule.rules[existing] = rule;
      else userModule.rules.push(rule);
      return { applied: true };
    }
    default:
      return { applied: false, error: `Cannot create user schemas of kind '${schema.kind}'` };
  }
}

/**
 * Decision ledger entry for design changes.
 */
export interface DesignDecision {
  id: string;
  action: 'create' | 'update' | 'delete';
  schemaId: string;
  schemaKind: string;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
  timestamp: string;
  hotReloadResult: { applied: boolean; error?: string };
}

/** In-memory decision ledger (persisted via design.history fact) */
const decisions: DesignDecision[] = [];

/** Record a design decision */
export function recordDecision(decision: Omit<DesignDecision, 'id' | 'timestamp'>): DesignDecision {
  const entry: DesignDecision = {
    ...decision,
    id: `dd-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    timestamp: new Date().toISOString(),
  };
  decisions.push(entry);
  return entry;
}

/** Get all design decisions */
export function getDecisionLedger(): DesignDecision[] {
  return [...decisions];
}
