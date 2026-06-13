# Reactive Rewiring — Implementation Plan

Status: IN PROGRESS
Started: 2026-06-13

## Goal

Wire the spine pipeline to use .px procedures via ReactiveRegistry instead of hardcoded Rust logic.
The .px files already exist. The Rust infra (ReactiveRegistry, PxBridge, DataflowBridge) already exists.
The gap: startup registration + InboundRouter delegation.

## Phase 1: Reactive Registration at Startup (THIS SESSION)

### 1.1 Create `crates/core/src/spine/bootstrap.rs`

New module that:
- Reads all .px files from `praxis/procedures/` and `praxis/spine/`
- Compiles them via `PxProcedureAdapter`
- Registers them in the `ReactiveRegistry` with appropriate trigger patterns:
  - `classify.px` → trigger on `inbound:*`
  - `routing.px` → trigger on `classification:*`
  - `context-window.px` → trigger on `inbound:*`
  - `heartbeat-logic.px` → trigger on `heartbeat:*`
  - `retention.px` → trigger on `memory:*`
  - `memory-correction.px` → trigger on `memory:*`

### 1.2 Modify `InboundRouter` (`crates/core/src/spine/procedures/inbound_router.rs`)

Currently: receives Inbound event → emits ModelRequest directly.
After: receives Inbound event → writes to PluresDB key `inbound:{id}` → ReactiveRegistry fires `classify.px` → classification result written to `classification:{id}` → ReactiveRegistry fires `routing.px` → routing result feeds into ModelRequest emission.

Fallback: if reactive path doesn't produce a result within timeout, fall back to direct ModelRequest emission (current behavior).

### 1.3 Modify `Pipeline::run()` to call `bootstrap::register_reactive_procedures()` before entering the event loop

### 1.4 Wire `Cerebellum` construction in `crates/cli/src/main.rs` (or wherever the app starts) to pass the ReactiveRegistry and load .px at boot.

## Phase 2: Classification via .px (NEXT)

- Remove direct `CerebellumClassifier` calls from `cerebellum/mod.rs`
- Replace with: write inbound to PluresDB → reactive fires classify.px → read result
- Keep Rust classifier as fallback (timeout or .px not loaded)

## Phase 3: Routing via .px

- Same pattern for `router.rs` → `routing.px`

## Phase 4: Context window, memory, personality

- Each gets the same treatment

## Files to Create/Modify

| File | Action |
|------|--------|
| `crates/core/src/spine/bootstrap.rs` | CREATE — reactive procedure loader |
| `crates/core/src/spine/mod.rs` | MODIFY — add `pub mod bootstrap;` |
| `crates/core/src/spine/procedures/inbound_router.rs` | MODIFY — add PluresDB write + reactive path |
| `crates/core/src/spine/pipeline.rs` | MODIFY — call bootstrap at startup |
| `crates/cli/src/main.rs` | MODIFY — pass praxis dir path to pipeline |

## Commit Strategy

One commit per phase step (1.1, 1.2, etc.) so each is reviewable and revertable.
