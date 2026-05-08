// src/lib/telemetry.js — Comprehensive Chronos instrumentation layer
// Every meaningful state change, API call, render, and error gets recorded.
// This is the "flight recorder" — when something goes wrong, this tells us what happened.

import { recordChronos, getChronosLog } from './api.js';

// ── Performance tracking ────────────────────────────────────────────────────

/**
 * Wrap an async function with Chronos instrumentation.
 * Records: call start, success (with duration), failure (with error).
 * @param {string} key - Chronos key (e.g., 'api:getSettings')
 * @param {Function} fn - The async function to wrap
 * @returns {Function} Instrumented function
 */
export function traced(key, fn) {
  return async function (...args) {
    const start = performance.now();
    const callId = Math.random().toString(36).slice(2, 8);
    recordChronos('Call', key, { callId, args: summarizeArgs(args) });
    try {
      const result = await fn.apply(this, args);
      const durationMs = Math.round(performance.now() - start);
      recordChronos('Return', key, { callId, durationMs, resultType: typeof result });
      return result;
    } catch (error) {
      const durationMs = Math.round(performance.now() - start);
      recordChronos('Error', key, { callId, durationMs, error: error?.message || String(error) });
      throw error;
    }
  };
}

/**
 * Record a component lifecycle event.
 * @param {string} componentName
 * @param {'mount'|'destroy'|'render'|'error'} event
 * @param {object} [data]
 */
export function componentEvent(componentName, event, data = {}) {
  recordChronos('Component', `ui:${componentName}`, { event, ...data });
}

/**
 * Record a navigation/routing event.
 * @param {string} from - Previous state
 * @param {string} to - New state
 * @param {object} [data]
 */
export function navigationEvent(from, to, data = {}) {
  recordChronos('Navigate', 'ui:navigation', { from, to, ...data });
}

/**
 * Record a store state change with before/after diff.
 * @param {string} storeName
 * @param {*} oldValue
 * @param {*} newValue
 */
export function storeChanged(storeName, oldValue, newValue) {
  recordChronos('StateChange', `store:${storeName}`, {
    before: summarizeValue(oldValue),
    after: summarizeValue(newValue),
  });
}

/**
 * Record a user interaction.
 * @param {string} element - What was interacted with
 * @param {string} action - click, keypress, etc.
 * @param {object} [data]
 */
export function userAction(element, action, data = {}) {
  recordChronos('UserAction', `ui:${element}`, { action, ...data });
}

/**
 * Record an error boundary catch.
 * @param {string} boundary - Component or module name
 * @param {Error} error
 */
export function errorCaught(boundary, error) {
  recordChronos('ErrorBoundary', `error:${boundary}`, {
    message: error?.message || String(error),
    stack: (error?.stack || '').split('\n').slice(0, 3).join(' | '),
  });
}

/**
 * Get a diagnostic snapshot — recent Chronos entries formatted for debugging.
 * @param {number} count - Number of recent entries to include
 * @returns {string} Formatted diagnostic output
 */
export function diagnosticSnapshot(count = 50) {
  const log = getChronosLog();
  const recent = log.slice(-count);
  return recent.map(e =>
    `${e.timestamp} [${e.action}] ${e.key} ${JSON.stringify(e.data)}`
  ).join('\n');
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function summarizeArgs(args) {
  return args.map(a => {
    if (typeof a === 'string') return a.length > 50 ? a.slice(0, 50) + '…' : a;
    if (typeof a === 'number' || typeof a === 'boolean') return a;
    if (a === null || a === undefined) return a;
    if (Array.isArray(a)) return `[Array(${a.length})]`;
    return `{${Object.keys(a).join(',')}}`;
  });
}

function summarizeValue(val) {
  if (val === null || val === undefined) return val;
  if (typeof val === 'string') return val.length > 80 ? val.slice(0, 80) + '…' : val;
  if (typeof val === 'number' || typeof val === 'boolean') return val;
  if (Array.isArray(val)) return `[${val.length} items]`;
  if (typeof val === 'object') {
    const keys = Object.keys(val);
    return `{${keys.slice(0, 5).join(',')}${keys.length > 5 ? ',…' : ''}}`;
  }
  return String(val);
}
