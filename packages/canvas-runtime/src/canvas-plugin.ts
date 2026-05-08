/**
 * AI Canvas Plugin — the runtime app creation interface.
 *
 * This plugin:
 * 1. Registers as a pares-radix plugin with its own view
 * 2. Provides the CanvasRenderer as its view component
 * 3. Exposes MCP tools for AI to create/modify canvas documents
 * 4. Manages canvas lifecycle (create, load, save, export, share)
 *
 * The AI interacts with this plugin by writing to PluresDB keys
 * under the "canvas:" prefix. The renderer picks up changes
 * instantly via reactive subscriptions.
 *
 * MCP tools exposed:
 * - canvas.create      — create a new empty canvas
 * - canvas.setTree     — set/update the component tree
 * - canvas.addNode     — add a node to the tree
 * - canvas.removeNode  — remove a node from the tree
 * - canvas.setData     — set initial data values
 * - canvas.addRule     — add a Praxis rule
 * - canvas.addProcedure — add a procedure
 * - canvas.export      — export as .canvas file
 * - canvas.import      — import a .canvas file
 * - canvas.catalog     — get the component catalog (for AI context)
 */

import type { CanvasDocument, CanvasNode, CanvasRule, CanvasProcedure } from './format.js';
import { createCanvas, exportCanvas, importCanvas, validateCanvas } from './format.js';
import { generateCatalog, registerDesignDojo } from './registry.js';

export interface CanvasPluginState {
  /** Currently active canvas document */
  activeCanvas: CanvasDocument | null;
  /** List of saved canvases (metadata only) */
  savedCanvases: Array<{ id: string; title: string; modifiedAt: string }>;
  /** Whether the plugin is initialized */
  initialized: boolean;
}

/**
 * Initialize the AI Canvas plugin.
 * Call once at app startup.
 */
export async function initCanvasPlugin(): Promise<CanvasPluginState> {
  // Register all design-dojo components in the runtime registry
  await registerDesignDojo();

  return {
    activeCanvas: null,
    savedCanvases: [],
    initialized: true,
  };
}

// ── MCP Tool Implementations ──────────────────────────────────────────────────

/**
 * Create a new canvas. The AI calls this to start building an app.
 */
export function toolCanvasCreate(params: {
  title: string;
  description?: string;
  author?: string;
}): CanvasDocument {
  return createCanvas({
    title: params.title,
    description: params.description ?? '',
    author: params.author ?? 'ai:canvas',
  });
}

/**
 * Set or replace the component tree of the active canvas.
 */
export function toolCanvasSetTree(
  canvas: CanvasDocument,
  tree: CanvasNode,
): CanvasDocument {
  return { ...canvas, tree, meta: { ...canvas.meta, modifiedAt: new Date().toISOString() } };
}

/**
 * Add a node to the canvas tree at a specific parent path.
 */
export function toolCanvasAddNode(
  canvas: CanvasDocument,
  parentId: string,
  node: CanvasNode,
): CanvasDocument {
  const newTree = addNodeToTree(canvas.tree, parentId, node);
  return { ...canvas, tree: newTree, meta: { ...canvas.meta, modifiedAt: new Date().toISOString() } };
}

/**
 * Remove a node from the canvas tree by ID.
 */
export function toolCanvasRemoveNode(
  canvas: CanvasDocument,
  nodeId: string,
): CanvasDocument {
  const newTree = removeNodeFromTree(canvas.tree, nodeId);
  return { ...canvas, tree: newTree, meta: { ...canvas.meta, modifiedAt: new Date().toISOString() } };
}

/**
 * Set data values in the canvas (seeds PluresDB on load).
 */
export function toolCanvasSetData(
  canvas: CanvasDocument,
  data: Record<string, unknown>,
): CanvasDocument {
  return {
    ...canvas,
    data: { ...canvas.data, ...data },
    meta: { ...canvas.meta, modifiedAt: new Date().toISOString() },
  };
}

/**
 * Add a Praxis rule to the canvas.
 */
export function toolCanvasAddRule(
  canvas: CanvasDocument,
  rule: CanvasRule,
): CanvasDocument {
  return {
    ...canvas,
    rules: [...canvas.rules, rule],
    meta: { ...canvas.meta, modifiedAt: new Date().toISOString() },
  };
}

/**
 * Add a procedure to the canvas.
 */
export function toolCanvasAddProcedure(
  canvas: CanvasDocument,
  procedure: CanvasProcedure,
): CanvasDocument {
  return {
    ...canvas,
    procedures: [...canvas.procedures, procedure],
    meta: { ...canvas.meta, modifiedAt: new Date().toISOString() },
  };
}

/**
 * Export the canvas as a .canvas file (JSON string).
 */
export function toolCanvasExport(canvas: CanvasDocument): string {
  const exported = exportCanvas(canvas);
  return JSON.stringify(exported, null, 2);
}

/**
 * Import a .canvas file from JSON string.
 */
export function toolCanvasImport(json: string): CanvasDocument {
  const raw = JSON.parse(json);
  return importCanvas(raw);
}

/**
 * Get the component catalog for AI context.
 * The AI uses this to know what components exist and how to use them.
 */
export function toolCanvasCatalog(): string {
  return generateCatalog();
}

/**
 * Validate a canvas document and return issues.
 */
export function toolCanvasValidate(canvas: CanvasDocument): string[] {
  return validateCanvas(canvas);
}

// ── Tree Manipulation Helpers ─────────────────────────────────────────────────

function addNodeToTree(tree: CanvasNode, parentId: string, newNode: CanvasNode): CanvasNode {
  if (tree.id === parentId) {
    return {
      ...tree,
      children: [...(tree.children || []), newNode],
    };
  }
  if (tree.children) {
    return {
      ...tree,
      children: tree.children.map((child) => addNodeToTree(child, parentId, newNode)),
    };
  }
  return tree;
}

function removeNodeFromTree(tree: CanvasNode, nodeId: string): CanvasNode {
  if (tree.id === nodeId) {
    // Can't remove root — return empty
    return { ...tree, children: [] };
  }
  if (tree.children) {
    return {
      ...tree,
      children: tree.children
        .filter((child) => child.id !== nodeId)
        .map((child) => removeNodeFromTree(child, nodeId)),
    };
  }
  return tree;
}
