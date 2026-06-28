# ADR-0028: Constraint-Engine Canonicalization — `pluresdb-px` (Rust, NAPI-bridged) is the Source of Truth; `@plures/praxis` Stays the Framework

- **Status:** Proposed (DESIGN stage of TASK-2026-06-27-006; FIX/rebind stage gated on this ADR)
- **Date:** 2026-06-27
- **Deciders:** kbristol (Level-1 architecture greenlight 2026-06-27: "fix architecture first, then orchestration"), dev-lifecycle orchestrator
- **Relates:** ADR-0017 (single praxis system-of-record = CrdtStore), ADR-0020 (single-PluresDB reactive memory), ADR-0022 (capability-host-contract), ADR-0023 (procedure observability event contract), ADR-0024 (canonical plugin format)
- **Filename note:** The 2026-06-25 directive said "land as ADR-0027." By the time this task ran, **ADR-0027 was already taken** (`ADR-0027-dev-lifecycle-spine-wiring.md`, Proposed 2026-06-25). Highest existing = 0027 → this lands as **ADR-0028**.
- **Supersedes framing of:** the 2026-06-25 verbal claim that *"Rust `pluresdb-px` never got a NAPI surface, so the constraint layer was rewritten in TS."* That premise is **factually wrong** (see Context); this ADR corrects it and pivots the decision accordingly.

## Context

The 2026-06-25 directive framed a "two constraint engines" problem: a Rust constraint engine
(`pluresdb-px`) supposedly never NAPI-published, forcing a TS rewrite (`@plures/praxis`); fix = expose the
Rust crate and demote the TS engine. A live read of the actual surfaces this session **refuted both halves
of that premise**:

**Finding 1 — `pluresdb-px`'s constraint/guidance core is ALREADY NAPI-exposed.** `pluresdb-node`
(`crates/pluresdb-node/src/lib.rs`) imports `pluresdb_px::db::{procedures, schema, seed, store}` + `px::parse`,
**seeds the built-in constraints into the CrdtStore**, and the NAPI-RS-generated `index.d.ts` declares a full
constraint API on `PluresDatabase`: `pxEvaluate`, `pxOnAction` (pre-action **block**, throws on error
severity), `pxCompileNl` (NL→enforcing `Condition` AST), `pxApplyCorrection`/`pxUndoCorrection`,
`pxLoadPxSource` (`.px` grammar), `pxInsertConstraint`, `pxQueryGaps`, plus `agensEmitPraxis` for praxis
lifecycle events. **The constraint enforcement path is already reachable from Node today.**

**Finding 2 — `@plures/praxis` is an app FRAMEWORK, not a constraint engine, and its engine slice is a
pass-through.** `src/index.ts` re-exports decision-ledger, chronos chronicle, an 8-phase lifecycle engine,
conversations, experiments, code-canvas, tauri, unum, hooks, schema codegen, and a unified reactive layer —
**none with a Rust equivalent.** The constraint slice (`src/core/engine.ts`, `src/core/rules.ts`) is literally
`export … from '@plures/praxis-core'`; the real engine lives in the separate workspace package
`packages/praxis-core/src/engine.ts`, where `LogicEngine.step(events)` checks constraints by invoking
**arbitrary TS closures** (`constraint.impl(newState)`), a Facts/Events/Rules *functional-derivation* model —
**semantically different** from `pluresdb-px`'s declarative `Condition`-AST invariant/blocking + `.px`
dataflow model.

So the true situation is: the *constraint-enforcement concept* overlaps, the Rust implementation is canonical
**and already bridged**, and the TS overlap is **one thin pass-through slice**. The genuinely-unbridged Rust
capability is the **guidance-metadata** layer (`db/guidance.rs`) and the broader `.px` procedure/dataflow
executor (only `exec_dsl`/`exec_ir` are exposed today). The fix is therefore **surgical**, not "replace the TS
package."

### Capability Venn (grounded — see evidence table for sources)

