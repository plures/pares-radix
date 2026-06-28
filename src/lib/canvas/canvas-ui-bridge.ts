/**
 * canvas-ui-bridge — APP-LAYER wiring that closes the responsive-engine loop on
 * Radix's real /canvas surface.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * The canvas-runtime engine is pure + reactive but UNWIRED in the app until a
 * producer writes its two trigger facts into the canvas reactive graph:
 *   - ui:viewport  ← window resize          (attachViewportBridge, the IO edge)
 *   - ui:theme     ← the existing app theme  (this file bridges 'theme.applied')
 *
 * This module provides the two small, honest bridges both the /canvas route
 * (src/routes/canvas/+page.svelte) and the plugin embed
 * (src/lib/plugins/canvas/CanvasView.svelte) call on mount, so the loop works on
 * either entry point with identical logic (no drift between the two surfaces).
 *
 * WHY THEME LIVES HERE AS A PURE PUT (not a graph subscription):
 * the canonical app theme is the praxis fact 'theme.applied' ({ value }), held in
 * a Svelte $state rune (praxis-svelte.svelte.ts). Its changes are observable via
 * the `query('theme.applied')` rune INSIDE a Svelte reactive context — not via a
 * reactive-graph subscription (the adapter persists the fact on the BASE graph,
 * which doesn't notify the reactive wrapper). So each component runs an $effect
 * that reads the rune and calls `bridgeThemeToGraph` here. We do NOT add a second
 * theme toggle — we only MIRROR the existing one.
 *
 * HONESTLY ABSENT: ui:density has no producer. No density control/app-state
 * exists yet, so we deliberately leave ui:density unproduced; the density resolve
 * practices simply stay inert until a density control is added. (Not faked.)
 * ─────────────────────────────────────────────────────────────────────────────
 */

import { browser } from '$app/environment';
import { attachViewportBridge } from '@plures/canvas-runtime';
import type { PluresDBGraph } from '$lib/stores/plures-db-adapter.js';

/** The graph surface these bridges need (put is enough; matches ReactiveGraph). */
type GraphLike = Pick<PluresDBGraph, 'put' | 'get'>;

/** The shape the existing praxis 'theme.applied' fact carries. */
export type ThemeApplied = { value: 'light' | 'dark' } | undefined;

/** The canvas resolver's ui:theme trigger key + shape. */
export const UI_THEME_KEY = 'ui:theme';

/**
 * Mount the viewport bridge (the single IO edge). Browser-only; on the server it
 * is a no-op returning a no-op detach. Returns the detach to call on destroy.
 */
export function mountViewportBridge(graph: GraphLike): () => void {
  if (!browser) return () => {};
  // attachViewportBridge seeds ui:viewport immediately and on every resize.
  return attachViewportBridge(graph as Parameters<typeof attachViewportBridge>[0]);
}

/**
 * Mirror the current 'theme.applied' value into the canvas graph as ui:theme.
 * Pure + idempotent: writes { mode } only when the value is a real mode and has
 * actually changed (avoids redundant puts/notifications). Returns true if it
 * wrote. Safe to call on the server (it still writes; the resolver reads it).
 */
export function bridgeThemeToGraph(graph: GraphLike, theme: ThemeApplied): boolean {
  const mode = theme?.value;
  if (mode !== 'light' && mode !== 'dark') return false; // honest: don't fake a mode
  const current = graph.get(UI_THEME_KEY) as { mode?: string } | undefined;
  if (current?.mode === mode) return false; // no-op when unchanged
  graph.put(UI_THEME_KEY, { mode });
  return true;
}
