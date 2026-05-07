// src/lib/praxis.js
// Client-side Praxis — rules that govern UI state transitions.
// Every mutation passes through validate() before committing.

import { readable } from 'svelte/store';

/**
 * @typedef {Object} Rule
 * @property {string} id
 * @property {string} description
 * @property {(state: any, mutation: any) => boolean} validate - returns true if valid
 * @property {string} violation - message when rule fails
 */

/** @type {Rule[]} */
const rules = [];

/** @type {Array<{rule: string, mutation: any, timestamp: string}>} */
const violations = [];

/** @type {Set<(count: number) => void>} */
const subscribers = new Set();

function notifySubscribers() {
  for (const fn of subscribers) fn(violations.length);
}

export function addRule(rule) {
  rules.push(rule);
}

/**
 * Validate a state mutation against all rules.
 * @param {any} state - current application state snapshot
 * @param {{ path: string, value: any }} mutation
 * @returns {{ valid: boolean, violations: string[] }}
 */
export function validate(state, mutation) {
  const failed = [];
  for (const rule of rules) {
    if (!rule.validate(state, mutation)) {
      failed.push(rule.violation);
      violations.push({ rule: rule.id, mutation, timestamp: new Date().toISOString() });
    }
  }
  if (failed.length > 0) notifySubscribers();
  return { valid: failed.length === 0, violations: failed };
}

/** Get all violation history */
export function getViolations() { return [...violations]; }

/** Clear violations */
export function clearViolations() {
  violations.length = 0;
  notifySubscribers();
}

/** Svelte readable store tracking violation count — push-based, no polling */
export const praxisViolationCount = readable(0, (set) => {
  const fn = (count) => set(count);
  subscribers.add(fn);
  return () => subscribers.delete(fn);
});

// ─── Built-in Rules ───────────────────────────────────────────────────────────

// Rule: Plugin must be registered before activation
addRule({
  id: 'plugin.registered-before-active',
  description: 'A plugin must be in the registry before it can be activated as the current view',
  validate: (state, mutation) => {
    if (mutation.path === 'radix/ui/activeView') {
      const pluginIds = (state.plugins || []).map(p => p.id);
      return pluginIds.includes(mutation.value) || mutation.value === 'extensions';
    }
    return true;
  },
  violation: 'Cannot switch to unregistered plugin view',
});

// Rule: Settings model must be a non-empty string
addRule({
  id: 'settings.model-required',
  description: 'Primary model setting must be a non-empty string',
  validate: (state, mutation) => {
    if (mutation.path === 'radix/settings') {
      return mutation.value?.model?.primary?.length > 0;
    }
    return true;
  },
  violation: 'Primary model cannot be empty',
});

// Rule: Panel height must be within bounds
addRule({
  id: 'ui.panel-height-bounds',
  description: 'Terminal panel height must be between 100 and 600 pixels',
  validate: (state, mutation) => {
    if (mutation.path === 'radix/ui/panelHeight') {
      return mutation.value >= 100 && mutation.value <= 600;
    }
    return true;
  },
  violation: 'Panel height must be between 100-600px',
});

// Rule: Cannot disable the chat plugin (it's the default view)
addRule({
  id: 'plugin.chat-always-enabled',
  description: 'The Chat plugin cannot be disabled — it is the default landing view',
  validate: (state, mutation) => {
    if (mutation.path === 'plugin.toggle' && mutation.value?.id === 'chat') {
      return mutation.value?.enabled !== false;
    }
    return true;
  },
  violation: 'Chat plugin cannot be disabled (default view)',
});
