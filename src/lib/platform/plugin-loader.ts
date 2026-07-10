/**
 * Plugin Loader — discovers, validates, and manages plugin lifecycle.
 *
 * Handles dependency resolution (topological sort), activation order,
 * and provides the aggregated registries (routes, nav, settings, etc.)
 * that the layout and pages consume.
 */

import type {
  RadixPlugin,
  PluginRoute,
  NavItem,
  PluginSetting,
  DashboardWidget,
  HelpSection,
  OnboardingStep,
  PaneContribution,
  InferenceRule,
  Expectation,
  Constraint,
  PluginContext,
} from '../types/plugin.js';

// ─── Plugin Registry ────────────────────────────────────────────────────────

interface LoadedPlugin {
  plugin: RadixPlugin;
  active: boolean;
}

const plugins = new Map<string, LoadedPlugin>();

/**
 * Test-only: clear the module-level plugin registry so each test starts from a
 * clean slate (the registry is a module singleton that otherwise persists
 * across tests). Not part of the public API; do not call from app code.
 * @internal
 */
export function __resetRegistryForTest(): void {
  plugins.clear();
}

// ─── Registration ───────────────────────────────────────────────────────────

/**
 * Register a plugin. Does not activate it yet.
 * Call `activateAll()` after all plugins are registered.
 */
export function registerPlugin(plugin: RadixPlugin): void {
  if (plugins.has(plugin.id)) {
    // eslint-disable-next-line plures/no-manual-logging
    console.warn(`[radix] Plugin "${plugin.id}" already registered, skipping.`);
    return;
  }
  plugins.set(plugin.id, { plugin, active: false });
}

/**
 * Activate all registered plugins in dependency order.
 *
 * Each plugin's `onActivate` receives a context. Pass either:
 *   - a context factory `(pluginId) => PluginContext` (preferred — gives every
 *     plugin a `pluginId`-scoped context so data is namespace-isolated), or
 *   - a single shared `PluginContext` (legacy; same ctx for all plugins).
 *
 * `isEligible(pluginId)` optionally gates activation: return false to SKIP a
 * plugin (it stays registered but inactive). The layout passes a predicate
 * backed by the hydrated `admin.plugins.enabled` + `admin.plugins.startup`
 * facts, so a disabled plugin — or one whose startup policy is off — does not
 * boot. When omitted, every registered plugin activates (default-on).
 */
export async function activateAll(
  ctxOrFactory: PluginContext | ((pluginId: string) => PluginContext),
  isEligible?: (pluginId: string) => boolean,
): Promise<void> {
  const order = topologicalSort(plugins);
  const makeCtx =
    typeof ctxOrFactory === 'function'
      ? ctxOrFactory
      : (_pluginId: string) => ctxOrFactory;

  for (const id of order) {
    const entry = plugins.get(id);
    if (!entry || entry.active) continue;
    // Enable/startup gate: a disabled or non-startup plugin is skipped here.
    // It remains registered and can be activated on demand via activatePlugin.
    if (isEligible && !isEligible(id)) {
      // eslint-disable-next-line plures/no-manual-logging
      console.log(`[radix] ⏸ Skipped plugin (disabled or startup-off): ${entry.plugin.name}`);
      continue;
    }

    try {
      await entry.plugin.onActivate?.(makeCtx(id));
      entry.active = true;
      // eslint-disable-next-line plures/no-manual-logging
      console.log(`[radix] ✓ Activated plugin: ${entry.plugin.name}`);
    } catch (err) {
      // eslint-disable-next-line plures/no-manual-logging
      console.error(`[radix] ✗ Failed to activate plugin "${id}":`, err);
    }
  }
}

/**
 * Activate a single registered plugin on demand (idempotent). Used when an
 * operator enables a plugin, or activates one whose startup policy is off,
 * without a full reboot. Returns true if the plugin is active afterwards.
 */
export async function activatePlugin(
  id: string,
  ctxOrFactory: PluginContext | ((pluginId: string) => PluginContext),
): Promise<boolean> {
  const entry = plugins.get(id);
  if (!entry) return false;
  if (entry.active) return true;
  const ctx =
    typeof ctxOrFactory === 'function' ? ctxOrFactory(id) : ctxOrFactory;
  try {
    await entry.plugin.onActivate?.(ctx);
    entry.active = true;
    // eslint-disable-next-line plures/no-manual-logging
    console.log(`[radix] ✓ Activated plugin on demand: ${entry.plugin.name}`);
    return true;
  } catch (err) {
    // eslint-disable-next-line plures/no-manual-logging
    console.error(`[radix] ✗ Failed to activate plugin "${id}":`, err);
    return false;
  }
}

/**
 * Deactivate a single plugin on demand (idempotent). Used when an operator
 * disables a plugin without a reboot. Calls the plugin's onDeactivate hook so
 * it can tear down its surface. Returns true if the plugin is inactive after.
 */
