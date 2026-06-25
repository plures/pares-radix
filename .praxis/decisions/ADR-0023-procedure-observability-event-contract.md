# ADR-0023: Procedure Observability Event Contract (plures.proc.event.v1)

## Status: Accepted

## Date: 2026-06-25

## Context

Orchestrated procedures (the dev-lifecycle staged runs; and, going forward, the
pluresLM→orchestration engine that executes `.px` procedures) are a **black box to
their supervisor while they run.**

Concrete incident (2026-06-25): a staged dev-lifecycle run (analyze → fix → test →
verify) executed correctly, but the supervising human received **no signal for ~9
minutes** during a from-scratch verify build. The reasons are structural, not a
one-off:

- Subagent auto-announce is **push-on-completion only** — exactly one event when a
  child finishes.
- We (correctly) do **not** poll in a loop while waiting, because that burns tokens
  for nothing (AGENTS.md "Reactive, Not Polling").

So the default behavior of any long orchestrated run is **silence until done.** For
multi-minute compiles inside a multi-stage pipeline, that is unacceptable
supervision ergonomics and it actively erodes trust ("is it stuck or working?").

The orchestration design already has the raw material to fix this:
- The `.px` language already has an `emit` primitive (used in `task-management.px`).
- `PRAXIS-LOGIC-COPROCESSOR-PLURESLM.md` already specifies procedures emitting
  events to the procedure layer (`praxis-analysis-ready`).
- `RADIX-ORCHESTRATION.md` already lists, as an unowned backlog line, *"Procedure
  log streaming (tail procedure logs in real-time)."*

Observability is therefore **not a new subsystem.** It is a missing, well-specified
set of lifecycle events plus a subscriber that relays them. This ADR makes that
contract binding so the in-flight engine designs it in rather than retrofitting it.

## Decision

**Every procedure run emits `plures.proc.event.v1` lifecycle events to a reserved
PluresDB channel, and a pluggable relay forwards phase changes to a transport
(chat / TUI tail / webhook / none). A procedure that does meaningful work and emits
nothing is, by this contract, incomplete.**

### Event channel

PluresDB writes under a reserved prefix (ordinary writes → automatically durable,
replayable via Chronos, and subscribable via the existing `subscribe` API):

```
proc.event:{run_id}:{seq}
```

### Event schema (`plures.proc.event.v1`)

```jsonc
{
  "schema": "plures.proc.event.v1",
  "run_id": "string",     // procedure run id (engine-assigned, survives restart)
  "proc":   "string",     // procedure name, e.g. "dev-lifecycle"
  "task_id":"string|null", // domain id when applicable
  "stage":  "string|null", // stage/step name when applicable
  "kind":   "started|progress|completed|failed|blocked|heartbeat",
  "seq":    0,             // monotonic per run (gap-detectable)
  "ts":     0,             // unix ms
  "pct":    null,          // optional coarse 0-100
  "detail": "string",     // human one-liner
  "data":   {}            // optional structured payload (counts, exit codes, paths)
}
```

`kind` semantics:
- `started` — procedure/stage began. MUST emit at entry.
- `progress` — incremental checkpoint. SHOULD emit at natural points; for long
  external commands the `run_command` action SHOULD translate stdout milestones
  (e.g. cargo `Compiling n/total`, `Finished`, linker phase) into `progress`.
- `heartbeat` — liveness ping for long silent work (≤ every N s); lets a supervisor
  distinguish *working* from *stalled*.
- `completed` / `failed` / `blocked` — terminal for the stage/procedure, `data`
  carrying exit status / gate result.

### `.px` author surface (no new grammar)

```px
emit {
  channel: "proc.event",
  kind: "started",
  proc: "dev-lifecycle",
  task_id: $task_id,
  stage: $stage_name,
  detail: "stage started"
}
```

The engine stamps `run_id`, `seq`, `ts` and persists to
`proc.event:{run_id}:{seq}`. Authors supply only semantic fields.

### Engine (action-handler / IO) responsibilities

1. **Stamp + persist** each `emit { channel: "proc.event", … }`.
2. **Command-output translation** in `run_command`: parse known progress markers →
   synthesize `progress` events (this is where the "9-minute-blind" problem is
   actually solved).
3. **Heartbeat** while a single action runs longer than `heartbeat_interval`
   (default 30 s) with no other event.
