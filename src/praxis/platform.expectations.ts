/**
 * Platform Shell Expectations
 *
 * Behavioral expectations for the pares-radix platform shell.
 * These declare what MUST be true when the platform operates correctly.
 * Verified against the praxis registry — any rule that can't satisfy
 * these expectations is incomplete.
 *
 * See: ADR-0010 — Pares-Agens as First Radix Plugin (Praxis-Native)
 */

import {
  expectBehavior,
  ExpectationSet,
  verify,
  formatVerificationReport,
} from '@plures/praxis/expectations';
import type { VerifiableRegistry } from '@plures/praxis/expectations';

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Plugin Lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

const pluginExpectations = new ExpectationSet({
  name: 'platform.plugin',
  description: 'Plugin registration, activation, and dependency resolution',
});

pluginExpectations
  .add(
    expectBehavior('plugin.registered')
      .onlyWhen('plugin.install.requested is received AND manifest passes schema validation AND all dependencies are already registered')
      .never('manifest is missing required fields (id, name, version, routes)')
      .never('a plugin with the same id is already registered')
      .never('any declared dependency is not yet registered')
      .always('includes the full validated manifest in payload')
      .always('persists to PluresDB via praxis adapter, not direct db.put()'),
  )
  .add(
    expectBehavior('plugin.rejected')
      .onlyWhen('plugin.install.requested is received AND manifest fails validation OR dependencies are unsatisfied')
      .never('manifest is valid with all dependencies satisfied')
      .always('includes reason for rejection')
      .always('includes which specific validation failed or which dependency is missing'),
  )
  .add(
    expectBehavior('plugin.activated')
      .onlyWhen('plugin.registered exists for this plugin AND onActivate lifecycle hook completes successfully')
      .never('plugin has not been registered')
      .never('onActivate threw an error')
      .always('plugin routes are available for navigation after activation')
      .always('plugin nav items appear in nav.visible after activation'),
  )
  .add(
    expectBehavior('plugin.deactivated')
      .onlyWhen('explicit deactivation requested OR dependency was deactivated')
      .never('plugin has active dependents that are still activated')
      .always('plugin routes are removed from active routing')
      .always('plugin nav items removed from nav.visible'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Navigation & Routing
// ═══════════════════════════════════════════════════════════════════════════════

const navigationExpectations = new ExpectationSet({
  name: 'platform.navigation',
  description: 'Route resolution, navigation state, and dead-end prevention',
});

navigationExpectations
  .add(
    expectBehavior('route.active')
      .onlyWhen('user.navigated event received AND path resolves to a registered plugin route')
      .never('path does not match any registered plugin route')
      .never('target plugin is not activated')
      .always('includes resolved pluginId and component reference')
      .always('previous route.active is superseded (last-write-wins)'),
  )
  .add(
    expectBehavior('route.not-found')
      .onlyWhen('user.navigated event received AND path does not resolve to any plugin route')
      .never('path matches a registered route in an activated plugin')
      .always('includes the attempted path for error display'),
  )
  .add(
    expectBehavior('nav.visible')
      .onlyWhen('a plugin is activated or deactivated, changing the set of available nav items')
      .always('aggregates nav items from ALL activated plugins')
      .always('items are sorted by plugin registration order, then by plugin-declared order')
      .always('no duplicate nav item IDs across plugins'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Settings & Theme
// ═══════════════════════════════════════════════════════════════════════════════

const settingsExpectations = new ExpectationSet({
  name: 'platform.settings',
  description: 'Platform and plugin settings persistence',
});

settingsExpectations
  .add(
    expectBehavior('settings.updated')
      .onlyWhen('settings.changed event received with valid key-value pairs')
      .never('settings key is not in the declared schema for the target plugin or platform')
      .always('persists to PluresDB via praxis adapter')
      .always('includes both old and new values for auditability'),
  )
  .add(
    expectBehavior('theme.applied')
      .onlyWhen('settings.changed event targets theme settings OR app.booted loads persisted theme')
      .always('design-dojo theme variables updated reactively')
      .always('all components reflect the new theme without page reload'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Application Lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

const lifecycleExpectations = new ExpectationSet({
  name: 'platform.lifecycle',
  description: 'Application boot, readiness gates, and shutdown',
});

lifecycleExpectations
  .add(
    expectBehavior('app.ready')
      .onlyWhen('app.booted event processed AND all core plugins registered AND all constraints satisfied')
      .never('any core plugin failed to register')
      .never('any constraint is violated')
      .always('gate status includes list of satisfied and unsatisfied constraints'),
  )
  .add(
    expectBehavior('app.error')
      .onlyWhen('a constraint violation is detected OR a rule throws an unrecoverable error')
      .always('includes the constraint ID or rule ID that failed')
      .always('includes human-readable error description')
      .always('recorded in decision ledger'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Agens Plugin (Three-Agent Cognitive Loop)
// ═══════════════════════════════════════════════════════════════════════════════

const agensExpectations = new ExpectationSet({
  name: 'agent',
  description: 'Three-agent cognitive architecture behavioral expectations',
});

agensExpectations
  .add(
    expectBehavior('agent.cerebellum.routed')
      .onlyWhen('agent.message.received event arrives')
      .always('autorecall procedure executed against PluresDB before routing')
      .always('intent classification performed')
      .always('conscious agent receives targeted context, NEVER raw memories')
      .always('routing decision recorded in decision ledger'),
  )
  .add(
    expectBehavior('agent.conscious.executed')
      .onlyWhen('cerebellum.routed directed a task to conscious')
      .never('conscious self-initiates without cerebellum direction')
      .never('conscious accesses raw PluresDB memories directly — only cerebellum-curated context')
      .always('result returned to cerebellum for assembly')
      .always('execution timing recorded for cerebellum performance tracking'),
  )
  .add(
    expectBehavior('agent.subconscious.insight')
      .onlyWhen('cerebellum triggered a background reasoning procedure')
      .never('subconscious produces output without cerebellum trigger')
      .always('insight stored in PluresDB for cerebellum retrieval')
      .always('insight includes confidence score and reasoning chain'),
  )
  .add(
    expectBehavior('agent.response.delivered')
      .onlyWhen('cerebellum has assembled response from conscious result + available subconscious insights')
      .never('response delivered without cerebellum assembly step')
      .always('every agent.message.received eventually produces agent.response.delivered')
      .always('response includes provenance: which agents contributed'),
  )
  .add(
    expectBehavior('agent.memory.captured')
      .onlyWhen('interaction completed AND cerebellum post-processing extracts primitives')
      .never('raw conversation text stored without primitive extraction')
      .always('captured as structured praxis facts in PluresDB')
      .always('tagged with interaction context for future autorecall'),
  )
  .add(
    expectBehavior('agent.tool.executed')
      .onlyWhen('conscious or cerebellum invoked a tool AND praxis constraint check passed')
      .never('tool invoked without passing through praxis safety gate')
      .always('tool invocation recorded in decision ledger with input/output')
      .always('constraint violations block execution and return rejection reason'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Domain: Praxis Compliance (Meta-Expectations)
// ═══════════════════════════════════════════════════════════════════════════════

const complianceExpectations = new ExpectationSet({
  name: 'platform.compliance',
  description: 'Meta-expectations: the platform itself follows praxis principles',
});

complianceExpectations
  .add(
    expectBehavior('compliance.no-raw-html')
      .always('zero raw HTML elements in radix source — every component from design-dojo')
      .never('a <button>, <div>, <input>, or <select> appears outside design-dojo wrappers'),
  )
  .add(
    expectBehavior('compliance.no-imperative-logic')
      .always('domain decisions expressed as praxis rules, never if/else chains')
      .never('business logic in Tauri commands — commands emit events only')
      .never('direct PluresDB calls — praxis adapter handles persistence'),
  )
  .add(
    expectBehavior('compliance.contract-coverage')
      .always('every rule has a defineContract with behavior, examples, and invariants')
      .always('praxis scan:rules reports 0 rules without contracts')
      .never('a rule is registered without an attached contract'),
  )
  .add(
    expectBehavior('compliance.fact-persistence')
      .always('all domain state stored as praxis facts in PluresDB')
      .always('application state fully recoverable from PluresDB after restart')
      .never('in-memory-only state for domain data — only transient UI state may be ephemeral'),
  );

// ═══════════════════════════════════════════════════════════════════════════════
// Verification
// ═══════════════════════════════════════════════════════════════════════════════

const allExpectations = [
  pluginExpectations,
  navigationExpectations,
  settingsExpectations,
  lifecycleExpectations,
  agensExpectations,
  complianceExpectations,
];

/**
 * Verify all platform expectations against a populated registry.
 */
export function verifyPlatformExpectations(registry: VerifiableRegistry) {
  const reports = allExpectations.map(set => verify(registry, set));
  return reports;
}

/**
 * Print verification report for all platform expectation sets.
 */
export function printPlatformReport(registry: VerifiableRegistry): void {
  for (const set of allExpectations) {
    const report = verify(registry, set);
    console.log(formatVerificationReport(report));
    console.log('');
  }
}

export {
  pluginExpectations,
  navigationExpectations,
  settingsExpectations,
  lifecycleExpectations,
  agensExpectations,
  complianceExpectations,
};
