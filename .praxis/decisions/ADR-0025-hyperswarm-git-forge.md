# ADR-0025: Procedure-Graph Forge with Hyperswarm Collaboration and Git Compatibility

## Status: Proposed (Revised)

## Date: 2026-07-24

> Driver: corrected Phase 2 architecture direction (kbristol, 2026-07-24) captured in
> `workspace/memory/phase2-git-forge-corrected-architecture.md`.
> This revision replaces ADR-0025's prior canonical-storage premise.
> Design-only (Pillar 1). No implementation in this ADR.

## Context

ADR-0025 previously treated conventional Git objects/packs as the canonical repository
content and PluresDB as pointer/metadata state around that content. That model is now
incorrect.

Per Phase 1 architecture decisions, the canonical repository is the **PluresDB procedure
graph**. Conventional Git objects are a **compatibility projection** for existing Git clients,
not the source of truth.

The corrected repository model is:

1. **Canonical repository:** PluresDB native graph + procedure history.
2. **Portable canonical snapshot:** deterministic `.px` procedure bundle.
3. **Bulk immutable content:** `plures-object`.
4. **Distribution/cache:** `pares-arca`.
5. **Conventional Git objects/refs:** generated compatibility artifacts only.

This ADR must preserve valid parts of the prior design (forge lifecycle, capability model,
Arca integration, Bastion/secrets integration, and explicit streaming transport gap), while
reframing data authority around the native procedure graph.

## Decision

Author `hyperswarm-git` as a canonical plugin that provides `git-repo@1.x`, but redefine
its architecture as **procedure-graph-first**:

- Native authoring, review, and merge semantics operate on PluresDB graph revisions.
- Git protocol endpoints expose a compatibility projection generated from canonical graph
  state.
- Replication/causal-state sync remains owned by PluresDB native Hyperswarm; radix sync
  does not reimplement replication.

### 1) Canonical data and projection layers

The forge uses explicit layers with non-overlapping authority:

- **Canonical layer (authoritative):**
  - PluresDB collaboration graph (repos, branches, proposals, reviews, policy decisions).
  - Append-only multi-writer graph revisions; divergence is preserved explicitly as graph
    structure, not flattened by last-writer-wins.
- **Portable canonical snapshot:**
  - Deterministic `.px` bundle export/import for reproducible transfer and recovery.
- **Bulk content layer:**
  - `plures-object` stores immutable payloads (blob-like content, bundles, pack artifacts,
    release/build artifacts).
- **Distribution layer:**
  - `pares-arca` handles cache/distribution segments (public, org, repo, release-channel,
    build-cache, private-workspace).
- **Git compatibility layer (non-authoritative):**
  - Conventional Git commits/trees/blobs/refs generated as a projection from canonical graph
    state for interoperability with unmodified Git clients.

### 2) Concurrency model: native graph vs Git-compatible refs

The system keeps two distinct mutation contracts:

- **Native graph revisions:** append-only, multi-writer, divergence-preserving.
  - Competing updates coexist as explicit graph branches/lineage.
  - Canonical branch selection is a policy-governed graph operation.
- **Git-compat refs:** strict compare-and-swap (CAS).
  - Ref updates require `expected-head`.
  - Mismatch is rejected (non-fast-forward behavior for Git clients).

This avoids imposing Git's single-head pointer semantics onto native graph collaboration while
still providing familiar Git behavior in the compatibility surface.

### 3) Hyperswarm ownership boundaries

- **PluresDB native Hyperswarm owns:** replication transport, causal state propagation,
  and graph sync semantics.
- **Radix sync/host layer owns:** pairing UX, subscriptions, partition policy,
  approval/control plane, and observability.
- **Radix forge does not own:** reimplementation of replication/CRDT conflict mechanics.

### 4) Capability and portfolio mapping (preserved and clarified)

This ADR aligns responsibilities without re-litigating ownership:

