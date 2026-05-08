/**
 * Radix MCP Dev Server — COMPLETE control over pares-radix via MCP.
 *
 * ⚠️  DEV BUILDS ONLY — gated by RADIX_DEV=1 environment variable.
 *
 * This server exposes EVERYTHING:
 *
 * PluresDB:
 *   db.get, db.put, db.delete, db.keys, db.subscribe
 *
 * Canvas:
 *   canvas.create, canvas.setTree, canvas.addNode, canvas.removeNode,
 *   canvas.setData, canvas.addRule, canvas.addProcedure,
 *   canvas.export, canvas.import, canvas.validate, canvas.catalog,
 *   canvas.load, canvas.list
 *
 * Plugins:
 *   plugin.list, plugin.activate, plugin.deactivate, plugin.info
 *
 * Praxis:
 *   praxis.evaluate, praxis.addRule, praxis.listRules, praxis.addConstraint
 *
 * App State:
 *   app.navigate, app.snapshot, app.theme, app.settings.get, app.settings.set
 *
 * Chronos:
 *   chronos.timeline, chronos.replay, chronos.setLevel
 *
 * Designed to be consumed by ANY MCP client — OpenClaw, Claude Desktop,
 * Cursor, or any other AI that speaks MCP.
 *
 * Protocol: JSON-RPC 2.0 over stdio (standard MCP transport).
 */

import {
  createCanvas,
  exportCanvas,
  importCanvas,
  validateCanvas,
  generateCatalog,
  toolCanvasCreate,
  toolCanvasSetTree,
  toolCanvasAddNode,
  toolCanvasRemoveNode,
  toolCanvasSetData,
  toolCanvasAddRule,
  toolCanvasAddProcedure,
  toolCanvasExport,
  toolCanvasImport,
  toolCanvasValidate,
} from '@plures/canvas-runtime';
import type { CanvasDocument, CanvasNode, CanvasRule, CanvasProcedure } from '@plures/canvas-runtime';

// ── Dev Gate ──────────────────────────────────────────────────────────────────

if (!process.env.RADIX_DEV) {
  console.error('⛔ radix-mcp-server is DEV ONLY. Set RADIX_DEV=1 to enable.');
  process.exit(1);
}

// ── In-Memory PluresDB (for standalone testing) ───────────────────────────────

const db = new Map<string, unknown>();
const subscribers = new Map<string, Set<(value: unknown) => void>>();

function dbGet(key: string): unknown {
  return db.get(key);
}

function dbPut(key: string, value: unknown): void {
  db.set(key, value);
  const subs = subscribers.get(key);
  if (subs) {
    for (const cb of subs) {
      try { cb(value); } catch { /* */ }
    }
  }
}

function dbDelete(key: string): void {
  db.delete(key);
  const subs = subscribers.get(key);
  if (subs) {
    for (const cb of subs) {
      try { cb(undefined); } catch { /* */ }
    }
  }
}

function dbKeys(prefix: string = ''): string[] {
  return [...db.keys()].filter((k) => k.startsWith(prefix));
}

// ── Canvas State ──────────────────────────────────────────────────────────────

let activeCanvas: CanvasDocument | null = null;
const savedCanvases = new Map<string, CanvasDocument>();

// ── Tool Registry ─────────────────────────────────────────────────────────────

interface ToolDef {
  name: string;
  description: string;
  inputSchema: object;
  handler: (params: Record<string, unknown>) => unknown;
}

