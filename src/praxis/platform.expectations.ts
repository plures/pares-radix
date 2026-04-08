/**
 * Platform Shell Expectations
 *
 * Behavioral expectations for the pares-radix platform shell.
 * These declare what MUST be true when the platform operates correctly.
 *
 * Self-contained: does not depend on @plures/praxis/expectations package
 * (subpath export not yet available). Uses local type definitions that
 * mirror the praxis expectations DSL.
 *
 * Once @plures/praxis adds the ./expectations export, replace these
 * local types with the canonical imports.
 *
 * See: ADR-0010 — Pares-Agens as First Radix Plugin (Praxis-Native)
 */

// ─── Local Expectations DSL (mirrors @plures/praxis/expectations) ──────────

interface ExpectationCondition {
  description: string;
  type: 'onlyWhen' | 'never' | 'always';
}

class Expectation {
  readonly name: string;
  private _conditions: ExpectationCondition[] = [];

  constructor(name: string) { this.name = name; }

  onlyWhen(condition: string): this {
    this._conditions.push({ description: condition, type: 'onlyWhen' });
    return this;
  }

  never(condition: string): this {
    this._conditions.push({ description: condition, type: 'never' });
    return this;
  }

  always(condition: string): this {
    this._conditions.push({ description: condition, type: 'always' });
    return this;
  }

  get conditions(): ReadonlyArray<ExpectationCondition> { return this._conditions; }
}

class ExpectationSet {
  readonly name: string;
  readonly description: string;
  private _expectations: Expectation[] = [];

  constructor(opts: { name: string; description: string }) {
    this.name = opts.name;
    this.description = opts.description;
  }

  add(expectation: Expectation): this {
    this._expectations.push(expectation);
    return this;
  }

  get expectations(): ReadonlyArray<Expectation> { return this._expectations; }
}

function expectBehavior(name: string): Expectation { return new Expectation(name); }

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Plugin Lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

export const pluginExpectations = new ExpectationSet({
  name: 'platform.plugin',
  description: 'Plugin registration, activation, and dependency resolution',
});

pluginExpectations
  .add(
    expectBehavior('plugin.registered')
      .onlyWhen('plugin.install.requested received AND manifest valid AND deps satisfied')
      .never('manifest missing required fields (id, name, version, routes)')
      .never('duplicate plugin id')
      .never('unsatisfied dependency')
      .always('includes full validated manifest')
      .always('persists via praxis adapter, never direct db.put()'),
  )
  .add(
    expectBehavior('plugin.rejected')
      .onlyWhen('manifest fails validation OR dependencies unsatisfied')
      .never('manifest valid with all deps satisfied')
      .always('includes rejection reason'),
  )
  .add(
    expectBehavior('plugin.activated')
      .onlyWhen('plugin.registered exists AND onActivate succeeds')
      .never('plugin not registered')
      .always('routes available after activation'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Navigation & Routing
// ═══════════════════════════════════════════════════════════════════════════════

export const navigationExpectations = new ExpectationSet({
  name: 'platform.navigation',
  description: 'Route resolution, navigation state, dead-end prevention',
});

navigationExpectations
  .add(
    expectBehavior('route.active')
      .onlyWhen('user.navigated AND path resolves to activated plugin route')
      .never('path matches no registered route')
      .always('includes pluginId and component reference'),
  )
  .add(
    expectBehavior('nav.visible')
      .onlyWhen('plugin activated or deactivated')
      .always('aggregates from ALL activated plugins')
      .always('no duplicate nav item IDs'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Settings & Theme
// ═══════════════════════════════════════════════════════════════════════════════

export const settingsExpectations = new ExpectationSet({
  name: 'platform.settings',
  description: 'Platform and plugin settings persistence',
});

settingsExpectations
  .add(
    expectBehavior('settings.updated')
      .onlyWhen('settings.changed with valid key-value pairs')
      .always('persists via praxis adapter')
      .always('includes old and new values'),
  )
  .add(
    expectBehavior('theme.applied')
      .onlyWhen('settings.changed targets theme OR app.booted loads persisted theme')
      .always('design-dojo theme variables updated reactively'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Application Lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

export const lifecycleExpectations = new ExpectationSet({
  name: 'platform.lifecycle',
  description: 'Application boot, readiness gates, and shutdown',
});

lifecycleExpectations
  .add(
    expectBehavior('app.ready')
      .onlyWhen('app.booted AND all core plugins registered AND all constraints satisfied')
      .never('any core plugin failed to register')
      .always('gate status includes satisfied/unsatisfied list'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Agens Plugin (Three-Agent Cognitive Loop)
// ═══════════════════════════════════════════════════════════════════════════════

export const agensExpectations = new ExpectationSet({
  name: 'agent',
  description: 'Three-agent cognitive architecture behavioral expectations',
});

agensExpectations
  .add(
    expectBehavior('agent.cerebellum.routed')
      .onlyWhen('agent.message.received')
      .always('autorecall executed before routing')
      .always('conscious receives targeted context, NEVER raw memories')
      .always('routing recorded in decision ledger'),
  )
  .add(
    expectBehavior('agent.conscious.executed')
      .onlyWhen('cerebellum directed a task')
      .never('conscious self-initiates')
      .never('conscious accesses raw PluresDB memories directly')
      .always('result returned to cerebellum'),
  )
  .add(
    expectBehavior('agent.subconscious.insight')
      .onlyWhen('cerebellum triggered background reasoning')
      .always('stored in PluresDB for cerebellum retrieval')
      .always('includes confidence score'),
  )
  .add(
    expectBehavior('agent.response.delivered')
      .onlyWhen('cerebellum assembled from conscious + subconscious')
      .always('every message.received eventually produces response.delivered')
      .always('includes provenance: which agents contributed'),
  )
  .add(
    expectBehavior('agent.tool.executed')
      .onlyWhen('tool invoked AND praxis constraint check passed')
      .never('tool invoked without safety gate')
      .always('recorded in decision ledger'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Praxis Compliance (Meta)
// ═══════════════════════════════════════════════════════════════════════════════

export const complianceExpectations = new ExpectationSet({
  name: 'platform.compliance',
  description: 'The platform itself follows praxis principles',
});

complianceExpectations
  .add(
    expectBehavior('compliance.no-raw-html')
      .always('zero raw HTML elements — every component from design-dojo')
      .never('<button>, <div>, <input> outside design-dojo'),
  )
  .add(
    expectBehavior('compliance.no-imperative-logic')
      .always('domain decisions expressed as praxis rules')
      .never('business logic in Tauri commands')
      .never('direct PluresDB calls'),
  )
  .add(
    expectBehavior('compliance.contract-coverage')
      .always('every rule has defineContract with examples and invariants')
      .always('scan:rules reports 0 uncovered')
      .never('rule registered without contract'),
  )
  .add(
    expectBehavior('compliance.fact-persistence')
      .always('all domain state stored as praxis facts in PluresDB')
      .always('recoverable after restart')
      .never('in-memory-only domain state'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// All Sets
// ═══════════════════════════════════════════════════════════════════════════════

export const allExpectationSets = [
  pluginExpectations,
  navigationExpectations,
  settingsExpectations,
  lifecycleExpectations,
  agensExpectations,
  complianceExpectations,
];
