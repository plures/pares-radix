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

// Demo Canvas — a real authored responsive canvas (single source of truth shared
// by the /canvas app surface and tests/canvas-demo-resolve.test.ts).
export { getDemoCanvas, DEMO_CANVAS_TREE } from './demo-canvas.js';

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

// ── UI Theme/Density/Contrast (Thread 1) ─────────────────────────────────────
// Resolve-mode practice sets keyed on ui:density and ui:theme, plus pure WCAG
// contrast math. ADD-ONLY block (do not reformat lines above) to minimize merge
// conflicts with Thread 2.
export {
  UI_DENSITY_PRACTICES,
  UI_THEME_PRACTICES,
  DENSITY_SCALE,
  DEFAULT_DENSITY_LEVEL,
  THEME_TOKENS,
  DEFAULT_THEME_MODE,
  THEMEABLE_ATTRS,
  THEMEABLE_ATTR_SET,
} from './ui-practices.js';
export type { DensityLevel, ThemeMode, ThemeTokenColors } from './ui-practices.js';
export type { DensityFact, ThemeFact } from './ui-resolve.js';
export {
  parseHexColor,
  relativeLuminance,
  contrastRatio,
  contrastRatioFromLuminance,
  meetsContrast,
  WCAG_AA_NORMAL,
  WCAG_AA_LARGE,
  WCAG_AAA_NORMAL,
} from './ui-contrast.js';
export type { Rgb } from './ui-contrast.js';

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
