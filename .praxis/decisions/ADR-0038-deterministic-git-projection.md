# ADR-0038: Deterministic Procedure-Graph-to-Git Projection

## Status: Proposed

## Date: 2026-07-30

> Companion to ADR-0025 (revised, "Procedure-Graph Forge with Hyperswarm Collaboration and
> Git Compatibility"). Design-only (Pillar 1). **No implementation code in this ADR.**
> This ADR exists because ADR-0025's corrected model treats git objects/packs as a
> **generated compatibility projection** of the canonical PluresDB procedure graph, and that
> projection is only correct if it is byte-reproducible across independent replicas — a
> requirement substantial enough to need its own explicit invariants, not a hand-wave
> inside ADR-0025 itself.

## Context

ADR-0025 (revised) establishes that the PluresDB procedure graph is the canonical
repository, and that conventional git objects/refs served to `git` clients are generated
compatibility artifacts — not the source of truth. This is only safe if:

1. **Any replica**, computing the projection independently from the same graph revision,
   produces **byte-identical** git objects (same SHA, same pack bytes) — otherwise two
   peers could silently disagree about what `git clone` returns for the same ref, which is
   a correctness failure invisible at the graph layer (the graph itself has no conflict;
   only its git-facing shadow diverges).
2. The projection is **reproducible over time** — recomputing the projection for the same
   historical graph revision, on any node, at any later time, yields the same bytes it
   would have at the time the ref was first advertised. Projected packs are a regenerable
   cache; if regeneration ever produced different bytes than what a client previously
   fetched and pinned locally, that client's repository would desync from the forge's
   notion of history.
3. The projection has a **precise entity mapping** back to graph nodes/`.px` procedures,
   not just "produces valid git bytes" — needed by any graph-native editor consuming the
   projection (e.g. pares-scribe) to map a byte range in the projected view back to the
   owning graph entity.

None of these are automatic. Naive "serialize the graph to git objects" code has many
silent non-determinism sources (hash-map iteration order, floating timestamp precision,
locale-dependent formatting, unstable sort comparators, differing compression producing
different pack bytes for the same logical content, etc.). This ADR pins the invariants
that close those gaps.

## Decision

### 1. Ordering invariants

- **Tree entries** (graph child-edges representing directory contents) MUST be projected
  in **strict byte-wise ordering of entry name**, exactly matching git's own tree-object
  ordering rule (sorted by name, treating a trailing `/` as part of the comparison for
  directories). This ordering MUST be computed at projection time from the graph edge set
  — it MUST NOT depend on any incidental storage/iteration order of the underlying
  PluresDB query.
- **Commit parents**: for a graph revision with multiple causal parents, parent order in
  the projected commit object MUST be a **deterministic function of the graph's recorded
  causal order** (e.g., first parent = the graph's designated primary predecessor,
  following parents = other predecessors in the order the merge procedure recorded them)
  — never re-sorted by hash, timestamp, or iteration order at projection time.
- **Pack object ordering**: when multiple objects are bundled into a pack, object order
  MUST follow a stated deterministic rule (e.g., topological order by graph causal history,
  then tree/blob objects in the order first referenced by that walk) — pinned as a CID/
  procedure invariant, not left to incidental collection iteration.

### 2. Timestamp / encoding invariants

- All projected timestamps (commit author/committer time) MUST be serialized in git's
  canonical `<unix-seconds> <±HHMM>` format, sourced from a single canonical timestamp
  field on the graph revision (not derived from wall-clock at projection time, not
  reformatted through a locale-aware date library whose output can vary by environment).
- Author/committer name and email MUST be projected verbatim from a single canonical
  string field on the graph node (no environment-dependent normalization).
- String encoding is UTF-8, no BOM, matching git's convention; content stored with a
  different declared encoding is transcoded at projection time via a single pinned
  transcoding path.

### 3. Hashing invariants

- Git object hashing (SHA-1 for compatibility with existing git tooling; SHA-256 is
  out of scope for v1, tracked as a later variant) MUST be computed over the exact git
  object byte format (`"<type> <length>\0<content>"`) using a single shared hashing
  routine — not reimplemented ad hoc at each call site.
- The projection function MUST be **pure**: `project(graph_revision_id) -> git_object_bytes`
  with no hidden inputs (no current wall-clock, no random IDs, no host-specific state). Any
  non-graph input the projection needs (e.g., a compression level) MUST be a **pinned
  constant** recorded in this ADR or the CID, not a runtime-configurable default that could
  differ across replica deployments.
- **Pack compression**: because delta selection can legitimately produce different but
  equally valid packs, the determinism requirement is scoped to **per-object hashes**
  (every loose object's SHA MUST be reproducible), while pack byte-layout MUST be
  reproducible only insofar as v1 uses a fixed, versioned delta/ordering algorithm — an
  implementation MAY skip delta-compression entirely for v1 (store all objects undeltified
  in the pack) to eliminate this axis of non-determinism until a later revision adds a
  pinned delta algorithm. This tradeoff MUST be an explicit implementation-stage decision
  recorded against this ADR, not silently assumed.

### 4. Reproducibility test requirement (blocks "done")

Before the projection is considered implementation-complete:

- **Cross-replica determinism test**: two independent processes (ideally two different
  machines/OSes, or at minimum two independent process invocations with no shared runtime
  state) project the **same graph revision** and assert **byte-identical** output for every
  object and the resulting pack (or byte-identical loose objects if v1 skips
  delta-packing per §3). This is the acceptance gate for this ADR's invariants.
- **Time-travel reproducibility test**: project the same historical graph revision twice,
  separated by wall-clock time (re-run after a delay or process restart), and assert
  identical output — proving no hidden dependency on current time or process-local state.
- **Entity-mapping test** (for graph-native editor consumers): for a representative graph
  revision, assert that every projected byte range can be mapped back to the exact owning
  graph entity, and that this mapping is stable across repeated projections of the same
  revision.

### 5. Scope boundary — what this ADR does NOT decide

- The **canonical-branch selection policy** (which graph revision projects to which git
  ref, when the native graph preserves divergent branches) is not decided here — it is
  graph/policy work tracked against ADR-0025, orthogonal to "given a chosen graph revision,
  project it deterministically."
- The **streaming-transport host capability** gap (how projected pack bytes are streamed to
  a git client) is separate foundation work — this ADR only concerns the correctness of
  the bytes produced, not how they are transported.
- SHA-256 object-hash mode, delta-compression algorithm selection beyond the v1
  undeltified-pack fallback (§3), and partial/shallow-clone projection are explicitly
  deferred, not stubbed — absent from v1 scope per C-NOSTUB-001, to be scoped in a future
  ADR revision when needed.

## Consequences

**Positive**
- Closes the single riskiest correctness gap in ADR-0025's graph-canonical model: without
  this, "any replica can serve fetch/clone" (a core HA/DR claim of the forge) would be
  false in practice the first time two replicas computed slightly different bytes.
- Gives graph-native editor consumers (e.g. pares-scribe) a precise contract (entity-range
  mapping) to build against, rather than an implicit assumption.
- Scopes pack compression honestly (§3: undeltified-pack fallback is an explicit, recorded
  tradeoff) rather than silently shipping non-deterministic delta selection and discovering
  the bug later.

**Negative / costs**
- Skipping delta-compression in v1 (§3 fallback) means larger pack transfers over the wire
  until a pinned delta algorithm is designed — an explicit, accepted cost of prioritizing
  correctness over size for v1.
- A shared, single-source hashing/serialization routine must be authored once and used by
  every projection call site — a second ad hoc implementation of "compute git object hash"
  anywhere in the codebase would reintroduce exactly the risk this ADR closes.

**Risks**
- **Silent divergence if a determinism test is skipped or weakened.** Mitigation: §4's
  cross-replica and time-travel tests are a hard implementation-completeness gate for
  ADR-0025, not optional; CI must run them against real independent process instances (not
  two calls in the same process, which would hide shared-state bugs).
- **Future delta-compression addition reintroducing non-determinism.** Mitigation: any
  future ADR revision that adds delta-packing must itself re-run and pass the §4 tests
  before shipping; this ADR's invariants apply to any future pack-layout algorithm, not
  just the v1 undeltified fallback.

## Implementation outline (gated; design = Pillar 1 only here)

1. Author the shared, pure `project(graph_revision_id) -> git_object_bytes` routine per
   §1–3 invariants — single implementation, no per-call-site reimplementation.
2. Implement the v1 undeltified pack fallback (§3) explicitly, recording the tradeoff.
3. Build the §4 cross-replica determinism test and time-travel reproducibility test as the
   acceptance gate — build and pass these before ADR-0025's push/fetch adapter work is
   considered mergeable.
4. Build the entity-mapping contract test for graph-native editor consumers.
5. Tests BLOCK (C-TEST-002): real independent process instances, not same-process calls;
   channel-independent verification against a real running pares-radix instance.

## References
- ADR-0025 (revised) — Procedure-Graph Forge with Hyperswarm Collaboration and Git Compatibility
- C-PLURES-002/003/004, C-NOSTUB-001, C-TEST-001/002
- Epic `px-repo:procedure-graph-substrate` (`memory/epic-registry.json`)
- `workspace/memory/phase2-git-forge-corrected-architecture.md`
