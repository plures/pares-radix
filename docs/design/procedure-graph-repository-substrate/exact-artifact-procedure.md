# Procedure-Graph Repository Substrate — `ExactArtifactProcedure` Interface Spec

**M0 deliverable #4 of epic `pares-radix:procedure-graph-repository-substrate`.**
Companion to ADR-0040, `graph-schema.md`, `px-bundle-format.md`. Design spec
source: `memory/design-procedure-graph-repository-substrate-2026-07-24.md`
§3.5 ("Exact fallback is mandatory"), §5.2 ("Exact artifact procedures"), §6
("Adapter contract"), §16 (component ownership — `px-repo` owns procedure
taxonomy including this one).

Design-only. No implementation code ships with this document — the Rust trait
below is a specification for the follow-on implementation PR, matching the
"illustrative, not shipped" convention RFC-0003 uses for its own Rust
sketches (RFC-0003 §2.1, §2.2, §3.2).

## 1. Purpose

Design-spec §3.5 requires that "any arbitrary repo must be importable before
a semantic adapter exists" — `ExactArtifactProcedure` is the universal,
always-available fallback procedure kind (design-spec §5.2) that makes this
possible: every file, regardless of whether any semantic adapter understands
it, becomes at minimum an `ExactArtifactProcedure` (L0 fidelity per §3.6).
This document defines its exact field set and the contract it must satisfy
(§6's adapter laws, specialized to the exact case).

## 2. Relationship to `Procedure` (schema doc §2.6)

`ExactArtifactProcedure` is the `exact_artifact` variant of `ProcedureKind`
(schema doc §2.6). It is not a separate graph entity — it is a `Procedure`
whose `ProcedureRevision.source_text` (schema doc §2.7) is generated
deterministically FROM the artifact's raw bytes and filesystem metadata,
rather than authored directly as `.px` prose. Concretely: importing a file
`src/main.rs` with unknown/unsupported syntax creates:
- One `Procedure { kind: exact_artifact, name: "main.rs", path: "src/main.rs",
  ... }`.
- One `Artifact { owning_procedure: <that ProcedureId>, blob: <BlobHash of
  main.rs's raw bytes>, mode, encoding, is_symlink, symlink_target }`
  (schema doc §2.9).
- One `ProcedureRevision` whose `source_text` is the canonical `.px`
  serialization of the `ExactArtifactProcedure` fields below (NOT the raw
  file content itself — the raw content lives in the `Blob`/`Artifact`, the
  `.px` procedure text is the structured wrapper naming which blob and which
  metadata).

## 3. Required fields

Per design-spec §5.2 ("blob hash, mode, encoding, line endings... Directory
structure/exec mode/symlink target/filename bytes/platform semantics
captured; timestamps NOT canonical"):

```rust
// docs/design/procedure-graph-repository-substrate/exact-artifact-procedure.md
// (illustrative Rust shape — not shipped by this document; follow-on
// implementation PR owns the actual crate types, per RFC-0003's own
// "illustrative, not shipped" convention for spec-stage Rust sketches)

pub struct ExactArtifactProcedure {
    /// Which logical Procedure this is a revision of (schema doc §2.6/§2.7).
    pub procedure_id: ProcedureId,

    /// Content hash of the raw file bytes, exactly as read from source —
    /// no line-ending normalization, no encoding transcoding. This is what
    /// makes the exact-import law (design-spec §6: `write(read(S)) = S`)
    /// checkable: re-materializing this artifact must reproduce a blob whose
    /// hash equals this field, byte for byte.
    pub blob: BlobHash,

    /// POSIX permission bits, octal string form (e.g. "0644", "0755").
    /// On platforms without POSIX permission bits (Windows), the importer
    /// MUST synthesize a canonical value (0644 for non-executable, 0755 for
    /// executable-by-convention, e.g. `.exe`/`.bat`/shebang-detected files)
    /// rather than omit the field — every ExactArtifactProcedure has a mode,
    /// full stop, so materialization is platform-independent (design-spec
    /// §14 "platform profile" is a RENDERING concern, not a reason this
    /// field becomes optional).
    pub mode: String,

    /// Declared text encoding, e.g. "utf-8", "binary". Detection heuristic
    /// (valid-UTF-8 vs not) is an importer implementation detail — NOT
    /// specified here beyond: if bytes are not valid UTF-8, `encoding` MUST
    /// be `"binary"` and no line-ending normalization of any kind may be
    /// applied to `blob` (binary content must never be text-processed).
    pub encoding: String,

    /// Whether this artifact is a symlink, and if so its raw target string
    /// (NOT resolved/followed — an exact import preserves the symlink
    /// itself, per design-spec §5.2 "symlink target... captured").
    pub is_symlink: bool,
    pub symlink_target: Option<String>,

    /// The path this artifact was imported from, and materializes back to
    /// by default (before any rendering-profile remapping). POSIX-style,
    /// per ADR-0040 §4 / bundle-format doc §2. Filename bytes are preserved
    /// exactly (design-spec §5.2 "filename bytes... captured") — this means
    /// non-UTF-8 filenames (permitted on POSIX filesystems) are represented
    /// as their exact byte sequence; this document does NOT resolve how a
    /// UTF-8-only `.px` text format (bundle-format doc §5) represents a
    /// non-UTF-8 filename byte-for-byte — flagged here as an open item for
    /// the follow-on implementation (candidate: percent-encoding or a
    /// separate raw-bytes sidecar field), matching this document's
    /// no-stub/no-invented-resolution discipline (C-NOSTUB-001): rather than
    /// silently picking an unverified encoding scheme, this is named
    /// explicitly as unresolved.
    pub original_path: String,

    /// Line-ending metadata: NOT applied as normalization (§`blob` above is
    /// never re-encoded), but RECORDED so a materializer can report/audit
    /// what encoding an artifact originally had, per design-spec §6
    /// "residual preservation... unresolved/malformed content." Values:
    /// "lf" | "crlf" | "mixed" | "n/a" (binary content).
    pub line_endings: String,
}
```

**Explicitly NOT a field:** any timestamp (mtime/ctime/atime). Design-spec
§4.4 and §20 both explicitly exclude filesystem timestamps from canonical
identity ("timestamps NOT canonical", "preserve meaningless filesystem
timestamps" is a listed non-goal) — `ExactArtifactProcedure` has no timestamp
field at all, by design, not by omission.

## 4. Capability declaration (per RFC-0003 / ADR-0040 §5)

Per ADR-0040 §5's consequence and design-spec §3.4's default-pure rule, every
`ExactArtifactProcedure`'s owning `Procedure.capability_grants` (schema doc
§2.7) list is **empty** in the ordinary import case — reading a file during
import and later materializing it back to a workspace root are both covered
by the substrate's own checkout-level allow-list (design-spec §13.2: "read
declared blobs... write declared artifacts inside isolated workspace"), not
by a per-procedure capability grant. `ExactArtifactProcedure` therefore never
needs `effects: [file_read]`/`effects: [file_write]` declared on itself — file
I/O for import/materialization is a substrate-runtime operation on a declared
blob, not a `.px`-level function call subject to RFC-0003's `FunctionRegistry`
seam. This is stated explicitly because it is the concrete instance of
RFC-0003 §6's own claim ("an imported byte-for-byte artifact procedure has no
business performing network/shell/db-write effects just to exist as a graph
node") — this document confirms that claim holds by construction: nothing in
§3's field list above requires or implies any effectful call.

## 5. Adapter-law compliance (design-spec §6)

The **exact adapter** (design-spec §6, §16 "px-adapter-exact" crate) is the
adapter whose `read`/`write` functions produce/consume
`ExactArtifactProcedure` values. It must satisfy:

- **Exact import law**: `write(read(S)) = S`. Concretely: given source bytes
  `S`, `read(S)` produces an `ExactArtifactProcedure` with `blob =
  blake3(S)` (ADR-0040 §3) plus the mode/encoding/symlink/path metadata read
  from the filesystem; `write` of that same `ExactArtifactProcedure` must
  reproduce filesystem bytes identical to `S` (verified by re-hashing the
  written file's bytes and comparing to the `blob` field) AND identical
  mode/symlink-target/encoding metadata. This is the M1 exit criterion
  ("`import(folder) → materialize → byte-and-metadata-equivalent folder`") —
  this document specifies the per-artifact contract; M1 implements the
  folder-level importer/materializer that applies it recursively.
- **Stable projection law** (`read(write(G)) ≡ G`): for the exact adapter
  specifically, this holds trivially by construction — `write` never
  transforms `blob`, `mode`, or any other field, so reading back a
  materialized exact artifact reproduces the identical
  `ExactArtifactProcedure` value. (Contrast with a semantic adapter, e.g. a
  future TypeScript adapter, where formatting/whitespace differences make
  this law meaningfully non-trivial — the exact adapter is the "easy case"
  baseline every other adapter is judged against.)
- **Residual preservation**: not applicable in the pure-exact case — there is
  no "residual" separate from the artifact, since the ENTIRE file is treated
  as opaque exact content already (residuals, schema doc §2.7, matter for
  *semantic* procedures that uplift PART of a file to L1+/L2+ representation
  while preserving the rest — an `ExactArtifactProcedure` has no partial
  uplift by definition, so its `ProcedureRevision.residuals` list is always
  empty).

## 6. Conformance-test cross-reference

This spec is written to satisfy design-spec §19 conformance test 3 ("Exact
folder import round-trips byte-for-byte") at the per-artifact level; the
folder-level (recursive, multi-artifact, Unicode-path, mixed-line-ending
corpus) test is M1 scope (design-spec §17 M1 exit criterion), not testable
from this document alone — this document defines what a conformant
per-artifact implementation must do; M1 builds and tests the importer that
does it across a real corpus.

## 7. Minimal serialization test (M0 scope, if time allows)

Per the M0 task's optional deliverable: a minimal test proving byte-identical
canonical serialization for one small `ExactArtifactProcedure` test graph,
computed by two independently-written encoding functions within the same
crate (the strongest same-language proxy for cross-implementation determinism
achievable without a second-language implementation — see ADR-0040
"Consequences"). This is deferred to the actual `px-repo-model` crate
scaffold (not written as part of this docs-only PR, per the epic's own
sequencing note: "milestones are strictly sequential... do NOT start coding
beyond M0 scope" — a serialization test requires at minimum a `Cargo.toml`
and working Rust types, which is the first *code* artifact of this epic; it
is flagged here as the natural next actionable step for the M0
implementation follow-up, not fabricated as an empty stub in this PR).
