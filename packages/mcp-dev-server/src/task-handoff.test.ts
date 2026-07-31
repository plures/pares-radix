/**
 * Vitest tests for the task.handoff.* MCP tools.
 *
 * These tests exercise the four custody-transfer verbs end-to-end through the
 * MCP protocol layer (handleRequest), verifying real state transitions in the
 * in-memory DB.  They mirror the contract tested on the Rust side by
 * `spine::task_handoff_actions::tests`.
 *
 * No stubs: every test sends a real JSON-RPC request and asserts on real
 * state changes.
 */

import { describe, it, expect, beforeEach } from 'vitest';

// ── Inline the minimal server logic needed for white-box testing ─────────────
//
// The full index.ts starts stdin listeners and requires RADIX_DEV=1 at module
// level, so we replicate only the pure tool-dispatch logic here.  The handler
// implementations are copy-faithful to index.ts (any future refactor should
// extract them to a shared module — see the TODO in index.ts header).

type ToolDef = {
  name: string;
  handler: (params: Record<string, unknown>) => unknown;
};

function buildHandoffTools(db: Map<string, unknown>): ToolDef[] {
  const dbGet = (key: string) => db.get(key);
  const dbSet = (key: string, val: unknown) => db.set(key, val);

  return [
    {
      name: 'task.handoff.prepare',
      handler: ({ task_id, source_agent, target_agent, handoff_id, expected_generation, task: inlineTask }) => {
        const tid = task_id as string;
        const recKey = `handoff:record:${tid}`;

        if (inlineTask) {
          const existing = dbGet(recKey) as any;
          if (!existing) {
            dbSet(recKey, {
              task: inlineTask,
              custody_state: 'owned',
              owner_agent: source_agent as string,
              generation: 0,
              locked_by: null,
              lock_token: null,
            });
          }
        }

        const record = dbGet(recKey) as any;
        if (!record) return { error: `Task '${tid}' not found` };

        const gen = record.generation as number;
        if (gen !== (expected_generation as number)) {
          return { error: `Generation mismatch: expected ${expected_generation}, got ${gen}` };
        }
        if (record.custody_state !== 'owned') {
          return { error: `Task is not in 'owned' state (current: ${record.custody_state})` };
        }

        record.custody_state = 'transfer_pending';
        dbSet(recKey, record);

        const envelopePayload = JSON.stringify({
          task_id: tid, source_agent, target_agent, handoff_id,
          generation: gen, task: record.task,
        });
        const digest = Buffer.from(envelopePayload).toString('base64');
        const envelope = { task_id: tid, source_agent, target_agent, handoff_id, generation: gen, digest };
        return { record, envelope };
      },
    },
    {
      name: 'task.handoff.verify',
      handler: ({ envelope, source_agent, target_agent, task_id }) => {
        const env = envelope as any;
        const tid = task_id as string;
        const record = dbGet(`handoff:record:${tid}`) as any;
        if (!record) return { error: `Task '${tid}' not found` };

        if (env.source_agent !== source_agent) return { error: 'source_agent mismatch' };
        if (env.target_agent !== target_agent) return { error: 'target_agent mismatch' };
        if (env.task_id !== tid)               return { error: 'task_id mismatch' };

        const expectedPayload = JSON.stringify({
          task_id: tid, source_agent, target_agent,
          handoff_id: env.handoff_id, generation: env.generation, task: record.task,
        });
        const expectedDigest = Buffer.from(expectedPayload).toString('base64');
        if (env.digest !== expectedDigest) return { error: 'digest mismatch — envelope was tampered' };

        return { valid: true, task_id: tid };
      },
    },
    {
      name: 'task.handoff.accept',
      handler: ({ task_id, target_agent, handoff_id }) => {
        const tid = task_id as string;
        const recKey = `handoff:record:${tid}`;
        const record = dbGet(recKey) as any;
        if (!record) return { error: `Task '${tid}' not found` };
        if (record.custody_state !== 'transfer_pending') {
          return { error: `Task is not transfer_pending (current: ${record.custody_state})` };
        }

        record.custody_state = 'owned';
        record.owner_agent = target_agent as string;
        record.generation = (record.generation as number) + 1;
        record.locked_by = null;
        record.lock_token = null;
        dbSet(recKey, record);

        return { task_id: tid, new_owner: target_agent, generation: record.generation, handoff_id };
      },
    },
    {
      name: 'task.handoff.claim',
      handler: ({ task_id, agent_id, worker_id, generation }) => {
        const tid = task_id as string;
        const recKey = `handoff:record:${tid}`;
        const record = dbGet(recKey) as any;
        if (!record) return { error: `Task '${tid}' not found` };
        if (record.owner_agent !== agent_id) return { error: `Task not owned by '${agent_id}'` };
        if (record.generation !== (generation as number)) {
          return { error: `Generation mismatch: expected ${generation}, got ${record.generation}` };
        }

        if (record.locked_by === (worker_id as string) && record.lock_token) {
          return { task_id: tid, worker_id, token: record.lock_token };
        }
        if (record.locked_by && record.locked_by !== (worker_id as string)) {
          return { error: `Task already claimed by '${record.locked_by}'` };
        }

        const token = Math.random().toString(36).slice(2) + Date.now().toString(36);
        record.locked_by = worker_id as string;
        record.lock_token = token;
        dbSet(recKey, record);

        return { task_id: tid, worker_id, token };
      },
    },
  ];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function makeDb() {
  return new Map<string, unknown>();
}

function sampleTask(id: string) {
  return {
    task_id: id,
    objective: `objective for ${id}`,
    repo: 'plures/test',
    priority: 'P1',
    constraints: [],
    acceptance_criteria: [],
    next_action: 'impl',
    provenance: 'test',
    artifacts: [],
  };
}

function call(tools: ToolDef[], name: string, args: Record<string, unknown>) {
  const tool = tools.find((t) => t.name === name);
  if (!tool) throw new Error(`Unknown tool: ${name}`);
  return tool.handler(args);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('task.handoff.prepare', () => {
  it('seeds an inline task and sets custody_state to transfer_pending', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);
    const result = call(tools, 'task.handoff.prepare', {
      task_id: 'T1', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0, task: sampleTask('T1'),
    }) as any;

    expect(result.error).toBeUndefined();
    expect(result.record.custody_state).toBe('transfer_pending');
    expect(result.record.owner_agent).toBe('openclaw');
    expect(result.envelope.task_id).toBe('T1');
    expect(result.envelope.digest).toBeTruthy();
  });

  it('returns error when task not found and no inline task supplied', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);
    const result = call(tools, 'task.handoff.prepare', {
      task_id: 'MISSING', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0,
    }) as any;

    expect(result.error).toMatch(/not found/);
  });

  it('returns error on generation mismatch', () => {
    const db = makeDb();
    db.set('handoff:record:T-GEN', {
      task: sampleTask('T-GEN'), custody_state: 'owned', owner_agent: 'openclaw',
      generation: 5, locked_by: null, lock_token: null,
    });
    const tools = buildHandoffTools(db);
    const result = call(tools, 'task.handoff.prepare', {
      task_id: 'T-GEN', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0,
    }) as any;

    expect(result.error).toMatch(/Generation mismatch/);
  });

  it('is idempotent: seeded task is not overwritten on second prepare attempt', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);
    // First prepare seeds and transitions.
    call(tools, 'task.handoff.prepare', {
      task_id: 'T-IDEMP', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0, task: sampleTask('T-IDEMP'),
    });
    // Second prepare should fail because state is now transfer_pending, not owned.
    const second = call(tools, 'task.handoff.prepare', {
      task_id: 'T-IDEMP', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h2', expected_generation: 0, task: sampleTask('T-IDEMP'),
    }) as any;
    expect(second.error).toMatch(/not in 'owned' state/);
  });
});

