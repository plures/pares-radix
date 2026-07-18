/**
 * Plugin Context Factory — the runtime bridge that makes `ctx.data.collection()`
 * (and the rest of `PluginContext`) REAL for domain plugins.
 *
 * Background: `PluginContext` (src/lib/types/plugin.ts) was type-only —
 * documented and referenced by plugins (e.g. financial-advisor's 8 pages call
 * `ctx.data.collection(name)`), but no code ever constructed it and
 * `activateAll()` was dead. Every plugin coded against the paved data API was
 * therefore non-functional at runtime.
 *
 * This module closes that gap WITHOUT inventing a new storage primitive. The
 * `data.collection()` implementation is a thin, real adapter over the
 * already-shipping PluresDB-namespaced layer (`getSharedAdapter()` →
 * `putPluginData/getPluginData/queryPluginData/deletePluginData`, keyed under
 * `pluresdb:plugin:{pluginId}/{subpath}` via PLUGIN_DATA_PREFIX). Because that
 * adapter sits on a swappable `PluresDBGraph` (localStorage today, real
 * PluresDB/Hyperswarm later), collections inherit the same swap seam — no
 * call-site hardcodes localStorage.
 *
 * The other context members reuse the existing real platform layers:
 *   - settings   → settingsAPI            (src/lib/stores/settings.ts)
 *   - llm        → createLLMAPI()          (src/lib/platform/llm.ts; available()-gated)
 *   - inference  → createInferenceEngine() (src/lib/platform/inference-engine.ts),
 *                  wired over two plugin-scoped collections
 *   - navigation → injectable goto (SSR/test-safe default) + breadcrumbs store
 *   - notify     → a real, swappable sink (console by default; the Svelte layer
 *                  can route it to a toast component via setNotifySink)
 *
 * Honest-absence note (C-NOSTUB-001): every member returns a real
 * implementation. None throws a placeholder. The only intentionally minimal
 * surface is `notify` (no toast component exists yet in the shell, so the
 * default sink logs); it is a real sink, swappable at runtime, not a fake.
 */

import type {
  PluginContext,
  DataAPI,
  CollectionAPI,
  NavigationAPI,
  NotifyAPI,
} from '../types/plugin.js';
import { getSharedAdapter } from '../stores/plures-db-adapter.js';
import { settingsAPI } from '../stores/settings.js';
import { breadcrumbs } from '../stores/breadcrumbs.svelte.js';
import { createLLMAPI } from './llm.js';
import { createInferenceEngine } from './inference-engine.js';

// ─── Notify sink (real, swappable) ──────────────────────────────────────────
//
// The shell does not yet ship a toast component. Rather than throw a stub or
// fake success, NotifyAPI writes to a real sink. The default sink logs to the
// console; the Svelte layer can install a toast-backed sink at startup via
// setNotifySink() and every plugin's ctx.notify.* will route to it.

export type NotifyLevel = 'success' | 'info' | 'warning' | 'error';

/** A notification sink: receives (level, message). */
export type NotifySink = (level: NotifyLevel, message: string) => void;

const defaultNotifySink: NotifySink = (level, message) => {
  const fn = level === 'error' ? console.error : level === 'warning' ? console.warn : console.log;
  fn(`[radix:notify:${level}] ${message}`);
};

let notifySink: NotifySink = defaultNotifySink;

/**
 * Install a notification sink (e.g. a toast-component-backed one) at startup.
 * Until called, notifications log to the console. Pass no argument or the
 * default to reset.
 */
export function setNotifySink(sink: NotifySink): void {
  notifySink = sink;
}

/** Reset the notification sink to the default console logger (used in tests). */
export function resetNotifySink(): void {
  notifySink = defaultNotifySink;
}

function createNotifyAPI(): NotifyAPI {
  return {
    success(message: string): void {
      notifySink('success', message);
    },
    info(message: string): void {
      notifySink('info', message);
    },
    warning(message: string): void {
      notifySink('warning', message);
    },
    error(message: string): void {
      notifySink('error', message);
    },
  };
}

// ─── Data API (THE GAP — now real) ──────────────────────────────────────────
//
// `data.collection(name)` returns a CollectionAPI backed by the shared
// PluresDB adapter. Records live at the namespaced key
// `pluresdb:plugin:{pluginId}/{name}/{id}`. Query/count read every subpath
// under `{name}/` and filter in memory.

/** Matches a record document for in-memory filtering. */
type DocRecord = Record<string, unknown>;

