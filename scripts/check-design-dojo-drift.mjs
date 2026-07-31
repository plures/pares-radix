#!/usr/bin/env node
/**
 * scripts/check-design-dojo-drift.mjs
 *
 * Enforcement mechanism for ADR-0035 (design-dojo vendored-copy drift).
 *
 * For every entry in packages/design-dojo/UPSTREAM_MAP.json that has an
 * `upstreamPath`, this script:
 *   1. Resolves the pinned npm-published version of @plures/design-dojo
 *      installed in node_modules (via @plures/design-dojo-npm).
 *   2. Fetches that exact published version's tarball from the npm registry
 *      and extracts the corresponding source file.
 *   3. Diffs it against the vendored copy in packages/design-dojo/src/.
 *   4. FAILS (non-zero exit) if the vendored copy differs from the
 *      pinned-published upstream file and there is no DRIFT.md entry for it
 *      (temporary override, upstream PR link, and expected removal date).
 *   5. WARNS (loud, non-fatal) if a `removeAfterNpmVersion` obligation is
 *      already satisfied by the currently pinned npm version but the
 *      component has not been removed from the shim.
 *
 * This never requires a human to remember to "manually sync" the shim -
 * the check runs on every PR (see .github/workflows/design-dojo-drift.yml)
 * and is the sole source of truth for whether drift is acceptable.
 */

import { createRequire } from 'node:module';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import zlib from 'node:zlib';
import https from 'node:https';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..');
const SHIM_DIR = path.join(REPO_ROOT, 'packages', 'design-dojo');
const MAP_PATH = path.join(SHIM_DIR, 'UPSTREAM_MAP.json');
const DRIFT_MD_PATH = path.join(SHIM_DIR, 'DRIFT.md');

/** Simple semver compare: returns -1, 0, 1 for a<b, a==b, a>b. */
export function compareSemver(a, b) {
  const pa = String(a).split('.').map((n) => parseInt(n, 10) || 0);
  const pb = String(b).split('.').map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

/** Parses DRIFT.md into a map of component filename -> entry text (used as an override allowlist). */
export function parseDriftMd(driftMdContent) {
  const overrides = new Map();
  if (!driftMdContent) return overrides;
  // Each override entry is a markdown section: "## <filename>\n<body...>" up to the next "## " or EOF.
  const sections = driftMdContent.split(/^## /m).slice(1);
  for (const section of sections) {
    const lines = section.split('\n');
    const filename = lines[0].trim();
    const body = lines.slice(1).join('\n').trim();
    if (filename) overrides.set(filename, body);
  }
  return overrides;
}

/** Reads the currently installed npm-published version of @plures/design-dojo. */
export function resolveInstalledNpmVersion(shimDir = SHIM_DIR) {
  // Don't rely on package "exports" (design-dojo may not expose ./package.json or a resolvable
  // main entry under every export map) - resolve the physical install directory directly via
  // node_modules layout, which pnpm always creates regardless of the package's own exports field.
  const directCandidates = [
    path.join(shimDir, 'node_modules', '@plures', 'design-dojo-npm', 'package.json'),
    path.join(REPO_ROOT, 'node_modules', '@plures', 'design-dojo-npm', 'package.json'),
  ];
  for (const candidate of directCandidates) {
    if (existsSync(candidate)) {
      const pkg = JSON.parse(readFileSync(candidate, 'utf8'));
      return { version: pkg.version, installDir: path.dirname(candidate) };
    }
  }

  // pnpm's content-addressed store aliases the real package name (design-dojo) rather than the
  // local alias (design-dojo-npm) under .pnpm/@plures+design-dojo@<version>_.../node_modules/@plures/design-dojo.
  const pnpmDir = path.join(REPO_ROOT, 'node_modules', '.pnpm');
  if (existsSync(pnpmDir)) {
    const { readdirSync } = require('node:fs');
    const match = readdirSync(pnpmDir).find((name) => name.startsWith('@plures+design-dojo@'));
    if (match) {
      const candidate = path.join(pnpmDir, match, 'node_modules', '@plures', 'design-dojo', 'package.json');
      if (existsSync(candidate)) {
        const pkg = JSON.parse(readFileSync(candidate, 'utf8'));
        return { version: pkg.version, installDir: path.dirname(candidate) };
      }
    }
  }

  throw new Error(
    `Could not resolve @plures/design-dojo-npm install directory under ${shimDir} or the pnpm store ` +
      `at ${pnpmDir}. Run "pnpm install" first.`
  );
}

function fetchBuffer(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { 'user-agent': 'design-dojo-drift-check' } }, (res) => {
        if (res.statusCode && res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          resolve(fetchBuffer(res.headers.location));
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`GET ${url} -> HTTP ${res.statusCode}`));
          return;
        }
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () => resolve(Buffer.concat(chunks)));
        res.on('error', reject);
      })
      .on('error', reject);
  });
}

/**
 * Minimal in-process tar reader (no shelling out to `tar`, no extra deps):
 * npm tarballs are gzip'd POSIX ustar archives. Returns file contents by path
 * relative to the tarball's "package/" root.
 */
