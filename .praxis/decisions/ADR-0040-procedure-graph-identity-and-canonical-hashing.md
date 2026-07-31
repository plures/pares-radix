# ADR-0040: Procedure-Graph Repository Substrate — Identity and Canonical-Hashing Model

## Status: Proposed

## Date: 2026-07-31

> M0 deliverable #1 of epic `pares-radix:procedure-graph-repository-substrate`
> (design spec: OpenClaw workspace `memory/design-procedure-graph-repository-substrate-2026-07-24.md`,
> §4.3 "Identity types" and §14 "Determinism and reproducibility"). Design-only
> (Pillar 1), same discipline as ADR-0038/ADR-0039: **no runtime wiring in this
> ADR** beyond the minimal conformance test required by the epic's M0 exit
> criterion (two independent serializers producing byte-identical output for
> one test graph — see `crates/px-repo-model`).
>
> **Blocked-on note:** this ADR was gated on `plures/praxis-lang` RFC-0003
> ("Effects and Capabilities") landing, since the substrate's `CapabilityGrant`
> entity (§4.1/§4.2 of the design spec) needed a concrete effect/capability
> shape to hash against. RFC-0003 merged as `plures/praxis-lang#21`
> (`docs/rfcs/RFC-0003-effects-and-capabilities.md`, commit `1e63568`) as a
> **design-only, no-code-shipped** RFC. Its `Effect` enum (§2.1: `db_read`,
`db_write`, `network`, `shell`, `file_read`, `file_write`, `env_read`, `clock`,
`random`) and `CapabilityGrant { effect, scope: Option<String> }` shape (§3.2)
are adopted directly below as the canonical shape for this repo's
`CapabilityGrant` graph entity — see §5.

## Context

The design spec (§2, §3, §4.3) requires every graph entity to carry **three
distinct kinds of identity**, and requires (§14) that canonical serialization
be pinned tightly enough that two independent implementations, given the same
logical graph, produce byte-identical output. Today neither exists anywhere in
`pares-radix`: `crates/radix-core/src/spine/git_projection.rs` (ADR-0038) gives
us a precedent for "deterministic projection with a documented byte-format and
a dogfood round-trip test," but it projects an already-existing git tree — it
does not define entity identity or canonical hashing for a *new* graph model.
This ADR is the identity/hashing foundation everything else in the epic
(schema doc, bundle format doc, `ExactArtifactProcedure`) is built on top of.

Three problems this ADR must solve, none of which the design spec fully
resolves on its own (it states the *requirement*, not the *encoding*):

1. **What exactly gets hashed, and in what byte order** — the design spec says
   "canonical `.px` bundle... deterministic serialization" (§1, §3.2) and lists
   the identity types (§4.3) but does not specify a wire encoding. Two
   implementations cannot agree on a hash without an unambiguous byte-for-byte
   specification of what bytes get fed to the hash function.