function shallowMatches(doc: unknown, filter: Record<string, unknown>): boolean {
  if (doc === null || typeof doc !== 'object') return false;
  const rec = doc as DocRecord;
  for (const [key, want] of Object.entries(filter)) {
    if (rec[key] !== want) return false;
  }
  return true;
}

function createCollection(pluginId: string, name: string): CollectionAPI {
  // Resolve the adapter lazily per call so collections created before
  // setSharedAdapter() (e.g. during plugin construction) still work once the
  // adapter is wired at boot. A missing adapter is a real, narrowly-scoped
  // error — not a silent fake.
  function adapter() {
    const a = getSharedAdapter();
    if (!a) {
      throw new Error(
        `[radix] PluresDB adapter not initialised; cannot access collection ` +
          `"${name}" for plugin "${pluginId}". Call setSharedAdapter() at boot ` +
          `before activating plugins.`,
      );
    }
    return a;
  }

  const sub = (id: string) => `${name}/${id}`;

  return {
    async get(id: string): Promise<unknown> {
      const value = adapter().getPluginData(pluginId, sub(id));
      return value ?? null;
    },

    async put(id: string, data: unknown): Promise<void> {
      adapter().putPluginData(pluginId, sub(id), data);
    },

    async delete(id: string): Promise<void> {
      adapter().deletePluginData(pluginId, sub(id));
    },

    async query(filter?: Record<string, unknown>): Promise<unknown[]> {
      const prefix = `${name}/`;
      const rows = adapter()
        .queryPluginData(pluginId)
        .filter((row) => row.subpath.startsWith(prefix))
        .map((row) => row.value);
      if (!filter || Object.keys(filter).length === 0) return rows;
      return rows.filter((doc) => shallowMatches(doc, filter));
    },

    async count(): Promise<number> {
      const prefix = `${name}/`;
      return adapter()
        .queryPluginData(pluginId)
        .filter((row) => row.subpath.startsWith(prefix)).length;
    },
  };
}

function createDataAPI(pluginId: string): DataAPI {
  // Memoise collections per name so repeated collection(name) calls within a
  // plugin share one instance (and one inference engine, below).
  const cache = new Map<string, CollectionAPI>();
  return {
    collection(name: string): CollectionAPI {
      let coll = cache.get(name);
      if (!coll) {
        coll = createCollection(pluginId, name);
        cache.set(name, coll);
      }
      return coll;
    },
  };
}

// ─── Navigation API ─────────────────────────────────────────────────────────
//
// `goto` is injectable so this factory has no hard dependency on
// `$app/navigation` (which is unavailable in SSR/unit-test contexts). The app
// bootstrap passes SvelteKit's goto; the default is a safe no-op that records
// intent to the console so a missing wire-up is visible, not silent.

export interface PluginContextOptions {
  /** Navigation function (SvelteKit `goto`). Defaults to a console-logging no-op. */
  goto?: (href: string) => void;
}

function createNavigationAPI(opts: PluginContextOptions): NavigationAPI {
  const goto =
    opts.goto ??
    ((href: string) => {
      // eslint-disable-next-line plures/no-manual-logging
      console.warn(`[radix:navigation] goto("${href}") called but no navigator wired.`);
    });
  return {
    goto(href: string): void {
      goto(href);
    },
    setBreadcrumbs(crumbs: { label: string; href?: string }[]): void {
      breadcrumbs.set(crumbs);
    },
  };
}

// ─── PluginContext Factory ──────────────────────────────────────────────────

/** Internal collection names for the per-plugin inference engine. */
const INFERENCE_COLLECTION = '_inferences';
const DECISION_COLLECTION = '_decisions';

/**
 * Build a real, `pluginId`-scoped PluginContext.
 *
 * Every member is functional:
 *   - data: collections persist through the PluresDB-namespaced adapter
 *   - settings: the shared SettingsAPI
 *   - llm: the shared LLMAPI (available() returns false until a provider is set)
 *   - inference: a real engine over two plugin-scoped collections
 *   - navigation: injected goto + breadcrumbs
 *   - notify: the swappable notify sink
 *
 * @param pluginId  The plugin's id; all data is namespaced under it.
 * @param opts      Optional wiring (e.g. SvelteKit goto).
 */
export function createPluginContext(
  pluginId: string,
  opts: PluginContextOptions = {},
): PluginContext {
  const data = createDataAPI(pluginId);
  return {
    settings: settingsAPI,
    data,
    llm: createLLMAPI(),
    inference: createInferenceEngine(
      data.collection(INFERENCE_COLLECTION),
      data.collection(DECISION_COLLECTION),
    ),
    navigation: createNavigationAPI(opts),
    notify: createNotifyAPI(),
  };
}
