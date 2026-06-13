# Reactive Rewiring — Implementation Plan

Status: **PHASE 1+2 COMPLETE** ✅
Started: 2026-06-13
Last updated: 2026-06-13

## Goal

Wire the spine pipeline to use .px procedures via ReactiveRegistry instead of hardcoded Rust logic.
The .px files already exist. The Rust infra (ReactiveRegistry, PxBridge, DataflowBridge) already exists.
The gap: startup registration + InboundRouter delegation.

## ✅ Phase 1: Reactive Registration at Startup (DONE)

### ✅ 1.1 `crates/core/src/spine/bootstrap.rs` — Created

- Reads all .px files from configured directories
- Compiles via `PxProcedureAdapter`
- Registers in `ReactiveRegistry` with trigger patterns extracted from procedure metadata
- Falls back to `inbound:*` for procedures without explicit triggers
- Tests pass (register_valid_procedure, register_from_empty_dir, register_from_nonexistent_dir, manual_procedures_skipped)

### ✅ 1.2 `crates/core/src/spine/procedures/inbound_router.rs` — Modified

- `InboundRouter::with_reactive(Arc<ReactiveRegistry>)` constructor
- Subscribes to `route_decision:{id}` before firing `inbound:{id}` write
- Awaits .px routing decision with configurable timeout (5s default, test-overrideable)
- Routes based on .px result:
  - `destination: "procedural"` → skips ModelRequest entirely
  - `destination: "heartbeat"` → emits fast-tier ModelRequest
  - `destination: "conversation" | "task_steering"` → emits ModelRequest with `model_tier` from .px in metadata
- Falls back to direct ModelRequest on timeout or no reactive (preserves pre-rewiring behavior)
- 6 tests pass including reactive_route_decision_drives_model_request and reactive_procedural_route_skips_model_request

### ✅ 1.3 `crates/core/src/spine/reactive.rs` — Enhanced

- `subscribe_result(key) -> oneshot::Receiver<Value>` — for awaiting specific write outputs
- `on_write()` notifies all waiters on exact key match
- `emitter` field wrapped in `RwLock<Option<>>` with `set_emitter()` for post-construction wiring
- 17 tests pass (pattern matching, registry ops, subscribe_result receive/timeout/multiple/non-matching)

### ✅ 1.4 `crates/cli/src/main.rs` — Wired

- Creates `ReactiveRegistry::new()` + `Pipeline::with_reactive(256, registry)`
- Calls `registry.set_emitter(pipeline.emitter())` to close the feedback loop
- Calls `bootstrap::register_reactive_procedures()` on `praxis/procedures/` and `praxis/spine/` dirs
- Passes registry to `InboundRouter::with_reactive()`
- Logs trigger count and procedure count on startup

## ✅ Phase 2: Classification-Driven Routing (DONE)

The InboundRouter now:
1. Subscribes to `route_decision:{event_id}` (oneshot channel)
2. Fires `reactive.on_write("inbound:{id}", ...)` — triggers `classify_and_route` from unified-router.px
3. Awaits the .px procedure chain to write back to `route_decision:{id}`
4. Uses the result (`tier`, `destination`, `reason`) to determine pipeline behavior
5. Falls back gracefully on timeout

**No more fire-and-forget.** The .px classification directly drives model tier selection.

## Phase 3: Model Invocation via .px (NEXT)

The `ModelInvoker` currently hardcodes model selection. With `model_tier` now flowing in metadata from .px routing:

- [ ] ModelInvoker reads `metadata.model_tier` to select model endpoint
- [ ] Tier → model mapping: `fast` → small/cheap, `standard` → default, `premium` → large/capable
- [ ] ModelInvoker respects `metadata.routed_by == "px"` to log provenance

## Phase 4: Context Assembly via .px

- [ ] `assemble_context` from unified-router.px replaces hardcoded history building
- [ ] Memory recall (embeddings + semantic search) wired through action handler
- [ ] Entity extraction wired through action handler

## Phase 5: Full Quiescence Model

- [ ] Remove remaining Rust orchestration procedures (HistoryRecorder, ResponseRouter)
- [ ] Replace with reactive cascades that fire on queue writes
- [ ] Pipeline event loop becomes purely a transport layer
- [ ] System is "done" when all reactive queues are empty

## Architecture Summary

```text
Inbound event → Pipeline dispatches to InboundRouter
  → InboundRouter subscribes to route_decision:{id}
  → InboundRouter writes inbound:{id} to ReactiveRegistry
  → ReactiveRegistry pattern-matches inbound:* → spawns classify_and_route.px
  → classify_and_route writes route_decision:{id}
  → Subscriber notified → InboundRouter reads decision
  → InboundRouter emits ModelRequest (with tier) or skips (procedural)
  → Pipeline continues with ModelInvoker → ToolExecutor → ResponseRouter
```

**IO boundaries in Rust only:**
- Model API calls (ModelInvoker)
- Tool execution (ToolExecutor)
- Channel delivery (ResponseRouter → Telegram/Discord)

**Everything else in .px:**
- Classification, routing, context assembly, memory recall, commitment detection

## Commits

| Hash | Phase | Description |
|------|-------|-------------|
| c2c53a7 | 1+2 | Wire reactive .px pipeline end-to-end |

## Test Results

```
88 spine tests pass:
- 17 reactive (pattern matching, registry, subscribe_result)
- 6 inbound_router (passthrough, fallback, reactive routing, procedural skip)
- 65 other spine (pipeline, bootstrap, model_invoker, tool_executor, etc.)
```
