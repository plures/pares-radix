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

// UI Best-Practice Enforcement
export { extractUiFacts } from './ui-facts.js';
export type { CanvasNodeLike, UiFacts } from './ui-facts.js';
export {
  UI_CONSTRAINTS,
  validateUi,
  formatUiViolations,
  evalExpr,
} from './ui-constraints.js';
export type {
  UiConstraint,
  UiViolation,
  UiValidationResult,
  Severity,
} from './ui-constraints.js';

// UI Schema (element kinds × attributes × breakpoints)
export {
  SCHEMA_KINDS,
  kindForComponent,
  RESPONSIVE_ATTRS,
  RESPONSIVE_ATTR_SET,
  BREAKPOINTS,
  BREAKPOINT_ORDER,
  breakpointFor,
  pickResponsive,
} from './ui-schema.js';
export type { SchemaKind, Breakpoint } from './ui-schema.js';

// UI Resolve Practices (resolve-mode best practices, mirror of ui-layout.px)
export { UI_PRACTICES, DEFAULT_BEHAVIORS } from './ui-practices.js';
export type { UiPractice, PracticeSource } from './ui-practices.js';

// UI Resolver (pure: authored tree + facts → resolved tree)
export { resolveUiTree } from './ui-resolve.js';
export type { UiRuntimeFacts, ViewportFact } from './ui-resolve.js';

// UI Reactive Wiring (facts + authored tree → derived resolved tree)
export { wireResolvedTree } from './ui-reactive.js';
export type { WireResolvedTreeOptions } from './ui-reactive.js';

// UI Viewport Bridge (the single IO edge: window resize → ui:viewport fact)
export { attachViewportBridge, readViewport } from './ui-viewport-bridge.js';
export type { WindowLike, ViewportBridgeOptions } from './ui-viewport-bridge.js';

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
