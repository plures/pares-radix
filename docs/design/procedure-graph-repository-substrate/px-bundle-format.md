# Procedure-Graph Repository Substrate — `.px` Bundle Directory Format

**M0 deliverable #3 of epic `pares-radix:procedure-graph-repository-substrate`.**
Companion to `.praxis/decisions/ADR-0040-procedure-graph-identity-and-canonical-hashing.md`
and `docs/design/procedure-graph-repository-substrate/graph-schema.md`. Design
spec source: `memory/design-procedure-graph-repository-substrate-2026-07-24.md`
§9.2 (canonical Git tree layout — reproduced and made concrete below) and §3.2
("`.px` is the canonical portable representation").

Design-only. No implementation code ships with this document.

## 1. Purpose

The design spec (§9.2) sketches the canonical Git tree shape but does not
specify byte-level encoding, file naming rules, or partitioning scheme needed
for two independent implementations to produce an identical tree for the same
graph revision (the M0 exit criterion). This document fixes those details.

## 2. Directory layout (elaborates design-spec §9.2)

```
.px/
  repository.px
  revision.px
  change.px
  procedures/
    <partition>/<ProcedureId>.px
  residuals/
    <BlobHash-hex>.px
  blobs/
    <BlobHash-hex>
  adapters.lock
  schemas.lock
  rendering.lock
  projection-manifest.px
```

All paths are POSIX-style (`/` separator) regardless of host OS, per
ADR-0040 §4's path-sorting rule — this directory layout is itself hashed as
part of `materialization_root`, so path separator ambiguity would break
byte-identical serialization exactly as it would break Merkle-root
determinism.

## 3. File-by-file specification

### 3.1 `repository.px`
One `.px` state-procedure document (design-spec §5.1) describing the
`Repository` entity itself: `name`, `default_ref`. Canonical `.px` text
encoding per §5 below. Not content-addressed itself (a `Repository` has no
identity of its own per the schema doc §2.1) — this file's bytes are
deterministic given the `Repository`'s current field values, but the file is
not looked up BY a hash; it is looked up by its fixed path.

### 3.2 `revision.px`
One `.px` document per **current** revision (the one `HEAD`/the active `Ref`
points to) — NOT a full history dump; history is reconstructed by walking
`parents` through Git commit ancestry once exported via `git.px` (design-spec
§9), or by walking prior `revision.px` blobs stored under earlier Git tree
states. Contains the full `RepositoryRevision` struct (schema doc §2.2) in
canonical `.px` form. The file's bytes MUST hash (per ADR-0040 §2) to exactly
the `RevisionId` recorded as this revision's identity — this is the concrete,
checkable form of the M0 exit criterion: given a `revision.px`, recomputing
`RevisionId` from its bytes and comparing to the `RevisionId` an independent
implementation assigns is the conformance check.

### 3.3 `change.px`
One `.px` document containing the list of `LogicalChange` records introduced
by this revision (schema doc §2.3), i.e. the content backing
`revision.px`'s `change_ids` field. Entries ordered exactly as listed in
`change_ids` (order-significant, not sorted — schema doc §2.3).

### 3.4 `procedures/<partition>/<ProcedureId>.px`
One file per `Procedure`'s current `ProcedureRevision` (schema doc §2.6/§2.7).
**Partitioning rule:** `<partition>` is the first two lowercase hex characters
of the `ProcedureId` UUID with hyphens stripped (e.g. `ProcedureId
018f2b3a-8c41-7000-9c21-4e6b1a2f9d10` → partition `01`, file
`procedures/01/018f2b3a-8c41-7000-9c21-4e6b1a2f9d10.px`). This mirrors Git's
own loose-object two-character sharding convention (`.git/objects/<xx>/<rest>`)
for the same reason Git adopted it — avoiding directories with an unbounded
flat file count — and keeps this bundle format visually/operationally
familiar to anyone who has worked inside a `.git` directory, consistent with
design-spec §2's explicit citation of Git's object model as the mapping
target for this bundle. Partitioning is purely a filesystem/lookup
convenience; it is NOT part of any hashed value (a `ProcedureRevisionHash`
does not encode which partition directory holds its file).

### 3.5 `residuals/<BlobHash-hex>.px`
One file per distinct residual (schema doc §2.7 `ResidualRef`) — structured
`.px` metadata (kind, attachment point) wrapping a reference to the residual's
own content stored under `blobs/`. Filename is the residual's OWN content
hash (a `BlobHash`, hex, no `blake3:` prefix in the filename — see §4 below on
filename vs canonical-text-form encoding), not the `ProcedureRevisionHash` of
the procedure it's attached to (a residual may be shared/referenced by
multiple procedure revisions if byte-identical, same dedup argument as
`blobs/`).

### 3.6 `blobs/<BlobHash-hex>`
Raw blob bytes (schema doc §2.10), stored verbatim — no `.px` wrapper, no
extension. Filename is the lowercase hex `BlobHash` **without** the `blake3:`
prefix used in canonical JSON text form (ADR-0040 §1) — the prefix exists to
disambiguate hash types when a hash value appears embedded as a field inside
JSON; a blob file's location is already unambiguously "a blob" by directory
context, so the prefix is redundant there and dropped for path brevity,
matching Git's own bare-hex object filenames. Same two-character-partition
sharding as §3.4 is RECOMMENDED for blobs at scale (`blobs/<xx>/<rest>`) but
NOT required for M0 conformance (a flat `blobs/` directory is valid for the
test-graph scale used in the M0 conformance corpus); this is called out
explicitly as an implementation choice, not a hashed/canonical property.

