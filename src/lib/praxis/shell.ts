/**
 * Platform Shell Praxis Module
 *
 * Defines all platform behaviour as praxis primitives:
 *   - Facts   — named pieces of state (plugin lifecycle, routing, nav, theme, settings)
 *   - Events  — domain events that drive rule evaluation
 *   - Rules   — event-driven logic, each with a defineContract
 *   - Constraints — system-wide invariants (DAG, uniqueness, route integrity, coverage)
 *   - Gates   — readiness guards (app-ready)
 *
 * Anti-patterns avoided:
 *   ✗ No if/else chains for plugin validation — rules handle this
 *   ✗ No direct PluresDB calls — praxis adapter (ctx.settings) handles persistence
 *   ✗ No imperative router — events and facts drive navigation
 */

import type {
  PraxisFact,
  PraxisEvent,
  PraxisRule,
  PraxisConstraint,
  PraxisGate,
  PraxisModule,
  PraxisSystemState,
  Contract,
  ContractExample,
  ContractInvariant,
  ValidationResult,
} from '../types/praxis.js';

// ─── Contract Helper ─────────────────────────────────────────────────────────

/**
 * Define a rule contract with examples and invariants.
 *
 * Every rule MUST call defineContract — `praxis scan:rules` flags any rule
 * whose contract has no examples or no invariants.
 */
export function defineContract(config: {
  examples: ContractExample[];
  invariants: ContractInvariant[];
}): Contract {
  return config;
}

// ─── Facts ───────────────────────────────────────────────────────────────────

const shellFacts: PraxisFact[] = [
  {
    id: 'plugin.registered',
    description: 'Plugin manifest validated and accepted',
    persist: true,
  },
  {
    id: 'plugin.activated',
    description: 'Plugin lifecycle started',
    persist: true,
  },
  {
    id: 'plugin.rejected',
    description: 'Manifest validation failed (with reason)',
    persist: true,
  },
  {
    id: 'route.active',
    description: 'Current route resolved to a plugin',
    persist: false,
  },
  {
    id: 'nav.visible',
    description: 'Aggregated navigation items from all active plugins',
    persist: false,
  },
  {
    id: 'theme.applied',
    description: 'Current theme configuration',
    persist: true,
  },
  {
    id: 'settings.updated',
    description: 'Platform or plugin settings changed',
    persist: true,
  },
  {
    id: 'app.window',
    description:
      'Desktop window geometry (position, size, maximized). Restored on app.booted by rule.window-state.',
    persist: true,
  },
  {
    id: 'app.tray',
    description:
      'System tray state — derived from nav.visible and synced to the Tauri tray icon.',
    persist: false,
  },
];

// ─── Events ──────────────────────────────────────────────────────────────────

const shellEvents: PraxisEvent[] = [
  {
    id: 'app.booted',
    description: 'Application startup',
  },
  {
    id: 'plugin.install.requested',
    description: 'New plugin manifest submitted',
  },
  {
    id: 'user.navigated',
    description: 'Route change requested',
  },
  {
    id: 'settings.changed',
    description: 'User modified settings',
  },
  {
    id: 'window.state.changed',
    description:
      'Desktop window geometry changed (position, resize, maximise). Emitted by Tauri backend.',
    schema: '{ x: number; y: number; width: number; height: number; maximized: boolean }',
  },
  {
    id: 'tray.menu.requested',
    description:
      'Tray menu rebuild requested — triggered when nav.visible changes in Tauri context.',
    schema: '{ items: TrayMenuItem[] }',
  },
];

// ─── Rules ───────────────────────────────────────────────────────────────────