2. **Which identifiers are content-derived vs. assigned** — `EntityId` and
   `ProcedureId` are explicitly required to survive rename/move/edit (§4.3
   table: "Survives rename and move", "Survives procedure edits"), meaning
   they CANNOT be pure content hashes (a content hash changes the instant the
   content changes). `ProcedureRevisionHash`, `BlobHash`, and
   `RenderingProfileHash` are explicitly content-addressed ("Changes with
   content", "Content-addressed", "Immutable"). Mixing these two identifier
   *kinds* under one hashing scheme was the failure mode this ADR exists to
   prevent.
3. **How graph-level structural hashes (`procedure_root`, `entity_root`,
   `materialization_root` — §4.4 `RepositoryRevision`) compose from
   entity-level hashes** — the design spec names these fields but does not
   define a Merkle/tree composition rule. §14 requires "materialize twice...
   compare artifact Merkle roots" and §19 conformance test 15 ("Repeated
   materialization produces the same Merkle root") both assume a defined
   composition rule exists.

## Decision

### 1. Two identifier kinds, never conflated

Per §4.3 of the design spec, every identifier in the repository graph is
exactly one of:

- **Assigned identity** (`EntityId`, `ProcedureId`, `LogicalChangeId`,
  `ProjectionId`) — a **UUIDv7** (time-ordered, per existing workspace
  convention — `uuid = { version = "1", features = ["v4"] }` is already a
  workspace dependency; this ADR adds the `v7` feature) minted exactly once
  when the entity is first created, then carried forward verbatim across every
  edit/rename/move/rebase. **Never recomputed from content.** Storage: 128-bit
  value, canonical text form is lowercase hyphenated UUID (RFC 4122 §3), e.g.
  `018f2b3a-8c41-7000-9c21-4e6b1a2f9d10`.
- **Content-derived identity** (`ProcedureRevisionHash`, `BlobHash`,
  `RenderingProfileHash`, `RevisionId`) — a **BLAKE3-256** digest (32 bytes)
  of a precisely-specified canonical byte sequence (§2 below). Canonical text
  form is lowercase hex, prefixed `blake3:` (e.g.
  `blake3:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08`) —
  the prefix exists so a `RevisionId` can never be silently compared against a
  differently-hashed `BlobHash` even if both happen to be 32 bytes.

  **Why BLAKE3, not SHA-256:** (a) it is already a transitive dependency in
  this workspace via `pluresdb` (verified: `pluresdb-px`'s own lockfile pulls
  `blake3` for content addressing — reusing the same primitive avoids a second
  hash function in the dependency graph for the same job), (b) it is faster
  for the repeated-hashing workload §14 requires (materialize-twice
  verification), (c) it has a native keyed/XOF mode this ADR does not need
  today but the fidelity model (§10 of the design spec) may need later for
  content fingerprinting at different granularities without changing the
  primitive.

- **Mixed identity** (`LogicalChangeId` per its own row: "Survives rebase") —
  assigned at creation time like `EntityId`/`ProcedureId` (a UUIDv7), NOT
  recomputed on rebase. This is the whole point of `LogicalChangeId` existing
  separately from `RevisionId` (§4.3: "LogicalChangeId vs RevisionId prevents
  rebase from making a logical change appear to be an unrelated new change").

### 2. Canonical byte encoding (what actually gets hashed)

All content-derived hashes in this repository use **one** canonical encoding
rule, so a new content-addressed identifier type never needs its own bespoke
byte format:

1. The logical value being hashed (a `ProcedureRevision`, a `Blob`, a
   `RenderingProfile`, or a `RepositoryRevision`) is first reduced to a
   **canonical JSON value** per the rules below, then serialized to UTF-8
   bytes with **no trailing newline**, then hashed with BLAKE3-256 over those
   exact bytes.
2. **Canonical JSON rules** (deterministic-JSON, not "any JSON serializer"):
   - Object keys sorted byte-wise ascending by their UTF-8 encoding (not
     locale-aware, not case-insensitive).
   - No insignificant whitespace: no spaces after `:` or `,`, no newlines,
     no indentation.
   - Numbers: integers only in this schema (no floats are part of any hashed
     value — timestamps are RFC 3339 strings, not numeric epoch values,
     because §4.4 explicitly excludes timestamps from content-addressed
     identity in the exact-artifact case and this ADR generalizes that: no
     hashed structure anywhere in this schema contains a raw float).
   - Strings: UTF-8, escaped per strict JSON (`"`, `\`, control chars `<
     0x20`), **not** HTML-safe-escaped (no `\u003c` substitution) — this
     matches `serde_json`'s default non-HTML-escaping behavior so the
     canonical encoder in Rust needs no extra configuration beyond stable key
     ordering.
   - Arrays: order is semantically significant and preserved exactly as
     defined by the field (e.g. `parents: [RevisionId]` order is NOT sorted —
     merge-parent order is meaningful and part of what's hashed).
   - `null` for an explicitly-absent optional field is **omitted from the
     object entirely**, never emitted as a `null` key — an object with an
     absent optional field and an object that never had that field defined
     must hash identically.
   - Content-derived identifiers embedded as fields (e.g. a `RevisionId`
     referencing a parent) are encoded as their canonical text form (the
     `blake3:`-prefixed hex string), never as raw bytes — this keeps the
     canonical JSON representation copy-pasteable and diffable, matching the
     design spec's §3.2 requirement that the canonical form be "reviewable."
   - Assigned identifiers (`EntityId`/`ProcedureId`/etc.) are encoded as their
     canonical lowercase-hyphenated UUID text form.
3. This rule is deliberately identical in shape to RFC 8785 (JSON Canonicalization
   Scheme, JCS) restricted to the strict subset this schema actually uses (no
   floats, no HTML-escaping ambiguity to resolve) — this ADR does not claim
   full JCS compliance (JCS's ECMAScript-number serialization rule is
   irrelevant here since no floats are hashed), but implementers should treat
   RFC 8785 §3.2 (string/number formatting) as the disambiguation reference
   for any edge case not enumerated above.

### 3. Blob hashing (leaf content)

A `Blob` (§4.1 entity) is hashed directly over its **raw bytes**, not wrapped
in a JSON envelope — `BlobHash = blake3(raw_bytes)`. This is intentionally
different from every other content-derived hash in this ADR (which hash a
canonical JSON encoding): a blob's bytes ARE its canonical form already (this
is what makes exact-artifact byte preservation, §5.2/§6 of the design spec,
possible — the blob hash is exactly what a plain `git hash-object`-equivalent
or `sha256sum`-equivalent would report on the same bytes, modulo BLAKE3 vs
SHA-1/256, making it trivially cross-checkable against external tools during
conformance testing).

### 4. Merkle composition for graph-level roots

`procedure_root`, `entity_root`, and `materialization_root` (§4.4) are each
the BLAKE3-256 hash of a **canonical JSON array** of `{id, hash}` pairs, sorted
by `id`'s canonical text form (byte-wise ascending), where:

- `procedure_root` — pairs are `{procedure_id: ProcedureId, revision_hash:
  ProcedureRevisionHash}` for every `Procedure` reachable from the revision,
  sorted by `procedure_id`.
- `entity_root` — pairs are `{entity_id: EntityId, entity_hash: BLAKE3}` where
  `entity_hash` is the canonical-JSON hash (rule §2) of that `SemanticEntity`'s
  own field set (excluding its own `entity_id`, to avoid the entity hashing
  its own identifier into itself), sorted by `entity_id`.
- `materialization_root` — pairs are `{path: Text, blob_hash: BlobHash}` for
  every materialized path in the workspace projection, sorted by `path`
  (byte-wise ascending on the UTF-8 path string, using `/`-separated
  POSIX-style paths regardless of host OS, per §3.3 "paths are mutable
  projection properties" — this also directly satisfies conformance test 15,
  "Repeated materialization produces the same Merkle root," since path sort
  order cannot depend on host directory-iteration order).

This is a flat sorted-list Merkle scheme, not a binary Merkle tree — chosen
deliberately over a tree structure for M0 because (a) the design spec's
conformance tests (§19: tests 1, 2, 15) only require *determinism and
stability*, not *logarithmic proof size*, and a tree's proof-of-inclusion
property is not needed anywhere in M0–M1 scope, (b) a sorted flat list is
trivially specifiable byte-for-byte (§2's rules apply directly, no additional
tree-shape rule needed) which is the actual M0 exit-criterion requirement
("two independent implementations serialize the same test graph identically"
— minimizing the surface two implementers must agree on). A binary Merkle
tree (for inclusion proofs, e.g. for partial sync) is an explicitly-deferred
future optimization, not a functional requirement dropped here — if a later
milestone needs inclusion proofs, it is layered on top of this flat hash
(a tree of the same sorted leaves) without changing the leaf encoding.

### 5. `CapabilityGrant` shape (adopted from RFC-0003)

Per the blocked-on note above, `CapabilityGrant` (design spec §4.1: `Procedure
REQUIRES CapabilityGrant`) is defined identically to RFC-0003 §3.2:

```
CapabilityGrant {
  effect: Effect,          // closed enum, RFC-0003 §2.1: db_read | db_write |
                            //   network | shell | file_read | file_write |
                            //   env_read | clock | random
  scope: Option<Text>,      // opaque scope qualifier, e.g. table name, host
                            //   pattern, path prefix; None = unscoped
}
```

`CapabilityGrant` values are hashed as part of their owning `Procedure`'s
canonical JSON (per §2 above; `effect` as its lowercase enum-variant string,
`scope` omitted when absent per the "omit, don't null" rule). This repo does
**not** re-derive its own effect taxonomy — RFC-0003's `Effect` enum is the
single source of truth for effect classes, matching design-spec §16's
component-ownership split (praxis-lang owns effect/capability *typing*;
`px-repo` owns the *graph schema* that references that typing). If RFC-0003's
`Effect` enum gains variants in a later amendment (RFC-0003 §7 item 5:
`secret.read` deferred), this repo's schema absorbs the new variant without a
schema-version bump to the identity/hashing model itself (adding an enum
variant does not change how a `CapabilityGrant` is canonically encoded).

### 6. What this ADR explicitly does NOT decide

- Storage layout inside PluresDB (owned by the schema doc, deliverable #2, and
  ultimately `pluresdb`'s own storage engine — §16 "pluresdb owns: graph
  storage, transactions...").
- The `.px` bundle **directory** layout (owned by deliverable #3 — this ADR
  only fixes the byte encoding of individual hashed values, not where files
  land on disk).
- `ExactArtifactProcedure`'s field set beyond what's needed to note it's
  hashed via the blob rule (§3) for its payload and the canonical-JSON rule
  (§2) for its metadata — full definition is deliverable #4.
- Signature/attestation encoding (design spec §4.4: "Validation evidence and
  signatures are linked objects, not RevisionId inputs" — explicitly excluded
  from the revision hash by the design spec itself, so out of scope here too).
- Any policy for *which* capability grants are permitted (RFC-0003 §1
  explicitly defers this to RFC-0004; this ADR only fixes the grant's shape
  for hashing purposes, not its semantics).

## Consequences

- Two independent implementations (Rust, or any other language) can now
  produce byte-identical `RevisionId`/`ProcedureRevisionHash`/`BlobHash`
  values for the same logical graph, given only this document — no shared
  code, no shared library, is required to satisfy the M0 exit criterion. This
  is verified directly by `crates/px-repo-model`'s conformance test (two
  independent Rust encoders in the same crate, deliberately not sharing an
  encoding function, both producing the same bytes for one fixture graph —
  the strongest same-language proxy for cross-implementation determinism
  available without standing up a second-language implementation in M0).
- `EntityId`/`ProcedureId` stability across rename/move (§4.3) is now
  mechanically guaranteed by construction: nothing about §2's canonical
  encoding rule ever recomputes an assigned identifier from content, so a
  rename (which changes a `name`/`path` field, itself just another key in the
  canonical JSON) changes that entity's `entity_hash` but never its
  `entity_id`. This directly satisfies design-spec conformance test 8
  ("Semantic rename preserves EntityId").
- Rebase preserving `LogicalChangeId` while changing `RevisionId` (conformance
  test 9) follows the same way: `LogicalChangeId` is assigned, `RevisionId` is
  a hash over (among other things) `parents: [RevisionId]` — a rebase changes
  `parents`, therefore changes the hash, therefore changes `RevisionId`,
  without ever touching the `LogicalChangeId` values listed in
  `change_ids: [LogicalChangeId]`.
- This ADR intentionally introduces exactly one new external dependency
  candidate (`blake3` crate) beyond what's already transitively present via
  `pluresdb-px` — the follow-on implementation should confirm the exact crate
  version already pinned by `pluresdb-px`'s lockfile and reuse it verbatim
  rather than introducing a second `blake3` version into the dependency graph.

## Relates

- Design spec: `memory/design-procedure-graph-repository-substrate-2026-07-24.md`
  §2, §3, §4.3, §4.4, §14, §16, §19 (tests 1, 2, 8, 9, 15).
- `plures/praxis-lang` RFC-0003 (`docs/rfcs/RFC-0003-effects-and-capabilities.md`,
  merged `plures/praxis-lang#21`) — source of the `Effect`/`CapabilityGrant`
  shape adopted in §5.
- ADR-0038 (`deterministic-git-projection`) — prior art for "deterministic,
  byte-reproducible projection with a documented format and a conformance
  test," same discipline applied here one layer up (graph identity, not git
  object projection).
- Next deliverables in this epic: repository graph schema doc, `.px` bundle
  directory format doc, `ExactArtifactProcedure` interface spec (all in
  `docs/design/procedure-graph-repository-substrate/`).
