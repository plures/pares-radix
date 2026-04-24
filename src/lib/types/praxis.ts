/**
 * Praxis Module System — type definitions
 *
 * Facts, events, rules (with contracts), constraints, gates, and modules
 * that together express all platform behaviour as declarative praxis primitives.
 */

import type { SettingsAPI } from './plugin.js';

// ─── Facts ──────────────────────────────────────────────────────────────────

/** A named piece of persistent or ephemeral state in the praxis system */
export interface PraxisFact<T = unknown> {
  /** Unique fact identifier (dot-notation, e.g. 'plugin.registered') */
  id: string;
  /** Human-readable description */
  description: string;
  /** Whether this fact persists to PluresDB via the praxis adapter */
  persist?: boolean;
  /** Initial/default value */
  initial?: T;
}

// ─── Events ──────────────────────────────────────────────────────────────────

/** A domain event that may trigger rule evaluation */
export interface PraxisEvent<T = unknown> {
  /** Unique event identifier (dot-notation, e.g. 'app.booted') */
  id: string;
  /** Human-readable description */
  description: string;
  /** Optional schema descriptor for the event payload */
  schema?: string;
  /** Phantom type field — never set at runtime, only used for type inference */
  readonly _payload?: T;
}

// ─── Contracts ───────────────────────────────────────────────────────────────

/** A concrete example demonstrating expected rule behaviour */
export interface ContractExample {
  /** The input given to the rule */
  given: unknown;
  /** The expected output / emitted fact */
  expect: unknown;
  /** Human-readable description of the scenario */
  description?: string;
}

/** An invariant that must always hold on rule output */
export interface ContractInvariant {
  /** Human-readable description of the invariant */
  description: string;
  /** Returns true if the invariant holds for the given output */
  check: (output: unknown) => boolean;
}

/** A contract specifying expected rule behaviour through examples and invariants */
export interface Contract {
  examples: ContractExample[];
  invariants: ContractInvariant[];
}

// ─── Rules ───────────────────────────────────────────────────────────────────

/** Context injected into rule evaluation */
export interface PraxisContext {
  /** Access platform settings (PluresDB-backed via praxis adapter) */
  settings: SettingsAPI;
  /** Emit a fact with a value */
  emitFact: (factId: string, value: unknown) => void;
  /** Query the current value of a fact */
  query?: (factId: string) => unknown;
}

/** A platform rule — driven by a triggering event, emits facts */
export interface PraxisRule {
  /** Unique rule identifier */
  id: string;
  /** Human-readable description */
  description: string;
  /** Event ID that triggers this rule */
  trigger: string;
  /** Fact IDs this rule may emit */
  emits: string[];
  /** Contract defining expected behaviour (examples + invariants) */
  contract: Contract;
  /** Evaluate the event and emit appropriate facts via ctx */
  evaluate: (event: unknown, ctx: PraxisContext) => Promise<Record<string, unknown>>;
}

// ─── Constraints ─────────────────────────────────────────────────────────────

/** The runtime state of the praxis system */
export interface PraxisSystemState {
  /** Current fact values keyed by fact ID */
  facts: Map<string, unknown>;
}

/** A constraint on system state — must always hold */
export interface PraxisConstraint {
  /** Unique constraint identifier */
  id: string;
  /** Human-readable description */
  description: string;
  /** Returns true if the constraint holds for the current state */
  check: (state: PraxisSystemState) => boolean;
  /** Human-readable error message shown on violation */
  message: string;
}

// ─── Gates ───────────────────────────────────────────────────────────────────

/** A gate — guards system readiness by requiring facts to be satisfied */
export interface PraxisGate {
  /** Unique gate identifier */
  id: string;
  /** Human-readable description */
  description: string;
  /** Fact IDs that must all be present (non-null/non-false) for the gate to open */
  conditions: string[];
  /** Full readiness check (may be more nuanced than simple fact presence) */
  check: (state: PraxisSystemState) => boolean;
}

// ─── Module ──────────────────────────────────────────────────────────────────

/** A complete praxis module definition */
export interface PraxisModule {
  /** Unique module identifier */
  id: string;
  /** Human-readable description */
  description: string;
  /** Facts declared by this module */
  facts: PraxisFact[];
  /** Events consumed by this module */
  events: PraxisEvent[];
  /** Rules driven by events, emitting facts */
  rules: PraxisRule[];
  /** Constraints on system state */
  constraints: PraxisConstraint[];
  /** Gates guarding readiness */
  gates: PraxisGate[];
}

// ─── Validation ──────────────────────────────────────────────────────────────

/** Result of a praxis module validation (`praxis validate`) */
export interface ValidationResult {
  /** Whether all contracts have full coverage */
  valid: boolean;
  /** Percentage of rules with full contract coverage (0–100) */
  contractCoverage: number;
  /** List of violation messages */
  violations: string[];
}
