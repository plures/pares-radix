// src/lib/store.js
// Application state via @plures/unum reactive bindings.
// All mutations governed by Praxis rules — invalid transitions are blocked.
// Backed by PluresDB when connected (Tauri), in-memory adapter otherwise.

import { persistentStore } from './unum-store.js';
import { validate } from './praxis.js';
import { recordChronos } from './api.js';

/**
 * Create a Praxis-governed persistent store.
 * Every .set() call is validated against Praxis rules before committing.
 * Invalid mutations are blocked and logged as ConstraintViolation chronos events.
 *
 * @template T
 * @param {string} path - PluresDB path
 * @param {T} initialValue - Default value
 * @param {() => any} [getState] - Optional fn to get current app state for cross-store validation
 * @returns {import('@plures/unum').PluresStore<T>}
 */
export function governedStore(path, initialValue, getState) {
  const store = persistentStore(path, initialValue);
  const originalSet = store.set.bind(store);
  const originalUpdate = store.update?.bind(store);

  store.set = (value) => {
    const state = getState ? getState() : {};
    const result = validate(state, { path, value });
    if (!result.valid) {
      console.warn('[praxis]', result.violations.join(', '));
      recordChronos('ConstraintViolation', path, { violations: result.violations });
      return; // Block the mutation
    }
    recordChronos('Update', path, { value: typeof value === 'object' ? '(object)' : value });
    originalSet(value);
  };

  if (originalUpdate) {
    store.update = (fn) => {
      // For update, we need current value — subscribe once
      let current;
      const unsub = store.subscribe(v => { current = v; });
      unsub();
      const next = fn(current);
      store.set(next); // Goes through governed set
    };
  }

  return store;
}

// ── State accessor for cross-store validation ────────────────────────────────

function getAppState() {
  let p = [];
  const unsub = plugins.subscribe(v => { p = v; });
  unsub();
  return { plugins: p };
}

// ── UI state (governed) ──────────────────────────────────────────────────────

export const activeView = governedStore('radix/ui/activeView', 'chat', getAppState);
export const panelHeight = governedStore('radix/ui/panelHeight', 200);
export const settings = governedStore('radix/settings', {
  model: { primary: 'claude-sonnet-4.5', deep: 'claude-opus-4.6', copilot: true },
  theme: 'dark',
  sidebarWidth: 280,
});

// ── UI state (ungoverned — no rules apply yet) ──────────────────────────────

export const sidebarOpen = persistentStore('radix/ui/sidebarOpen', true);
export const sidebarPanel = persistentStore('radix/ui/sidebarPanel', 'memory');
export const commandPaletteOpen = persistentStore('radix/ui/commandPaletteOpen', false);
export const panelOpen = persistentStore('radix/ui/panelOpen', false);

// Plugin registry
export const plugins = persistentStore('radix/plugins', [
  { id: 'chat', name: 'Chat', icon: '💬', component: 'Chat', active: true },
  { id: 'procedures', name: 'Procedures', icon: '⚡', component: 'Procedures', active: true },
  { id: 'config-browser', name: 'Config Browser', icon: '🖥️', component: 'ConfigBrowser', active: false },
  { id: 'chronicle', name: 'Timeline', icon: '📜', component: 'Chronicle', active: false },
]);