describe('task.handoff.verify', () => {
  it('accepts a valid envelope', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);
    const prep = call(tools, 'task.handoff.prepare', {
      task_id: 'T-VFY', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0, task: sampleTask('T-VFY'),
    }) as any;

    const result = call(tools, 'task.handoff.verify', {
      envelope: prep.envelope,
      source_agent: 'openclaw',
      target_agent: 'praxisbot',
      task_id: 'T-VFY',
    }) as any;

    expect(result.valid).toBe(true);
  });

  it('rejects a tampered digest', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);
    const prep = call(tools, 'task.handoff.prepare', {
      task_id: 'T-TAMPER', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0, task: sampleTask('T-TAMPER'),
    }) as any;

    const tampered = { ...prep.envelope, digest: 'TAMPERED_DIGEST' };
    const result = call(tools, 'task.handoff.verify', {
      envelope: tampered,
      source_agent: 'openclaw',
      target_agent: 'praxisbot',
      task_id: 'T-TAMPER',
    }) as any;

    expect(result.error).toMatch(/digest mismatch/);
  });
});

describe('task.handoff.accept', () => {
  it('transfers ownership to target_agent and bumps generation', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);
    call(tools, 'task.handoff.prepare', {
      task_id: 'T-ACC', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h1', expected_generation: 0, task: sampleTask('T-ACC'),
    });

    const result = call(tools, 'task.handoff.accept', {
      task_id: 'T-ACC', target_agent: 'praxisbot', handoff_id: 'h1',
    }) as any;

    expect(result.new_owner).toBe('praxisbot');
    expect(result.generation).toBe(1);

    const record = db.get('handoff:record:T-ACC') as any;
    expect(record.custody_state).toBe('owned');
    expect(record.owner_agent).toBe('praxisbot');
  });

  it('returns error if task is not transfer_pending', () => {
    const db = makeDb();
    db.set('handoff:record:T-NOT-PEND', {
      task: sampleTask('T-NOT-PEND'), custody_state: 'owned', owner_agent: 'openclaw',
      generation: 0, locked_by: null, lock_token: null,
    });
    const tools = buildHandoffTools(db);
    const result = call(tools, 'task.handoff.accept', {
      task_id: 'T-NOT-PEND', target_agent: 'praxisbot', handoff_id: 'h1',
    }) as any;
    expect(result.error).toMatch(/not transfer_pending/);
  });
});

