# ADR-0020: Canvas Elicitation Feasibility — `mcp-dev-server` Cannot Support It Without a Transport Rework

**Status:** Accepted (design stage — no PR at this stage per dev-lifecycle gate; blocks proposal #2
from `memory/mcp-3.0-research.md` Part 4 on implementation, not on decision)
**Date:** 2026-07-29
**Deciders:** epic-orchestrator subagent, resolving proposal #2 (Canvas elicitation) for epic
`plures-org:mcp-protocol-3.0-adoption`
**Related:** ADR-0019 (dual-MCP-server boundary — resolves that these are not duplicates),
`memory/mcp-3.0-research.md` Part 4 proposal #2

## Context

Proposal #1 (error classification audit) is closed — already compliant, no PR needed. Proposal #2
asks: when `radix__canvas-*` validation fails on an enum-constrained prop (e.g. `Select.options`),
use MCP elicitation (`EnumSchema`, titled single/multi-select, 2025-11-25 spec) to ask the calling
agent for a corrected value instead of returning an opaque error string, without violating the
"dev-instrument for ONE running radix app process" boundary established in ADR-0019.

This ADR answers **whether and how**, based on reading the actual implementation, not assuming SDK
support.

## Evidence (read directly)

**Transport implementation — `packages/mcp-dev-server/src/index.ts` lines 1019-1114:**
- The server is a **hand-rolled, synchronous, request/response-only JSON-RPC loop over stdio** — it
  does **not** depend on `@modelcontextprotocol/sdk`. `package.json` has zero
  `@modelcontextprotocol/*` dependency; only `@plures/canvas-runtime`, `vitest`, `tsx`.
  `node_modules/@modelcontextprotocol` does not exist in this package.
- `handleRequest()` (line 1019) is a plain synchronous `switch` over `method`: `initialize`,
  `notifications/initialized`, `tools/list`, `tools/call`. There is **no code path that ever writes
  an unsolicited JSON-RPC request from server→client** (e.g. `elicitation/create`). The stdin handler
  (line 1087) only reacts to lines with an `id`, and only ever writes a `response` keyed to that
  same request's `id` (line 1102: `if (req.id !== undefined) { process.stdout.write(...) }`). There
  is no pending-request table, no correlation ID generator for server-originated calls, no way to
  suspend one `tools/call` and resume it later on a different inbound message.
- `initialize`'s declared `capabilities` (line 1030) is `{ tools: { listChanged: false } }` only —
  no `elicitation` capability negotiation exists in either direction (elicitation capability is
  normally declared by the **client**, and the server must be able to *check* for it before issuing
  an `elicitation/create` request — this server has no client-capabilities storage at all; `params`
  from `initialize` are read (`protocolVersion` line unused even) and discarded).
- Every tool `handler` (e.g. `toolCanvasValidate` call at line 342) is a **plain synchronous
  function returning a plain value** — `tool.handler(toolArgs)` (line 1057) is not awaited, has no
  `async`, and the surrounding `tools/call` case has no mechanism to pause mid-handler, emit a
  side-channel request, block on a correlated reply, then resume and produce the final
  `CallToolResult`. Retrofitting this requires making `tools/call` async, adding a pending-elicitation
  map keyed by a generated request ID, writing an `elicitation/create` request to `stdout` mid-handler,
  and having the stdin reader route replies with matching IDs back into that pending map instead of
  always treating inbound messages as top-level requests. This is a **transport-level change**, not
  a Canvas-side change.

**Validation implementation — confirms enum data exists, but is a plain string reporter:**
- `canvas-runtime/src/format.ts:312` `validateCanvas(doc)` returns `string[]` — free-text issue
  messages (`Missing meta.id`, `Node at ... missing type`, etc.), not structured
  `{ field, code, validValues }` records. It does **not** currently distinguish "enum-constrained
  prop got an invalid value" as its own case at all — the closest existing hook,
  `registry.ts` `PropSchema` (line 39+), does carry per-prop metadata (`name`, `type`, `required`,
  `bindable`, `default`) for components like `Select` (line 341: `options: Array<{value,label}>`),
  but `type` is a free-form TypeScript-like string (`'Array<{ value: string, label: string }>'`),
  **not a machine-checkable enum of the component's own valid *prop names or types***. There is no
  existing "this value must be one of these N options" constraint surfaced by `validateCanvas`
  today — it would need to be added as new validation logic before an elicitation prompt could even
  have an accurate enum list to offer.

## Analysis: does this violate the single-running-app dev-inspector boundary (ADR-0019)?

**No — bidirectional elicitation is orthogonal to the boundary question, not in conflict with it.**
ADR-0019 draws the line on *what tool surface* `mcp-dev-server` exposes (radix-internal
canvas/db/praxis/chronos/plugin introspection only, never general agent-runtime tools). Elicitation
is a *transport capability* — it changes how one existing tool call (`canvas.validate` or any
future `canvas.*` mutation) can prompt mid-call, not what state it touches or which process it
talks to. Adding elicitation support does not add new tool surface, does not reach outside the one
running radix app instance, and does not blur the boundary with `pares-agens/crates/mcp-server`.
**The boundary is not the blocker. The transport implementation is.**