export async function deactivatePlugin(id: string): Promise<boolean> {
  const entry = plugins.get(id);
  if (!entry) return false;
  if (!entry.active) return true;
  try {
    await entry.plugin.onDeactivate?.();
    entry.active = false;
    // eslint-disable-next-line plures/no-manual-logging
    console.log(`[radix] ⏹ Deactivated plugin on demand: ${entry.plugin.name}`);
    return true;
  } catch (err) {
    // eslint-disable-next-line plures/no-manual-logging
    console.error(`[radix] ✗ Failed to deactivate plugin "${id}":`, err);
    return false;
  }
}

/**
 * Deactivate all plugins in reverse order.
 */
export async function deactivateAll(): Promise<void> {
  const order = topologicalSort(plugins);

  for (const id of order.reverse()) {
    const entry = plugins.get(id);
    if (!entry || !entry.active) continue;

    try {
      await entry.plugin.onDeactivate?.();
      entry.active = false;
    } catch (err) {
      // eslint-disable-next-line plures/no-manual-logging
      console.error(`[radix] Failed to deactivate plugin "${id}":`, err);
    }
  }
}

// ─── Aggregated Registries ──────────────────────────────────────────────────

/** All routes from all active plugins, namespaced by plugin ID */
export function getAllRoutes(): Array<PluginRoute & { pluginId: string }> {
  const routes: Array<PluginRoute & { pluginId: string }> = [];
  for (const [id, { plugin, active }] of plugins) {
    if (!active) continue;
    for (const route of plugin.routes) {
      routes.push({
        ...route,
        path: `/${id}${route.path === '/' ? '' : route.path}`,
        pluginId: id,
      });
    }
  }
  return routes;
}

/** All nav items from all active plugins */
export function getAllNavItems(): NavItem[] {
  const items: NavItem[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    items.push(...plugin.navItems);
  }
  return items;
}

/** All pane contributions from all active plugins, scoped by plugin ID. */
export function getAllPaneContributions(): Array<PaneContribution & { pluginId: string }> {
  const out: Array<PaneContribution & { pluginId: string }> = [];
  for (const [id, { plugin, active }] of plugins) {
    if (!active) continue;
    for (const pane of plugin.panes ?? []) {
      out.push({ ...pane, pluginId: id });
    }
  }
  return out;
}

/** All settings from all active plugins */
export function getAllSettings(): PluginSetting[] {
  const settings: PluginSetting[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    settings.push(...plugin.settings);
  }
  return settings;
}

/** All dashboard widgets, sorted by priority */
export function getAllDashboardWidgets(): DashboardWidget[] {
  const widgets: DashboardWidget[] = [
    // Platform widgets (always present)
    {
      id: 'platform.cluster',
      title: '🖥️ Cluster',
      component: () => import('../components/widgets/ClusterStatus.svelte'),
      colspan: 2,
      priority: 10,
    },
    {
      id: 'platform.personality',
      title: '🧠 Personality',
      component: () => import('../components/widgets/PersonalityRules.svelte'),
      colspan: 1,
      priority: 20,
    },
    {
      id: 'platform.omniscient',
      title: '🔍 File Index',
      component: () => import('../components/widgets/OmniscientIndex.svelte'),
      colspan: 1,
      priority: 30,
    },
  ];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    widgets.push(...(plugin.dashboardWidgets ?? []));
  }
  return widgets.sort((a, b) => (a.priority ?? 50) - (b.priority ?? 50));
}

/** All help sections, sorted by priority */
export function getAllHelpSections(): HelpSection[] {
  const sections: HelpSection[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    sections.push(...(plugin.helpSections ?? []));
  }
  return sections.sort((a, b) => (a.priority ?? 50) - (b.priority ?? 50));
}

/** All onboarding steps, dependency-ordered (topological sort by `after` chain) */
export function getAllOnboardingSteps(): OnboardingStep[] {
  const steps: OnboardingStep[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    steps.push(...(plugin.onboardingSteps ?? []));
  }
  return sortStepsByDependency(steps);
}

/**
 * Topological sort of onboarding steps by their `after` dependency chain.
 * Steps listed in `after` (by title) must appear before the dependent step.
 * Detects and warns on cycles; unknown dependency titles are warned and skipped.
 */
