/**
 * Canvas Document Format — the shareable, replayable app definition.
 *
 * A .canvas file is a self-contained app that can be:
 * - Created by AI in real-time (writing to PluresDB)
 * - Exported as a portable file
 * - Shared (send to someone, they open it, app works)
 * - Versioned (git-friendly JSON)
 * - Replayed (Chronos timeline can be embedded)
 * - Forked (modify someone else's canvas)
 * - Published (marketplace, like Jupyter notebooks but interactive)
 *
 * Think: Jupyter notebook × Figma × VS Code extension × live app
 *
 * The format is designed so that:
 * 1. Every field maps directly to a PluresDB key
 * 2. The AI can write any part of it incrementally
 * 3. Import = seed PluresDB with the document's data
 * 4. Export = snapshot all canvas: prefixed keys from PluresDB
 */

// ── Core Types ────────────────────────────────────────────────────────────────

import { validateUi, formatUiViolations } from './ui-constraints.js';

/**
 * A node in the component tree.
 * This is what gets rendered — recursively, reactively.
 */
export interface CanvasNode {
  /** Unique node ID within the canvas */
  id: string;
  /** Component type (must be registered in ComponentRegistry) */
  type: string;
  /** Static props passed to the component */
  props?: Record<string, unknown>;
  /** Dynamic bindings — prop name → PluresDB key path */
  bindings?: Record<string, CanvasBinding>;
  /** Child nodes (for layout components) */
  children?: CanvasNode[];
  /**
   * Responsive intent. Maps an attribute name (see RESPONSIVE_ATTRS in
   * ui-schema.ts) to a breakpoint-keyed value map, e.g.
   *   { direction: { base: 'column', md: 'row' }, gap: { base: '8px', md: '16px' } }.
   * The reactive resolver collapses these against the active `ui:viewport`
   * breakpoint and writes the concrete value into `props[attr]` on the DERIVED
   * tree (canvas:tree:resolved). Authored intent here stays pristine.
   */
  responsive?: Record<string, Record<string, unknown>>;
  /** Visibility condition — PluresDB key that must be truthy */
  visible?: string | CanvasCondition;
  /** CSS class override (design-dojo utility classes only) */
  class?: string;
}

/**
 * A binding connects a component prop to a PluresDB key.
 * Changes to the key reactively update the prop (via Unum).
 * Changes from user interaction write back to the key.
 */
export interface CanvasBinding {
  /** PluresDB key path to bind to */
  key: string;
  /** Transform applied when reading from DB → prop */
  readTransform?: string;
  /** Transform applied when writing from prop → DB */
  writeTransform?: string;
  /** Two-way binding (default: true for inputs, false for displays) */
  twoWay?: boolean;
}

/**
 * A condition for visibility/gates.
 */
export interface CanvasCondition {
  /** PluresDB key to evaluate */
  key: string;
  /** Operator */
  op: 'eq' | 'neq' | 'gt' | 'lt' | 'gte' | 'lte' | 'truthy' | 'falsy' | 'contains';
  /** Value to compare against */
  value?: unknown;
}

/**
 * A Praxis rule stored as data in the canvas.
 * These govern validation, gates, and constraints.
 */
export interface CanvasRule {
  /** Unique rule ID */
  id: string;
  /** Human-readable description */
  description: string;
  /** When this rule applies (PluresDB key pattern or event) */
  when: string | CanvasCondition;
  /** What must be true (constraint) */
  require?: string | CanvasCondition;
  /** What to do when violated */
  action: 'block' | 'warn' | 'gate' | 'emit';
  /** Message shown on violation */
  message?: string;
  /** Severity */
  severity: 'error' | 'warning' | 'info';
}

/**
 * A procedure — what happens in response to user actions or data changes.
 * These are the "event handlers" but stored as data, not code.
 */
export interface CanvasProcedure {
  /** Unique procedure ID */
  id: string;
  /** Human-readable description */
  description: string;
  /** Trigger: what causes this to fire */
  trigger: CanvasTrigger;
  /** Steps to execute (in order) */
  steps: CanvasStep[];
}

