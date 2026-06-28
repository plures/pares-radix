#!/usr/bin/env node
/**
 * MCP stdio JSON-RPC smoke test — exercises the REAL surface.
 *
 * Spawns the radix-mcp dev server (tsx src/index.ts) the same way any MCP
 * client (OpenClaw, Claude Desktop, Cursor) would, sends a JSON-RPC
 * `initialize` then `tools/list`, and asserts well-formed responses over
 * stdio. No mocks, no CLI, no SSH — this is the server's native transport.
 *
 * Exit 0 = MCP server is runnable and speaks JSON-RPC. Non-zero = broken.
 */
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const entry = join(here, 'src', 'index.ts');

const child = spawn('npx', ['tsx', entry], {
  cwd: here,
  env: { ...process.env, RADIX_DEV: '1' },
  stdio: ['pipe', 'pipe', 'inherit'],
  shell: process.platform === 'win32',
});

let out = '';
child.stdout.setEncoding('utf-8');
child.stdout.on('data', (d) => { out += d; });

const fail = (msg) => { console.error(`SMOKE FAIL: ${msg}`); child.kill('SIGTERM'); process.exit(1); };
const timer = setTimeout(() => fail('no complete response within 20s'), 20000);

child.on('error', (e) => fail(`spawn error: ${e.message}`));

// Send initialize, then tools/list (newline-delimited JSON-RPC).
child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'smoke', version: '1.0.0' } } }) + '\n');
child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' }) + '\n');

const deadline = Date.now() + 18000;
const tick = setInterval(() => {
  const lines = out.split('\n').filter((l) => l.trim());
  const responses = [];
  for (const l of lines) { try { responses.push(JSON.parse(l)); } catch { /* partial */ } }
  const init = responses.find((r) => r.id === 1);
  const list = responses.find((r) => r.id === 2);
  if (init && list) {
    clearInterval(tick); clearTimeout(timer);
    if (init.result?.serverInfo?.name !== 'radix-mcp-dev') fail(`bad initialize result: ${JSON.stringify(init)}`);
    const tools = list.result?.tools;
    if (!Array.isArray(tools) || tools.length === 0) fail(`tools/list returned no tools: ${JSON.stringify(list)}`);
    console.log(`SMOKE OK: initialize -> ${init.result.serverInfo.name}, tools/list -> ${tools.length} tools`);
    child.kill('SIGTERM'); process.exit(0);
  }
  if (Date.now() > deadline) fail('responses incomplete');
}, 250);
