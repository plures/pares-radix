/**
 * Reactive PluresDB Bridge — connects CanvasRenderer to the PluresDB graph
 * with reactive subscriptions via a simple pub/sub pattern.
 *
 * This bridges the PluresDBGraph interface (put/get/keys/delete) with the
 * reactive requirements of the canvas renderer (subscribe to key changes).
 *
 * When running inside pares-radix, this wraps the shared graph from
 * plures-db-adapter.ts. When running standalone (e.g. embedded canvas),
 * it can wrap any PluresDBGraph implementation.
 */

/**
 * Minimal PluresDB graph interface — duplicated from the adapter for package independence.
 * In production, this is the same type from plures-db-adapter.ts.
 */
export interface PluresDBGraph {
  put(key: string, value: unknown): void;
  get(key: string): unknown;
  keys(prefix?: string): string[];
  delete(key: string): void;
}

export interface ReactiveGraph extends PluresDBGraph {
  /** Subscribe to changes on a specific key */
  subscribe(key: string, callback: (value: unknown) => void): () => void;
  /** Subscribe to changes on any key with a prefix */
  subscribePrefix(prefix: string, callback: (key: string, value: unknown) => void): () => void;
  /** Get the actor for the last write to a key */
  getLastActor(key: string): { kind: string; id: string } | null;
}

export interface WriteOptions {
  /** Actor making this change */
  actor?: { kind: string; id: string };
}

/**
 * Create a reactive graph that wraps a PluresDBGraph and adds pub/sub.
 *
 * Every put() notifies subscribers for that key AND any prefix subscribers
 * that match. This is what makes the CanvasRenderer live — when the AI writes
 * to PluresDB, the UI updates instantly.
 */
export function createReactiveGraph(base: PluresDBGraph): ReactiveGraph {
  // Subscriber maps
  const keySubscribers = new Map<string, Set<(value: unknown) => void>>();
  const prefixSubscribers = new Map<string, Set<(key: string, value: unknown) => void>>();
  const lastActors = new Map<string, { kind: string; id: string }>();

  function notifyKey(key: string, value: unknown): void {
    const subs = keySubscribers.get(key);
    if (subs) {
      for (const cb of subs) {
        try { cb(value); } catch { /* subscriber error — don't break others */ }
      }
    }

    // Notify prefix subscribers
    for (const [prefix, prefSubs] of prefixSubscribers) {
      if (key.startsWith(prefix)) {
        for (const cb of prefSubs) {
          try { cb(key, value); } catch { /* */ }
        }
      }
    }
  }

  return {
    put(key: string, value: unknown): void {
      base.put(key, value);
      notifyKey(key, value);
    },

    get(key: string): unknown {
      return base.get(key);
    },

    keys(prefix?: string): string[] {
      return base.keys(prefix);
    },

    delete(key: string): void {
      base.delete(key);
      notifyKey(key, undefined);
    },

    subscribe(key: string, callback: (value: unknown) => void): () => void {
      if (!keySubscribers.has(key)) {
        keySubscribers.set(key, new Set());
      }
      keySubscribers.get(key)!.add(callback);

      // Immediately fire with current value
      const current = base.get(key);
      if (current !== undefined) {
        callback(current);
      }

      // Return unsubscribe
      return () => {
        const subs = keySubscribers.get(key);
        if (subs) {
          subs.delete(callback);
          if (subs.size === 0) keySubscribers.delete(key);
        }
      };
    },

    subscribePrefix(prefix: string, callback: (key: string, value: unknown) => void): () => void {
      if (!prefixSubscribers.has(prefix)) {
        prefixSubscribers.set(prefix, new Set());
      }
      prefixSubscribers.get(prefix)!.add(callback);

      return () => {
        const subs = prefixSubscribers.get(prefix);
        if (subs) {
          subs.delete(callback);
          if (subs.size === 0) prefixSubscribers.delete(prefix);
        }
      };
    },

    getLastActor(key: string): { kind: string; id: string } | null {
      return lastActors.get(key) ?? null;
    },
  };
}

/**
 * Extended put with actor tracking — for use by the log-gate and canvas.
 */
export function putWithActor(
  graph: ReactiveGraph,
  key: string,
  value: unknown,
  actor: { kind: string; id: string },
): void {
  // The actor information flows through the graph's notification system
  // and can be picked up by the ProvisionalTracker and Chronos log-gate
  (graph as any)._pendingActor = actor;
  graph.put(key, value);
  (graph as any)._pendingActor = null;
}
