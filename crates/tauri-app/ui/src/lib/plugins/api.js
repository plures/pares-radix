// src/lib/plugins/api.js — Formal Plugin API contract for pares-radix
// ⛔ CONSTRAINTS: design-dojo ONLY. Praxis rules. Chronos logging. Unum state. PluresDB data.

/**
 * RadixPlugin — the formal contract for pares-radix plugins.
 *
 * Every plugin MUST implement this interface to be loaded by radix.
 *
 * @typedef {Object} RadixPlugin
 * @property {string} id — Unique plugin identifier
 * @property {string} name — Human-readable name
 * @property {string} iconPath — SVG path data for the activity bar icon (16x16 viewBox)
 * @property {string} [description] — Short description for the extensions panel
 * @property {string} [version] — SemVer version string
 *
 * @property {import('svelte').Component} view — Main canvas component (rendered when plugin is active)
 * @property {import('svelte').Component} [sidebarPanel] — Optional sidebar component (shown alongside view)
 * @property {StatusBarContribution[]} [statusBarItems] — Items contributed to the status bar
 * @property {Command[]} [commands] — Commands contributed to the command palette
 * @property {function} [onActivate] — Called when plugin becomes active (receives PluginContext)
 * @property {function} [onDeactivate] — Called when plugin becomes inactive
 */

/**
 * @typedef {Object} StatusBarContribution
 * @property {string} id
 * @property {string} text — Text to display
 * @property {'left'|'right'} [position] — Which side of the status bar (default: left)
 * @property {number} [priority] — Sort order (higher = more left/right)
 * @property {function} [onclick] — Click handler
 */

/**
 * @typedef {Object} Command
 * @property {string} id — Unique command ID (e.g. 'chat.clear')
 * @property {string} label — Display label in command palette
 * @property {string} [keybinding] — Keyboard shortcut (e.g. 'Ctrl+Shift+C')
 * @property {function} action — What happens when command is triggered
 */

/**
 * Platform APIs available to plugins via the context object.
 * Passed to onActivate() and available throughout plugin lifecycle.
 */
export class PluginContext {
  constructor(pluginId) {
    this.pluginId = pluginId;
  }

  /** Show a notification toast */
  notify(message, type = 'info') {
    // type: 'info' | 'success' | 'warning' | 'error'
    // TODO: wire to design-dojo NotificationStack
    console.log(`[${this.pluginId}] ${type}: ${message}`);
  }

  /** Record a Chronos event from this plugin */
  async recordEvent(action, data = {}) {
    const { recordChronos } = await import('../api.js');
    recordChronos(action, `plugin:${this.pluginId}`, data);
  }

  /** Access the Unum store for this plugin's namespace */
  getStore(key, defaultValue) {
    // Dynamic import to avoid circular deps at module level
    return import('../unum-store.js').then(m =>
      m.persistentStore(`plugin/${this.pluginId}/${key}`, defaultValue)
    );
  }
}

/**
 * Validate a plugin object against the RadixPlugin contract.
 * Returns { valid: boolean, errors: string[] }
 */
export function validatePlugin(plugin) {
  const errors = [];

  if (!plugin.id || typeof plugin.id !== 'string') errors.push('id is required (string)');
  if (!plugin.name || typeof plugin.name !== 'string') errors.push('name is required (string)');
  if (!plugin.iconPath || typeof plugin.iconPath !== 'string') errors.push('iconPath is required (SVG path string)');
  if (!plugin.view && !plugin.component && !plugin.commands?.length) errors.push('view or commands required');
  if (plugin.commands && !Array.isArray(plugin.commands)) errors.push('commands must be an array');
  if (plugin.statusBarItems && !Array.isArray(plugin.statusBarItems)) errors.push('statusBarItems must be an array');

  return { valid: errors.length === 0, errors };
}
