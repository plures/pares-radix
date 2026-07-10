/**
 * Plugin Resolver — discovers and loads plugins from multiple sources.
 *
 * Sources (in priority order):
 * 1. Local filesystem — for development (`radix plugin add ./path/to/plugin`)
 * 2. GitHub — from a repo (`radix plugin add github:owner/repo`)
 * 3. npm — published package (`radix plugin add @scope/plugin-name`)
 * 4. Modulus registry — curated marketplace (subset of npm with quality gates)
 *
 * Plugin manifest: manifest.json at the plugin root.
 * Plugin entry: src/index.ts exporting a RadixPlugin.
 *
 * Installed plugins are stored in:
 *   $PARES_DATA_DIR/plugins/<plugin-id>/
 *
 * The plugin loader reads this directory on startup and activates all plugins.
 */

export interface PluginSource {
  type: 'local' | 'github' | 'npm' | 'modulus';
  /** For local: path. For github: owner/repo. For npm: package name. */
  specifier: string;
  /** Optional version/branch/tag. */
  version?: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  /** Surface class; defaults to panel. Discriminates panel plugins from agent-runtime plugins. */
  type?: 'panel' | 'agent';
  version: string;
  description: string;
  author: string;
  license: string;
  icon: string;
  entry: string;
  keywords?: string[];
  radix?: string; // minimum radix version
  dependencies?: string[];
  peerDependencies?: Record<string, string>;
}

export interface InstalledPlugin {
  manifest: PluginManifest;
  source: PluginSource;
  installPath: string;
  installedAt: string;
  enabled: boolean;
}

/**
 * Parse a plugin specifier into a PluginSource.
 *
 * Examples:
 *   ./my-plugin                      → { type: 'local', specifier: './my-plugin' }
 *   /absolute/path/to/plugin         → { type: 'local', specifier: '/absolute/...' }
 *   github:plures/plugin-finance     → { type: 'github', specifier: 'plures/plugin-finance' }
 *   github:plures/plugin-finance#v1  → { type: 'github', specifier: '...', version: 'v1' }
 *   @plures/plugin-financial-advisor → { type: 'npm', specifier: '@plures/plugin-...' }
 *   plugin-name                      → { type: 'modulus', specifier: 'plugin-name' }
 */
export function parsePluginSpecifier(spec: string): PluginSource {
  // Local path
  if (spec.startsWith('./') || spec.startsWith('/') || spec.startsWith('../')) {
    return { type: 'local', specifier: spec };
  }

  // GitHub
  if (spec.startsWith('github:')) {
    const rest = spec.slice('github:'.length);
    const [repo, version] = rest.split('#');
    return { type: 'github', specifier: repo, version };
  }

  // npm scoped package
  if (spec.startsWith('@')) {
    const [name, version] = spec.split('@').filter(Boolean);
    return { type: 'npm', specifier: `@${name}`, version };
  }

  // Modulus registry (default)
  return { type: 'modulus', specifier: spec };
}

/**
 * Plugin registry — manages installed plugins.
 *
 * Stored in PluresDB under the `plugins:` namespace.
 */
export class PluginRegistry {
  private plugins: Map<string, InstalledPlugin> = new Map();

  /**
   * Load the registry from PluresDB.
   */
  loadFromDB(dbGet: (key: string) => unknown): void {
    const stored = dbGet('plugins:installed') as InstalledPlugin[] | undefined;
    if (stored) {
      for (const p of stored) {
        this.plugins.set(p.manifest.id, p);
      }
    }
  }

  /**
   * Save the registry to PluresDB.
   */
  saveToDB(dbPut: (key: string, value: unknown) => void): void {
    dbPut('plugins:installed', [...this.plugins.values()]);
  }

  /**
   * Register an installed plugin.
   */
  install(plugin: InstalledPlugin): void {
    this.plugins.set(plugin.manifest.id, plugin);
  }

  /**
   * Uninstall a plugin by ID.
   */
  uninstall(id: string): boolean {
    return this.plugins.delete(id);
  }

  /**
   * Get all installed plugins.
   */
  list(): InstalledPlugin[] {
    return [...this.plugins.values()];
  }

  // NOTE: enabled/disabled state is NOT owned here. The single source of truth
  // for whether a plugin may activate is the persisted praxis fact
  // `admin.plugins.enabled` (see praxis/admin.ts + plugin-loader.activateAll).
  // A per-registry enabled()/toggle() previously lived here but was orphaned
  // (zero callers) and would have been a competing authority — removed to avoid
  // drift. `InstalledPlugin.enabled` remains only as descriptive manifest state.

  /**
   * Get a specific plugin.
   */
  get(id: string): InstalledPlugin | undefined {
    return this.plugins.get(id);
  }
}
