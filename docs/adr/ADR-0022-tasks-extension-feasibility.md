# ADR-0022: MCP Tasks extension feasibility for `radix__dev-lifecycle` orchestration

- **Status:** Proposed
- **Date:** 2026-07-29
- **Decision scope:** Proposal #4 of `plures-org:mcp-protocol-3.0-adoption`
  (`memory/mcp-3.0-research.md` Part 4)
- **Related:** ADR-0020 (Canvas elicitation feasibility), ADR-0021 (MCP cache-hints feasibility) -
  same "SDK doesn't support it yet, don't fake it" outcome pattern

## Context

Proposal #4 asks: replace ad hoc subagent-polling orchestration for
`radix__dev-lifecycle` stage runs (analyze -> fix -> test -> deploy -> verify) with
protocol-native task handles via the MCP **Tasks extension**
(`tasks/get`, `tasks/update`, `tasks/cancel`), once the extension finalizes and an
SDK ships support.

Two things needed verification before any implementation: (1) does
`packages/mcp-dev-server` currently expose anything resembling a `dev-lifecycle`
tool or long-running task surface, and (2) does the currently available MCP
TypeScript SDK support the Tasks extension.

**Verified against the actual code**, `packages/mcp-dev-server/src/index.ts`
(1363 lines) has:

- No tool named `dev-lifecycle`, `dev_lifecycle`, or anything under a
  `radix__dev-lifecycle` prefix. `git grep -in "dev-lifecycle"` inside
  `packages/mcp-dev-server/` returns zero matches.
- 40 registered tools total (`db.*`, `canvas.*`, `app.*`, `praxis.*`,
  `chronos.*`, `plugin.*`, `task.handoff.*`) - none of them long-running; every
  handler in `callTool()` (around line 1240) executes synchronously and
  returns immediately, no `setTimeout`/`Promise`-based deferred execution
  except an unrelated debounce timer for DB persistence (`persistTimer`,
  line ~90).
- `handleRequest()` (line ~1256) implements exactly three JSON-RPC methods:
  `initialize`, `notifications/initialized`, `tools/list`, `tools/call`. There
  is no `tasks/get`, `tasks/update`, `tasks/cancel`, or any polling/handle
  concept in the transport at all.
- The actual `dev-lifecycle` orchestration that exists today
  (`STAGE_ORDER = ['analyze','fix','test','deploy','verify']`,
  `~/.openclaw/workspace/scripts/dev-lifecycle.mjs`) is **not** part of
  pares-radix's MCP surface. It is a main-session-side executor that spawns
  OpenClaw subagents per stage and calls a `.px` procedure
  (`praxis/procedures/dev-lifecycle.px`) for gating - it never goes through
  `mcp-dev-server`'s JSON-RPC loop. So "adopt Tasks for
  `radix__dev-lifecycle` stage runs" is not a small patch to an existing tool;
  it would require inventing a new dev-lifecycle *tool* inside
  `mcp-dev-server` first, then giving that new tool a task-handle lifecycle -
  two unbuilt things, not one.

On the SDK side: `npm view @modelcontextprotocol/sdk version` returns
`1.29.0` (checked live, 2026-07-29). The MCP TypeScript SDK's own `v2`
branch/release notes state it passes the official conformance suite "except
the tasks suite: tasks moved to an extension in 2026-07-28, and we aim to
ship support for it with the stable release or soon after"
(`github.com/modelcontextprotocol/typescript-sdk` releases page, checked
live). So the Tasks extension (SEP-2663, Final status per
`modelcontextprotocol.io/seps/2663-tasks-extension`) is standardized, but
**no published TypeScript SDK version implements it yet**. `mcp-dev-server`
also doesn't depend on `@modelcontextprotocol/sdk` at all today - it
hand-rolls the transport, same fact already recorded in ADR-0020/ADR-0021.

## Decision

