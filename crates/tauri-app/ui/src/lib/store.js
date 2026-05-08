// src/lib/store.js
// Application state via @plures/unum reactive bindings.
// All mutations governed by Praxis rules — invalid transitions are blocked.
// Backed by PluresDB when connected (Tauri), in-memory adapter otherwise.

import { persistentStore } from './unum-store.js';
import { validate } from './praxis.js';
import { recordChronos } from './api.js';
import { derived } from 'svelte/store';

/**
 * Create a Praxis-governed persistent store.
 * Every .set() call is validated against Praxis rules before committing.
 * Invalid mutations are blocked and logged as ConstraintViolation chronos events.
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
      return;
    }
    recordChronos('Update', path, { value: typeof value === 'object' ? '(object)' : value });
    originalSet(value);
  };

  if (originalUpdate) {
    store.update = (fn) => {
      let current;
      const unsub = store.subscribe(v => { current = v; });
      unsub();
      const next = fn(current);
      store.set(next);
    };
  }

  return store;
}

// ── UI state (governed) ──────────────────────────────────────────────────────

// Canvas panes — array of { id, pluginId } objects
// Single pane = [{ id: '1', pluginId: 'chat' }]
// Split = [{ id: '1', pluginId: 'chat' }, { id: '2', pluginId: 'chronicle' }]
export const canvasPanes = persistentStore('radix/ui/canvas', [{ id: '1', pluginId: 'chat' }]);

// Which pane is focused (receives keyboard input)
export const focusedPane = persistentStore('radix/ui/focusedPane', '1');

// Backward-compatible derived store: the focused pane's pluginId
export const activeView = derived([canvasPanes, focusedPane], ([$panes, $focused]) => {
  const pane = $panes.find(p => p.id === $focused);
  return pane?.pluginId || 'chat';
});

export const settings = governedStore('radix/settings', {
  model: { primary: 'claude-sonnet-4.5', deep: 'claude-opus-4.6', copilot: true },
  theme: 'dark',
});

// ── UI state (ungoverned) ────────────────────────────────────────────────────

export const commandPaletteOpen = persistentStore('radix/ui/commandPaletteOpen', false);

// ── Canvas manipulation helpers ──────────────────────────────────────────────

export function splitRight() {
  let panes, focused;
  canvasPanes.subscribe(v => panes = v)();
  focusedPane.subscribe(v => focused = v)();
  const current = panes.find(p => p.id === focused);
  if (!current) return;
  const newId = String(Date.now());
  canvasPanes.set([...panes, { id: newId, pluginId: current.pluginId }]);
  focusedPane.set(newId);
  recordChronos('Update', 'canvas', { action: 'splitRight', pluginId: current.pluginId });
}

export function closeActivePane() {
  let panes, focused;
  canvasPanes.subscribe(v => panes = v)();
  focusedPane.subscribe(v => focused = v)();
  if (panes.length <= 1) return;
  const remaining = panes.filter(p => p.id !== focused);
  canvasPanes.set(remaining);
  focusedPane.set(remaining[0].id);
  recordChronos('Update', 'canvas', { action: 'closePane', paneId: focused });
}

export function focusNextPane() {
  let panes, focused;
  canvasPanes.subscribe(v => panes = v)();
  focusedPane.subscribe(v => focused = v)();
  const idx = panes.findIndex(p => p.id === focused);
  const next = panes[(idx + 1) % panes.length];
  focusedPane.set(next.id);
}

export function singlePane() {
  let panes, focused;
  canvasPanes.subscribe(v => panes = v)();
  focusedPane.subscribe(v => focused = v)();
  const current = panes.find(p => p.id === focused);
  if (!current) return;
  canvasPanes.set([{ id: '1', pluginId: current.pluginId }]);
  focusedPane.set('1');
  recordChronos('Update', 'canvas', { action: 'singlePane' });
}

// Canvas commands for command palette registration
export const canvasCommands = [
  { label: 'Split Right', action: splitRight },
  { label: 'Close Pane', action: closeActivePane },
  { label: 'Focus Next Pane', action: focusNextPane },
  { label: 'Single Pane', action: singlePane },
];
