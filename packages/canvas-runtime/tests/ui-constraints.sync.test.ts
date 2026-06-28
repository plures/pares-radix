import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { UI_CONSTRAINTS } from '../src/ui-constraints.js';

/**
 * DRIFT GUARD (C-DRIFT-001).
 *
 * praxis/ui/ui-best-practices.px is the human-readable source of truth and the
 * artifact loaded into PluresDB for the live Praxis engine. ui-constraints.ts
 * is a derived runtime mirror so canvas-runtime can validate without a running
 * engine. They MUST stay identical. This test parses the .px and asserts every
 * constraint's name, `require`, `when`, and `severity` match the TS array.
 *
 * If this fails: you edited one and not the other. Sync them.
 */

const here = dirname(fileURLToPath(import.meta.url));
const pxPath = resolve(here, '../../../praxis/ui/ui-best-practices.px');

interface ParsedConstraint {
  name: string;
  when?: string;
  require?: string;
  severity?: string;
}

function parsePx(src: string): ParsedConstraint[] {
  const out: ParsedConstraint[] = [];
  const lines = src.split(/\r?\n/);
  let current: ParsedConstraint | null = null;

  const fieldRe = /^\s+(when|require|severity|message|phases):\s*(.*)$/;
  const headRe = /^constraint\s+([A-Za-z0-9_]+):\s*$/;

  for (const line of lines) {
    if (line.trimStart().startsWith('#')) continue; // comment
    const head = headRe.exec(line);
    if (head) {
      if (current) out.push(current);
      current = { name: head[1] };
      continue;
    }
    if (!current) continue;
    const field = fieldRe.exec(line);
    if (!field) continue;
    const [, key, rawVal] = field;
    const val = rawVal.trim();
    if (key === 'when') current.when = val;
    else if (key === 'require') current.require = val;
    else if (key === 'severity') current.severity = val;
  }
  if (current) out.push(current);
  return out;
}

describe('ui-best-practices.px <-> UI_CONSTRAINTS drift guard', () => {
  const src = readFileSync(pxPath, 'utf8');
  const parsed = parsePx(src);

  it('the .px file is readable and has constraints', () => {
    expect(parsed.length).toBeGreaterThan(0);
  });

  it('has the same number of constraints', () => {
    expect(parsed.length).toBe(UI_CONSTRAINTS.length);
  });

  it('every .px constraint exists in TS with identical require/when/severity', () => {
    for (const p of parsed) {
      const ts = UI_CONSTRAINTS.find((c) => c.name === p.name);
      expect(ts, `TS missing constraint '${p.name}'`).toBeDefined();
      if (!ts) continue;
      expect(p.require, `require mismatch for ${p.name}`).toBe(ts.require);
      expect(p.when, `when mismatch for ${p.name}`).toBe(ts.when);
      expect(p.severity, `severity mismatch for ${p.name}`).toBe(ts.severity);
    }
  });

  it('every TS constraint exists in the .px file', () => {
    for (const ts of UI_CONSTRAINTS) {
      const p = parsed.find((c) => c.name === ts.name);
      expect(p, `.px missing constraint '${ts.name}'`).toBeDefined();
    }
  });
});