| Capability | Rust `pluresdb-px` | TS `@plures/praxis(-core)` | Verdict |
|---|---|---|---|
| Constraint evaluation (ctx→violations) | `procedures::evaluate`→`pxEvaluate` | `LogicEngine.step` loop | DUPLICATED concept; Rust **ALREADY-BRIDGED** |
| Pre-action blocking | `procedures::on_action`→`pxOnAction` | non-blocking diagnostics | DUPLICATED; Rust canonical + **ALREADY-BRIDGED** |
| NL→constraint compile | `compile_nl`→`pxCompileNl` | — | **RUST-ONLY** + bridged |
| `.px` grammar parse/persist | `px::parse`→`pxLoadPxSource` | — | **RUST-ONLY** + bridged |
| Corrections persist/undo | `apply_correction`→`pxApplyCorrection`/Undo | — | **RUST-ONLY** + bridged |
| Evidence gaps | `query_gaps`→`pxQueryGaps` | — | **RUST-ONLY** + bridged |
| Constraints persisted (CrdtStore SoR) | `seed_praxis_into_crdt` + constraint nodes | in-memory registry only | **RUST-ONLY** + bridged |
| **Guidance metadata** (entries/spans/analysis events) | `db/guidance.rs` GuidanceStore | — | **RUST-ONLY, NOT yet NAPI-exposed** ← real gap |
| `.px` dataflow/procedure executor | `px/{compiler,executor,dataflow,resolver,watcher,…}` | — | **RUST-ONLY**, partially bridged (`exec_dsl`/`exec_ir`) |
| Facts/Events/Rules derivation | — | praxis-core `defineFact/Event/Rule` | **TS-FRAMEWORK-ONLY** |
| Decision-ledger / chronos / lifecycle / conversations / experiments / code-canvas / tauri / unum / hooks / schema-codegen / unified | — | the rest of `@plures/praxis` | **TS-FRAMEWORK-ONLY** |
| Schema validate/template | `db/schema.rs` (constraint schema) | `core/schema/types.ts` (app/orchestration schema) | **NOT duplicated** (different schemas) |

## Decision

### 1. Canonical constraint/guidance engine = Rust `pluresdb-px`, via the existing `pluresdb-node` NAPI surface.
All **declarative**, `.px`-defined, CrdtStore-persisted constraints and the guidance/correction lifecycle route
through the already-exported NAPI methods (`pxEvaluate`/`pxOnAction`/`pxCompileNl`/`pxApplyCorrection`/
`pxUndoCorrection`/`pxLoadPxSource`/`pxInsertConstraint`/`pxQueryGaps`). **No new Rust export is required for
the constraint path** — it already exists. This makes `pluresdb-px` + the CrdtStore the single system of record
(consistent with ADR-0017), reachable from every Node consumer.

### 2. Surgical TS rebind — additive adapter in `praxis-core`, not a rewrite.
Introduce an opt-in `PluresDbConstraintAdapter` in `@plures/praxis-core` so that the `LogicEngine.step`
constraint loop can **delegate declarative constraints** to `pxOnAction`/`pxEvaluate` (single source of truth),
while **TS-closure rules and the Facts/Events derivation engine remain in TS** (they have no Rust peer). This is
the *only* code change to the engine slice and it is **additive**: default behavior unchanged; delegation is
enabled by config.

### 3. KEEP the entire `@plures/praxis` framework; public TS API stays stable.
Chronos, the 8-phase lifecycle engine, conversations, experiments, code-canvas, tauri, unum, hooks, schema
codegen, decision-ledger, and the unified reactive layer are **framework-only with no Rust equivalent** and are
**not demoted**. Framework consumers see **no breaking change**.

### 4. One real new Rust export — guidance metadata — DEFERRED and scoped (C-NOSTUB).
The genuinely-unbridged piece is `db/guidance.rs` (`GuidanceStore`, `GuidanceEntry`, `GuidanceCategory`,
`SourceSpan`, `AnalysisEvent`). Add `#[napi]` wrappers **only when the pluresLM→orchestration refactor concretely
consumes them** (analysis events / source spans feeding the coprocessor). **Do not stub now** — leave absent and
honestly reported as not-built until there is a real caller.

### 5. Sequencing — this is the prerequisite for "then orchestration."
- **Now (this ADR):** record the corrected canonical decision; no code.
- **Next (FIX stage):** land the `PluresDbConstraintAdapter` in `praxis-core` + tests proving declarative
  constraints resolve via `pxOnAction`; framework API regression-tested.
- **Then (orchestration refactor):** consume the bridged constraint/guidance + agens lifecycle events; add the
  deferred `guidance.rs` NAPI wrappers at the point of real use; wire ADR-0023 observability on the same events.

