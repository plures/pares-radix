import { describe, it, expect } from 'vitest';
import {
  createCanvas,
  exportCanvas,
  importCanvas,
  validateCanvas,
} from '../src/format.js';
import type { CanvasDocument, CanvasNode } from '../src/format.js';

describe('createCanvas', () => {
  it('creates with defaults', () => {
    const canvas = createCanvas();
    expect(canvas.version).toBe('1.0.0');
    expect(canvas.meta.title).toBe('Untitled Canvas');
    expect(canvas.meta.id).toBeTruthy();
    expect(canvas.tree.id).toBe('root');
    expect(canvas.tree.type).toBe('PluginContentArea');
    expect(canvas.data).toEqual({});
    expect(canvas.rules).toEqual([]);
    expect(canvas.procedures).toEqual([]);
    expect(canvas.schema).toEqual([]);
  });

  it('creates with custom meta', () => {
    const canvas = createCanvas({
      title: 'My App',
      description: 'Test app',
      author: 'ai:cerebellum',
      tags: ['test'],
    });
    expect(canvas.meta.title).toBe('My App');
    expect(canvas.meta.description).toBe('Test app');
    expect(canvas.meta.author).toBe('ai:cerebellum');
    expect(canvas.meta.tags).toEqual(['test']);
  });
});

describe('exportCanvas', () => {
  it('wraps document in export envelope', () => {
    const canvas = createCanvas({ title: 'Export Test' });
    const exported = exportCanvas(canvas);
    expect(exported.format).toBe('plures-canvas');
    expect(exported.formatVersion).toBe('1.0.0');
    expect(exported.document.meta.title).toBe('Export Test');
  });

  it('includes timeline when provided', () => {
    const canvas = createCanvas();
    const exported = exportCanvas(canvas, {
      timeline: [
        { ts: 1000, actor: { kind: 'ai', id: 'ai:test' }, key: 'k', before: null, after: 1 },
      ],
    });
    expect(exported.timeline).toHaveLength(1);
    expect(exported.timeline![0].actor.kind).toBe('ai');
  });
});

describe('importCanvas', () => {
  it('round-trips through export/import', () => {
    const canvas = createCanvas({ title: 'Round Trip' });
    canvas.data = { 'todo:items': [1, 2, 3] };
    canvas.rules = [{ id: 'r1', description: 'test', when: 'k', action: 'warn', severity: 'info' }];

    const exported = exportCanvas(canvas);
    const json = JSON.stringify(exported);
    const parsed = JSON.parse(json);
    const imported = importCanvas(parsed);

    expect(imported.meta.title).toBe('Round Trip');
    expect(imported.data['todo:items']).toEqual([1, 2, 3]);
    expect(imported.rules).toHaveLength(1);
  });

  it('rejects invalid format', () => {
    expect(() => importCanvas({})).toThrow('Invalid canvas file');
    expect(() => importCanvas({ format: 'wrong' })).toThrow('Invalid canvas file');
  });
});

describe('validateCanvas', () => {
  it('returns no issues for valid canvas', () => {
    const canvas = createCanvas({ title: 'Valid' });
    const issues = validateCanvas(canvas);
    expect(issues).toEqual([]);
  });

  it('detects missing tree type', () => {
    const canvas = createCanvas();
    (canvas.tree as any).type = '';
    const issues = validateCanvas(canvas);
    expect(issues.some((i) => i.includes('missing type'))).toBe(true);
  });

  it('detects procedure with no steps', () => {
    const canvas = createCanvas();
    canvas.procedures = [
      { id: 'p1', description: 'empty', trigger: { kind: 'on_click' }, steps: [] },
    ];
    const issues = validateCanvas(canvas);
    expect(issues.some((i) => i.includes('no steps'))).toBe(true);
  });

  it('validates nested tree nodes', () => {
    const canvas = createCanvas();
    canvas.tree.children = [
      { id: '', type: 'Button' }, // missing id
    ];
    const issues = validateCanvas(canvas);
    expect(issues.some((i) => i.includes('missing id'))).toBe(true);
  });
});