Do **not** implement Tasks-extension support (`tasks/get`, `tasks/update`,
`tasks/cancel`) in `mcp-dev-server` at this time, and do **not** invent a new
`dev-lifecycle` MCP tool as a vehicle for it.

Two independent blockers exist, either of which alone would defer this:

1. No TypeScript MCP SDK release implements the Tasks extension yet - there is
   nothing to build the task-handle plumbing against without hand-rolling a
   second protocol extension on top of an already-hand-rolled transport
   (compounding the same debt ADR-0020/ADR-0021 already flagged).
2. `mcp-dev-server` has no `dev-lifecycle` tool to attach task semantics to.
   The real dev-lifecycle orchestrator lives outside the MCP boundary
   entirely, in the OpenClaw main-session executor
   (`scripts/dev-lifecycle.mjs` + `praxis/procedures/dev-lifecycle.px`).
   Exposing that orchestrator as an MCP tool is a separate, unscoped design
   decision (what does `tools/call` on "run a dev-lifecycle stage" even mean
   from a stateless JSON-RPC server that has no subagent-spawning
   capability?) that proposal #4 did not actually ask to resolve and that
   this ADR does not resolve either.

## Consequences

### Positive

- No wasted implementation against an SDK surface that doesn't exist and
  would have to be re-done once the real Tasks extension ships.
- Avoids inventing a `dev-lifecycle` MCP tool speculatively, before there is
  a concrete need for external MCP clients (not just this OpenClaw session)
  to drive dev-lifecycle stages.
- Keeps the "SDK adoption + protocol migration is one testable boundary, not
  N incompatible partial migrations" principle from ADR-0021 intact for a
  third proposal in a row.

### Negative

- Proposal #4 remains unimplemented; subagent-polling orchestration continues
  as the only mechanism for dev-lifecycle stage tracking.
- Other MCP clients (besides the OpenClaw main session) still have no
  standard way to observe or drive a long-running dev-lifecycle run.

## Acceptance criteria for a later implementation

1. A released TypeScript MCP SDK (or the SDK's `v2` stable line) implements
   `tasks/get`, `tasks/update`, `tasks/cancel` per the Tasks extension
   (SEP-2663).
2. `mcp-dev-server` adopts that SDK for its transport (superseding the
   hand-rolled JSON-RPC loop), consistent with the transport-rework
   precondition already recorded in ADR-0020.
3. A `dev-lifecycle` MCP tool is explicitly designed and scoped - deciding
   what a task handle represents (one stage? the whole run?), how retries
   and gate escalation map to `tasks/update` semantics, and whether the
   existing `praxis/procedures/dev-lifecycle.px` gate logic is called from
   the tool handler or stays external.
4. Integration tests prove a real long-running dev-lifecycle stage can be
   started via `tools/call`, polled via `tasks/get`, and cancelled via
   `tasks/cancel` without losing gate state.

## Evidence

- `packages/mcp-dev-server/src/index.ts`: `git grep -in "dev-lifecycle"` = 0
  matches; `handleRequest()` implements only `initialize`,
  `notifications/initialized`, `tools/list`, `tools/call`; 40 synchronous
  tool handlers, no deferred/task-handle execution path.
- `npm view @modelcontextprotocol/sdk version` = `1.29.0` (checked
  2026-07-29).
- `github.com/modelcontextprotocol/typescript-sdk` releases page: v2 passes
  conformance "except the tasks suite... aim to ship support ... with the
  stable release or soon after" (checked live 2026-07-29).
- `modelcontextprotocol.io/seps/2663-tasks-extension`: SEP-2663 status
  `Final`, Extensions Track (checked live 2026-07-29).
- `~/.openclaw/workspace/scripts/dev-lifecycle.mjs` +
  `praxis/procedures/dev-lifecycle.px`: the actual dev-lifecycle executor,
  confirmed to run outside `mcp-dev-server`'s process/transport.