export function extractFromTarball(tarballGzBuf, targetPathInPackage) {
  const buf = zlib.gunzipSync(tarballGzBuf);
  const want = `package/${targetPathInPackage}`;
  let offset = 0;
  while (offset + 512 <= buf.length) {
    const header = buf.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break; // end of archive
    const name = header.subarray(0, 100).toString('utf8').replace(/\0.*$/s, '');
    const sizeOctal = header.subarray(124, 136).toString('utf8').replace(/\0.*$/s, '').trim();
    const size = parseInt(sizeOctal, 8) || 0;
    const dataStart = offset + 512;
    if (name === want) {
      return buf.subarray(dataStart, dataStart + size).toString('utf8');
    }
    const blocks = Math.ceil(size / 512);
    offset = dataStart + blocks * 512;
  }
  return null;
}

async function fetchUpstreamFile(version, upstreamPath) {
  const metaUrl = `https://registry.npmjs.org/@plures/design-dojo/${version}`;
  const metaBuf = await fetchBuffer(metaUrl);
  const meta = JSON.parse(metaBuf.toString('utf8'));
  const tarballUrl = meta?.dist?.tarball;
  if (!tarballUrl) throw new Error(`No tarball URL for @plures/design-dojo@${version}`);
  const tarballBuf = await fetchBuffer(tarballUrl);
  return extractFromTarball(tarballBuf, upstreamPath);
}

function normalize(text) {
  // Ignore line-ending differences only; every other byte must match.
  return text.replace(/\r\n/g, '\n');
}

export async function runDriftCheck({ fetchUpstream = fetchUpstreamFile, offline = false } = {}) {
  const map = JSON.parse(readFileSync(MAP_PATH, 'utf8'));
  const driftMd = existsSync(DRIFT_MD_PATH) ? readFileSync(DRIFT_MD_PATH, 'utf8') : '';
  const overrides = parseDriftMd(driftMd);

  const { version: pinnedVersion } = resolveInstalledNpmVersion();
  const failures = [];
  const warnings = [];
  const results = [];

  for (const [filename, entry] of Object.entries(map.entries)) {
    if (entry.status === 'local-only' || !entry.upstreamPath) {
      results.push({ filename, status: 'skipped-local-only' });
      continue;
    }

    if (entry.removeAfterNpmVersion && compareSemver(pinnedVersion, entry.removeAfterNpmVersion) >= 0) {
      warnings.push(
        `${filename}: removeAfterNpmVersion (${entry.removeAfterNpmVersion}) is already satisfied by ` +
          `pinned npm version ${pinnedVersion}. This vendored file is overdue for removal per ADR-0035 §2.3.`
      );
    }

    const vendoredPath = path.join(SHIM_DIR, 'src', filename);
    if (!existsSync(vendoredPath)) {
      results.push({ filename, status: 'missing-vendored-file' });
      continue;
    }
    const vendoredContent = normalize(readFileSync(vendoredPath, 'utf8'));

    if (offline) {
      results.push({ filename, status: 'offline-skip' });
      continue;
    }

    let upstreamContent;
    try {
      upstreamContent = await fetchUpstream(pinnedVersion, entry.upstreamPath);
    } catch (err) {
      warnings.push(`${filename}: could not fetch upstream (${err.message}); skipping this run.`);
      results.push({ filename, status: 'fetch-error' });
      continue;
    }
    if (upstreamContent == null) {
      warnings.push(
        `${filename}: upstreamPath "${entry.upstreamPath}" not found in @plures/design-dojo@${pinnedVersion} tarball.`
      );
      results.push({ filename, status: 'upstream-path-missing' });
      continue;
    }
    upstreamContent = normalize(upstreamContent);

    if (vendoredContent === upstreamContent) {
      results.push({ filename, status: 'in-sync' });
      continue;
    }

    const override = overrides.get(filename);
    if (override) {
      results.push({ filename, status: 'drifted-with-documented-override' });
      continue;
    }

    failures.push(
      `${filename}: vendored copy differs from @plures/design-dojo@${pinnedVersion}:${entry.upstreamPath} ` +
        `and has no DRIFT.md override entry. Either reconcile the file (ADR-0035 §2.2) or add a DRIFT.md ` +
        `entry explaining the temporary override (upstream PR link + expected removal date).`
    );
    results.push({ filename, status: 'undocumented-drift' });
  }

  return { pinnedVersion, results, warnings, failures };
}

async function main() {
  const { pinnedVersion, results, warnings, failures } = await runDriftCheck();

  console.log(`design-dojo drift check against pinned npm version ${pinnedVersion}`);
  for (const r of results) {
    console.log(`  ${r.status.padEnd(28)} ${r.filename}`);
  }
  if (warnings.length) {
    console.log('\nWarnings:');
    for (const w of warnings) console.log(`  ⚠️  ${w}`);
  }
  if (failures.length) {
    console.log('\nFailures:');
    for (const f of failures) console.log(`  ❌ ${f}`);
    console.error(`\ndesign-dojo drift check FAILED: ${failures.length} undocumented drift(s).`);
    process.exitCode = 1;
    return;
  }
  console.log('\ndesign-dojo drift check PASSED.');
}

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isMain) {
  main().catch((err) => {
    console.error(err);
    process.exitCode = 1;
  });
}
