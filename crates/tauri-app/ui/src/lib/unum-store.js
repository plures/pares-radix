// src/lib/unum-store.js
// Unum-backed persistent stores for pares-radix.
// Uses PluresDB adapter when available (Tauri mode), memory adapter otherwise.
// All stores implement Svelte store contract (subscribe/set/update).

import { initDb, createPluresStore, createMemoryAdapter, db } from '@plures/unum';

// Initialize with memory adapter immediately.
// In Tauri mode, the app entry point can call upgradeAdapter() with a PluresDB adapter.
let initialized = false;

function ensureInit() {
  if (!initialized) {
    initDb(createMemoryAdapter());
    initialized = true;
  }
}

/**
 * Upgrade the backing adapter (e.g. when PluresDB becomes available in Tauri).
 * Existing stores will pick up the new adapter on next write.
 * @param {import('@plures/unum').DbAdapter} adapter
 */
export function upgradeAdapter(adapter) {
  const { destroyDb, initDb: init } = /** @type {any} */ (
    import.meta.glob ? undefined : undefined
  );
  // Dynamic re-init — import at call site to avoid circular
  import('@plures/unum').then(({ destroyDb, initDb }) => {
    destroyDb();
    initDb(adapter);
  });
}

/**
 * Create a reactive store backed by PluresDB (via Unum).
 * Falls back to in-memory adapter when PluresDB is unavailable.
 * Svelte store contract: subscribe(), set(), update().
 *
 * @template T
 * @param {string} path - PluresDB path (e.g. 'radix/ui/activeView')
 * @param {T} [initialValue] - Default value before first DB read
 * @returns {import('@plures/unum').PluresStore<T>}
 */
export function persistentStore(path, initialValue) {
  ensureInit();
  return createPluresStore(path, initialValue);
}
