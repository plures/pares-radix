/**
 * UI Viewport Bridge — THE single IO boundary of the reactive layout engine.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * Everything else in the engine is pure (resolveUiTree) or pure pub/sub
 * (reactive-graph). The only side-effecting INPUT is the window size, which lives
 * outside PluresDB (C-PLURES-004: side-effecting input at the edge). This module
 * listens for resize and writes `ui:viewport → { width, height, breakpoint }`
 * into the graph. That write triggers the reactive resolver (ui-reactive.ts).
 *
 * SSR / test safety: if there is no DOM window, attach is a no-op that returns a
 * no-op detach — the engine degrades to "no viewport fact", and the resolver
 * returns identity clones. Nothing throws.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import { breakpointFor } from './ui-schema.js';
import type { PluresDBGraph } from './reactive-graph.js';

/** Minimal window surface this bridge needs (keeps it testable without a real DOM). */
export interface WindowLike {
  innerWidth: number;
  innerHeight?: number;
  addEventListener(type: 'resize', listener: () => void): void;
  removeEventListener(type: 'resize', listener: () => void): void;
}

export interface ViewportBridgeOptions {
  /** Key to write the viewport fact to. Default 'ui:viewport'. */
  key?: string;
  /** Window to observe. Defaults to globalThis if it looks like a window. */
  win?: WindowLike;
}

/** True when `w` has the surface we need. */
function isWindowLike(w: unknown): w is WindowLike {
  return (
    !!w &&
    typeof (w as WindowLike).innerWidth === 'number' &&
    typeof (w as WindowLike).addEventListener === 'function' &&
    typeof (w as WindowLike).removeEventListener === 'function'
  );
}

/** Read the current viewport fact from a window. */
export function readViewport(win: WindowLike): { width: number; height: number; breakpoint: string } {
  const width = win.innerWidth;
  const height = typeof win.innerHeight === 'number' ? win.innerHeight : 0;
  return { width, height, breakpoint: breakpointFor(width) };
}

/**
 * Attach the viewport bridge: write the current viewport immediately, then on
 * every resize. Returns a detach function that removes the listener.
 *
 * No-ops safely when there is no DOM window (returns a no-op detach).
 */
export function attachViewportBridge(
  graph: PluresDBGraph,
  options: ViewportBridgeOptions = {},
): () => void {
  const key = options.key ?? 'ui:viewport';
  const win = options.win ?? (isWindowLike(globalThis) ? (globalThis as unknown as WindowLike) : undefined);

  if (!win) {
    // Headless/SSR/test without a window: nothing to observe.
    return () => {};
  }

  const write = () => {
    graph.put(key, readViewport(win));
  };

  write(); // seed the initial fact
  win.addEventListener('resize', write);

  return () => {
    win.removeEventListener('resize', write);
  };
}
