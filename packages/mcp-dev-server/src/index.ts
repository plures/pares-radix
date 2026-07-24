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
 * Task Handoff (custody-transfer protocol):
 *   task.handoff.prepare, task.handoff.verify, task.handoff.accept, task.handoff.claim
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

// ── Persistent PluresDB (file-backed JSON store) ─────────────────────────────

import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { homedir } from 'node:os';

// Determine storage path: RADIX_DB_PATH env or default ~/.radix/pluresdb.json
const RADIX_DB_PATH = process.env.RADIX_DB_PATH
  ?? join(homedir(), '.radix', 'pluresdb.json');

function loadDbFromDisk(): Map<string, unknown> {
  try {
    if (existsSync(RADIX_DB_PATH)) {
      const raw = readFileSync(RADIX_DB_PATH, 'utf-8');
      const parsed = JSON.parse(raw);
      if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
        return new Map(Object.entries(parsed));
      }
    }
  } catch (err: any) {
    process.stderr.write(`⚠️  Failed to load DB from ${RADIX_DB_PATH}: ${err.message}\n`);
  }
  return new Map();
}

let persistTimer: ReturnType<typeof setTimeout> | null = null;
const PERSIST_DEBOUNCE_MS = 500;

function schedulePersist(): void {
  if (persistTimer) return;
  persistTimer = setTimeout(() => {
    persistTimer = null;
    persistDbToDisk();
  }, PERSIST_DEBOUNCE_MS);
}

function persistDbToDisk(): void {
  try {
    const dir = dirname(RADIX_DB_PATH);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    const obj: Record<string, unknown> = {};
    for (const [k, v] of db) obj[k] = v;
    writeFileSync(RADIX_DB_PATH, JSON.stringify(obj, null, 2), 'utf-8');
  } catch (err: any) {
    process.stderr.write(`⚠️  Failed to persist DB to ${RADIX_DB_PATH}: ${err.message}\n`);
  }
}

const db = loadDbFromDisk();
const subscribers = new Map<string, Set<(value: unknown) => void>>();

function dbGet(key: string): unknown {
  return db.get(key);
}

function dbPut(key: string, value: unknown): void {
  // Fix double-serialization: if value is a JSON string, parse it before storing.
  // This handles MCP clients that pre-serialize their values.
  let resolved = value;
  if (typeof value === 'string') {
    try {
      const parsed = JSON.parse(value);
      // Only use parsed value if it's actually structured (object/array)
      if (typeof parsed === 'object' && parsed !== null) {
        resolved = parsed;
      }
    } catch { /* not JSON, store as-is */ }
  }
  db.set(key, resolved);
  schedulePersist();
  const subs = subscribers.get(key);
  if (subs) {
    for (const cb of subs) {
      try { cb(resolved); } catch { /* */ }
    }
  }
}

