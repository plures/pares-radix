// src/lib/plugins/registry.js

import { writable, derived } from 'svelte/store';
import { validatePlugin, PluginContext } from './api.js';

/**
 * @typedef {import('./api.js').RadixPlugin} RadixPlugin
 */

/** @type {import('svelte/store').Writable<RadixPlugin[]>} */
export const pluginRegistry = writable([]);

/** Active (enabled) plugins only */
export const activePlugins = derived(pluginRegistry, $plugins =>
  $plugins.filter(p => p.enabled !== false)
);

/** Register a plugin — validates against RadixPlugin contract */
export function registerPlugin(plugin) {
  const { valid, errors } = validatePlugin(plugin);
  if (!valid) {
    console.error(`[radix] Plugin '${plugin.id || 'unknown'}' failed validation:`, errors);
    return false;
  }

  // Normalize: accept 'component' as alias for 'view' (backward compat)
  if (!plugin.view && plugin.component) {
    plugin.view = plugin.component;
  }

  pluginRegistry.update(plugins => {
    if (plugins.find(p => p.id === plugin.id)) return plugins;
    return [...plugins, plugin];
  });

  // Call onActivate with context if provided
  if (plugin.onActivate) {
    plugin.onActivate(new PluginContext(plugin.id));
  }

  return true;
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
  for (const p of $plugins.filter(p => p.enabled !== false)) {
    for (const cmd of (p.commands || [])) {
      cmds.push({ ...cmd, pluginId: p.id, label: `${p.name}: ${cmd.label}` });
    }
  }
  return cmds;
});

/** Collect all status bar items from active plugins, sorted by priority */
export const allStatusBarItems = derived(activePlugins, $plugins => {
  const items = [];
  for (const p of $plugins) {
    for (const item of (p.statusBarItems || [])) {
      items.push({ ...item, pluginId: p.id });
    }
  }
  return items;
});