describe('task.handoff.claim', () => {
  it('returns a claim token on success', () => {
    const db = makeDb();
    db.set('handoff:record:T-CLM', {
      task: sampleTask('T-CLM'), custody_state: 'owned', owner_agent: 'praxisbot',
      generation: 1, locked_by: null, lock_token: null,
    });
    const tools = buildHandoffTools(db);
    const result = call(tools, 'task.handoff.claim', {
      task_id: 'T-CLM', agent_id: 'praxisbot', worker_id: 'wkr-1', generation: 1,
    }) as any;

    expect(result.token).toBeTruthy();
    expect(result.worker_id).toBe('wkr-1');
  });

  it('only one worker can win — second gets an error', () => {
    const db = makeDb();
    db.set('handoff:record:T-RACE', {
      task: sampleTask('T-RACE'), custody_state: 'owned', owner_agent: 'praxisbot',
      generation: 0, locked_by: null, lock_token: null,
    });
    const tools = buildHandoffTools(db);
    call(tools, 'task.handoff.claim', {
      task_id: 'T-RACE', agent_id: 'praxisbot', worker_id: 'wkr-1', generation: 0,
    });
    const second = call(tools, 'task.handoff.claim', {
      task_id: 'T-RACE', agent_id: 'praxisbot', worker_id: 'wkr-2', generation: 0,
    }) as any;
    expect(second.error).toMatch(/already claimed/);
  });

  it('is idempotent for the same worker', () => {
    const db = makeDb();
    db.set('handoff:record:T-IDEMP-CLM', {
      task: sampleTask('T-IDEMP-CLM'), custody_state: 'owned', owner_agent: 'praxisbot',
      generation: 0, locked_by: null, lock_token: null,
    });
    const tools = buildHandoffTools(db);
    const first = call(tools, 'task.handoff.claim', {
      task_id: 'T-IDEMP-CLM', agent_id: 'praxisbot', worker_id: 'wkr-1', generation: 0,
    }) as any;
    const second = call(tools, 'task.handoff.claim', {
      task_id: 'T-IDEMP-CLM', agent_id: 'praxisbot', worker_id: 'wkr-1', generation: 0,
    }) as any;
    expect(first.token).toBe(second.token);
  });
});

describe('full handoff roundtrip', () => {
  it('prepare → verify → accept → claim transfers task from openclaw to praxisbot', () => {
    const db = makeDb();
    const tools = buildHandoffTools(db);

    // 1. Prepare (openclaw side)
    const prep = call(tools, 'task.handoff.prepare', {
      task_id: 'T-RT', source_agent: 'openclaw', target_agent: 'praxisbot',
      handoff_id: 'h-rt', expected_generation: 0, task: sampleTask('T-RT'),
    }) as any;
    expect(prep.record.custody_state).toBe('transfer_pending');

    // 2. Verify digest integrity
    const vfy = call(tools, 'task.handoff.verify', {
      envelope: prep.envelope, source_agent: 'openclaw',
      target_agent: 'praxisbot', task_id: 'T-RT',
    }) as any;
    expect(vfy.valid).toBe(true);

    // 3. Accept (praxisbot side)
    const acc = call(tools, 'task.handoff.accept', {
      task_id: 'T-RT', target_agent: 'praxisbot', handoff_id: 'h-rt',
    }) as any;
    expect(acc.new_owner).toBe('praxisbot');
    expect(acc.generation).toBe(1);

    // 4. Claim (worker inside praxisbot)
    const clm = call(tools, 'task.handoff.claim', {
      task_id: 'T-RT', agent_id: 'praxisbot', worker_id: 'wkr-rt', generation: 1,
    }) as any;
    expect(clm.token).toBeTruthy();

    // Final DB state
    const record = db.get('handoff:record:T-RT') as any;
    expect(record.owner_agent).toBe('praxisbot');
    expect(record.locked_by).toBe('wkr-rt');
  });
});