function dbDelete(key: string): void {
  db.delete(key);
  schedulePersist();
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
        dbPath: RADIX_DB_PATH,
        persistent: true,
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
      schedulePersist();
      activeCanvas = null;
      savedCanvases.clear();
      return { ok: true, message: 'All state cleared' };
    },
  },

  // ── Praxis ────────────────────────────────────────────────────────────────

  {
    name: 'praxis.evaluate',
    description: 'Evaluate Praxis constraints against a given context/state. Returns violations.',
    inputSchema: {
      type: 'object',
      properties: {
        context: { type: 'object', description: 'State to evaluate constraints against' },
        phase: { type: 'string', description: 'Optional phase filter (e.g. pre-commit, pre-push)' },
      },
      required: ['context'],
    },
    handler: ({ context, phase }) => {
      const constraints = dbKeys('px:constraint/').map((k) => db.get(k) as any).filter(Boolean);
      const violations: Array<{ constraint: string; severity: string; message: string }> = [];

      for (const c of constraints) {
        // Skip if phase filter is set and constraint doesn't match
        if (phase && c.phases?.length > 0 && !c.phases.includes(phase)) continue;

        // Evaluate `when` guard — if set, constraint only applies when condition holds
        // Wrap context so BOTH `context.foo` and bare `foo` resolve: constraints in the
        // ledger are written against top-level domain keys (config/trade/policy/security/
        // devex/ops) as well as `context.*`. Exposing only `{ context }` made every bare-key
        // constraint resolve to undefined → falsy → false-positive violation (the faithfulness
        // bug). Spreading the context's own keys makes the evaluator faithful to the require expr.
        const evalScope =
          context && typeof context === 'object'
            ? ({ context, ...(context as Record<string, unknown>) } as Record<string, unknown>)
            : ({ context } as Record<string, unknown>);

        if (c.when) {
          const whenResult = simpleEval(c.when, evalScope);
          if (!whenResult) continue;
        }

        // Evaluate `require` — if set, this must be true or it's a violation
        if (c.require) {
          const requireResult = simpleEval(c.require, evalScope);
          if (!requireResult) {
            violations.push({
              constraint: c.name,
              severity: c.severity || 'error',
              message: c.message || `Constraint '${c.name}' violated: require(${c.require}) failed`,
            });
          }
        }
      }

      return { evaluated: constraints.length, violations, passed: violations.length === 0 };
    },
  },
  {
    name: 'praxis.addRule',
    description: 'Add a Praxis rule to the database',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string' },
        priority: { type: 'number' },
        conditions: { type: 'array', items: { type: 'string' } },
        actions: { type: 'array' },
      },
      required: ['name'],
    },
    handler: ({ name, priority, conditions, actions }) => {
      const record = { type: 'rule', name, priority: priority ?? 50, conditions: conditions ?? [], actions: actions ?? [] };
      dbPut(`px:rule/${name}`, record);
      return { ok: true, key: `px:rule/${name}` };
    },
  },
  {
    name: 'praxis.addConstraint',
    description: 'Add a Praxis constraint',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string' },
        when: { type: 'string' },
        require: { type: 'string' },
        severity: { type: 'string' },
        message: { type: 'string' },
        phases: { type: 'array', items: { type: 'string' } },
      },
      required: ['name', 'severity'],
    },
    handler: ({ name, when, require: req, severity, message, phases }) => {
      const record = { type: 'constraint', name, when, require: req, severity, message, phases: phases ?? [] };
      dbPut(`px:constraint/${name}`, record);
      return { ok: true, key: `px:constraint/${name}` };
    },
  },
  {
    name: 'praxis.listRules',
    description: 'List all Praxis rules and constraints',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      const rules = dbKeys('px:rule/').map((k) => ({ key: k, ...(db.get(k) as any) }));
      const constraints = dbKeys('px:constraint/').map((k) => ({ key: k, ...(db.get(k) as any) }));
      return { rules, constraints };
    },
  },

  // ── Chronos ───────────────────────────────────────────────────────────────

  {
    name: 'chronos.timeline',
    description: 'Get the event timeline (last N events)',
    inputSchema: {
      type: 'object',
      properties: {
        limit: { type: 'number', description: 'Max events to return (default 50)' },
        since: { type: 'string', description: 'ISO timestamp — only events after this time' },
      },
    },
    handler: ({ limit, since }) => {
      const allEvents = (db.get('chronos:timeline') as any[]) ?? [];
      let filtered = allEvents;
      if (since) {
        const sinceTime = new Date(since as string).getTime();
        filtered = filtered.filter((e) => new Date(e.timestamp).getTime() > sinceTime);
      }
      const maxLimit = Math.min((limit as number) || 50, 500);
      return { events: filtered.slice(-maxLimit), total: allEvents.length };
    },
  },
  {
    name: 'chronos.record',
    description: 'Record an event to the Chronos timeline',
    inputSchema: {
      type: 'object',
      properties: {
        event: { type: 'string', description: 'Event name/type' },
        data: { type: 'object', description: 'Event payload' },
        level: { type: 'string', enum: ['debug', 'info', 'warn', 'error'] },
      },
      required: ['event'],
    },
    handler: ({ event, data, level }) => {
      const timeline = (db.get('chronos:timeline') as any[]) ?? [];
      const entry = {
        id: `evt_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
        event,
        data: data ?? {},
        level: level ?? 'info',
        timestamp: new Date().toISOString(),
      };
      timeline.push(entry);
      dbPut('chronos:timeline', timeline);
      return { ok: true, id: entry.id };
    },
  },
  {
    name: 'chronos.replay',
    description: 'Replay timeline events through the Praxis engine (dry-run evaluation)',
    inputSchema: {
      type: 'object',
      properties: {
        fromId: { type: 'string', description: 'Start replay from this event id' },
        toId: { type: 'string', description: 'End replay at this event id' },
      },
    },
    handler: ({ fromId, toId }) => {
      const timeline = (db.get('chronos:timeline') as any[]) ?? [];
      let startIdx = fromId ? timeline.findIndex((e) => e.id === fromId) : 0;
      let endIdx = toId ? timeline.findIndex((e) => e.id === toId) + 1 : timeline.length;
      if (startIdx < 0) startIdx = 0;
      if (endIdx <= 0) endIdx = timeline.length;
      const segment = timeline.slice(startIdx, endIdx);
      // In standalone mode, replay just returns the segment for analysis
      return { replayed: segment.length, events: segment };
    },
  },
  {
    name: 'chronos.setLevel',
    description: 'Set the minimum recording level for Chronos',
    inputSchema: {
      type: 'object',
      properties: {
        level: { type: 'string', enum: ['debug', 'info', 'warn', 'error'] },
      },
      required: ['level'],
    },
    handler: ({ level }) => {
      dbPut('chronos:config:level', level);
      return { ok: true, level };
    },
  },

  // ── Plugin Management ─────────────────────────────────────────────────────

  {
    name: 'plugin.list',
    description: 'List all registered plugins and their status',
    inputSchema: { type: 'object', properties: {} },
    handler: () => {
      const plugins = dbKeys('plugin:').map((k) => ({ key: k, ...(db.get(k) as any) }));
      return { plugins };
    },
  },
  {
    name: 'plugin.register',
    description: 'Register a plugin manifest',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string' },
        version: { type: 'string' },
        description: { type: 'string' },
        capabilities: { type: 'array', items: { type: 'string' } },
      },
      required: ['name', 'version'],
    },
    handler: ({ name, version, description, capabilities }) => {
      const record = { name, version, description, capabilities: capabilities ?? [], active: false, registeredAt: new Date().toISOString() };
      dbPut(`plugin:${name}`, record);
      return { ok: true, key: `plugin:${name}` };
    },
  },
  {
    name: 'plugin.activate',
    description: 'Activate a registered plugin',
    inputSchema: { type: 'object', properties: { name: { type: 'string' } }, required: ['name'] },
    handler: ({ name }) => {
      const record = db.get(`plugin:${name}`) as any;
      if (!record) return { error: `Plugin '${name}' not found` };
      record.active = true;
      record.activatedAt = new Date().toISOString();
      dbPut(`plugin:${name}`, record);
      return { ok: true, plugin: record };
    },
  },
  {
    name: 'plugin.deactivate',
    description: 'Deactivate a plugin',
    inputSchema: { type: 'object', properties: { name: { type: 'string' } }, required: ['name'] },
    handler: ({ name }) => {
      const record = db.get(`plugin:${name}`) as any;
      if (!record) return { error: `Plugin '${name}' not found` };
      record.active = false;
      record.deactivatedAt = new Date().toISOString();
      dbPut(`plugin:${name}`, record);
      return { ok: true, plugin: record };
    },
  },
  {
    name: 'plugin.info',
    description: 'Get detailed info about a specific plugin',
    inputSchema: { type: 'object', properties: { name: { type: 'string' } }, required: ['name'] },
    handler: ({ name }) => {
      const record = db.get(`plugin:${name}`) as any;
      if (!record) return { error: `Plugin '${name}' not found` };
      return record;
    },
  },

  // ── Task Handoff ─────────────────────────────────────────────────────────
  // Mirrors the four Rust action verbs wired in TaskHandoffActionHandler.
  // The MCP dev server operates on the same PluresDB key-space so it can
  // exercise the full custody-transfer protocol from any MCP client.
  {
    name: 'task.handoff.prepare',
    description: 'Mark a task TransferPending and return a signed HandoffEnvelope. '
      + 'Params: task_id, source_agent, target_agent, handoff_id, expected_generation, '
      + 'optional task (inline definition).',
    inputSchema: {
      type: 'object',
      properties: {
        task_id:             { type: 'string' },
        source_agent:        { type: 'string' },
        target_agent:        { type: 'string' },
        handoff_id:          { type: 'string' },
        expected_generation: { type: 'number' },
        task:                { type: 'object', description: 'Optional inline task definition to seed' },
      },
      required: ['task_id', 'source_agent', 'target_agent', 'handoff_id', 'expected_generation'],
    },
    handler: ({ task_id, source_agent, target_agent, handoff_id, expected_generation, task: inlineTask }) => {
      const tid = task_id as string;
      const recKey = `handoff:record:${tid}`;

      // Seed inline task if provided and not already present.
      if (inlineTask) {
        const existing = db.get(recKey) as any;
        if (!existing) {
          dbPut(recKey, {
            task: inlineTask,
            custody_state: 'owned',
            owner_agent: source_agent as string,
            generation: 0,
            locked_by: null,
            lock_token: null,
          });
        }
      }

      const record = db.get(recKey) as any;
      if (!record) return { error: `Task '${tid}' not found — seed it first or pass inline task` };

      const gen = record.generation as number;
      if (gen !== (expected_generation as number)) {
        return { error: `Generation mismatch: expected ${expected_generation}, got ${gen}` };
      }
      if (record.custody_state !== 'owned') {
        return { error: `Task is not in 'owned' state (current: ${record.custody_state})` };
      }

      record.custody_state = 'transfer_pending';
      dbPut(recKey, record);

      // Build envelope (SHA-256 not available in this JS runtime without crypto;
      // we use a deterministic content string as the digest stand-in).
      const envelopePayload = JSON.stringify({
        task_id: tid,
        source_agent,
        target_agent,
        handoff_id,
        generation: gen,
        task: record.task,
      });
      const digest = Buffer.from(envelopePayload).toString('base64');
      const envelope = { task_id: tid, source_agent, target_agent, handoff_id, generation: gen, digest };

      return { record, envelope };
    },
  },
  {
    name: 'task.handoff.verify',
    description: 'Verify a HandoffEnvelope digest without mutating state. '
      + 'Params: envelope (object with digest field), source_agent, target_agent, task_id.',
    inputSchema: {
      type: 'object',
      properties: {
        envelope:     { type: 'object' },
        source_agent: { type: 'string' },
        target_agent: { type: 'string' },
        task_id:      { type: 'string' },
      },
      required: ['envelope', 'source_agent', 'target_agent', 'task_id'],
    },
    handler: ({ envelope, source_agent, target_agent, task_id }) => {
      const env = envelope as any;
      const tid = task_id as string;
      const record = db.get(`handoff:record:${tid}`) as any;
      if (!record) return { error: `Task '${tid}' not found` };

      if (env.source_agent !== source_agent) return { error: 'source_agent mismatch' };
      if (env.target_agent !== target_agent) return { error: 'target_agent mismatch' };
      if (env.task_id !== tid)               return { error: 'task_id mismatch' };

      // Re-derive expected digest and compare.
      const expectedPayload = JSON.stringify({
        task_id: tid,
        source_agent,
        target_agent,
        handoff_id: env.handoff_id,
        generation: env.generation,
        task: record.task,
      });
      const expectedDigest = Buffer.from(expectedPayload).toString('base64');
      if (env.digest !== expectedDigest) return { error: 'digest mismatch — envelope was tampered' };

      return { valid: true, task_id: tid };
    },
  },
  {
    name: 'task.handoff.accept',
    description: 'Transfer custody to target_agent (receiving side). '
      + 'Params: task_id, target_agent, handoff_id.',
    inputSchema: {
      type: 'object',
      properties: {
        task_id:     { type: 'string' },
        target_agent:{ type: 'string' },
        handoff_id:  { type: 'string' },
      },
      required: ['task_id', 'target_agent', 'handoff_id'],
    },
    handler: ({ task_id, target_agent, handoff_id }) => {
      const tid = task_id as string;
      const recKey = `handoff:record:${tid}`;
      const record = db.get(recKey) as any;
      if (!record) return { error: `Task '${tid}' not found` };
      if (record.custody_state !== 'transfer_pending') {
        return { error: `Task is not transfer_pending (current: ${record.custody_state})` };
      }

      record.custody_state = 'owned';
      record.owner_agent = target_agent as string;
      record.generation = (record.generation as number) + 1;
      record.locked_by = null;
      record.lock_token = null;
      dbPut(recKey, record);

      return { task_id: tid, new_owner: target_agent, generation: record.generation, handoff_id };
    },
  },
  {
    name: 'task.handoff.claim',
    description: 'Atomically claim a task for a worker (compare-and-swap). '
      + 'Params: task_id, agent_id, worker_id, generation.',
    inputSchema: {
      type: 'object',
      properties: {
        task_id:    { type: 'string' },
        agent_id:   { type: 'string' },
        worker_id:  { type: 'string' },
        generation: { type: 'number' },
      },
      required: ['task_id', 'agent_id', 'worker_id', 'generation'],
    },
    handler: ({ task_id, agent_id, worker_id, generation }) => {
      const tid = task_id as string;
      const recKey = `handoff:record:${tid}`;
      const record = db.get(recKey) as any;
      if (!record) return { error: `Task '${tid}' not found` };
      if (record.owner_agent !== agent_id) return { error: `Task not owned by '${agent_id}'` };
      if (record.generation !== (generation as number)) {
        return { error: `Generation mismatch: expected ${generation}, got ${record.generation}` };
      }

      // Idempotent: same worker re-claiming returns same token.
      if (record.locked_by === (worker_id as string) && record.lock_token) {
        return { task_id: tid, worker_id, token: record.lock_token };
      }

      // Already claimed by a different worker.
      if (record.locked_by && record.locked_by !== (worker_id as string)) {
        return { error: `Task already claimed by '${record.locked_by}'` };
      }

      const token = Math.random().toString(36).slice(2) + Date.now().toString(36);
      record.locked_by = worker_id as string;
      record.lock_token = token;
      dbPut(recKey, record);

      return { task_id: tid, worker_id, token };
    },
  },
];

// ── Simple Expression Evaluator (for Praxis evaluate) ─────────────────────────

function simpleEval(expr: string, context: Record<string, unknown>): boolean {
  const trimmed = expr.trim();
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;

  // Handle && (logical AND) — split and require all parts to be true
  // Use a regex that matches ' && ' to avoid splitting inside strings
  if (trimmed.includes(' && ')) {
    const parts = trimmed.split(' && ');
    return parts.every((part) => simpleEval(part, context));
  }

  // Handle || (logical OR) — split and require at least one part to be true
  if (trimmed.includes(' || ')) {
    const parts = trimmed.split(' || ');
    return parts.some((part) => simpleEval(part, context));
  }

  // Handle negation prefix: !expr
  if (trimmed.startsWith('!') && !trimmed.startsWith('!=')) {
    return !simpleEval(trimmed.slice(1), context);
  }

  // Handle Array.includes(x): `path.to.array.includes(valueExpr)`
  // Must run BEFORE the comparison branches (this expression has no ==/</> operators,
  // so it would otherwise fall through to a bare-path truthy check and misfire).
  const includesMatch = trimmed.match(/^(.+)\.includes\((.*)\)$/);
  if (includesMatch) {
    const arrVal = resolvePath(includesMatch[1].trim(), context);
    const needle = resolveValue(includesMatch[2].trim(), context);
    return Array.isArray(arrVal) ? arrVal.includes(needle) : false;
  }

  // Handle === comparison (must check before == to avoid false split)
  if (trimmed.includes('===')) {
    const [lhs, rhs] = trimmed.split('===').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsVal = resolveValue(rhs, context);
    return lhsVal === rhsVal;
  }

  // Handle !== comparison (must check before != to avoid false split)
  if (trimmed.includes('!==')) {
    const [lhs, rhs] = trimmed.split('!==').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsVal = resolveValue(rhs, context);
    return lhsVal !== rhsVal;
  }

  // Handle >= comparison (must check before > to avoid false match)
  if (trimmed.includes('>=')) {
    const [lhs, rhs] = trimmed.split('>=').map((s) => s.trim());
    return resolveNumeric(lhs, context) >= resolveNumeric(rhs, context);
  }

  // Handle <= comparison (must check before < to avoid false match)
  if (trimmed.includes('<=')) {
    const [lhs, rhs] = trimmed.split('<=').map((s) => s.trim());
    return resolveNumeric(lhs, context) <= resolveNumeric(rhs, context);
  }

  // Handle > comparison
  if (trimmed.includes('>')) {
    const [lhs, rhs] = trimmed.split('>').map((s) => s.trim());
    return resolveNumeric(lhs, context) > resolveNumeric(rhs, context);
  }

  // Handle < comparison
  if (trimmed.includes('<')) {
    const [lhs, rhs] = trimmed.split('<').map((s) => s.trim());
    return resolveNumeric(lhs, context) < resolveNumeric(rhs, context);
  }

  // Handle == comparison
  if (trimmed.includes('==')) {
    const [lhs, rhs] = trimmed.split('==').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsClean = rhs.replace(/^["']|["']$/g, '');
    return String(lhsVal) === rhsClean;
  }

  // Handle != comparison
  if (trimmed.includes('!=')) {
    const [lhs, rhs] = trimmed.split('!=').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsClean = rhs.replace(/^["']|["']$/g, '');
    return String(lhsVal) !== rhsClean;
  }

  // Bare value — truthy check
  const val = resolvePath(trimmed, context);
  return !!val;
}

function resolveValue(raw: string, context: Record<string, unknown>): unknown {
  const trimmed = raw.trim();
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;
  if (trimmed === 'null') return null;
  if (trimmed === 'undefined') return undefined;
  if (/^\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  if (/^["'].*["']$/.test(trimmed)) return trimmed.slice(1, -1);
  // Treat as path into context
  return resolvePath(trimmed, context);
}

function resolvePath(path: string, obj: Record<string, unknown>): unknown {
  const parts = path.split('.');
  let current: unknown = obj;
  for (const part of parts) {
    if (current == null || typeof current !== 'object') return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

// Resolve a comparison operand to a number, supporting simple arithmetic on paths/literals,
// e.g. `(trade.dailySpentUsd + trade.notionalUsd)` or `policy.dailyMaxUsd`. Only + - * / and
// parentheses are honored; each atom is a numeric literal or a resolved path. Any unparseable
// atom yields NaN (so the comparison is false), never a thrown error.
function resolveNumeric(expr: string, context: Record<string, unknown>): number {
  let s = expr.trim();
  // Strip one layer of wrapping parens if they enclose the whole expression.
  while (s.startsWith('(') && s.endsWith(')')) {
    s = s.slice(1, -1).trim();
  }
  // Fast path: no arithmetic operator → a single literal or path.
  if (!/[+\-*/]/.test(s.replace(/^-/, ''))) {
    return Number(resolveValue(s, context));
  }
  // Tokenize into numbers and operators; resolve each non-operator atom to a number.
  const tokens = s.match(/[+\-*/]|[^+\-*/\s]+/g);
  if (!tokens || tokens.length === 0) return Number.NaN;
  const resolved = tokens
    .map((t) => (/^[+\-*/]$/.test(t) ? t : String(Number(resolveValue(t, context)))))
    .join(' ');
  // Evaluate the pure numeric arithmetic string safely (digits, operators, dot, space only).
  if (!/^[-+*/.\d\s]+$/.test(resolved)) return Number.NaN;
  try {
    // eslint-disable-next-line no-new-func
    const val = Function(`"use strict"; return (${resolved});`)() as unknown;
    return typeof val === 'number' ? val : Number.NaN;
  } catch {
    return Number.NaN;
  }
}

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
process.stderr.write(`💾 DB: ${RADIX_DB_PATH} (${db.size} keys loaded)\n`);
process.stderr.write('⚠️  This server has FULL ACCESS to app state\n');

// Flush pending writes on exit
process.on('beforeExit', () => {
  if (persistTimer) {
    clearTimeout(persistTimer);
    persistTimer = null;
    persistDbToDisk();
  }
});
process.on('SIGINT', () => {
  persistDbToDisk();
  process.exit(0);
});
process.on('SIGTERM', () => {
  persistDbToDisk();
  process.exit(0);
});
