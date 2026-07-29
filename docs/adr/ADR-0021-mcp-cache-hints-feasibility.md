# ADR-0021: MCP 2026-07-28 cache-hint feasibility for Radix reads

- **Status:** Proposed
- **Date:** 2026-07-29
- **Decision scope:** Proposal #5 of `plures-org:mcp-protocol-3.0-adoption`
- **Related:** ADR-0020 (Canvas elicitation feasibility)

## Context

MCP specification `2026-07-28` is final.  Its `CacheableResult` contract adds
`ttlMs` and `cacheScope` to **list/read protocol results**: `tools/list`,
`prompts/list`, `resources/list`, `resources/read`, and
`resources/templates/list`.  The fields are a freshness hint and visibility
contract, not a general annotation for arbitrary `tools/call` results.

`packages/mcp-dev-server/src/index.ts` is a synchronous newline-delimited
JSON-RPC stdio server.  It has a `tools/list` handler, but has no
`resources/list`, `resources/read`, `resources/templates/list`, or
`prompts/list` handler.  Its state reads are tools:

- `db.get`, `db.keys`, and `db.dump` read the file-backed in-memory database.
- `chronos.timeline` reads the in-process event history.

Consequently the original proposal wording -- attach standard `ttlMs` and
`cacheScope` to Chronos and PluresDB *tool-call* reads -- is not valid MCP
2026-07-28.  Adding those fields to an arbitrary `tools/call` response would
not cause conforming clients to cache it and would advertise an unsupported
protocol behavior.

The currently published TypeScript SDK package is `@modelcontextprotocol/sdk`
`1.29.0` (`latest` as checked from npm on 2026-07-29).  The v2 line described
for the 2026-07-28 protocol is not available from the public npm registry yet.
Radix also does not currently depend on that SDK; it hand-rolls the older
initialize-based transport.  A partial cache-hint implementation would thus
both target the wrong response kind and increase the later migration surface.

## Decision

Do **not** add cache fields to `chronos.timeline`, `db.get`, `db.keys`, or
`db.dump` tool results.

Defer implementation of standard MCP cache hints until the TypeScript SDK line
supporting MCP 2026-07-28 is published and Radix adopts the stateless transport
as a coherent change.  At that point, implement cache hints only on protocol
list/read endpoints.

If client-cacheable Radix state is still required, model it as resources first:

1. Expose immutable/snapshot resources with stable URIs (for example a
   Chronos snapshot or a DB prefix snapshot), rather than pretending a mutable
   tool call is cacheable.
2. Implement `resources/list` and `resources/read` through the SDK transport.
3. Set a short `ttlMs` only where the source has an evidence-backed freshness
   budget; use `cacheScope: "private"` for process/user-specific snapshots.
4. Leave mutable live views uncached, or return a versioned snapshot URI whose
   immutability permits a longer hint.

This is deliberately not an SDK-free JSON-RPC patch.  The 2026-07-28 revision
also removes `initialize` and requires `server/discover` plus per-request
protocol metadata, so an isolated hand-rolled cache-field addition would be
misleading and disposable.

## Consequences

### Positive

- Radix remains conformant: cache metadata appears only where the protocol
  defines it.
- The eventual implementation can use resource identity and source-specific
  freshness evidence rather than arbitrary hard-coded TTLs.
- SDK adoption and the protocol migration have one testable boundary instead
  of two incompatible partial migrations.

### Negative

- Existing MCP clients continue to poll state tools until the resource surface
  and 2026-07-28 transport are implemented.
- Proposal #5 cannot deliver its originally described polling reduction as a
  small tool-result change.

## Acceptance criteria for a later implementation

1. A released TypeScript MCP SDK supports 2026-07-28 server discovery,
   stateless request metadata, and typed cacheable list/read results.
2. Radix serves both its selected backward-compatible protocol behavior and
   2026-07-28 behavior through tested transport integration, not an
   untyped ad-hoc field patch.
3. Every cacheable Radix resource has a stable URI and an explicit freshness
   justification; mutable process-local data is `private`.
4. Integration tests prove a `resources/read` response carries valid
   `ttlMs`/`cacheScope` values and that a changed resource is not served as a
   stale snapshot after its TTL.
5. Tool calls do not claim standard cache semantics unless a future MCP
   specification explicitly defines them.

## Evidence

- MCP `2026-07-28` Key Changes, SEP-2549 / `CacheableResult` scope.
- `packages/mcp-dev-server/src/index.ts`: only `tools/list` among the
  cacheable list/read methods; database and Chronos reads are tool handlers.
- npm registry query on 2026-07-29:
  `@modelcontextprotocol/sdk@latest = 1.29.0`; no v2 dist-tag/version was
  published.