- **pares-radix:** capability host + forge procedures/orchestration in this repo.
- **pares-scribe:** graph/projected editor path
  (`editor -> projection doc -> semantic ranges -> owning .px procedures -> PluresDB tx`).
- **pares-bastion + `secrets@1.x`:** secrets, identity, signing, key material.
- **pares-arca:** artifact distribution/cache only.
- **pares-modulus:** forge/app packaging and governance; issues/PRs/projects/CI/releases as
  governed Modulus plugins.
- **pluresdb (native Hyperswarm):** canonical repository + collaboration graph + replication.

### 5) Transport surface and streaming gap (preserved)

Transport sequence remains:

- v1: smart-HTTP (`upload-pack`/`receive-pack` compatibility projection).
- later: `git://` and SSH compatibility endpoints.

The prior capability gap remains valid and is now more explicit:

- Git transport is long-lived, duplex, and streaming.
- Current request/response IO actor shape is insufficient for production-scale push/fetch.
- A **general streaming transport host capability** is required at radix host level.

This is foundation work; do not hide it behind buffered approximations presented as complete.

### 6) Forge lifecycle scope in this ADR

v1 procedures remain design targets (no implementation here):

- Repo lifecycle (create/archive/delete policy-gated)
- Graph branch/proposal lifecycle
- Git-compat projection generation and ref CAS updates
- PR/issue/review lifecycle as governed procedures/plugins
- Artifact publication hooks to Arca

Deferred (explicitly absent until built):

- Full release automation surfaces
- Webhook ecosystem breadth
- Advanced Git features dependent on streaming maturity (e.g., very large transfer
  optimizations)

## Consequences

### Positive

- Restores architectural truth: canonical source is the procedure graph, not Git object
  storage.
- Clean separation between collaboration semantics (native graph) and interoperability
  semantics (Git projection).
- Preserves multi-writer collaboration richness without sacrificing strict Git client
  expectations at projection refs.
- Reuses existing foundations appropriately:
  PluresDB for state/replication, `plures-object` for immutable payloads,
  `pares-arca` for distribution/cache, Bastion/`secrets@1.x` for security/signing.

### Negative / costs

- Projection pipeline becomes a critical subsystem and must be deterministic and auditable.
- Canonical-branch policy on divergent graph histories requires strong governance defaults.
- Streaming host capability remains a prerequisite for production-grade transport behavior.

### Risks and mitigations

- **Risk:** accidental authority inversion (treating projection as source of truth).
  - **Mitigation:** explicit contract: projection artifacts are generated, never canonical.
- **Risk:** CAS bugs in Git-compat ref updates causing client-visible inconsistency.
  - **Mitigation:** strict expected-head validation with deterministic rejection semantics.
- **Risk:** overlap/confusion between radix sync and PluresDB replication responsibilities.
  - **Mitigation:** boundary codified in this ADR; replication work routes to PluresDB.

## Implementation outline (design-stage guidance only)

1. Update `git-repo@1.x` CID language to encode canonical-vs-projection authority.
2. Define deterministic projection procedures from graph revisions to Git-compat artifacts.
3. Preserve strict CAS contract for Git-compat refs (`expected-head`).
4. Route replication features to PluresDB; restrict radix sync scope to control-plane duties.
5. Keep Arca/Bastion/Modulus integration points explicit in procedure lifecycle definitions.
6. Validate with design-time invariants before dev stage: no projection write path may bypass
   canonical graph policy.

## References

- `workspace/memory/phase2-git-forge-corrected-architecture.md`
- ADR-0022 — capability/CID model
- ADR-0024 — canonical plugin format + capability dependencies
- Prior ADR-0025 revision (2026-06-26) — retained valid transport/capability/lifecycle framing
- Foundation components: PluresDB (native Hyperswarm), `plures-object`, `pares-arca`,
  `pares-bastion`, `secrets@1.x`, `pares-modulus`, `pares-scribe`
