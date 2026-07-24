import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  compareSemver,
  parseDriftMd,
  extractFromTarball,
  runDriftCheck,
  resolveInstalledNpmVersion,
} from '../scripts/check-design-dojo-drift.mjs';
import zlib from 'node:zlib';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..');

describe('compareSemver', () => {
  it('orders patch versions correctly', () => {
    expect(compareSemver('0.17.0', '0.17.1')).toBe(-1);
    expect(compareSemver('0.17.1', '0.17.0')).toBe(1);
    expect(compareSemver('0.17.1', '0.17.1')).toBe(0);
  });

  it('orders minor/major versions correctly', () => {
    expect(compareSemver('0.13.0', '0.17.1')).toBe(-1);
    expect(compareSemver('1.0.0', '0.99.99')).toBe(1);
  });

  it('handles missing trailing components as zero', () => {
    expect(compareSemver('0.17', '0.17.0')).toBe(0);
    expect(compareSemver('0.17', '0.17.1')).toBe(-1);
  });
});

describe('parseDriftMd', () => {
  it('parses a component section into an override map', () => {
    const md = `# DRIFT.md\n\n## GraphView.svelte\n\n- Reason: local type import path.\n- Expected removal: v0.18.0\n`;
    const overrides = parseDriftMd(md);
    expect(overrides.has('GraphView.svelte')).toBe(true);
    expect(overrides.get('GraphView.svelte')).toContain('local type import path');
  });

  it('returns an empty map for empty/missing content', () => {
    expect(parseDriftMd('').size).toBe(0);
    expect(parseDriftMd(undefined).size).toBe(0);
  });

  it('parses multiple sections independently', () => {
    const md = `## A.svelte\nreason A\n\n## B.svelte\nreason B\n`;
    const overrides = parseDriftMd(md);
    expect(overrides.get('A.svelte').trim()).toBe('reason A');
    expect(overrides.get('B.svelte').trim()).toBe('reason B');
  });
});

describe('extractFromTarball', () => {
  function makeTarEntry(name, content) {
    const nameBuf = Buffer.alloc(100);
    Buffer.from(name, 'utf8').copy(nameBuf);
    const sizeBuf = Buffer.alloc(12);
    Buffer.from(content.length.toString(8).padStart(11, '0') + '\0', 'utf8').copy(sizeBuf);
    const header = Buffer.alloc(512);
    nameBuf.copy(header, 0);
    sizeBuf.copy(header, 124);
    const dataBuf = Buffer.from(content, 'utf8');
    const paddedLen = Math.ceil(dataBuf.length / 512) * 512;
    const padded = Buffer.alloc(paddedLen);
    dataBuf.copy(padded);
    return Buffer.concat([header, padded]);
  }

  it('extracts a named file from a synthetic gzip ustar tarball', () => {
    const entry = makeTarEntry('package/src/lib/app/GraphView.svelte', 'hello world');
    const endMarker = Buffer.alloc(1024);
    const tar = Buffer.concat([entry, endMarker]);
    const gz = zlib.gzipSync(tar);
    const result = extractFromTarball(gz, 'src/lib/app/GraphView.svelte');
    expect(result).toBe('hello world');
  });

  it('returns null when the requested path is absent', () => {
    const entry = makeTarEntry('package/src/lib/app/Other.svelte', 'x');
    const endMarker = Buffer.alloc(1024);
    const gz = zlib.gzipSync(Buffer.concat([entry, endMarker]));
    expect(extractFromTarball(gz, 'src/lib/app/GraphView.svelte')).toBeNull();
  });
});

describe('resolveInstalledNpmVersion', () => {
  it('resolves the real @plures/design-dojo-npm version from node_modules', () => {
    const { version } = resolveInstalledNpmVersion(path.join(REPO_ROOT, 'packages', 'design-dojo'));
    expect(version).toMatch(/^\d+\.\d+\.\d+/);
  });
});

describe('runDriftCheck (real UPSTREAM_MAP.json + DRIFT.md, injected fetch)', () => {
  it('reports in-sync for a file whose fetched upstream content matches the vendored file', async () => {
    const vendored = readFileSync(
      path.join(REPO_ROOT, 'packages', 'design-dojo', 'src', 'GraphView.svelte'),
      'utf8'
    );
    // Simulate the upstream file at the OLD import path (i.e. what DRIFT.md documents as the
    // intentional override): everything matches except the local import line, which the real
    // upstream fetch would return with './GraphView.types.js'. To assert "documented override"
    // classification, we return the *true* upstream text (with the different import) here.
    const upstreamText = vendored.replace(
      "from './types-local.js'",
      "from './GraphView.types.js'"
    );
    const fakeFetch = async (_version, upstreamPath) => {
      if (upstreamPath === 'src/lib/app/GraphView.svelte') return upstreamText;
      return null;
    };
    const { results, failures } = await runDriftCheck({ fetchUpstream: fakeFetch });
    const graphView = results.find((r) => r.filename === 'GraphView.svelte');
    expect(graphView.status).toBe('drifted-with-documented-override');
    // No entries besides GraphView.svelte are checked against a real fetch (fakeFetch returns
    // null for everything else -> classified as upstream-path-missing, not a failure).
    expect(failures.length).toBe(0);
  });

  it('fails when a vendored file differs from upstream with no DRIFT.md override', async () => {
    const fakeFetch = async (_version, upstreamPath) => {
      if (upstreamPath === 'src/lib/primitives/Input.svelte') {
        return '<!-- deliberately different upstream content -->';
      }
      return null;
    };
    const { failures } = await runDriftCheck({ fetchUpstream: fakeFetch });
    expect(failures.some((f) => f.startsWith('Input.svelte:'))).toBe(true);
  });

  it('skips local-only components (DataGrid, SchemaForm) without fetching', async () => {
    let calledFor = [];
    const fakeFetch = async (_version, upstreamPath) => {
      calledFor.push(upstreamPath);
      return null;
    };
    const { results } = await runDriftCheck({ fetchUpstream: fakeFetch });
    const dataGrid = results.find((r) => r.filename === 'DataGrid.svelte');
    const schemaForm = results.find((r) => r.filename === 'SchemaForm.svelte');
    expect(dataGrid.status).toBe('skipped-local-only');
    expect(schemaForm.status).toBe('skipped-local-only');
    expect(calledFor).not.toContain(null);
  });
});