const tools: ToolDef[] = [
  // ── PluresDB ────────────────────────────────────────────────────────────
  {
    name: 'db.get',
    description: 'Read a value from PluresDB by key',
    inputSchema: { type: 'object', properties: { key: { type: 'string' } }, required: ['key'] },
    handler: ({ key }) => ({ value: dbGet(key as string) }),
  },
  {
    name: 'db.put',
    description: 'Write a value to PluresDB',
    inputSchema: { type: 'object', properties: { key: { type: 'string' }, value: {} }, required: ['key', 'value'] },
    handler: ({ key, value }) => { dbPut(key as string, value); return { ok: true }; },
  },
  {
    name: 'db.delete',
    description: 'Delete a key from PluresDB',
    inputSchema: { type: 'object', properties: { key: { type: 'string' } }, required: ['key'] },
    handler: ({ key }) => { dbDelete(key as string); return { ok: true }; },
  },
  {
    name: 'db.keys',
    description: 'List all keys with a given prefix',
    inputSchema: { type: 'object', properties: { prefix: { type: 'string' } } },
    handler: ({ prefix }) => ({ keys: dbKeys((prefix as string) ?? '') }),
  },
  {
    name: 'db.dump',
    description: 'Dump all PluresDB contents (key-value pairs)',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      const entries: Record<string, unknown> = {};
      for (const [k, v] of db) entries[k] = v;
      return entries;
    },
  },

  // ── Canvas ──────────────────────────────────────────────────────────────
  {
    name: 'canvas.create',
    description: 'Create a new canvas app. Returns the canvas document.',
    inputSchema: {
      type: 'object',
      properties: {
        title: { type: 'string' },
        description: { type: 'string' },
      },
      required: ['title'],
    },
    handler: ({ title, description }) => {
      activeCanvas = toolCanvasCreate({ title: title as string, description: description as string });
      // Seed data into DB
      for (const [k, v] of Object.entries(activeCanvas.data)) {
        dbPut(`canvas:${k}`, v);
      }
      dbPut('canvas:_active', activeCanvas);
      return activeCanvas;
    },
  },
  {
    name: 'canvas.addNode',
    description: 'Add a component to the canvas tree under a parent node',
    inputSchema: {
      type: 'object',
      properties: {
        parentId: { type: 'string', description: 'ID of the parent node' },
        node: { type: 'object', description: 'CanvasNode: { id, type, props?, bindings?, children?, visible? }' },
      },
      required: ['parentId', 'node'],
    },
    handler: ({ parentId, node }) => {
      if (!activeCanvas) return { error: 'No active canvas. Call canvas.create first.' };
      activeCanvas = toolCanvasAddNode(activeCanvas, parentId as string, node as CanvasNode);
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, tree: activeCanvas.tree };
    },
  },
  {
    name: 'canvas.removeNode',
    description: 'Remove a node from the canvas tree by ID',
    inputSchema: { type: 'object', properties: { nodeId: { type: 'string' } }, required: ['nodeId'] },
    handler: ({ nodeId }) => {
      if (!activeCanvas) return { error: 'No active canvas' };
      activeCanvas = toolCanvasRemoveNode(activeCanvas, nodeId as string);
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, tree: activeCanvas.tree };
    },
  },
  {
    name: 'canvas.setData',
    description: 'Set data values in the canvas (seeds PluresDB)',
    inputSchema: { type: 'object', properties: { data: { type: 'object' } }, required: ['data'] },
    handler: ({ data }) => {
      if (!activeCanvas) return { error: 'No active canvas' };
      activeCanvas = toolCanvasSetData(activeCanvas, data as Record<string, unknown>);
      for (const [k, v] of Object.entries(data as Record<string, unknown>)) {
        dbPut(`canvas:${k}`, v);
      }
      dbPut('canvas:_active', activeCanvas);
      return { ok: true };
    },
  },
  {
    name: 'canvas.addRule',
    description: 'Add a Praxis validation rule to the canvas',
    inputSchema: { type: 'object', properties: { rule: { type: 'object' } }, required: ['rule'] },
    handler: ({ rule }) => {
      if (!activeCanvas) return { error: 'No active canvas' };
      activeCanvas = toolCanvasAddRule(activeCanvas, rule as CanvasRule);
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, rules: activeCanvas.rules };
    },
  },
  {
    name: 'canvas.addProcedure',
    description: 'Add a behavior procedure to the canvas',
    inputSchema: { type: 'object', properties: { procedure: { type: 'object' } }, required: ['procedure'] },
    handler: ({ procedure }) => {
      if (!activeCanvas) return { error: 'No active canvas' };
      activeCanvas = toolCanvasAddProcedure(activeCanvas, procedure as CanvasProcedure);
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, procedures: activeCanvas.procedures };
    },
  },
  {
    name: 'canvas.setTree',
    description: 'Replace the entire component tree',
    inputSchema: { type: 'object', properties: { tree: { type: 'object' } }, required: ['tree'] },
    handler: ({ tree }) => {
      if (!activeCanvas) return { error: 'No active canvas' };
      activeCanvas = toolCanvasSetTree(activeCanvas, tree as CanvasNode);
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, tree: activeCanvas.tree };
    },
  },
  {
    name: 'canvas.get',
    description: 'Get the current active canvas document',
    inputSchema: { type: 'object', properties: {} },
    handler: () => activeCanvas ?? { error: 'No active canvas' },
  },
  {
    name: 'canvas.export',
    description: 'Export the active canvas as a .canvas JSON string',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      if (!activeCanvas) return { error: 'No active canvas' };
      return { json: toolCanvasExport(activeCanvas) };
    },
  },
  {
    name: 'canvas.import',
    description: 'Import a .canvas file from JSON string',
    inputSchema: { type: 'object', properties: { json: { type: 'string' } }, required: ['json'] },
    handler: ({ json }) => {
      activeCanvas = toolCanvasImport(json as string);
      for (const [k, v] of Object.entries(activeCanvas.data)) {
        dbPut(`canvas:${k}`, v);
      }
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, title: activeCanvas.meta.title };
    },
  },
  {
    name: 'canvas.validate',
    description: 'Validate the active canvas and return issues',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      if (!activeCanvas) return { error: 'No active canvas' };
      return { issues: toolCanvasValidate(activeCanvas) };
    },
  },
  {
    name: 'canvas.catalog',
    description: 'Get the full component catalog — what components are available and how to use them',
    inputSchema: { type: 'object', properties: {} },
    handler: () => ({ catalog: '(component registry not initialized in standalone mode — run inside pares-radix for full catalog)' }),
  },
  {
    name: 'canvas.save',
    description: 'Save the active canvas to the saved canvases list',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      if (!activeCanvas) return { error: 'No active canvas' };
      savedCanvases.set(activeCanvas.meta.id, activeCanvas);
      dbPut(`canvas:_saved:${activeCanvas.meta.id}`, {
        title: activeCanvas.meta.title,
        modifiedAt: activeCanvas.meta.modifiedAt,
      });
      return { ok: true, id: activeCanvas.meta.id };
    },
  },
  {
    name: 'canvas.list',
    description: 'List all saved canvases',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      const list = [...savedCanvases.entries()].map(([id, c]) => ({
        id, title: c.meta.title, modifiedAt: c.meta.modifiedAt,
      }));
      return { canvases: list };
    },
  },
  {
    name: 'canvas.load',
    description: 'Load a saved canvas by ID',
    inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] },
    handler: ({ id }) => {
      const canvas = savedCanvases.get(id as string);
      if (!canvas) return { error: `Canvas ${id} not found` };
      activeCanvas = canvas;
      for (const [k, v] of Object.entries(canvas.data)) {
        dbPut(`canvas:${k}`, v);
      }
      dbPut('canvas:_active', activeCanvas);
      return { ok: true, title: canvas.meta.title };
    },
  },

  // ── App Control ─────────────────────────────────────────────────────────
  {
    name: 'app.snapshot',
    description: 'Snapshot the entire app state (all PluresDB keys)',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      const state: Record<string, unknown> = {};
      for (const [k, v] of db) state[k] = v;
      return {
        dbSize: db.size,
        activeCanvas: activeCanvas?.meta?.title ?? null,
        savedCount: savedCanvases.size,
        state,
      };
    },
  },
  {
    name: 'app.reset',
    description: 'Reset all app state — nuclear option',
    inputSchema: { type: 'object', properties: { confirm: { type: 'boolean' } }, required: ['confirm'] },
    handler: ({ confirm }) => {
      if (!confirm) return { error: 'Pass confirm: true to reset all state' };
      db.clear();
      activeCanvas = null;
      savedCanvases.clear();
      return { ok: true, message: 'All state cleared' };
    },
  },
];