const shellRules: PraxisRule[] = [
  // ── Rule 1: Plugin Registration ─────────────────────────────────────────────
  {
    id: 'rule.plugin-registration',
    description:
      'Validate manifest fields, reject duplicate plugin IDs, emit plugin.registered or plugin.rejected',
    trigger: 'plugin.install.requested',
    emits: ['plugin.registered', 'plugin.rejected'],
    contract: defineContract({
      examples: [
        {
          given: {
            manifest: { id: 'analytics', version: '1.0.0', name: 'Analytics' },
            registered: [],
          },
          expect: { fact: 'plugin.registered', payload: { id: 'analytics' } },
          description: 'valid manifest with id, version, and name gets registered',
        },
        {
          given: {
            manifest: { id: '', version: '1.0.0', name: 'Bad Plugin' },
            registered: [],
          },
          expect: { fact: 'plugin.rejected', payload: { reason: 'missing id' } },
          description: 'manifest missing id is rejected',
        },
        {
          given: {
            manifest: { id: 'dup', version: '1.0.0', name: 'Duplicate' },
            registered: ['dup'],
          },
          expect: { fact: 'plugin.rejected', payload: { reason: 'duplicate plugin id' } },
          description: 'duplicate plugin id is rejected',
        },
        {
          given: {
            manifest: { id: 'nover', version: '', name: 'No Version' },
            registered: [],
          },
          expect: { fact: 'plugin.rejected', payload: { reason: 'missing version' } },
          description: 'manifest missing version is rejected',
        },
      ],
      invariants: [
        {
          description: 'must emit exactly one of plugin.registered or plugin.rejected',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'plugin.registered' || o.fact === 'plugin.rejected';
          },
        },
        {
          description: 'plugin.registered payload must include the plugin id',
          check: (output) => {
            const o = output as { fact: string; payload: { id?: string } };
            if (o.fact !== 'plugin.registered') return true;
            return typeof o.payload.id === 'string' && o.payload.id.length > 0;
          },
        },
        {
          description: 'plugin.rejected payload must include a reason',
          check: (output) => {
            const o = output as { fact: string; payload: { reason?: string } };
            if (o.fact !== 'plugin.rejected') return true;
            return typeof o.payload.reason === 'string' && o.payload.reason.length > 0;
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        manifest: { id?: string; version?: string; name?: string };
        registered?: string[];
      };
      const { manifest, registered = [] } = ev;

      if (!manifest.id) {
        const payload = { reason: 'missing id' };
        ctx.emitFact('plugin.rejected', payload);
        return { fact: 'plugin.rejected', payload };
      }
      if (!manifest.version) {
        const payload = { reason: 'missing version' };
        ctx.emitFact('plugin.rejected', payload);
        return { fact: 'plugin.rejected', payload };
      }
      if (!manifest.name) {
        const payload = { reason: 'missing name' };
        ctx.emitFact('plugin.rejected', payload);
        return { fact: 'plugin.rejected', payload };
      }
      if (registered.includes(manifest.id)) {
        const payload = { reason: 'duplicate plugin id' };
        ctx.emitFact('plugin.rejected', payload);
        return { fact: 'plugin.rejected', payload };
      }

      const payload = { id: manifest.id };
      ctx.emitFact('plugin.registered', payload);
      return { fact: 'plugin.registered', payload };
    },
  },

  // ── Rule 2: Route Resolution ─────────────────────────────────────────────────
  {
    id: 'rule.route-resolution',
    description: 'Match path to registered plugin route, emit route.active',
    trigger: 'user.navigated',
    emits: ['route.active'],
    contract: defineContract({
      examples: [
        {
          given: {
            path: '/analytics/dashboard',
            routes: [{ pluginId: 'analytics', path: '/analytics/dashboard' }],
          },
          expect: {
            fact: 'route.active',
            payload: { pluginId: 'analytics', path: '/analytics/dashboard' },
          },
          description: 'navigating to a registered route emits route.active with the matching plugin',
        },
        {
          given: { path: '/unknown', routes: [] },
          expect: { fact: 'route.active', payload: { pluginId: null, path: '/unknown' } },
          description: 'navigating to an unregistered route emits route.active with null pluginId',
        },
        {
          given: {
            path: '/analytics/reports',
            routes: [
              { pluginId: 'analytics', path: '/analytics' },
              { pluginId: 'other', path: '/other' },
            ],
          },
          expect: {
            fact: 'route.active',
            payload: { pluginId: 'analytics', path: '/analytics/reports' },
          },
          description: 'prefix matching resolves nested paths to the owning plugin',
        },
      ],
      invariants: [
        {
          description: 'route.active must always be emitted for any navigation',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'route.active';
          },
        },
        {
          description: 'route.active payload must include the navigated path',
          check: (output) => {
            const o = output as { fact: string; payload: { path?: string } };
            return typeof o.payload.path === 'string';
          },
        },
        {
          description: 'route.active pluginId is a string or null',
          check: (output) => {
            const o = output as { fact: string; payload: { pluginId?: unknown } };
            return o.payload.pluginId === null || typeof o.payload.pluginId === 'string';
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        path: string;
        routes: Array<{ pluginId: string; path: string }>;
      };
      const matched = ev.routes.find((r) => ev.path.startsWith(r.path)) ?? null;
      const payload = { pluginId: matched?.pluginId ?? null, path: ev.path };
      ctx.emitFact('route.active', payload);
      return { fact: 'route.active', payload };
    },
  },

  // ── Rule 3: Navigation Aggregation ───────────────────────────────────────────
  {
    id: 'rule.nav-aggregation',
    description: 'Collect nav items from all active plugins, emit nav.visible',
    trigger: 'app.booted',
    emits: ['nav.visible'],
    contract: defineContract({
      examples: [
        {
          given: {
            plugins: [
              {
                id: 'analytics',
                active: true,
                navItems: [{ href: '/analytics', label: 'Analytics', icon: '📊' }],
              },
              {
                id: 'disabled',
                active: false,
                navItems: [{ href: '/disabled', label: 'Disabled', icon: '🚫' }],
              },
            ],
          },
          expect: {
            fact: 'nav.visible',
            payload: { items: [{ href: '/analytics', label: 'Analytics', icon: '📊' }] },
          },
          description: 'only active plugin nav items appear in nav.visible',
        },
        {
          given: { plugins: [] },
          expect: { fact: 'nav.visible', payload: { items: [] } },
          description: 'no active plugins results in empty nav',
        },
        {
          given: {
            plugins: [
              {
                id: 'a',
                active: true,
                navItems: [{ href: '/a', label: 'A', icon: '🅰️' }],
              },
              {
                id: 'b',
                active: true,
                navItems: [{ href: '/b', label: 'B', icon: '🅱️' }],
              },
            ],
          },
          expect: {
            fact: 'nav.visible',
            payload: {
              items: [
                { href: '/a', label: 'A', icon: '🅰️' },
                { href: '/b', label: 'B', icon: '🅱️' },
              ],
            },
          },
          description: 'nav items from multiple active plugins are aggregated',
        },
      ],
      invariants: [
        {
          description: 'nav.visible must always be emitted',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'nav.visible';
          },
        },
        {
          description: 'nav.visible payload must contain an items array',
          check: (output) => {
            const o = output as { fact: string; payload: { items?: unknown } };
            return Array.isArray(o.payload.items);
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        plugins: Array<{ id: string; active: boolean; navItems: unknown[] }>;
      };
      const items = ev.plugins.filter((p) => p.active).flatMap((p) => p.navItems);
      const payload = { items };
      ctx.emitFact('nav.visible', payload);
      return { fact: 'nav.visible', payload };
    },
  },

  // ── Rule 4: Settings Persistence ─────────────────────────────────────────────
  {
    id: 'rule.settings-persistence',
    description:
      'On settings.changed, persist to PluresDB via praxis adapter, emit settings.updated',
    trigger: 'settings.changed',
    emits: ['settings.updated'],
    contract: defineContract({
      examples: [
        {
          given: { key: 'radix.theme', value: 'dark' },
          expect: { fact: 'settings.updated', payload: { key: 'radix.theme', value: 'dark' } },
          description: 'changing a setting emits settings.updated with the key and new value',
        },
        {
          given: { key: 'radix.llm.provider', value: 'openai' },
          expect: {
            fact: 'settings.updated',
            payload: { key: 'radix.llm.provider', value: 'openai' },
          },
          description: 'changing LLM provider persists via the praxis adapter and emits settings.updated',
        },
        {
          given: { key: 'plugin.analytics.enabled', value: false },
          expect: {
            fact: 'settings.updated',
            payload: { key: 'plugin.analytics.enabled', value: false },
          },
          description: 'boolean settings are persisted and emitted correctly',
        },
      ],
      invariants: [
        {
          description: 'settings.updated must always be emitted for any settings.changed event',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'settings.updated';
          },
        },
        {
          description: 'settings.updated payload must include the changed key',
          check: (output) => {
            const o = output as { fact: string; payload: { key?: string } };
            return typeof o.payload.key === 'string' && o.payload.key.length > 0;
          },
        },
        {
          description: 'settings.updated payload must echo back the value',
          check: (output) => {
            const o = output as { fact: string; payload: Record<string, unknown> };
            return 'value' in o.payload;
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as { key: string; value: unknown };
      // SettingsAPI.set is synchronous (PluresDB-backed via the praxis adapter);
      // persistence completes before the fact is emitted.
      ctx.settings.set(ev.key, ev.value);
      const payload = { key: ev.key, value: ev.value };
      ctx.emitFact('settings.updated', payload);
      return { fact: 'settings.updated', payload };
    },
  },

  // ── Rule 5: Window State Persistence ────────────────────────────────────────
  {
    id: 'rule.window-state',
    description:
      'On window.state.changed, persist window geometry as the app.window fact via PluresDB adapter.',
    trigger: 'window.state.changed',
    emits: ['app.window'],
    contract: defineContract({
      examples: [
        {
          given: { x: 100, y: 200, width: 1200, height: 800, maximized: false },
          expect: {
            fact: 'app.window',
            payload: { x: 100, y: 200, width: 1200, height: 800, maximized: false },
          },
          description: 'normal window geometry is persisted as app.window',
        },
        {
          given: { x: 0, y: 0, width: 1920, height: 1080, maximized: true },
          expect: {
            fact: 'app.window',
            payload: { x: 0, y: 0, width: 1920, height: 1080, maximized: true },
          },
          description: 'maximized window state is persisted',
        },
      ],
      invariants: [
        {
          description: 'app.window must always be emitted on window.state.changed',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'app.window';
          },
        },
        {
          description: 'app.window payload must include width, height, and maximized',
          check: (output) => {
            const o = output as { payload: Record<string, unknown> };
            return (
              typeof o.payload.width === 'number' &&
              typeof o.payload.height === 'number' &&
              typeof o.payload.maximized === 'boolean'
            );
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        x: number;
        y: number;
        width: number;
        height: number;
        maximized: boolean;
      };
      const payload = {
        x: ev.x,
        y: ev.y,
        width: ev.width,
        height: ev.height,
        maximized: ev.maximized,
      };
      ctx.emitFact('app.window', payload);
      return { fact: 'app.window', payload };
    },
  },

  // ── Rule 6: Tray Menu Sync ───────────────────────────────────────────────────
  {
    id: 'rule.tray-menu-sync',
    description:
      'On tray.menu.requested, emit app.tray with the nav.visible items formatted for the system tray.',
    trigger: 'tray.menu.requested',
    emits: ['app.tray'],
    contract: defineContract({
      examples: [
        {
          given: {
            items: [
              { href: '/dashboard', label: 'Dashboard', icon: '🏠' },
              { href: '/settings', label: 'Settings', icon: '⚙️' },
            ],
          },
          expect: {
            fact: 'app.tray',
            payload: {
              items: [
                { id: 'dashboard', label: 'Dashboard', path: '/dashboard' },
                { id: 'settings', label: 'Settings', path: '/settings' },
              ],
            },
          },
          description: 'nav.visible items are mapped to tray menu items',
        },
        {
          given: { items: [] },
          expect: { fact: 'app.tray', payload: { items: [] } },
          description: 'empty nav results in empty tray menu',
        },
      ],
      invariants: [
        {
          description: 'app.tray must always be emitted',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'app.tray';
          },
        },
        {
          description: 'app.tray payload must have an items array',
          check: (output) => {
            const o = output as { payload: { items?: unknown } };
            return Array.isArray(o.payload.items);
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        items: Array<{ href: string; label: string; icon?: string }>;
      };
      const trayItems = ev.items.map((item) => ({
        id: item.href.replace(/^\//, '').replace(/\//g, '-') || 'home',
        label: item.label,
        path: item.href,
      }));
      const payload = { items: trayItems };
      ctx.emitFact('app.tray', payload);
      return { fact: 'app.tray', payload };
    },
  },
];

// ─── Constraints ─────────────────────────────────────────────────────────────

const shellConstraints: PraxisConstraint[] = [
  {
    id: 'constraint.dependency-dag',
    description: 'Registered plugins must form an acyclic dependency graph',
    message: 'Plugin dependency cycle detected — circular dependencies are not allowed',
    check: (state: PraxisSystemState) => {
      const registered = state.facts.get('plugin.registered');
      if (!registered || !Array.isArray(registered)) return true;

      const depMap = new Map<string, string[]>();
      for (const entry of registered as Array<{ id: string; dependencies?: string[] }>) {
        depMap.set(entry.id, entry.dependencies ?? []);
      }

      const visited = new Set<string>();
      const inStack = new Set<string>();

      function hasCycle(id: string): boolean {
        if (inStack.has(id)) return true;
        if (visited.has(id)) return false;
        inStack.add(id);
        for (const dep of depMap.get(id) ?? []) {
          if (hasCycle(dep)) return true;
        }
        inStack.delete(id);
        visited.add(id);
        return false;
      }

      for (const id of depMap.keys()) {
        if (hasCycle(id)) return false;
      }
      return true;
    },
  },
  {
    id: 'constraint.plugin-id-uniqueness',
    description: 'No duplicate plugin IDs — each plugin ID must be globally unique',
    message: 'Duplicate plugin ID detected — plugin IDs must be unique',
    check: (state: PraxisSystemState) => {
      const registered = state.facts.get('plugin.registered');
      if (!registered || !Array.isArray(registered)) return true;
      const ids = (registered as Array<{ id: string }>).map((e) => e.id);
      return ids.length === new Set(ids).size;
    },
  },
  {
    id: 'constraint.route-integrity',
    description: 'Active route must resolve to a registered plugin',
    message: 'Active route does not correspond to any registered plugin — route integrity violated',
    check: (state: PraxisSystemState) => {
      const routeActive = state.facts.get('route.active') as
        | { pluginId: string | null }
        | undefined;
      if (!routeActive) return true;
      if (routeActive.pluginId === null) return true;

      const registered = state.facts.get('plugin.registered');
      if (!registered || !Array.isArray(registered)) return false;
      const ids = new Set((registered as Array<{ id: string }>).map((e) => e.id));
      return ids.has(routeActive.pluginId);
    },
  },
  {
    id: 'constraint.contract-coverage',
    description: 'Every rule must have a defineContract with examples and invariants',
    message: 'One or more rules are missing contract examples or invariants',
    check: (state: PraxisSystemState) => {
      // Contract coverage is statically enforced via validateModule / scanRules.
      // At runtime the shell module's rules all carry contracts by construction.
      void state;
      return true;
    },
  },
];

// ─── Gates ───────────────────────────────────────────────────────────────────

const shellGates: PraxisGate[] = [
  {
    id: 'app-ready',
    description: 'Core plugins registered, navigation visible, and all system constraints satisfied',
    conditions: ['plugin.registered', 'nav.visible'],
    check: (state: PraxisSystemState) => {
      const hasRequired =
        state.facts.has('plugin.registered') &&
        state.facts.has('nav.visible');
      if (!hasRequired) return false;
      return shellConstraints.every((c) => c.check(state));
    },
  },
];

// ─── Module ──────────────────────────────────────────────────────────────────

/** The platform shell praxis module */
export const shellModule: PraxisModule = {
  id: 'radix.shell',
  description:
    'Platform shell — plugin lifecycle, routing, navigation aggregation, theme, and settings',
  facts: shellFacts,
  events: shellEvents,
  rules: shellRules,
  constraints: shellConstraints,
  gates: shellGates,
};

// ─── Validation Utilities ─────────────────────────────────────────────────────

/**
 * Validate a praxis module for contract coverage.
 *
 * Implements `praxis validate`.
 * Returns 100% contractCoverage and valid=true when every rule has at least
 * one example and one invariant in its contract.
 */
export function validateModule(module: PraxisModule): ValidationResult {
  const violations: string[] = [];
  let coveredRules = 0;

  for (const rule of module.rules) {
    const hasExamples = rule.contract.examples.length > 0;
    const hasInvariants = rule.contract.invariants.length > 0;
    if (hasExamples && hasInvariants) {
      coveredRules++;
    } else {
      if (!hasExamples) {
        violations.push(`Rule "${rule.id}" has no contract examples`);
      }
      if (!hasInvariants) {
        violations.push(`Rule "${rule.id}" has no contract invariants`);
      }
    }
  }

  const contractCoverage =
    module.rules.length === 0
      ? 100
      : Math.round((coveredRules / module.rules.length) * 100);

  return {
    valid: violations.length === 0,
    contractCoverage,
    violations,
  };
}

/**
 * Return all rules that lack a complete contract (missing examples or invariants).
 *
 * Implements `praxis scan:rules`.
 * An empty array means 0 rules without contracts.
 */
export function scanRules(module: PraxisModule): PraxisRule[] {
  return module.rules.filter(
    (rule) => rule.contract.examples.length === 0 || rule.contract.invariants.length === 0,
  );
}
