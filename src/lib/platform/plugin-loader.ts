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

// ─── Registration ───────────────────────────────────────────────────────────

/**
 * Register a plugin. Does not activate it yet.
 * Call `activateAll()` after all plugins are registered.
 */
export function registerPlugin(plugin: RadixPlugin): void {
  if (plugins.has(plugin.id)) {
    console.warn(`[radix] Plugin "${plugin.id}" already registered, skipping.`);
    return;
  }
  plugins.set(plugin.id, { plugin, active: false });
}

/**
 * Activate all registered plugins in dependency order.
 */
export async function activateAll(ctx: PluginContext): Promise<void> {
  const order = topologicalSort(plugins);

  for (const id of order) {
    const entry = plugins.get(id);
    if (!entry || entry.active) continue;

    try {
      await entry.plugin.onActivate?.(ctx);
      entry.active = true;
      console.log(`[radix] ✓ Activated plugin: ${entry.plugin.name}`);
    } catch (err) {
      console.error(`[radix] ✗ Failed to activate plugin "${id}":`, err);
    }
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
  const widgets: DashboardWidget[] = [];
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

/** All onboarding steps, dependency-ordered */
export function getAllOnboardingSteps(): OnboardingStep[] {
  const steps: OnboardingStep[] = [];
  for (const [, { plugin, active }] of plugins) {
    if (!active) continue;
    steps.push(...(plugin.onboardingSteps ?? []));
  }
  // Simple dependency ordering: steps with `after` go later
  return steps.sort((a, b) => {
    if (a.after?.length && !b.after?.length) return 1;
    if (!a.after?.length && b.after?.length) return -1;
    return 0;
  });
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
      console.error(`[radix] Plugin "${id}" export failed:`, err);
    }
  }
  return result;
}

/**
 * Distribute imported data slices to the corresponding plugins.
 * Each plugin receives only its own slice (keyed by plugin ID).
 */
export async function importAllPluginData(data: Record<string, unknown>): Promise<void> {
  for (const [id, { plugin, active }] of plugins) {
    if (!active || data[id] === undefined) continue;
    try {
      await plugin.onDataImport?.(data[id]);
    } catch (err) {
      console.error(`[radix] Plugin "${id}" import failed:`, err);
    }
  }
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