/**
 * What causes a procedure to fire.
 */
export interface CanvasTrigger {
  /** Trigger type */
  kind: 'on_click' | 'on_change' | 'on_submit' | 'on_load' | 'on_interval' | 'on_event';
  /** Target node ID (for click/change/submit) */
  nodeId?: string;
  /** PluresDB key (for on_change) */
  key?: string;
  /** Event name (for on_event) */
  event?: string;
  /** Interval in ms (for on_interval) */
  intervalMs?: number;
}

/**
 * A single step in a procedure.
 */
export interface CanvasStep {
  /** Step type */
  kind: 'set' | 'toggle' | 'increment' | 'append' | 'remove' | 'navigate' | 'emit' | 'fetch' | 'transform' | 'condition';
  /** Target PluresDB key */
  key?: string;
  /** Value to set (can reference other keys with ${key} syntax) */
  value?: unknown;
  /** For condition steps: if true branch */
  then?: CanvasStep[];
  /** For condition steps: if false branch */
  else?: CanvasStep[];
  /** Condition to evaluate */
  condition?: CanvasCondition;
  /** For fetch: URL template */
  url?: string;
  /** For fetch: store result in this key */
  resultKey?: string;
  /** For transform: expression */
  expression?: string;
}

// ── Canvas Document ───────────────────────────────────────────────────────────

/**
 * The complete canvas document — everything needed to run an app.
 */
export interface CanvasDocument {
  /** Document format version */
  version: '1.0.0';
  /** Canvas metadata */
  meta: CanvasMeta;
  /** Component tree — what gets rendered */
  tree: CanvasNode;
  /** Initial data state — seeds PluresDB on import */
  data: Record<string, unknown>;
  /** Praxis rules — validation and constraints */
  rules: CanvasRule[];
  /** Procedures — behavior and interactions */
  procedures: CanvasProcedure[];
  /** Data schema — describes what keys the app uses */
  schema: CanvasSchema[];
}

export interface CanvasMeta {
  /** Canvas title */
  title: string;
  /** Description */
  description: string;
  /** Author */
  author: string;
  /** Created timestamp */
  createdAt: string;
  /** Last modified timestamp */
  modifiedAt: string;
  /** Tags for discovery */
  tags: string[];
  /** Canvas ID (UUID) */
  id: string;
  /** Thumbnail (base64 or URL) */
  thumbnail?: string;
  /** License */
  license?: string;
}

export interface CanvasSchema {
  /** PluresDB key path */
  key: string;
  /** TypeScript-like type */
  type: string;
  /** Description */
  description: string;
  /** Default value */
  default?: unknown;
}

// ── Export Format ─────────────────────────────────────────────────────────────

/**
 * The exported .canvas file format.
 * Includes the document plus optional Chronos timeline for replay.
 */
export interface CanvasExport {
  /** Format identifier */
  format: 'plures-canvas';
  /** Format version */
  formatVersion: '1.0.0';
  /** The canvas document */
  document: CanvasDocument;
  /** Optional: Chronos timeline for full replay */
  timeline?: CanvasTimelineEntry[];
  /** Optional: embedded assets (images, etc.) */
  assets?: Record<string, string>;
}

export interface CanvasTimelineEntry {
  /** Timestamp */
  ts: number;
  /** Actor who made the change */
  actor: { kind: string; id: string };
  /** PluresDB key that changed */
  key: string;
  /** Previous value */
  before: unknown;
  /** New value */
  after: unknown;
}

// ── Factory Functions ─────────────────────────────────────────────────────────

/**
 * Create a new empty canvas document.
 */