## Decision

**Proposal #2 cannot be implemented as "genuinely small and fully specified."** Two independent
prerequisites are missing, either of which is a multi-day change on its own:

1. **Transport rework (the larger blocker):** `mcp-dev-server`'s hand-rolled JSON-RPC loop has no
   support for server-initiated mid-call requests in any direction — this is required by the
   *current stable* spec (2025-11-25, session-pinned request/response over the same stdio stream)
   and would be required differently again once the RC 2026-07-28 model (`InputRequiredResult` +
   opaque `requestState` + client re-issues the call) is adopted. Given the RC explicitly redesigns
   this exact mechanism (see `memory/mcp-3.0-research.md` Part 2A), implementing the *current*
   stable elicitation flow today risks building throwaway code once the RC's SDK support lands.
   **Recommendation: do not hand-roll either version. Wait for `@modelcontextprotocol/sdk`
   TypeScript server support to stabilize post-RC, then adopt the SDK (replacing the current
   hand-rolled JSON-RPC loop wholesale) rather than adding a second bespoke elicitation
   implementation on top of code that doesn't even use the SDK today.**
2. **Validation-side enum modeling (smaller, but still real work):** `validateCanvas` returns
   free-text strings; there is no structured `{field, invalidValue, validOptions}` record for any
   prop today, including `Select.options`. Elicitation's `EnumSchema` needs a concrete, sourced list
   of valid values per failure — that data model does not exist yet in `canvas-runtime`.

Both are needed together; neither alone satisfies proposal #2. This is real, evidence-based
scoping, not a stub.

## Acceptance criteria for a future implementation PR (testable, to unblock #2 later)

A follow-up PR closing proposal #2 must satisfy ALL of:

1. `mcp-dev-server` depends on `@modelcontextprotocol/sdk` (TypeScript, Tier-1) at a version that
   supports elicitation for whichever spec revision is targeted (state which one explicitly in the
   PR description — 2025-11-25 stable or the finalized RC).
2. `initialize` capability negotiation records the connecting client's declared `elicitation`
   capability; if absent, `canvas.validate`/mutation tools **must fall back to today's plain
   string-array error behavior** (no silent failure, no assuming elicitation support).
3. `canvas-runtime` adds a structured validation record type (e.g.
   `{ field: string; kind: 'enum'; invalidValue: unknown; validOptions: string[] }[]`) alongside
   (not replacing) the existing `string[]` `validateCanvas` output, with a unit test asserting a
   `Select` node with an out-of-range `value` against its own `options` array produces exactly one
   such record.
4. `mcp-dev-server`'s `canvas.validate` (or a new `canvas.*` mutation tool) issues a real
   `elicitation/create` request carrying an `EnumSchema` built from that structured record, blocks
   the in-flight `tools/call`, and resumes it correctly on the matching client reply — proven by an
   integration test that drives both sides of the stdio protocol (a test harness playing the client
   role, feeding a scripted `elicitation/create` response) and asserts the final `tools/call` result
   reflects the corrected value.
5. No new tool crosses the ADR-0019 boundary (no file/shell/browser/media/cron/general-agent tool
   added to `mcp-dev-server` as a side effect of this work).

## Consequences

- Proposal #2 is **not implemented in this pass** — no code changes ship. This is the honest
  "feature does not exist yet" state, not a stub: no elicitation call, no fallback pretending to be
  elicitation, nothing merged.
- Recommend re-opening this work only after `@modelcontextprotocol/sdk` TS server-side elicitation
  support for the RC (or confirmed continued 2025-11-25 stable support window) is verified via a
  targeted spike, so the SDK adoption and the elicitation feature land together instead of building
  a second throwaway hand-rolled transport now.
- `memory/epic-registry.json` entry for `plures-org:mcp-protocol-3.0-adoption` updated to reflect:
  proposal #1 closed, proposal #2 blocked-by-design (ADR filed, no PR), next action = SDK-adoption
  spike before re-attempting #2.

## Evidence

- `pares-radix/packages/mcp-dev-server/src/index.ts` lines 1019-1114 (`handleRequest`, stdin/stdout
  loop, `initialize` capabilities).
- `pares-radix/packages/mcp-dev-server/package.json` (no `@modelcontextprotocol/sdk` dependency).
- `pares-radix/packages/canvas-runtime/src/format.ts` line 312 (`validateCanvas` string-array
  output).
- `pares-radix/packages/canvas-runtime/src/registry.ts` lines 39, 112, 341-347 (`PropSchema`,
  `generateCatalog`, `Select` component's `options` prop metadata).
- `pares-radix/packages/canvas-runtime/src/canvas-plugin.ts` line 174 (`toolCanvasValidate`
  wrapper).
- `memory/mcp-3.0-research.md` Part 2A (RC 2026-07-28 elicitation redesign:
  `InputRequiredResult`/`requestState`), Part 4 proposal #2.
- `pares-agens/praxis/decisions/ADR-0019-dual-mcp-server-boundary.md` (confirms boundary question
  is already resolved and not the blocker here).
