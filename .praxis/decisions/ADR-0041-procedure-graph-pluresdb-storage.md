# ADR-0041: Procedure graph PluresDB storage schema

**Status:** Accepted
**Date:** 2026-07-31
**Epic:** `pares-radix:procedure-graph-repository-substrate`, M2

## Context

M0 defined the procedure graph's Rust domain model and canonical BLAKE3
identity rules in ADR-0040 (`px-repo-model`). M1 added exact artifact import
and materialization. M2 needs durable graph persistence and a query surface.
C-PLURES-003/004 require persistent state to use PluresDB rather than custom
files, maps, or a second graph store. C-DEV-001 requires pure query semantics
to be expressed in `.px`, leaving Rust as the PluresDB IO boundary.

PluresDB's `CrdtStore` provides a flat, CRDT-merged `NodeId -> JSON` keyspace;
it has no native edge table. The graph therefore needs a typed record layout
inside that keyspace without copying M0's domain schema.

## Decision

`crates/px-graph-store` persists all M2 graph state in a `pluresdb::CrdtStore`
backed by `SledStorage` for durable deployments. The test-only constructor uses
PluresDB's `MemoryStorage`; it is not an alternate application persistence
implementation.

### Nodes

Node records use a storage key:

```
px_graph_node:v1:<NodeKey>
```

`NodeKey` is either:

```
repository:<stable externally-assigned repository name>
procedure:<ProcedureId canonical UUIDv7>
revision:<RevisionId canonical blake3:... text>
```

A repository key is a typed edge endpoint only: `Repository` has no
content-derived identity or M0 Rust payload, so M2 deliberately does not
invent a duplicate `Repository` storage struct. Procedure and revision keys
address versioned `GraphNode` payloads.

The JSON payload is:

```json
{
  "schema_version": 1,
  "key": "procedure:...",
  "node": { "kind": "procedure", "value": "<px-repo-model Procedure>" }
}
```

The `value` is serialized directly from `px_repo_model::schema::Procedure` or
`RevisionContent`. No storage-only copy of their fields exists. Unsupported
M0 entities are intentionally absent from M2 rather than represented by empty
or fake records.

### Edges

An edge is an independent PluresDB record with deterministic key:

```
px_graph_edge:v1:<edge kind>:<from NodeKey>:<to NodeKey>
```

Its payload is `EdgeRecord { from, kind, to }`. Deterministic keys make
repeated writes of the same relation idempotent under `CrdtStore::put`.
M2 defines `repository_contains_procedure`, `revision_parent_of`, and
`revision_includes_procedure_revision`; `put_revision` derives and persists
its parent → child relations from `RevisionContent.parents`.

### Hashing and historical queries

`GraphStore::current_procedure_root` gathers persisted `Procedure` records and
delegates to `px_repo_model::merkle::procedure_root`. Thus sorting, canonical
JSON encoding, and BLAKE3 hashing remain exactly those specified by ADR-0040;
M2 introduces no parallel hash implementation.

`GraphStore::merkle_root_at_revision(RevisionId)` returns the immutable
`RevisionContent.procedure_root` stored under that content-addressed revision.
It intentionally does not recompute current state, which preserves the stated
point-in-time semantics.

### Query logic

`praxis/procedures/procedure-graph-queries.px` owns direct-child selection and
historical-root decision semantics. Rust scans/deserializes PluresDB records
and applies those declared rules; it contains no separate persistence model or
custom graph index.

## Consequences

- PluresDB is the only M2 persistence substrate.
- M2 can read nodes, query direct children, recompute the current procedure
  root, and return a revision's historical root.
- The flat edge scan is correct and simple for this milestone. M3 may add a
  PluresDB-native secondary-index/projection strategy after query-volume
  evidence exists, without changing node identities or hash rules.
- M3 should extend the typed node/edge vocabulary to the remaining graph
  entities and wire persisted procedure revisions/artifact relationships into
  the repository import/materialization workflows.
