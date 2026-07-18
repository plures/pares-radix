#!/usr/bin/env node
// Runs the Tauri CLI with NAPI_RS_NATIVE_LIBRARY_PATH forced to the resolved
// native binding. Works around a NAPI-RS auto-loader failure under Node v26 +
// pnpm's virtual store, where the CLI's index.js resolves an index whose
// run/logError are undefined (symptom: "cli.logError is not a function").
// See: https://github.com/napi-rs/napi-rs (optional-dep resolution) and the
// documented NAPI_RS_NATIVE_LIBRARY_PATH escape hatch in the generated loader.
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const root = path.dirname(fileURLToPath(import.meta.url)) + path.sep + '..';

// Map current platform+arch to the NAPI binding package name the CLI publishes.
const { platform, arch } = process;
const triples = {
  'win32:x64': '@tauri-apps/cli-win32-x64-msvc',
  'win32:arm64': '@tauri-apps/cli-win32-arm64-msvc',
  'win32:ia32': '@tauri-apps/cli-win32-ia32-msvc',
  'darwin:x64': '@tauri-apps/cli-darwin-x64',
  'darwin:arm64': '@tauri-apps/cli-darwin-arm64',
  'linux:x64': '@tauri-apps/cli-linux-x64-gnu',
  'linux:arm64': '@tauri-apps/cli-linux-arm64-gnu',
};
const key = `${platform}:${arch}`;
const bindingPkg = triples[key];

const env = { ...process.env };
if (bindingPkg) {
  try {
    // Resolve the package's package.json, then locate the sibling .node file.
    const pkgJson = require.resolve(`${bindingPkg}/package.json`);
    const pkgDir = path.dirname(pkgJson);
    const { readdirSync } = require('node:fs');
    const nodeFile = readdirSync(pkgDir).find((f) => f.endsWith('.node'));
    if (nodeFile) {
      env.NAPI_RS_NATIVE_LIBRARY_PATH = path.join(pkgDir, nodeFile);
    }
  } catch {
    // Fall through: if the binding resolves normally, the CLI still works.
  }
}

// Locate the Tauri CLI entrypoint and run it with the forced binding.
const tauriBin = require.resolve('@tauri-apps/cli/tauri.js');
const res = spawnSync(process.execPath, [tauriBin, ...process.argv.slice(2)], {
  stdio: 'inherit',
  cwd: root,
  env,
});
process.exit(res.status ?? 1);