// ── MCP JSON-RPC Server (stdio) ───────────────────────────────────────────────

const toolMap = new Map(tools.map((t) => [t.name, t]));

function handleRequest(req: { method: string; params?: Record<string, unknown>; id?: number | string }): object {
  const { method, params, id } = req;

  switch (method) {
    case 'initialize':
      return {
        jsonrpc: '2.0',
        id,
        result: {
          protocolVersion: '2024-11-05',
          serverInfo: { name: 'radix-mcp-dev', version: '0.1.0' },
          capabilities: { tools: { listChanged: false } },
        },
      };

    case 'notifications/initialized':
      return { jsonrpc: '2.0', id }; // ack

    case 'tools/list':
      return {
        jsonrpc: '2.0',
        id,
        result: {
          tools: tools.map((t) => ({
            name: t.name,
            description: t.description,
            inputSchema: t.inputSchema,
          })),
        },
      };

    case 'tools/call': {
      const toolName = (params as any)?.name as string;
      const toolArgs = (params as any)?.arguments ?? {};
      const tool = toolMap.get(toolName);
      if (!tool) {
        return { jsonrpc: '2.0', id, error: { code: -32601, message: `Unknown tool: ${toolName}` } };
      }
      try {
        const result = tool.handler(toolArgs);
        return {
          jsonrpc: '2.0',
          id,
          result: {
            content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
          },
        };
      } catch (err: any) {
        return {
          jsonrpc: '2.0',
          id,
          result: {
            content: [{ type: 'text', text: `Error: ${err.message}` }],
            isError: true,
          },
        };
      }
    }

    default:
      return { jsonrpc: '2.0', id, error: { code: -32601, message: `Unknown method: ${method}` } };
  }
}

// ── stdio transport ───────────────────────────────────────────────────────────

let buffer = '';

process.stdin.setEncoding('utf-8');
process.stdin.on('data', (chunk: string) => {
  buffer += chunk;

  // Process complete JSON-RPC messages (newline-delimited)
  let newlineIndex: number;
  while ((newlineIndex = buffer.indexOf('\n')) !== -1) {
    const line = buffer.slice(0, newlineIndex).trim();
    buffer = buffer.slice(newlineIndex + 1);

    if (!line) continue;

    try {
      const req = JSON.parse(line);
      const response = handleRequest(req);
      if (req.id !== undefined) {
        process.stdout.write(JSON.stringify(response) + '\n');
      }
    } catch (err: any) {
      process.stdout.write(JSON.stringify({
        jsonrpc: '2.0',
        id: null,
        error: { code: -32700, message: `Parse error: ${err.message}` },
      }) + '\n');
    }
  }
});

process.stderr.write('🔧 radix-mcp-dev server started (DEV MODE)\n');
process.stderr.write(`📦 ${tools.length} tools available\n`);
process.stderr.write('⚠️  This server has FULL ACCESS to app state\n');
