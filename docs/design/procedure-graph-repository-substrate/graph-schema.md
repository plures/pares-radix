# Procedure-Graph Repository Substrate — Graph Schema

**M0 deliverable #2 of epic `pares-radix:procedure-graph-repository-substrate`.**
Companion to `.praxis/decisions/ADR-0040-procedure-graph-identity-and-canonical-hashing.md`
(identity/hashing) and design spec
`memory/design-procedure-graph-repository-substrate-2026-07-24.md` §4.1/§4.2
(entities/relationships, reproduced and made concrete here) and §4.4
(revision structure).

Design-only. No implementation code ships with this document. Owning crate
(per design-spec §16): `px-repo-model` (new crate — see the initial layout in
design-spec §18; this document specifies what that crate's types must satisfy,
not the crate's own internal Rust representation).

## 1. Scope and non-goals

This document fixes:
- The full entity list and their required fields.
- Relationship cardinality (one-to-many vs many-to-many) and direction.
- Which fields participate in canonical hashing (per ADR-0040 §2) and which
  are excluded (e.g. timestamps, per design-spec §4.4 and §20).

This document does NOT fix:
- Storage/indexing strategy inside PluresDB (owned by `pluresdb` per §16).
- `.px` bundle directory layout (deliverable #3).
- `ExactArtifactProcedure`'s full field set (deliverable #4 — it is a subtype
  of `Procedure` and is only summarized here for cross-reference).

## 2. Entities

Each entity below lists: identity type (per ADR-0040 §1), required fields,
optional fields, and hashing treatment (per ADR-0040 §2–§4).

### 2.1 `Repository`
- Identity: none of its own — a `Repository` is the root container, addressed
  by a stable, externally-assigned name/URI, not a content or graph identity.
- Fields: `name: Text`, `default_ref: RefName` (e.g. `"main"`),
  `created_at: RFC3339 Text` (metadata only, never hashed).
- Relationships: `Repository CONTAINS Procedure` (1:N), `Repository HAS_REF
  Ref` (1:N).

### 2.2 `Revision` (`RepositoryRevision`, per design-spec §4.4)
- Identity: `RevisionId` (content-derived — BLAKE3 over canonical JSON of the
  fields below, per ADR-0040 §2, EXCLUDING `author`/`timestamp`/`message`
  metadata fields per the rule that a revision's identity must be stable
  under attestation/annotation but the design spec's own §4.4 struct includes
  `author`/`timestamp`/`message` as struct fields — **this document resolves
  that apparent tension**: `author`, `timestamp`, and `message` ARE included
  in the hash, because unlike validation/attestation (explicitly excluded by
  design-spec §4.4: "Validation evidence and signatures are linked objects,
  not RevisionId inputs"), authorship and the commit message are asserted
  facts about what the author intended this revision to be, not evidence
  added by a third party afterward — the same distinction Git itself makes
  (author+message are part of a commit's hash; a later GPG signature is not,
  for signed commits using the detached-trailer convention). Timestamps ARE
  hashed here specifically because a `Revision`'s timestamp is an author
  assertion of when *that revision* was authored, not the ambient/materialized
  filesystem mtimes design-spec §20 says are non-canonical ("preserve
  meaningless filesystem timestamps" is the non-goal — a revision's own
  `timestamp` field is meaningful, a file's `mtime` is not).
- Fields (from design-spec §4.4, verbatim field names):
  - `id: RevisionId` (self — excluded from its own hash input, same
    self-reference rule as `entity_hash` in ADR-0040 §4)
  - `parents: [RevisionId]` (order-significant, per ADR-0040 §2 array rule)
  - `procedure_root: Hash` (per ADR-0040 §4)
  - `entity_root: Hash` (per ADR-0040 §4)
  - `change_ids: [LogicalChangeId]` (order-significant: the order changes were
    applied within this revision, not sorted)
  - `rendering_profile: RenderingProfileHash`
  - `materialization_root: Hash` (per ADR-0040 §4)
  - `author: PrincipalId` (Text — an opaque principal identifier; this
    document does not define `PrincipalId`'s own format, deferred to whichever
    milestone wires real authentication, per design-spec §13)
  - `timestamp: RFC3339 Text`
  - `message: Text`
- Relationships: `Revision PARENT_OF Revision` (N:N, DAG — a revision may have
  multiple parents for merges, multiple children if the graph is later
  forked), `Revision INCLUDES ProcedureRevision` (1:N), `Revision RECORDS
  LogicalChange` (1:N), `Ref POINTS_TO Revision` (N:1 — a ref points to exactly
  one revision at a time, but a revision may be pointed to by multiple refs),
  `Validation VALIDATES Revision` (N:1), `Attestation ATTESTS Revision` (N:1).

### 2.3 `LogicalChange`
- Identity: `LogicalChangeId` (assigned, UUIDv7 — per ADR-0040 §1, survives
  rebase).
- Fields: `id: LogicalChangeId`, `kind: Text` (e.g. `"rename"`, `"add_field"`,
  free-form for M0, closed-enum candidate for M6 change-procedure work),
  `summary: Text` (human-readable one-line description), `procedure_ids:
  [ProcedureId]` (which procedures this logical change touched — order not
  significant, MUST be sorted by canonical text form when hashed/serialized).
- Relationships: `Revision RECORDS LogicalChange` (N:1 from `LogicalChange`'s
  perspective — a change belongs to exactly one revision, the one that first
  recorded it; the SAME `LogicalChangeId` may be referenced by a later
  revision's `change_ids` list after a rebase per §4.3's rebase-preservation
  rule, but "recorded by" vs "carried forward by" are the same relationship
  type at the graph-query level — this is a modeling nuance the schema
  implementation must get right: `change_ids` on a `Revision` is a reference
  list, not an ownership list).
- Hashing: a `LogicalChange`'s own `LogicalChangeId` is never recomputed;
  its content (summary/kind/procedure_ids) is not independently hashed in M0
  (no `LogicalChangeHash` type exists in ADR-0040 §1) — a `LogicalChange`'s
  content is covered transitively via whichever `Revision.change_ids` entry
  references it plus the `ProcedureRevisionHash`es of the procedures it
  touched.

### 2.4 `Workspace`
- Identity: none of its own in M0 — a workspace is a local, ephemeral
  materialization context (design-spec §3.3: "fully disposable/reconstructable
  from `.px` bundle + referenced blobs"), addressed by a local path, not a
  graph-portable identifier. (A future milestone may need a stable
  `WorkspaceId` for multi-machine workspace sync; not required by M0's exit
  criterion.)
- Fields: `root_path: Text` (local, host-specific, never part of any hashed
  value), `checked_out_revision: RevisionId`, `rendering_profile:
  RenderingProfileHash`.
- Relationships: none beyond referencing a `Revision` and `RenderingProfile`
  by their content-derived identities.

### 2.5 `Ref`
- Identity: none of its own — a `Ref` is addressed by its `name` (a `RefName`,
  e.g. `"main"`, `"refs/heads/feature-x"`), which is itself mutable pointer
  metadata, not a stable entity identity (this matches design-spec §3.3:
  "paths are mutable projection properties, not entity identities" — a ref
  name is the graph-level analogue of a file path).
- Fields: `name: RefName (Text)`, `target: RevisionId`.
- Relationships: `Repository HAS_REF Ref` (1:N), `Ref POINTS_TO Revision`
  (N:1).

### 2.6 `Procedure`
- Identity: `ProcedureId` (assigned, UUIDv7 — survives procedure edits per
  ADR-0040 §1).
- Fields: `id: ProcedureId`, `kind: ProcedureKind` (closed enum: `state |
  exact_artifact | change | projection | reconciliation | validation |
  compatibility` — mirroring design-spec §5's taxonomy 5.1–5.7 exactly),
  `name: Text` (mutable, presentational — NOT part of `ProcedureId`), `path:
  Text` (mutable, presentational, POSIX-style per ADR-0040 §4), `current_revision:
  ProcedureRevisionHash`.
- Relationships: `Repository CONTAINS Procedure` (1:N), `Procedure
  HAS_REVISION ProcedureRevision` (1:N — full history), `Procedure PRODUCES
  Artifact` (1:N), `Procedure DEPENDS_ON SemanticEntity` (N:N), `Procedure
  REQUIRES CapabilityGrant` (1:N, per ADR-0040 §5).

### 2.7 `ProcedureRevision`
- Identity: `ProcedureRevisionHash` (content-derived — BLAKE3 over canonical
  JSON of this revision's full `.px` source text plus its structured
  metadata, per ADR-0040 §2).
- Fields: `hash: ProcedureRevisionHash` (self, excluded from own hash input),
  `procedure_id: ProcedureId` (which logical procedure this is a revision
  of — included in the hash, since a `ProcedureRevisionHash` must be unique
  per-owning-procedure even if two different procedures happen to share
  identical `.px` text, per design-spec §3.6's progressive-uplift model where
  distinct procedures may converge in content but never in identity),
  `source_text: Text` (the canonical `.px` text — see bundle-format doc for
  the on-disk encoding of this field), `capability_grants:
  [CapabilityGrant]` (per ADR-0040 §5; order not significant, sorted by
  `effect` variant name then `scope` when hashed), `residuals: [ResidualRef]`
  (references to preserved non-canonical content per design-spec §6
  "Residual preservation" — `ResidualRef` is `{blob_hash: BlobHash, kind:
  Text, attachment_point: Text}`, defined here as a nested value type, not a
  standalone graph entity, since residuals have no independent identity
  outside their owning `ProcedureRevision`).
- Relationships: `Procedure HAS_REVISION ProcedureRevision` (N:1 from this
  entity's perspective).

### 2.8 `SemanticEntity`
- Identity: `EntityId` (assigned, UUIDv7 — survives rename/move per ADR-0040
  §1).
- Fields: `id: EntityId`, `entity_kind: Text` (open string in M0 — e.g.
  `"type"`, `"function"`, `"module"`; a closed enum is deferred to whichever
  semantic adapter milestone (M5) first needs entity kinds to be exhaustively
  matched), `name: Text` (mutable), `owning_procedure: ProcedureId`.
- Relationships: `Procedure DEPENDS_ON SemanticEntity` (N:N), `SemanticEntity
  REFERENCES SemanticEntity` (N:N, and MAY be cyclic — e.g. mutually
  recursive types; the schema does not forbid cycles, callers that need
  acyclic traversal must detect and handle cycles themselves), `Conflict
  INVOLVES SemanticEntity` (N:N).

### 2.9 `Artifact`
- Identity: none of its own — an `Artifact` is a projection-facing output,
  identified by the combination of its owning `Procedure` and its
  `PROJECTS_TO Path` relationship; its content identity lives entirely in the
  `Blob` it references.
- Fields: `owning_procedure: ProcedureId`, `blob: BlobHash`, `mode: Text`
  (POSIX permission bits as an octal string, e.g. `"0644"`, `"0755"` for
  executable — per design-spec §5.2 "exec mode... captured"), `encoding: Text`
  (e.g. `"utf-8"`, `"binary"`), `is_symlink: Bool`, `symlink_target: Option<Text>`.
- Relationships: `Procedure PRODUCES Artifact` (1:N), `Artifact PROJECTS_TO
  Path` (1:1 per rendering profile — an artifact may project to different
  paths under different rendering profiles, but within one profile the
  mapping is 1:1), `Artifact USES_BLOB Blob` (N:1).

### 2.10 `Blob`
- Identity: `BlobHash` (content-derived — BLAKE3 over raw bytes directly, per
  ADR-0040 §3, NOT wrapped in canonical JSON).
- Fields: `hash: BlobHash` (self), `size_bytes: UInt64`, `content: Bytes`
  (the raw payload — storage-layer concern whether this is inline or
  externally chunked; schema-level, a `Blob` is addressed and hashed as one
  logical byte sequence regardless of physical storage chunking).
- Relationships: `Artifact USES_BLOB Blob` (N:1 — content-addressing means
  many artifacts across many procedures/revisions may share one `Blob` when
  bytes are identical, exactly like Git's blob dedup, noted explicitly in
  design-spec §9.2 "Git already dedupes unchanged trees/blobs across commits
  naturally").

### 2.11 `Projection`
- Identity: `ProjectionId` (assigned, UUIDv7 — "a named materialization
  configuration", per ADR-0040 §1 table, stable across edits to the
  configuration itself).
- Fields: `id: ProjectionId`, `name: Text`, `adapter: Text` (references an
  `Adapter` by name/version — see §2.12), `root_path: Text` (relative
  materialization root within a workspace).
- Relationships: `Projection USES_ADAPTER Adapter` (N:1).

### 2.12 `Adapter`
- Identity: none of its own graph-level identity in M0 — an `Adapter` is
  referenced by `(name, version)` pair; its *behavior* is pinned by a
  `RenderingProfileHash` (§2.13), not by an `AdapterId`, since the design
  spec's determinism model (§14) pins "adapter implementation hash, adapter
  schema version" as inputs INTO the rendering profile hash, not as a
  separate first-class identity of their own in this schema.
- Fields: `name: Text`, `version: Text`, `implementation_hash: BlobHash`
  (content hash of the adapter's own implementation artifact, for §14's
  determinism pinning).
- Relationships: `Projection USES_ADAPTER Adapter` (N:1).

### 2.13 `RenderingProfile`
- Identity: `RenderingProfileHash` (content-derived — BLAKE3 over canonical
  JSON of the fields below, per ADR-0040 §2).
- Fields: `adapter_name: Text`, `adapter_version: Text`,
  `adapter_implementation_hash: BlobHash`, `formatter_version: Text`,
  `platform_profile: Text` (e.g. `"linux-x86_64"` — per design-spec §14
  "platform profile" as a pinned determinism input), `parameters: Object`
  (canonical-JSON-encodable configuration map, adapter-specific).
- Relationships: `Workspace` and `Revision` both reference a
  `RenderingProfileHash` by value (N:1 conceptually, no separate graph edge
  type needed beyond the field reference itself).

### 2.14 `Validation`
- Identity: none of its own — a `Validation` result is scoped to the
  `Revision` it validates; repeated validation runs are distinct
  `Validation` records (not overwritten), each linked to the same
  `RevisionId`.
- Fields: `revision: RevisionId`, `kind: Text` (e.g. `"test_suite"`,
  `"policy_check"`, `"type_check"`), `result: Text` (`"pass" | "fail" |
  "error"`), `evidence_blob: Option<BlobHash>` (e.g. test output log),
  `run_at: RFC3339 Text`.
- Relationships: `Validation VALIDATES Revision` (N:1).

### 2.15 `Attestation`
- Identity: none of its own — same shape as `Validation`.
- Fields: `revision: RevisionId`, `principal: PrincipalId`, `statement: Text`,
  `signature: Option<Bytes>` (deferred — signature scheme not fixed by M0),
  `attested_at: RFC3339 Text`.
- Relationships: `Attestation ATTESTS Revision` (N:1).

### 2.16 `Conflict`
- Identity: none of its own in M0 (a future milestone, M6, may need a stable
  `ConflictId` for conflict-resolution UI referencing across sessions — not
  required by M0's exit criterion; this schema leaves room for it as a later
  additive field without breaking anything specified here).
- Fields: `base: RevisionId`, `left: RevisionId`, `right: RevisionId`,
  `subject_entities: [EntityId]`, `reason: Text`, `alternatives: [Text]`
  (human-readable candidate resolutions — structured resolution options are
  M6 scope per design-spec §12.3).
- Relationships: `Conflict INVOLVES SemanticEntity` (N:N).

### 2.17 `CapabilityGrant`
- Identity: none of its own — a value type, not an independently-addressed
  entity (matches design-spec §4.1 listing it alongside entities, but its
  actual shape, per ADR-0040 §5, is a plain struct attached to a `Procedure`,
  not something separately queried by its own ID).
- Fields: per ADR-0040 §5 — `effect: Effect`, `scope: Option<Text>`.
- Relationships: `Procedure REQUIRES CapabilityGrant` (1:N).

### 2.18 `DraftOverlay`
- Identity: none of its own — scoped to the `Artifact`/path it overlays;
  addressed by that path within a workspace, not a portable graph identity
  (drafts are inherently workspace-local and transient per design-spec §8.2).
- Fields: `artifact_path: Text`, `prior_owner: Option<ProcedureId>`,
  `content_blob: BlobHash`, `parser_state: Text` (`"incomplete" | "invalid" |
  "recoverable"`), `diagnostics: [Text]`.
- Relationships: none beyond referencing an `Artifact`'s path and a `Blob`.

### 2.19 `ExternalImport`
- Identity: none of its own in M0 — addressed by its source commit/patch
  reference (e.g. a Git commit SHA string) plus the target `Repository`; a
  stable `ExternalImportId` is deferred to M7 (external contribution
  workflow) when import review-state needs to be tracked across sessions.
- Fields: `source_ref: Text` (e.g. a Git commit SHA), `proposed_changes:
  [LogicalChangeId]`, `fidelity_report: FidelityVector` (per design-spec §10
  — structure defined in the fidelity-vector schema, itself a future
  deliverable per epic backlog item 14, not fixed by this document).
- Relationships: `ExternalImport PROPOSES LogicalChange` (1:N).

## 3. Relationship summary table

| Relationship | From | To | Cardinality |
|---|---|---|---|
| CONTAINS | Repository | Procedure | 1:N |
| HAS_REF | Repository | Ref | 1:N |
| POINTS_TO | Ref | Revision | N:1 |
| PARENT_OF | Revision | Revision | N:N (DAG) |
| INCLUDES | Revision | ProcedureRevision | 1:N |
| RECORDS | Revision | LogicalChange | 1:N |
| HAS_REVISION | Procedure | ProcedureRevision | 1:N |
| PRODUCES | Procedure | Artifact | 1:N |
| DEPENDS_ON | Procedure | SemanticEntity | N:N |
| REQUIRES | Procedure | CapabilityGrant | 1:N |
| PROJECTS_TO | Artifact | Path (value) | 1:1 per profile |
| USES_BLOB | Artifact | Blob | N:1 |
| REFERENCES | SemanticEntity | SemanticEntity | N:N (may cycle) |
| USES_ADAPTER | Projection | Adapter | N:1 |
| VALIDATES | Validation | Revision | N:1 |
| ATTESTS | Attestation | Revision | N:1 |
| INVOLVES | Conflict | SemanticEntity | N:N |
| PROPOSES | ExternalImport | LogicalChange | 1:N |

## 4. Conformance-test cross-reference

This schema is written to directly satisfy design-spec §19 conformance tests
1, 2, 8, 9, 15 (see ADR-0040 "Consequences" for the identity-level argument);
tests 3–7, 10–14, 16–18 are M1+ scope and depend on the exact-artifact
importer, reconciliation engine, and merge logic respectively — not
satisfiable by schema alone, listed here only so the M0 reader can see which
tests this document does and does not close.

## 5. Open items carried to later deliverables

- `PrincipalId` format (authentication milestone, not M0).
- `FidelityVector` full structure (epic backlog item 14).
- `ProcedureKind`/`entity_kind` closed-enum promotion timing (M5/M6 —
  deliberately left open string in M0 per design-spec §3.6 progressive
  uplift).
- Signature scheme for `Attestation.signature` (deferred, unblocked whenever
  a concrete signing mechanism is chosen).
