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

export const activeView = governedStore('radix/ui/activeView', 'chat');
export const settings = governedStore('radix/settings', {
  model: { primary: 'claude-sonnet-4.5', deep: 'claude-opus-4.6', copilot: true },
  theme: 'dark',
});

// ── UI state (ungoverned) ────────────────────────────────────────────────────

export const commandPaletteOpen = persistentStore('radix/ui/commandPaletteOpen', false);