export function createCanvas(meta: Partial<CanvasMeta> = {}): CanvasDocument {
  return {
    version: '1.0.0',
    meta: {
      title: meta.title ?? 'Untitled Canvas',
      description: meta.description ?? '',
      author: meta.author ?? 'unknown',
      createdAt: meta.createdAt ?? new Date().toISOString(),
      modifiedAt: meta.modifiedAt ?? new Date().toISOString(),
      tags: meta.tags ?? [],
      id: meta.id ?? crypto.randomUUID(),
      thumbnail: meta.thumbnail,
      license: meta.license,
    },
    tree: { id: 'root', type: 'PluginContentArea', children: [] },
    data: {},
    rules: [],
    procedures: [],
    schema: [],
  };
}

/**
 * Export a canvas to the portable .canvas format.
 * Includes optional Chronos timeline for full replay.
 */
export function exportCanvas(
  document: CanvasDocument,
  options: { timeline?: CanvasTimelineEntry[]; assets?: Record<string, string> } = {},
): CanvasExport {
  return {
    format: 'plures-canvas',
    formatVersion: '1.0.0',
    document: {
      ...document,
      meta: { ...document.meta, modifiedAt: new Date().toISOString() },
    },
    timeline: options.timeline,
    assets: options.assets,
  };
}

/**
 * Import a .canvas file — validates and returns the document.
 * The caller is responsible for seeding PluresDB with document.data.
 */
export function importCanvas(raw: unknown): CanvasDocument {
  const obj = raw as CanvasExport;
  if (obj?.format !== 'plures-canvas') {
    throw new Error('Invalid canvas file: missing format identifier');
  }
  if (!obj.document?.version) {
    throw new Error('Invalid canvas file: missing document version');
  }
  return obj.document;
}

/**
 * Validate a canvas document structure.
 * Returns an array of issues (empty = valid).
 */
export function validateCanvas(doc: CanvasDocument): string[] {
  const issues: string[] = [];

  if (!doc.version) issues.push('Missing version');
  if (!doc.meta?.id) issues.push('Missing meta.id');
  if (!doc.meta?.title) issues.push('Missing meta.title');
  if (!doc.tree?.id) issues.push('Missing tree root');
  if (!doc.tree?.type) issues.push('Missing tree root type');

  // Validate all nodes reference registered components
  function walkTree(node: CanvasNode, path: string) {
    if (!node.type) issues.push(`Node at ${path} missing type`);
    if (!node.id) issues.push(`Node at ${path} missing id`);
    if (node.children) {
      for (let i = 0; i < node.children.length; i++) {
        walkTree(node.children[i], `${path}.children[${i}]`);
      }
    }
  }
  walkTree(doc.tree, 'tree');

  // Validate procedures reference valid trigger targets
  for (const proc of doc.procedures) {
    if (!proc.id) issues.push(`Procedure missing id`);
    if (!proc.trigger?.kind) issues.push(`Procedure ${proc.id} missing trigger.kind`);
    if (!proc.steps || proc.steps.length === 0) {
      issues.push(`Procedure ${proc.id} has no steps`);
    }
  }

  // ── UI best-practice enforcement ──────────────────────────────────────────
  // Run the encoded web-UI best practices (accessibility, hierarchy, feedback,
  // destructive-action guarding) against the tree. These come from
  // praxis/ui/ui-best-practices.px, mirrored in ui-constraints.ts. Good UI is
  // enforced here automatically — authors don't have to remember the rules.
  //
  // NOTE (contrast): validateUi also hosts the WCAG-AA contrast constraint
  // (ui_text_contrast_aa), but it needs the ACTIVE theme mode to know the
  // surface to contrast against. A CanvasDocument carries no theme-mode field,
  // so we honestly pass none here: the contrast constraint stays INERT in this
  // string[] path (contrastChecked=false). It activates when a caller that knows
  // the mode invokes validateUi(root, { themeMode }) directly. This is the
  // honest "surface unknown" state, not a silent pass — do NOT fabricate a mode.
  if (doc.tree) {
    const ui = validateUi(doc.tree);
    for (const issue of formatUiViolations(ui.violations)) issues.push(issue);
  }

  return issues;
}
