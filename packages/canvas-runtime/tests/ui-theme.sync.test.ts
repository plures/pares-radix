/**
 * Drift guard (C-DRIFT-001): praxis/ui/ui-theme.px  ↔  UI_THEME_PRACTICES.
 *
 * The .px file is the human-readable source of truth; UI_THEME_PRACTICES
 * (ui-practices.ts) is the executable mirror the resolver consumes. This test
 * parses the `practice` blocks out of the .px and asserts the same count +
 * identical name/kind/appliesTo/set/source/when for each, in order. If they
 * drift, this fails — you cannot silently change one without the other.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { UI_THEME_PRACTICES, THEMEABLE_ATTR_SET, THEME_SURFACE, type UiPractice } from '../src/ui-practices.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
// tests/ -> packages/canvas-runtime -> packages -> repo root
const PX_PATH = resolve(__dirname, '../../../praxis/ui/ui-theme.px');

interface ParsedPractice {
  name: string;
  kind?: string;
  appliesTo?: string;
  when?: string;
  set?: string;
  from?: string;
  default?: string;
  rationale?: string;
}

function parsePractices(src: string): ParsedPractice[] {
  const lines = src.split(/\r?\n/);
  const out: ParsedPractice[] = [];
  let cur: ParsedPractice | null = null;
  const headRe = /^practice\s+([A-Za-z0-9_]+):\s*$/;
  const fieldRe = /^\s+(kind|appliesTo|when|set|from|default|rationale):\s*(.*)$/;
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
    rationale: p.rationale,
  };
  if (p.source.kind === 'responsive') return { ...base, from: 'responsive' };
  return { ...base, default: p.source.value };
}

/**
 * Parse the documented SURFACE table out of the .px comment block. Lines look
 * like `#   light        #ffffff` / `#   dark         #0b0b0b`. We anchor on the
 * exact mode names so prose hex mentions elsewhere can't match.
 */
function parseSurfaceTable(src: string): Record<string, string> {
  const out: Record<string, string> = {};
  const re = /^#\s+(light|dark)\s+(#[0-9a-fA-F]{3,8})\s*$/;
  for (const raw of src.split(/\r?\n/)) {
    const m = re.exec(raw.replace(/\s+$/, ''));
    if (m) out[m[1]] = m[2].toLowerCase();
  }
  return out;
}

describe('ui-theme practices drift guard', () => {
  const parsed = parsePractices(readFileSync(PX_PATH, 'utf8'));

  it('the .px parsed at least one practice', () => {
    expect(parsed.length).toBeGreaterThan(0);
  });

  it('same number of practices in .px and UI_THEME_PRACTICES', () => {
    expect(parsed.length).toBe(UI_THEME_PRACTICES.length);
  });

  it('each practice matches name/kind/appliesTo/set/source/when in order', () => {
    for (let i = 0; i < UI_THEME_PRACTICES.length; i++) {
      const ts = normalizeTs(UI_THEME_PRACTICES[i]);
      const px = parsed[i];
      expect(px.name, `practice[${i}] name`).toBe(ts.name);
      expect(px.kind, `${ts.name}.kind`).toBe(ts.kind);
      expect(px.appliesTo, `${ts.name}.appliesTo`).toBe(ts.appliesTo);
      expect(px.set, `${ts.name}.set`).toBe(ts.set);
      // when: .px omits the field when there's no guard; TS normalizes to ''
      expect((px.when ?? '').trim(), `${ts.name}.when`).toBe(ts.when);
      // rationale parity: the .px is the human source of truth; the TS mirror
      // must carry the identical author-facing sentence (C-DRIFT-001).
      expect(px.rationale, `${ts.name}.rationale`).toBe(ts.rationale);
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

  it('every practice carries a non-empty author-facing rationale', () => {
    for (let i = 0; i < UI_THEME_PRACTICES.length; i++) {
      const px = parsed[i];
      expect(px.rationale && px.rationale.trim().length, `${px.name} .px rationale`).toBeGreaterThan(0);
      expect(UI_THEME_PRACTICES[i].rationale.trim().length, `${UI_THEME_PRACTICES[i].name} TS rationale`).toBeGreaterThan(0);
    }
  });

  it('every practice sets a themeable prop (allow-list)', () => {
    // Honesty invariant: theme practices may only write THEMEABLE_ATTR_SET props
    // (color today). No invented props, and NO background (no container exposes it).
    for (const p of UI_THEME_PRACTICES) {
      expect(THEMEABLE_ATTR_SET.has(p.set), `${p.name} sets non-themeable attr ${p.set}`).toBe(true);
    }
  });

  it('the .px SURFACE table matches THEME_SURFACE (drift guard)', () => {
    // The base surface per mode is theme data: the .px documents it and
    // ui-practices.ts mirrors it as THEME_SURFACE. They must agree, same as the
    // token palette, so the contrast linter's background can never silently drift
    // between source-of-truth and the executable mirror.
    const surfaces = parseSurfaceTable(readFileSync(PX_PATH, 'utf8'));
    expect(Object.keys(surfaces).sort()).toEqual(['dark', 'light']);
    expect(surfaces.light).toBe(THEME_SURFACE.light.background.toLowerCase());
    expect(surfaces.dark).toBe(THEME_SURFACE.dark.background.toLowerCase());
  });
});