### 3.7 `adapters.lock`
Canonical JSON (per ADR-0040 §2 encoding rules) array of `{name, version,
implementation_hash}` triples (schema doc §2.12 `Adapter`), sorted by `name`
then `version`. Analogous to a package-manager lockfile — pins exactly which
adapter implementations this revision's projections depend on, per
design-spec §14's determinism requirement.

### 3.8 `schemas.lock`
Canonical JSON recording the schema-doc version this bundle was produced
against (a simple `{schema_version: Text}` for M0 — this document IS schema
version `"m0-2026-07-31"`; a future schema revision increments this string).
Exists so a loader can detect "this bundle predates a schema change" rather
than silently misinterpreting fields, per design-spec §14 "Adapter upgrades
must be explicit migration changes — never silently reformat the repo,"
generalized here to schema versions, not just adapter versions.

### 3.9 `rendering.lock`
Canonical JSON of the current `RenderingProfile` (schema doc §2.13),
content-addressed by its own `RenderingProfileHash` — this file's bytes MUST
hash to the `rendering_profile` field value recorded in `revision.px`, same
self-consistency check as §3.2.

### 3.10 `projection-manifest.px`
One `.px` document listing every `Projection` (schema doc §2.11) and its
`Artifact → Path` mapping (schema doc §2.9's `PROJECTS_TO` relationship) for
the CURRENT `materialization_root`. This is the file a materializer reads to
know which blobs go to which output paths — i.e., the concrete input to
`materialize(G, R) = W` (design-spec §1).

## 4. Hash filename convention vs canonical text form

Two distinct hex encodings appear in this bundle, and they must not be
confused:

- **Canonical text form** (used INSIDE `.px`/JSON field values, per ADR-0040
  §1): `blake3:<64 lowercase hex chars>` — always prefixed, always used when a
  hash appears as embedded structured data.
- **Bare filename form** (used for `blobs/`, `residuals/`, `procedures/`
  paths): `<64 lowercase hex chars>` (or the ProcedureId UUID form for
  `procedures/`) — never prefixed, since the directory context already
  disambiguates the hash's type.

A conformant implementation MUST be able to convert between these two forms
mechanically (strip/add the `blake3:` prefix) — this is stated explicitly
because a subtle mismatch here (e.g. one implementation using the prefixed
form as a filename) would silently break byte-for-byte tree comparison
between independent implementations, defeating the M0 exit criterion.

## 5. `.px` text encoding within bundle files

Every `.px` file in this bundle (`repository.px`, `revision.px`, `change.px`,
`procedures/*.px`, `residuals/*.px`, `projection-manifest.px`) is:

- UTF-8, **no BOM**.
- **LF line endings only** (`\n`, never `\r\n`) — this is a hard requirement
  distinct from a stylistic preference: mixed line endings would make the
  same logical content hash differently depending on which platform produced
  it, directly defeating determinism (design-spec §14). Any implementation
  running on Windows MUST normalize to LF before writing bundle files (this is
  the same class of defect flagged elsewhere in this workspace's own tooling
  notes about PowerShell `-replace` introducing bare `\r` bytes — a concrete,
  previously-hit failure mode this bundle format explicitly guards against by
  fixing LF-only as a conformance requirement, not an assumption).
- **Exactly one trailing newline** at end-of-file (POSIX text-file
  convention) — but this trailing newline is NOT part of what gets hashed
  (per ADR-0040 §2: hashing operates over the canonical-JSON-then-UTF-8
  encoding of the LOGICAL value, not the literal file bytes on disk for
  `.px`-formatted structural files; only `blobs/<hash>` files are hashed as
  literal raw bytes per ADR-0040 §3). This means a `.px` file's on-disk
  trailing newline is a filesystem/editor courtesy, not a hash input — two
  implementations may differ on trailing-newline presence in the WRITTEN file
  without breaking the M0 exit criterion, since the exit criterion is about
  the `revision.px` **content hash matching the recorded RevisionId**, which
  is computed from the logical value, not literal file bytes. (Contrast with
  `blobs/<hash>`, where file bytes ARE the hash input exactly, with no
  normalization license at all.)
- No trailing whitespace on any line (lint-level convention for reviewability
  per design-spec §3.2 "reviewable serialization" — not a hash-affecting rule
  per the point above, but enforced by tooling to keep diffs clean).

## 6. Directory-level determinism note

`materialization_root` (ADR-0040 §4) is computed from the `.px` bundle's
LOGICAL contents (sorted `{path, blob_hash}` pairs), not from directory
listing order, mtimes, or any filesystem metadata — so this bundle format's
directory layout is a **lookup/storage convenience**, not itself a hash
input. Two implementations that choose different (but internally consistent)
partitioning schemes for `procedures/` or `blobs/` would still agree on every
content hash and every Merkle root; they would only disagree on where a given
file physically lives on disk. The M0 exit criterion ("two independent
implementations serialize the same test graph identically") is satisfied at
the hash/canonical-JSON level (ADR-0040) regardless of directory-layout
choice — this document's specific partitioning scheme (§3.4, §3.6) is
RECOMMENDED for interoperability and Git-familiarity, not itself
independently conformance-tested beyond "produces the files listed in §2."

## 7. Relationship to `git.px` (design-spec §9)

This directory IS the tree `git.px` (design-spec §9.1) serializes into Git
blobs/trees/commits — `.px/` as specified here is exactly the tree structure
design-spec §9.2 names; this document is the byte-level elaboration `git.px`'s
own implementation (M4 milestone, out of scope for M0) will need. Nothing in
`git.px`'s responsibilities changes this document's layout; `git.px` only adds
the Git object-ID mapping layer (design-spec §9.3) on top of it.