### What this explicitly forbids
- Re-implementing declarative constraint evaluation a third time in TS.
- "Demoting"/deleting framework subsystems that have no Rust peer.
- Adding speculative NAPI surface for `px/` procedure ops or `guidance.rs` with no concrete caller (no inert
  steps).

## Consequences

**Positive**
- Single source of truth for declarative constraints (Rust + CrdtStore), already reachable — minimal new code.
- Framework consumers unaffected (additive adapter, stable API).
- Unblocks the pluresLM→orchestration refactor and ADR-0023 observability (both ride already-bridged
  constraint/guidance + agens events).
- Kills the "rewrite the TS engine" misframing with evidence, preventing wasted re-implementation.

**Negative / risks**
- Two constraint *paradigms* coexist (declarative AST in Rust; TS-closure rules in praxis-core). Mitigation:
  the adapter routes only **declarative** constraints to Rust; closure rules stay TS by design, not by accident.
- Guidance-metadata NAPI work is deferred — orchestration must add it at point-of-use (tracked as C-NOSTUB
  deferred item, not a hidden gap).
- `pxOnAction` throws on block; the TS adapter must translate that to praxis `constraint-violation` diagnostics
  to preserve the existing TS contract (covered in FIX-stage tests).

## Evidence

| Observation | Tested? | Source |
|---|---|---|
| `pluresdb-px` constraint API IS NAPI-exposed (pxEvaluate/pxOnAction/pxCompileNl/pxApplyCorrection/pxUndoCorrection/pxLoadPxSource/pxInsertConstraint/pxQueryGaps) | Yes — read generated `.d.ts` | `crates/pluresdb-node/index.d.ts` |
| `pluresdb-node` imports px procedures/schema/seed/store + seeds constraints into CrdtStore | Yes | `crates/pluresdb-node/src/lib.rs` (`use pluresdb_px::db::…`, `seed_praxis_into_crdt`) |
| Rust constraint model = declarative `Condition` AST + `Severity`, `evaluate`→`Vec<Violation>`, `on_action`→`ActionBlocked` | Yes | `crates/pluresdb-px/src/db/procedures.rs:97,142,213,532,578`; `…/db/schema.rs:30,135,151,261` |
| Guidance-metadata layer exists but is NOT individually NAPI-exposed (real gap) | Yes | `crates/pluresdb-px/src/db/guidance.rs` (GuidanceStore/Entry/Category/SourceSpan/AnalysisEvent); absent from `index.d.ts` |
| `@plures/praxis` is a framework; constraint slice is a re-export of `@plures/praxis-core` | Yes | `praxis/src/index.ts`; `praxis/src/core/engine.ts` + `core/rules.ts` (`export … from '@plures/praxis-core'`) |
| Real TS engine lives in separate package; constraints = TS closures in `step()` | Yes | `praxis/packages/praxis-core/src/engine.ts:82,133,259-290` (`constraint.impl(newState)`) |
| ADR-0027 already taken → use 0028 | Yes | `pares-radix/.praxis/decisions/ADR-0027-dev-lifecycle-spine-wiring.md`; `git ls-files` max = 0027 |
| House style (Status/Date/Deciders, Evidence "Tested?" table) | Yes | `ADR-0023-…md` (evidence table), `ADR-0024-…md` (structure) |
| New `guidance.rs` NAPI wrappers — designed, not built (deferred C-NOSTUB) | No — intentionally deferred | this ADR §4; no caller yet |
| `PluresDbConstraintAdapter` rebind — designed, not built (FIX stage) | No — design only | this ADR §2; gated on approval |

## References
- ADR-0017 (single praxis system-of-record = CrdtStore) — `pares-radix/.praxis/decisions/`
- ADR-0020, ADR-0022, ADR-0023, ADR-0024 — `pares-radix/.praxis/decisions/`
- Rust engine: `pluresdb/crates/pluresdb-px/src/{lib.rs,db/{procedures,guidance,schema,seed,store}.rs,px/*}`
- NAPI bridge: `pluresdb/crates/pluresdb-node/{src/lib.rs,index.d.ts,Cargo.toml}`
- TS framework + engine: `praxis/src/index.ts`, `praxis/src/core/{engine,rules}.ts`, `praxis/packages/praxis-core/src/engine.ts`
- Analysis note: `~/.openclaw/workspace/memory/arch-fix-adr-2026-06-27.md`
