// src/lib/plugins/registry.js

import { writable, derived } from 'svelte/store';

/**
 * @typedef {Object} RadixPlugin
 * @property {string} id
 * @property {string} name
 * @property {string} icon
 * @property {string} description
 * @property {boolean} enabled
 * @property {import('svelte').Component | null} component - Main view component
 * @property {import('svelte').Component | null} sidebarComponent - Optional sidebar panel
 * @property {Object[]} commands - Commands this plugin registers
 * @property {Object} settings - Plugin-specific settings schema
 */

/** @type {import('svelte/store').Writable<RadixPlugin[]>} */
export const pluginRegistry = writable([]);

/** Active (enabled) plugins only */
export const activePlugins = derived(pluginRegistry, $plugins =>
  $plugins.filter(p => p.enabled)
);

/** Register a plugin */
export function registerPlugin(plugin) {
  pluginRegistry.update(plugins => {
    if (plugins.find(p => p.id === plugin.id)) return plugins;
    return [...plugins, plugin];
  });
}

/** Enable/disable a plugin */
export function togglePlugin(id) {
  pluginRegistry.update(plugins =>
    plugins.map(p => p.id === id ? { ...p, enabled: !p.enabled } : p)
  );
}

/** Get commands from all active plugins */
export const allCommands = derived(pluginRegistry, $plugins => {
  const cmds = [];
  for (const p of $plugins.filter(p => p.enabled)) {
    for (const cmd of (p.commands || [])) {
      cmds.push({ ...cmd, pluginId: p.id, label: `${p.name}: ${cmd.label}` });
    }
  }
  return cmds;
});
