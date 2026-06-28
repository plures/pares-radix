/**
 * Drift guard (C-DRIFT-001): praxis/ui/ui-layout.px  ↔  UI_PRACTICES (ui-practices.ts).
 *
 * The .px file is the human-readable source of truth; ui-practices.ts is the
 * executable mirror the resolver consumes. This test parses the `practice` blocks
 * out of the .px and asserts the same count + identical name/kind/appliesTo/set/
 * source/when for each, in order. If they drift, this fails — you cannot silently
 * change one without the other.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { UI_PRACTICES, type UiPractice } from '../src/ui-practices.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
// tests/ -> packages/canvas-runtime -> packages -> repo root
const PX_PATH = resolve(__dirname, '../../../praxis/ui/ui-layout.px');

interface ParsedPractice {
  name: string;
  kind?: string;
  appliesTo?: string;
  when?: string;
  set?: string;
  from?: string;
  default?: string;
}

function parsePractices(src: string): ParsedPractice[] {
  const lines = src.split(/\r?\n/);
  const out: ParsedPractice[] = [];
  let cur: ParsedPractice | null = null;
  const headRe = /^practice\s+([A-Za-z0-9_]+):\s*$/;
  const fieldRe = /^\s+(kind|appliesTo|when|set|from|default):\s*(.*)$/;
  for (const raw of lines) {
    const line = raw.replace(/\s+$/, '');
    if (line.trim().startsWith('#') || line.trim() === '') continue;
    const head = headRe.exec(line);
    if (head) {
      if (cur) out.push(cur);
      cur = { name: head[1] };
      continue;
    }
    const field = fieldRe.exec(line);
    if (field && cur) {
      (cur as Record<string, string>)[field[1]] = field[2].trim();
    }
  }
  if (cur) out.push(cur);
  return out;
}

/** Normalize a TS UiPractice into the same comparable shape as the parsed .px. */
function normalizeTs(p: UiPractice): Required<Omit<ParsedPractice, 'from' | 'default'>> & {
  from?: string;
  default?: string;
} {
  const base = {
    name: p.name,
    kind: p.kind,
    appliesTo: p.appliesTo,
    when: p.when ?? '',
    set: p.set,
  };
  if (p.source.kind === 'responsive') return { ...base, from: 'responsive' };
  return { ...base, default: p.source.value };
}

describe('ui-practices drift guard', () => {
  const parsed = parsePractices(readFileSync(PX_PATH, 'utf8'));

  it('the .px parsed at least one practice', () => {
    expect(parsed.length).toBeGreaterThan(0);
  });

  it('same number of practices in .px and UI_PRACTICES', () => {
    expect(parsed.length).toBe(UI_PRACTICES.length);
  });

  it('each practice matches name/kind/appliesTo/set/source/when in order', () => {
    for (let i = 0; i < UI_PRACTICES.length; i++) {
      const ts = normalizeTs(UI_PRACTICES[i]);
      const px = parsed[i];
      expect(px.name, `practice[${i}] name`).toBe(ts.name);
      expect(px.kind, `${ts.name}.kind`).toBe(ts.kind);
      expect(px.appliesTo, `${ts.name}.appliesTo`).toBe(ts.appliesTo);
      expect(px.set, `${ts.name}.set`).toBe(ts.set);
      // when: .px omits the field when there's no guard; TS normalizes to ''
      expect((px.when ?? '').trim(), `${ts.name}.when`).toBe(ts.when);
      // source parity
      if (ts.from) {
        expect(px.from, `${ts.name}.from`).toBe(ts.from);
        expect(px.default, `${ts.name} should have no default`).toBeUndefined();
      } else {
        expect(px.default, `${ts.name}.default`).toBe(ts.default);
        expect(px.from, `${ts.name} should have no from`).toBeUndefined();
      }
    }
  });

  it('every practice set-attribute is a known responsive attribute name', () => {
    // Guard the honesty invariant at the practice level (names only; the
    // resolver enforces the prop mapping at runtime).
    const known = new Set([
      'direction', 'padding', 'gap', 'align', 'justify', 'wrap',
      'columns', 'hidden', 'size', 'maxLines',
    ]);
    for (const p of UI_PRACTICES) {
      expect(known.has(p.set), `${p.name} sets unknown attr ${p.set}`).toBe(true);
    }
  });
});