function sortStepsByDependency(steps: OnboardingStep[]): OnboardingStep[] {
  if (steps.length === 0) return steps;

  const byTitle = new Map<string, OnboardingStep>();
  for (const step of steps) {
    if (byTitle.has(step.title)) {
      // eslint-disable-next-line plures/no-manual-logging
      console.error(
        `[radix] Duplicate onboarding step title "${step.title}" — later entry ignored; titles must be globally unique.`,
      );
      continue;
    }
    byTitle.set(step.title, step);
  }
  const visited = new Set<string>();
  const inStack = new Set<string>();
  const sorted: OnboardingStep[] = [];

  function visit(title: string): void {
    if (visited.has(title)) return;
    if (inStack.has(title)) {
      // eslint-disable-next-line plures/no-manual-logging
      console.warn(`[radix] Onboarding step cycle detected at: "${title}"`);
      return;
    }
    inStack.add(title);
    const step = byTitle.get(title);
    if (step) {
      for (const dep of step.after ?? []) {
        if (byTitle.has(dep)) {
          visit(dep);
        } else {
          // eslint-disable-next-line plures/no-manual-logging
          console.warn(`[radix] Onboarding step "${title}" depends on unknown step "${dep}"`);
        }
      }
    }
    inStack.delete(title);
    visited.add(title);
    if (step) sorted.push(step);
  }

  for (const step of steps) {
    visit(step.title);
  }

  return sorted;
}

/** All inference rules from all active plugins */
export function getAllInferenceRules(): InferenceRule[] {
  const rules: InferenceRule[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    rules.push(...(plugin.rules ?? []));
  }
  return rules;
}

/** All expectations from all active plugins */
export function getAllExpectations(): Expectation[] {
  const expectations: Expectation[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    expectations.push(...(plugin.expectations ?? []));
  }
  return expectations;
}

/** All constraints from all active plugins */
export function getAllConstraints(): Constraint[] {
  const constraints: Constraint[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    constraints.push(...(plugin.constraints ?? []));
  }
  return constraints;
}

/** Get a specific plugin by ID */
export function getPlugin(id: string): RadixPlugin | undefined {
  return plugins.get(id)?.plugin;
}

/** Get all registered plugin IDs */
export function getPluginIds(): string[] {
  return [...plugins.keys()];
}

/** Check if a plugin is active */
export function isPluginActive(id: string): boolean {
  return plugins.get(id)?.active ?? false;
}

/**
 * Collect export data from every active plugin.
 * Returns a map of pluginId → exported data slice.
 */
export async function exportAllPluginData(): Promise<Record<string, unknown>> {
  const result: Record<string, unknown> = {};
  for (const [id, { plugin, active }] of plugins) {
    if (!active) continue;
    try {
      result[id] = await plugin.onDataExport?.();
    } catch (err) {
      // eslint-disable-next-line plures/no-manual-logging
      console.error(`[radix] Plugin "${id}" export failed:`, err);
    }
  }
  return result;
}

/**
 * Distribute imported data slices to the corresponding plugins.
 * Each plugin receives only its own slice (keyed by plugin ID).
 *
 * @param data      Per-plugin data keyed by plugin ID.
 * @param onProgress  Optional callback fired after each plugin is processed.
 *                    Receives (done, total, pluginId).
 */
export async function importAllPluginData(
  data: Record<string, unknown>,
  onProgress?: (done: number, total: number, pluginId: string) => void,
): Promise<void> {
  const targets = [...plugins.entries()].filter(
    ([id, { active }]) => active && data[id] !== undefined,
  );
  const total = targets.length;
  let done = 0;

  for (const [id, { plugin }] of targets) {
    try {
      await plugin.onDataImport?.(data[id]);
    } catch (err) {
      // eslint-disable-next-line plures/no-manual-logging
      console.error(`[radix] Plugin "${id}" import failed:`, err);
    }
    done++;
    onProgress?.(done, total, id);
  }
}

/**
 * Return a lightweight manifest entry for every currently active plugin.
 * Used to embed provenance metadata in the export envelope.
 */
export function getActivePluginManifests(): Array<{
  id: string;
  name: string;
  version: string;
  icon: string;
}> {
  const manifests: Array<{ id: string; name: string; version: string; icon: string }> = [];
  for (const [id, { plugin, active }] of plugins) {
    if (!active) continue;
    manifests.push({ id, name: plugin.name, version: plugin.version, icon: plugin.icon });
  }
  return manifests;
}

// ─── Dependency Resolution ──────────────────────────────────────────────────

function topologicalSort(plugins: Map<string, LoadedPlugin>): string[] {
  const visited = new Set<string>();
  const sorted: string[] = [];

  function visit(id: string): void {
    if (visited.has(id)) return;
    visited.add(id);

    const entry = plugins.get(id);
    if (!entry) return;

    for (const dep of entry.plugin.dependencies ?? []) {
      if (plugins.has(dep)) {
        visit(dep);
      } else {
        // eslint-disable-next-line plures/no-manual-logging
        console.warn(`[radix] Plugin "${id}" depends on "${dep}" which is not registered.`);
      }
    }

    sorted.push(id);
  }

  for (const id of plugins.keys()) {
    visit(id);
  }

  return sorted;
}
