import { describe, it, expect } from 'vitest';
import {
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
} from '../src/canvas-plugin.js';
import type { CanvasNode, CanvasRule, CanvasProcedure } from '../src/format.js';

describe('Canvas Plugin Tools', () => {
  it('canvas.create makes a new document', () => {
    const canvas = toolCanvasCreate({ title: 'Test App' });
    expect(canvas.meta.title).toBe('Test App');
    expect(canvas.version).toBe('1.0.0');
    expect(canvas.tree.id).toBe('root');
  });

  it('canvas.setTree replaces the tree', () => {
    const canvas = toolCanvasCreate({ title: 'Test' });
    const newTree: CanvasNode = { id: 'new-root', type: 'Sidebar', children: [] };
    const updated = toolCanvasSetTree(canvas, newTree);
    expect(updated.tree.id).toBe('new-root');
    expect(updated.tree.type).toBe('Sidebar');
  });

  it('canvas.addNode appends to parent', () => {
    const canvas = toolCanvasCreate({ title: 'Test' });
    const node: CanvasNode = { id: 'btn1', type: 'Button', props: { label: 'Click' } };
    const updated = toolCanvasAddNode(canvas, 'root', node);
    expect(updated.tree.children).toHaveLength(1);
    expect(updated.tree.children![0].id).toBe('btn1');
  });

  it('canvas.addNode works on nested parent', () => {
    let canvas = toolCanvasCreate({ title: 'Test' });
    const container: CanvasNode = { id: 'container', type: 'PluginContentArea', children: [] };
    canvas = toolCanvasAddNode(canvas, 'root', container);

    const btn: CanvasNode = { id: 'btn1', type: 'Button' };
    canvas = toolCanvasAddNode(canvas, 'container', btn);

    expect(canvas.tree.children![0].children).toHaveLength(1);
    expect(canvas.tree.children![0].children![0].id).toBe('btn1');
  });

  it('canvas.removeNode removes by id', () => {
    let canvas = toolCanvasCreate({ title: 'Test' });
    canvas = toolCanvasAddNode(canvas, 'root', { id: 'a', type: 'Button' });
    canvas = toolCanvasAddNode(canvas, 'root', { id: 'b', type: 'Button' });
    expect(canvas.tree.children).toHaveLength(2);

    canvas = toolCanvasRemoveNode(canvas, 'a');
    expect(canvas.tree.children).toHaveLength(1);
    expect(canvas.tree.children![0].id).toBe('b');
  });

  it('canvas.setData merges data', () => {
    let canvas = toolCanvasCreate({ title: 'Test' });
    canvas = toolCanvasSetData(canvas, { 'items': [1, 2], 'count': 2 });
    canvas = toolCanvasSetData(canvas, { 'count': 3 });
    expect(canvas.data['items']).toEqual([1, 2]);
    expect(canvas.data['count']).toBe(3);
  });

  it('canvas.addRule appends rule', () => {
    let canvas = toolCanvasCreate({ title: 'Test' });
    const rule: CanvasRule = {
      id: 'r1',
      description: 'No empty',
      when: { key: 'name', op: 'falsy' },
      action: 'gate',
      severity: 'error',
    };
    canvas = toolCanvasAddRule(canvas, rule);
    expect(canvas.rules).toHaveLength(1);
    expect(canvas.rules[0].id).toBe('r1');
  });

  it('canvas.addProcedure appends procedure', () => {
    let canvas = toolCanvasCreate({ title: 'Test' });
    const proc: CanvasProcedure = {
      id: 'p1',
      description: 'Add item',
      trigger: { kind: 'on_click', nodeId: 'btn1' },
      steps: [{ kind: 'set', key: 'count', value: 1 }],
    };
    canvas = toolCanvasAddProcedure(canvas, proc);
    expect(canvas.procedures).toHaveLength(1);
    expect(canvas.procedures[0].trigger.nodeId).toBe('btn1');
  });

  it('canvas.export/import round-trip', () => {
    let canvas = toolCanvasCreate({ title: 'Export Test' });
    canvas = toolCanvasAddNode(canvas, 'root', { id: 'b1', type: 'Button' });
    canvas = toolCanvasSetData(canvas, { items: [1] });

    const json = toolCanvasExport(canvas);
    const reimported = toolCanvasImport(json);

    expect(reimported.meta.title).toBe('Export Test');
    expect(reimported.tree.children).toHaveLength(1);
    expect(reimported.data['items']).toEqual([1]);
  });

  it('canvas.validate catches issues', () => {
    let canvas = toolCanvasCreate({ title: 'Test' });
    canvas.procedures = [
      { id: 'empty', description: 'x', trigger: { kind: 'on_click' }, steps: [] },
    ];
    const issues = toolCanvasValidate(canvas);
    expect(issues.some((i) => i.includes('no steps'))).toBe(true);
  });
});