4. **Relay (pluggable)**: a standing subscriber bridges `proc.event:*` to a
   transport, throttled to phase changes (coalesce `progress`, forward
   started/completed/failed). Turning it off MUST NOT change procedure behavior.

### MCP surface

- `radix__procedure-status(run_id)` → latest event.
- `radix__procedure-tail(run_id)` → stream the channel (the
  RADIX-ORCHESTRATION "tail procedure logs in real-time" line).

### Non-invasiveness invariant

With relay transport = `none`, procedure outcomes MUST be byte-identical to relay
= `telegram`. Observability never alters logic. (Aligns with C-TEST-002: never
depend on a single adapter; the channel is transport-agnostic.)

## Stopgap (already shipped, forward-validates this contract)

Until the engine reserves `proc.event:*`, the workspace dev-lifecycle driver
(`scripts/dev-lifecycle.mjs`) emits the **same `plures.proc.event.v1` records** to a
per-task milestone file (`memory/dev-lifecycle-runs/<taskId>.events.jsonl`) on every
state transition, and `formatProgress(taskId, afterSeq)` yields incremental
phase-change lines the agent relays between `sessions_yield`s. The ONLY difference
from the target design is the transport (a JSONL file instead of the PluresDB
prefix). When the engine ships, the file-append is replaced by a real `emit` +
`proc.event:*` subscriber — **the event records are identical**, so the stopgap is
forward-compatible, not throwaway.

## Consequences

### Positive
- Long orchestrated runs become observable: the supervisor sees phase changes and
  liveness without polling.
- Stall detection (RADIX-ORCHESTRATION Phase 6) becomes a subscriber on
  `proc.event:*` (`heartbeat` gaps), not a separate polling loop.
- Any procedure gets observability for free by emitting; no per-procedure plumbing.
- One schema spans the stopgap and the engine → no rework when the engine lands.

### Negative
- Engine must reserve/manage the `proc.event:*` prefix and a standing subscriber.
- `run_command` stdout→`progress` translation is per-tool heuristic (acceptable; the
  common cases — cargo/npm/linker — cover most real waits).

### Risks
- Event spam if `progress` is emitted too eagerly (mitigated: relay throttles to
  phase changes; `progress` coalesced).
- Relay failure must never break a run (mitigated: emission is best-effort; the
  stopgap already swallows all errors on the observability path).

## Evidence

| Observation | Tested? | Source |
|-------------|---------|--------|
| Supervisor blind ~9 min during verify build | Yes | 2026-06-25 incident, memory/2026-06-25.md |
| `.px` already has `emit` primitive | Yes | praxis/procedures/task-management.px |
| Coprocessor design already emits events to procedure layer | Yes | design/PRAXIS-LOGIC-COPROCESSOR-PLURESLM.md (`praxis-analysis-ready`) |
| "log streaming" already a backlog line | Yes | design/RADIX-ORCHESTRATION.md Phase 5 |
| Stopgap emits well-formed v1 events on every transition | Yes | dev-lifecycle.mjs; smoke: 12 events, schema-valid, seq-monotonic, terminal `completed` present |
| Stopgap is non-invasive (driver still green) | Yes | scripts/dev-lifecycle.test.mjs — 41/41 pass (was 31; +10 event/relay assertions) |
| `formatProgress` relays incrementally without dupes | Yes | Test 8: nextSeq advances; second call from nextSeq empty |

## References

- design/OBSERVABILITY-EVENT-CONTRACT.md (the full contract this ADR ratifies) —
  development-guide @ 76d3c1f
- design/RADIX-ORCHESTRATION.md — Phase 5 (procedure mgmt + tail) / Phase 6 (health)
- design/PRAXIS-LOGIC-COPROCESSOR-PLURESLM.md — event-emission handshake
- design/PLURESDB-NATIVE-PROCEDURES.md — AgensRuntime subscribe/emit
- C-PLURES-003 (all persistent state via PluresDB), C-TEST-002 (no single-adapter
  dependency), AGENTS.md "Reactive, Not Polling"
- Stopgap implementation: workspace `scripts/dev-lifecycle.mjs`
  (`emitEvent`/`readEvents`/`formatProgress`) + `scripts/dev-lifecycle.test.mjs`
