/**
 * Verifies MCP spec 2025-11-25 (SEP-1303) error classification: tool
 * argument-validation failures and handler exceptions must come back as
 * tool-execution errors (`result.isError === true`) so a calling model can
 * self-correct within the conversation, while genuine protocol-level
 * failures (unknown tool name, malformed request shape, unknown method)
 * must still surface as JSON-RPC `error` objects.
 *
 * `handleRequest` is not exported (the module is a stdio-transport entry
 * point that self-executes on import), so this suite spawns the server as a
 * child process and talks newline-delimited JSON-RPC over stdio — the same
 * transport a real MCP client uses.
 */
import { describe, expect, it, beforeAll, afterAll } from 'vitest';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const SERVER_PATH = join(process.cwd(), 'src', 'index.ts');

let child: ChildProcessWithoutNullStreams;
let dbDir: string;
let nextId = 1;
let stdoutBuf = '';
const pending = new Map<number, (msg: any) => void>();

function send(method: string, params?: Record<string, unknown>): Promise<any> {
  const id = nextId++;
  const payload = { jsonrpc: '2.0', id, method, params };
  return new Promise((resolve) => {
    pending.set(id, resolve);
    child.stdin.write(`${JSON.stringify(payload)}\n`);
  });
}

beforeAll(async () => {
  dbDir = mkdtempSync(join(tmpdir(), 'radix-mcp-test-'));
  child = spawn('npx', ['tsx', SERVER_PATH], {
    env: { ...process.env, RADIX_DEV: '1', RADIX_DB_PATH: join(dbDir, 'db.json') },
    stdio: ['pipe', 'pipe', 'pipe'],
    shell: true,
  });

  child.stdout.on('data', (chunk: Buffer) => {
    stdoutBuf += chunk.toString('utf-8');
    let idx: number;
    while ((idx = stdoutBuf.indexOf('\n')) !== -1) {
      const line = stdoutBuf.slice(0, idx).trim();
      stdoutBuf = stdoutBuf.slice(idx + 1);
      if (!line) continue;
      let msg: any;
      try {
        msg = JSON.parse(line);
      } catch {
        continue;
      }
      const resolve = pending.get(msg.id);
      if (resolve) {
        pending.delete(msg.id);
        resolve(msg);
      }
    }
  });

  // Wait for the server's stderr banner so we know it's ready before sending.
  await new Promise<void>((resolve) => {
    const onData = (chunk: Buffer) => {
      if (chunk.toString('utf-8').includes('radix-mcp-dev server started')) {
        child.stderr.off('data', onData);
        resolve();
      }
    };
    child.stderr.on('data', onData);
  });

  await send('initialize', {});
}, 30_000);

afterAll(() => {
  child.kill();
  rmSync(dbDir, { recursive: true, force: true });
});

describe('MCP tool error classification (SEP-1303)', () => {
  it('returns isError:true tool-result (not a protocol error) for missing required args', async () => {
    const res = await send('tools/call', { name: 'db.get', arguments: {} });
    expect(res.error).toBeUndefined();
    expect(res.result).toBeDefined();
    expect(res.result.isError).toBe(true);
    expect(res.result.content[0].text).toContain('Invalid arguments');
    expect(res.result.content[0].text.toLowerCase()).toContain('key');
  });

  it('returns isError:true tool-result for wrong-typed args', async () => {
    const res = await send('tools/call', { name: 'db.get', arguments: { key: 123 } });
    expect(res.error).toBeUndefined();
    expect(res.result.isError).toBe(true);
    expect(res.result.content[0].text).toContain('Invalid arguments');
  });

  it('returns isError:true tool-result for handler exceptions on otherwise-valid args', async () => {
    // canvas.import with a non-JSON string triggers a thrown exception inside
    // the handler (JSON.parse failure) — this must ALSO be a tool-result
    // error, not an unhandled protocol failure that crashes the request.
    const res = await send('tools/call', { name: 'canvas.import', arguments: { json: 'not json {' } });
    expect(res.error).toBeUndefined();
    expect(res.result).toBeDefined();
    expect(res.result.isError).toBe(true);
  });

  it('succeeds (no isError) with valid arguments', async () => {
    const res = await send('tools/call', { name: 'db.put', arguments: { key: 'test:foo', value: { a: 1 } } });
    expect(res.error).toBeUndefined();
    expect(res.result.isError).toBeUndefined();
    const parsed = JSON.parse(res.result.content[0].text);
    expect(parsed.ok).toBe(true);
  });

  it('surfaces unknown tool name as a genuine JSON-RPC protocol error', async () => {
    const res = await send('tools/call', { name: 'does.not.exist', arguments: {} });
    expect(res.result).toBeUndefined();
    expect(res.error).toBeDefined();
    expect(res.error.code).toBe(-32601);
    expect(res.error.message).toContain('does.not.exist');
  });

  it('surfaces malformed tools/call request (missing name) as a protocol error', async () => {
    const res = await send('tools/call', { arguments: {} });
    expect(res.result).toBeUndefined();
    expect(res.error).toBeDefined();
    expect(res.error.code).toBe(-32602);
  });

  it('surfaces unknown method as a protocol error', async () => {
    const res = await send('not/a/real/method', {});
    expect(res.result).toBeUndefined();
    expect(res.error).toBeDefined();
    expect(res.error.code).toBe(-32601);
  });
});
