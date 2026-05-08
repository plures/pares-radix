/**
 * @plures/canvas-runtime — the engine that turns PluresDB data into running apps.
 *
 * The AI writes data. Apps materialize. No code. No compile. No deploy.
 *
 * @module @plures/canvas-runtime
 */

// Component Registry
export {
  ComponentRegistry,
  registerComponent,
  resolveComponent,
  listComponents,
  listByCategory,
  getRegistry,
  generateCatalog,
  registerDesignDojo,
} from './registry.js';
export type { ComponentMeta, PropSchema } from './registry.js';

// Canvas Document Format
export type {
  CanvasDocument,
  CanvasNode,
  CanvasBinding,
  CanvasCondition,
  CanvasRule,
  CanvasProcedure,
  CanvasTrigger,
  CanvasStep,
  CanvasMeta,
  CanvasSchema,
  CanvasExport,
  CanvasTimelineEntry,
} from './format.js';
export { createCanvas, exportCanvas, importCanvas, validateCanvas } from './format.js';

// Reactive Graph Bridge
export { createReactiveGraph, putWithActor } from './reactive-graph.js';
export type { ReactiveGraph, WriteOptions } from './reactive-graph.js';

// AI Canvas Plugin
export {
  initCanvasPlugin,
  toolCanvasCreate,
  toolCanvasSetTree,
  toolCanvasAddNode,
  toolCanvasRemoveNode,
  toolCanvasSetData,
  toolCanvasAddRule,
  toolCanvasAddProcedure,
  toolCanvasExport,
  toolCanvasImport,
  toolCanvasCatalog,
  toolCanvasValidate,
} from './canvas-plugin.js';
export type { CanvasPluginState } from './canvas-plugin.js';
