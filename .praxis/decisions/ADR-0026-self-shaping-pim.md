# ADR-0026: Self-Shaping Personal Information Manager (`pim`)

## Status: Proposed

## Date: 2026-06-26

> Driver: kbristol directive (Radix Plugin Program §2.B,
> `workspace/memory/plan-radix-plugins-2026-06-26.md`). Design (Pillar 1) ONLY — no
> implementation code. Authors the `pim` plugin and its `pim@1.x` CID against the canonical
> plugin format (ADR-0024) and the capability host contract (ADR-0022). Companion artifact:
> `capabilities/pim.cid.toml`.

## Context

Every PIM on the market (Notion, Obsidian, AnyType, Capacities, Tana, Reflect, Apple/Google
contacts+calendar+notes silos) freezes a schema and then asks the user to bend their life to
it. Even the "flexible" ones (Notion databases, Tana supertags) make schema a **manual,
human-authored** artifact: you design the table, you add the property, you migrate the rows by
hand. The schema is a wall the user maintains.

**The thesis (kbristol):** the PIM database is **not fixed-schema**. It **reshapes itself to
the user's needs** — proactively (AI notices a pattern and proposes a shape change) or
just-in-time (the user captures something that doesn't fit, and the shape grows to hold it).
`unum` renders a CRUD UI directly from the *current* shape for manual operations; `agens`
(the AI/cognition layer) drives automated operations, reporting, analysis, and
integrity/maintenance. The goal is to do more, more efficiently, than any frozen-model PIM —
because **the model is data, and the model evolves**.

This sits entirely on existing foundation — it is composition, not reinvention
(C-PLURES-001/002/004):

| Need | Foundation primitive | Role here |
|------|----------------------|-----------|
| CRUD UI from data shape | **`unum`** (`@plures/unum`) — reactive PluresDB→Svelte, backend-agnostic `DbAdapter`, Svelte 5 runes, Graph + Collection APIs | Renders manual CRUD from the *current* `entity_def`/`field_def`/`view_def` |
| AI/cognition | **`agens`** (`pares-agens`, PRIVATE) | Proposes shape changes, infers schema, completes records, generates reports, runs integrity checks — via the `llm` capability |
| State + CRDT + graph + embeddings + reactive procedures | **PluresDB** | Holds BOTH the records AND the schema-as-data; embeddings power lookup/search; procedures react to shape/record events (C-PLURES-003/004) |
| Decision/validation logic | **`.px`** (`pluresdb-px` v3.0.1, grammar STABLE) | `entity`/`rule`/`constraint`/`procedure` constructs gate and apply shape changes; **no `capability` keyword — CIDs are TOML** |
| Sensitive fields | **`secrets@1.x`** provider (vault, ADR-0024 §3) | Optional dependency for fields flagged sensitive (SSNs, account numbers, passwords-as-data) |

The **net-new work** is three pieces, and the riskiest is the first:
1. a **schema-evolution / record-migration model** that is conflict-safe under CRDT (this ADR
   argues the general primitive belongs in **PluresDB**, not the plugin — see Decision §2);
2. an **unum binding over a runtime-defined (dynamic) shape** (likely an unum gap — §3);
3. the **propose→confirm→apply loop** as `.px` procedures + mediated events, with `agens`
   proposing and `.px` constraints gating (§4).

This ADR extends ADR-0010 (agens-first), ADR-0011 (plugin security), ADR-0022 (capability
host contract), and ADR-0024 (canonical plugin format). It defines the `pim` consumer plugin
and the `pim@1.x` provider CID, and it explicitly flags the foundation dependencies that feed
Task 3 (`plan-radix-plugins` §3.B).

## Decision

### 1. Meta-schema model: the schema IS PluresDB data

The PIM stores **shape as data**. There is no compiled, fixed entity set. Instead, four
meta-schema node types in PluresDB describe the *current* shape, and ordinary record nodes
conform to that shape at read time:

| Meta-node | Describes | Key (namespaced `pim:`) |
|-----------|-----------|--------------------------|
| `pim:entity_def` | A user-facing "kind of thing" (Contact, Project, Habit, Recipe…) | `pim:entity_def:{slug}` |
| `pim:field_def` | A field on an entity_def (name, type, required, sensitive, default, embedding-indexed) | `pim:field_def:{entity_slug}:{field_slug}` |
| `pim:view_def` | A saved view/layout (table/board/list/detail; columns, filters, sort, grouping) over an entity_def | `pim:view_def:{slug}` |
| `pim:relation` | A typed edge type between two entity_defs (Contact→works_at→Org), cardinality, inverse | `pim:relation:{slug}` |
| `pim:record` | An actual user datum, tagged with its `entity_slug` + `schema_version` it was written under | `pim:record:{entity_slug}:{id}` |

A `pim:record` does **not** embed a rigid struct. It is a property bag (CRDT map) plus two
control fields: `entity_slug` (which `entity_def` it claims to be) and `schema_version` (the
`entity_def` version it was last written/migrated under). Validation is **projective**: a
record is "valid against the current shape" iff its properties satisfy the current
`field_def`s for its `entity_slug`. This makes the schema editable as ordinary data — the user
(via unum) or `agens` (via a proposal) edits `entity_def`/`field_def`/`view_def`/`relation`
nodes, and the UI + validation re-derive immediately.

**Why meta-as-data and not codegen:** codegen would require a compile/redeploy on every shape
change, defeating "reshapes itself … just-in-time." Meta-as-data lets the shape evolve at
runtime, lets CRDT handle concurrent shape edits, and lets the decision ledger record every
shape mutation as a normal PluresDB event (auditable, replayable — C-PLURES-003/004).

### 2. THE HARD PROBLEM — self-modifying schema under CRDT

This is the riskiest, most novel part. Naïve "edit the schema, then loop over every record and
rewrite it" is **not** safe under CRDT: two peers can evolve the shape concurrently, records
written under an old shape must remain readable, and a destructive edit (drop field, retype)
must never silently lose data. The decision:

**2.1 Schema versioning (monotonic, per entity_def).** Every `entity_def` carries a monotonic
`version` (a Lamport-style counter that only advances) and an append-only `migrations` log:
an ordered list of **shape-change operations** (`add_field`, `remove_field`, `rename_field`,
`retype_field`, `add_relation`, …). The shape at version *N* is the fold of operations
1..*N* over the empty shape. **Records reference the version they conform to** (`schema_version`),
so the engine always knows which migration prefix a record has already been brought through.

**2.2 Shape-change operations are CRDT-mergeable, not last-write-wins.** Concurrent shape
edits merge by **operation commutativity + a deterministic total order**, not by clobbering:

- The `migrations` log is a **grow-only, causally-ordered set** of operations (RGA/OR-set
  style). Two peers adding *different* fields concurrently → both operations survive (union);
  the shape gains both fields. This is the common, safe case and needs no human.
- **Conflicting** operations on the *same* field (e.g. peer A `rename age→years`, peer B
  `retype age:int→string` concurrently) are detected by overlap on the field's stable
  `field_id` (fields carry a UUID `field_id` independent of their slug, so rename ≠ delete+add).
  Conflicts are resolved by a **deterministic, total tiebreak** (causal order, then
  Lamport timestamp, then actor id) so all peers converge to the *same* resolved shape **and**
  the losing operation is preserved in the ledger as a recorded, surfaced conflict (never
  silently dropped). A conflict that would be destructive (§2.4) is additionally surfaced to
  the user before it can take effect.
- **Rename is a metadata op, not a data rewrite.** Because records key properties by
  `field_id` internally (slug is a display projection), a rename mutates only the `field_def`'s
  slug; **zero records change**. This is what makes the common evolution case O(1).

**2.3 Lazy, idempotent record migration ("migrate on read/touch", not a stop-the-world sweep).**
A record at `schema_version = k` is migrated to the current version *N* by replaying migration
operations k+1..N over it. Migration is:

- **Lazy** — a record is migrated when it is read, edited, or proactively swept by a background
  `agens`/procedure pass; never as a blocking global rewrite. The UI always migrates-on-read so
  the user sees current shape.
- **Idempotent + pure** — each migration op is a deterministic function of (record, op); re-running
  the prefix yields the same result, so concurrent migration on two peers converges (CRDT-safe).
- **Field semantics:**
  - `add_field` → record gains the field's `default` (or "absent"); no data loss.
  - `rename_field` → no-op on the record (keyed by `field_id`); display only.
  - `retype_field` → apply a declared, total **coercion** (int→string always succeeds;
    string→int may fail). A value that cannot be coerced is **not destroyed**: the original is
    preserved in a `pim:record` shadow property (`__migration_residue[field_id]`) and the record
    is flagged for review, not dropped.
  - `remove_field` → the value is **moved to residue**, not deleted, so a concurrent peer that
    still has the field (or a later "undo") can recover it. Hard purge is a separate, explicit,
    user-confirmed, ledgered operation.

**2.4 No destructive shape change without confirmation (invariant).** Operations classified
**destructive** (`remove_field`, `retype_field` with a lossy coercion, deleting an `entity_def`
with extant records, removing a `relation` with extant edges) **cannot apply** from an `agens`
proposal without an explicit user confirmation event (§4) AND they always route data to residue
rather than deleting in place. Non-destructive operations (`add_field`, `rename_field`,
`add_relation`, additive `view_def` edits) MAY be configured to auto-apply (still ledgered).

**2.5 Backward/forward compatibility of records vs evolving entity_defs.**
- **Backward** (old record, new shape): always readable via migrate-on-read; the migration
  prefix is total over old records.
- **Forward** (new record arrives at a peer with an older shape via P2P sync): the record's
  `schema_version` is ahead of the local `entity_def`. The CRDT merge of the *shape* (the
  migrations log) brings the local shape forward first (operations are part of the synced
  graph), so the record's version becomes resolvable. If a property references a `field_id` the
  local shape has not yet learned, it is retained verbatim (CRDT preserves unknown keys) until
  the corresponding `add_field` op syncs — **no data loss across version skew**.

**2.6 🔴 FOUNDATION FLAG (feeds Task 3 → routes to PluresDB).** A general
**schema-evolution + record-migration primitive** — versioned node-types, a CRDT-mergeable
migration-operation log, migrate-on-read, lossless residue, deterministic conflict resolution —
is **broadly useful to every data-driven plugin**, not just `pim`. Per the foundation routing
map (`plan-radix-plugins` §3.C: "CRDT/storage/sync/**schema-evolution+migration**/embeddings →
pluresdb"), this primitive **belongs in PluresDB**, not in the `pim` plugin. The `pim` plugin
should **consume** it via the `storage` capability, not re-implement CRDT-safe migration. This
ADR designs the *model*; the *primitive* is a separate, gated foundation work item in pluresdb
(likely the single biggest net-new foundation piece for this program). Honest deferral
(C-NOSTUB-001): if the PluresDB primitive does not yet exist, `pim` ships the **meta-schema
nodes + propose→confirm→apply loop** and the **non-destructive, rename-as-metadata, additive**
evolution path (which needs no new primitive), and **leaves lossy migration ABSENT + reported**
until the foundation primitive lands — it does NOT fake a migration engine inside the plugin.

### 3. unum over a DYNAMIC shape

The manual CRUD UI is **unum rendering the current shape**: a generic
`EntityList`/`RecordDetail`/`ViewBoard` set of Svelte 5 components that read
`pim:entity_def` + `pim:field_def` + `pim:view_def` and render fields by type, with create/
edit/delete writing `pim:record` nodes through unum's `DbAdapter`. Sensitive fields render via
the `secrets` capability, never as plaintext in a generic input.

**🔴 OPEN DEPENDENCY / LIKELY unum GAP (feeds Task 3).** unum's published surface (reactive
PluresDB→Svelte, `DbAdapter`, Graph + Collection APIs) is oriented toward collections whose
shape is **known at bind time** (Svelte 5 runes typed to a schema). The PIM needs unum to bind
to a **runtime-defined** collection — a "**schema-from-data**" mode where the set of fields and
their types come from `field_def` nodes at runtime, not from compile-time types. Two
possibilities, to be confirmed against the unum repo (NOT assumed here — C-NOSTUB-001):

- **(a) unum already supports dynamic collections** (e.g. an untyped/`Record<string,unknown>`
  collection + runtime field metadata). If so, `pim` binds directly; no foundation change.
- **(b) unum needs a `schema-from-data` extension** — a way to declare a collection whose
  field set + validators are supplied at runtime from `field_def` nodes, with runes that track
  a dynamic field map. This is **broadly useful to every data-driven plugin** (forms, admin
  UIs, any user-customizable view), so per §3.C it is generalized **in the unum repo**, not
  hacked into `pim`.

This ADR records it as an **open dependency on unum**, resolved during dev (Pillar 2) by
reading the unum source, not by guessing. Until resolved, the CRUD UI targets the
known-at-bind-time path for the *seed* shape and the dynamic path is flagged pending — absent,
not stubbed.

### 4. AI authority + guardrails: propose → confirm → apply

`agens` is **powerful but not unilateral**. Every shape change — proactive or JIT — flows
through a mediated, gated, ledgered loop. No silent destructive schema edits, ever.

```
agens (llm)                .px constraints              user                PluresDB
   │  observe patterns          │                         │                     │
   ├─ emit pim.shape.change.proposed (a pim:shape_change_proposal node) ───────►│
   │                            │  PROPOSE-GATE procedure  │                     │
   │                            ├─ validate proposal vs constraints ────────────│
   │                            │   (well-formed? destructive? within authority?)│
   │                            │                          │                     │
   │             (non-destructive + auto-apply allowed?) ──┤                     │
   │                            │  yes → APPLY ─────────────────────────────────►│ (apply + migrate, ledger)
   │                            │  no  → require confirmation                    │
   │                            ├─ emit pim.shape.change.awaiting_confirmation ─►│
   │                            │                          │ unum surfaces it    │
   │                            │     user confirms/rejects│                     │
   │                            │◄─ pim.shape.change.confirmed / .rejected ──────┤
   │                            │  APPLY-GATE procedure     │                     │
   │                            ├─ on confirmed: apply ops + migrate ───────────►│ (ledger applied)
   │                            └─ on rejected: archive proposal ───────────────►│ (ledger rejected)
```

- **`agens` proposes; `.px` constraints gate; the user confirms; PluresDB applies + migrates.**
  agens never writes `entity_def`/`field_def` directly — it can only write a
  `pim:shape_change_proposal` and emit `pim.shape.change.proposed`. The authority to *apply*
  lives in `.px` procedures the agens layer cannot bypass (the boundary is mediated per
  ADR-0011 — no direct refs).
- **`.px` constraints** (in `procedures/*.px`) enforce: proposal well-formedness; the
  destructive-needs-confirmation invariant (§2.4); rate/authority limits (agens cannot propose
  unbounded mass-deletes); and that the resulting shape stays internally consistent (no field
  referencing a dropped relation, etc.).
- **Decision ledger.** Every proposal, confirmation, rejection, and application is a node +
  event in PluresDB (`pim:shape_change_proposal` transitions `proposed → {applied | rejected |
  superseded}`), giving a complete, replayable audit trail of how the shape evolved and who/what
  drove each change. This is the same decision-ledger discipline the workspace mandates for
  cross-repo automation, applied to schema evolution.
- **Records always migrate-able or quarantined.** Apply is paired with migration (§2.3). A
  record that cannot be migrated cleanly (lossy coercion residue) is **flagged/quarantined for
  review**, never silently dropped (invariant, mirrored in the CID).

### 5. Capabilities

**REQUIRES (consumer):**

```toml
[capabilities.required]
storage = "^1.0"     # PluresDB collections: meta-schema nodes + records + ledger (and the
                     # schema-evolution/migration primitive once it lands — §2.6)
llm     = "^1.0"     # agens-driven inference: shape proposals, record completion, reports,
                     # analysis, integrity checks
ui      = "^1.0"     # unum-rendered CRUD over the dynamic shape (§3)

[capabilities.optional]
secrets = "^1.0"     # vault-provided; for fields flagged sensitive — degrade gracefully if absent
                     # (sensitive fields simply cannot be stored until a provider is present)

[dependencies]
capabilities = ["secrets@^1.0"]   # ADR-0024 §3: install-time guarantee a secrets provider exists
```

`storage`, `llm`, `ui`, `secrets` are routed appropriately: `storage`/`llm`/`ui` as platform
capabilities (ADR-0011 permission axis) and `secrets` as a **provider capability** resolved to
the vault provider (ADR-0022 + ADR-0024 §3). `llm` here is the seam to `agens` — **all AI
logic lives in agens (the public/private foundation boundary), not in open radix.**

**PROVIDES (provider):**

```toml
[capabilities.provided]
pim = "1.0.0"        # the pim@1.x CID (capabilities/pim.cid.toml)
```

`pim` provides a capability so that **other** plugins/agents can drive the PIM through the
mediated surface (e.g. a calendar plugin proposes a `pim:relation`, an assistant completes a
record) without direct refs — exactly the CID-mediated model (ADR-0022 §2).

### 6. Feature surface

- **Entities / fields / views / relations CRUD** — manual shape editing via unum over the
  meta-schema nodes (the user can also drive evolution by hand, not only via AI).
- **Records CRUD** — create/read/update/delete `pim:record`s, migrate-on-read against the
  current shape (§2.3); residue/quarantine surfaced for review.
- **AI shape proposals** — `agens` proposes `entity_def`/`field_def`/`view_def`/`relation`
  changes proactively (pattern noticed) or JIT (capture doesn't fit), gated by §4.
- **Lookups / search** — PluresDB **embeddings** power semantic lookup across records (find
  the contact/note/project by meaning, not exact key); embedding-indexed fields are declared on
  `field_def`.
- **Automated reports / analysis** — `agens` generates reports/rollups over the current shape
  (e.g. "everyone I haven't contacted in 90 days", "subscriptions renewing this month") as a
  mediated `generate_report` operation; output is a PluresDB node the UI renders.
- **Integrity / maintenance** — `agens`-driven `run_integrity_check`: find records failing
  current `field_def`s, dangling relations, migration residue needing attention, duplicate
  entities; emit `pim.integrity.alert`. Maintenance proposals route through the same
  propose→confirm→apply loop (no silent fixes to user data).

## Consequences

**Positive**
- A PIM that **outgrows frozen-schema competitors** because the model is data and evolves —
  proactively or JIT — without a compile/redeploy or manual row migration.
- Pure composition of foundation (unum + agens + PluresDB + `.px` + secrets); little net-new
  Rust in the *plugin* (the one big foundation piece — CRDT schema-evolution — is correctly
  routed to PluresDB where it benefits everyone).
- C-PLURES-003/004 hold: schema AND records AND the decision ledger all live in PluresDB; pure
  shape-gating logic in `.px`; AI strictly behind the `llm`/agens seam; UI is unum.
- Every shape change is auditable and replayable (decision ledger) — schema evolution is
  governed, not chaotic.

**Negative / costs**
- The CRDT schema-evolution/migration primitive (§2.6) is real, hard foundation work in
  pluresdb; until it lands, `pim` is limited to the additive/rename/non-destructive evolution
  path (still genuinely useful, and honest about the gap).
- unum may need a `schema-from-data` extension (§3) before the fully-dynamic CRUD UI is real;
  until confirmed, the dynamic path is pending.
- The propose→confirm→apply loop adds friction the user could find chatty if proactive
  proposals are too eager — mitigated by auto-apply for non-destructive ops + tunable agens
  proactivity.

**Risks**
- **Concurrent destructive shape edits** are the sharp edge. Mitigation: stable `field_id`s
  (rename ≠ delete), deterministic conflict resolution that preserves the losing op, residue
  instead of deletion, and the destructive-needs-confirmation invariant — but this must be
  proven with property tests at the pluresdb primitive layer (C-TEST-002), the same way the
  OASIS nullifier/double-spend guarantees were property-tested before `commerce` shipped.
- **agens over-reach** — an AI that proposes too aggressively, or tries to apply directly.
  Mitigation: the mediated boundary (agens can only write proposals), `.px` apply-gates it
  cannot bypass, authority/rate constraints, and a full decision ledger.
- **unum dynamic-shape gap turns out larger than expected** (§3) — mitigation: ship the
  additive/known-shape path first; treat full schema-from-data as a tracked unum work item, not
  a plugin hack; never stub it (C-NOSTUB-001).
- **Schema-version skew across P2P peers** producing transient "unknown field" states —
  mitigation: CRDT preserves unknown keys verbatim until the `add_field` op syncs; the shape
  (migrations log) is part of the synced graph, so peers converge on shape before record
  validity is judged (§2.5).

## Foundation dependencies this ADR surfaces (feed Task 3, `plan-radix-plugins` §3.B)

| Gap | Routes to (per §3.C) | Honest status now |
|-----|----------------------|-------------------|
| **Schema-evolution + record-migration primitive** (versioned node-types, CRDT-mergeable migration-op log, migrate-on-read, lossless residue, deterministic conflict resolution) — §2.6 | **PluresDB** (foundation) | ABSENT until built; `pim` ships additive/rename/non-destructive evolution (needs no new primitive) and reports lossy migration as not-yet-built (C-NOSTUB-001). Likely the biggest net-new foundation piece in the program. |
| **unum `schema-from-data` / runtime-defined collections** — §3 | **unum** repo | OPEN DEPENDENCY; confirm against unum source during dev (Pillar 2). If absent, generalize in unum (useful to every data-driven plugin), do not hack into `pim`. |
| **`secrets@1.x` provider** (vault) for sensitive fields | **pares-radix** capability (ADR-0024 §3) | Shared with financial-advisor / hyperswarm-git; `pim` is a consumer, not a re-implementer. |
| **`ctx.data` PluresDB-backed paved path** (no localStorage) | **pares-radix** host | `pim` is greenfield → MUST use `ctx.data`/PluresDB from day one (no localStorage debt to migrate). |

These are designed broadly (not pim-only patches) per the governing Task-3 rule: a homegrown
plugin revealing a missing host/foundation capability triggers a **general** capability, useful
to current and future plugins and community authors.

## Implementation outline (lifecycle work, gated; this ADR = Pillar 1 / design only)

The pares-radix dev lifecycle (design → dev → document → QA → deploy → verify) drives each
stage as gated subagents; this ADR is design only. No code here.

1. **`pim` plugin skeleton** — `plugin.toml` (the §5 capabilities + `[contributes.*]` for the
   CRUD UI), `procedures/*.px` (propose-gate, apply-gate, migrate-on-read, integrity-check),
   `ui/*.svelte` (generic EntityList/RecordDetail/ViewBoard on `@plures/design-dojo` + unum),
   `adapter/*.ts` (the agens/`llm` IO seam only), `capabilities/pim.cid.toml` (this CID),
   `tests/` (BLOCKING — `pim` is a provider, ADR-0024 §6 / C-TEST-002).
2. **Confirm the unum dynamic-shape path** (§3) by reading unum source; if a `schema-from-data`
   extension is needed, that is a separate gated unum work item.
3. **Land the additive/non-destructive evolution path** end-to-end (no new foundation primitive
   required) and prove the propose→confirm→apply loop + decision ledger against a real radix
   instance (verify-on-target).
4. **Foundation track (parallel, pluresdb):** the CRDT schema-evolution/migration primitive
   (§2.6) as its own ADR + gated lifecycle in pluresdb; `pim` consumes it once real, unlocking
   lossy/destructive migration with the residue+confirmation guarantees.
5. **QA channel-independent** (C-TEST-002): load `pim` in a real radix instance, drive
   `propose_shape_change`/`apply_shape_change`/`complete_record`/`generate_report`/
   `run_integrity_check` through the host API + `ctx.data`, assert PluresDB state (meta-nodes,
   records, ledger transitions) — never through a chat adapter.

## References
- ADR-0010 — Agens-first plugin model (AI/cognition behind the agens seam)
- ADR-0011 — Plugin security (platform capabilities, mediated IO, trust tiers, no direct refs)
- ADR-0021 — Grammar as generated artifact (`.px` grammar generated from praxis `px-ast`)
- ADR-0022 — Capability host contract (provider capabilities, CIDs, resolver, binding policy)
- ADR-0024 — Canonical plugin format (`plugin.toml` + `.px` + adapter + `ui/`; capability
  dependency registration; design-dojo UI home; provider test gate BLOCKS)
- `capabilities/commerce.cid.toml` — the reference CID this descriptor mirrors
- `capabilities/pim.cid.toml` — the companion `pim@1.x` CID authored alongside this ADR
- `unum` (`@plures/unum`) — reactive PluresDB→Svelte bindings; the CRUD-from-shape layer
- `pares-agens` (PRIVATE) — the AI/cognition layer driving proposals/reports/analysis
- C-PLURES-001..004 — extend don't reinvent; state in PluresDB; pure logic in PluresDB, IO at
  the boundary
- C-NOSTUB-001 — no stubs; lossy migration + unum dynamic-shape left ABSENT + reported until real
- C-TEST-001 / C-TEST-002 — channel-independent QA; build the binary, hit the API; provider
  tests BLOCK
- C-DRIFT-001 — generated artifacts derive from source (manifest from `plugin.toml`) in CI
- Program plan: `workspace/memory/plan-radix-plugins-2026-06-26.md` (§2.B thesis + hard questions)