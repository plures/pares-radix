/**
 * Canvas Runtime — the engine that turns PluresDB data into running apps.
 *
 * Architecture:
 *
 * 1. COMPONENT REGISTRY — maps string names to Svelte components at runtime.
 *    All design-dojo components are pre-registered. Plugins can register more.
 *
 * 2. CANVAS DOCUMENT — a PluresDB-native format describing an app:
 *    - Component tree (what to render, props, bindings to PluresDB keys)
 *    - Praxis rules (validation, gates, constraints — stored as data)
 *    - Procedures (what happens on user actions — stored as data)
 *    - Data schema (what PluresDB keys the app uses)
 *
 * 3. DYNAMIC RENDERER — walks the component tree, resolves components from
 *    the registry, binds props to PluresDB via Unum, renders the result.
 *
 * 4. EXPORT FORMAT (.canvas) — serializable, shareable, replayable.
 *    A .canvas file is a self-contained app definition that can be:
 *    - Shared (send someone a file, they open it, app works)
 *    - Versioned (git-friendly JSON)
 *    - Replayed (Chronos timeline embedded)
 *    - Forked (modify someone else's canvas app)
 *
 * The key insight: if the app IS data, then creating an app is just
 * writing data. The AI doesn't generate code — it describes.
 */

export { ComponentRegistry, getRegistry, registerComponent } from './registry.js';
export type {
  CanvasDocument,
  CanvasNode,
  CanvasBinding,
  CanvasRule,
  CanvasProcedure,
  CanvasExport,
} from './format.js';
export { createCanvas, exportCanvas, importCanvas, validateCanvas } from './format.js';
