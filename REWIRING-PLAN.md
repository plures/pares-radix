# Reactive Rewiring — Implementation Plan

Status: **PHASES 1–4 COMPLETE, Phase 5 IN PROGRESS** ✅
Started: 2026-06-13
Last updated: 2026-06-13

## Goal

Wire the spine pipeline to use .px procedures via ReactiveRegistry instead of hardcoded Rust logic.
The .px files already exist. The Rust infra (ReactiveRegistry, PxBridge, DataflowBridge) already exists.
The gap was: startup registration, procedure execution, cascading, and IO boundary handlers.

## ✅ Phase 1: Reactive Registration at Startup (DONE)

- `bootstrap.rs`: Loads all .px files, registers in ReactiveRegistry with trigger patterns
- `main.rs`: Creates ReactiveRegistry, wires Pipeline::with_reactive, calls bootstrap
- `InboundRouter::with_reactive(Arc<ReactiveRegistry>)` constructor

## ✅ Phase 2: Classification-Driven Routing (DONE)

- InboundRouter subscribes to `route_decision:{id}` before firing `inbound:{id}`
- Awaits .px classification with configurable timeout (5s default)
- Routes based on .px decision (destination/tier/reason)
- Falls back to direct ModelRequest on timeout

## ✅ Phase 3: Model Tier Selection (DONE)

- ModelInvoker reads `metadata.model_tier` from .px routing decisions
- Tier mapping: fast → small model, standard → default, premium → large
- Logs .px provenance (routed_by, route_reason)

## ✅ Phase 4: IO Boundary Action Handlers (DONE)

- `CoreActionHandler`: read_state, write_state, read_history, append_history
- `CompositeActionHandler`: composes core + tool dispatch for .px procedures
- Wired into main.rs bootstrap — .px procedures get real conversation state access
- Pipeline passes full serialized SpineEvent to on_write (not just type+id)

## ✅ Phase 4.5: Reactive Cascade (DONE)

- Procedure output is written back to registry via `derive_output_key`
- Key derivation: inbound→route_decision, route_decision→model_request, model_response→delivery
- waiters field is Arc<RwLock<...>> for spawned task notification
- End-to-end test: subscribe→trigger→cascade→receive verified

## Phase 5: Full Quiescence Model (IN PROGRESS)

The remaining work to reach the "PluresDB IS the router" architecture:

### Done:
- [x] Pipeline event loop feeds ALL events through ReactiveRegistry
- [x] .px procedures fire on every event type (inbound, model_response, delivery_request, etc.)
- [x] Procedure outputs cascade to downstream subscribers
- [x] CoreActionHandler provides real state access to .px

### Remaining:
- [ ] Remove ResponseRouter Rust procedure (now redundant — route_model_response.px handles it)
- [ ] Remove HistoryRecorder Rust procedure (track_inbound.px handles it via append_history action)
- [ ] Remove CommitmentDetector Rust procedure (detect_and_store_commitments.px handles it)
- [ ] Wire PluresDB for write_state (currently stub)
- [ ] Add embed_text/recall_memories actions to CoreActionHandler (for assemble_context)
- [ ] Add channel_send action (for deliver_response — the final IO boundary)
- [ ] Pipeline becomes pure transport (no procedures list, just ReactiveRegistry + IO actors)

### IO Actors That Remain in Rust (by design):
1. **ModelInvoker** — calls model API (HTTP), returns response
2. **ToolExecutor** — runs tools (shell, file, network), returns results
3. **ChannelDelivery** — sends to Telegram/Discord/etc.
4. **EmbeddingService** — calls embedding model for semantic recall

Everything else is .px → ReactiveRegistry → action handlers.

## Architecture Summary (Current State)

```text
Inbound event → Pipeline dispatches to:
  ├─ InboundRouter (Rust) — subscribes/fires/awaits reactive cascade
  │     └→ ReactiveRegistry on_write("inbound:{id}") — full event payload
  │           └→ classify_and_route.px fires
  │                 └→ output cascades to route_decision:{id}
  │                       └→ InboundRouter subscriber receives decision
  │
  ├─ HistoryRecorder (Rust) — records to ConversationStore [TO REMOVE]
  │     (Redundant: track_inbound.px does same via append_history action)
  │
  ├─ ModelInvoker (Rust IO) — reads tier from metadata, calls model
  │     └→ emits ModelResponse
  │
  ├─ ResponseRouter (Rust) — skip if tool_calls, else emit delivery [TO REMOVE]
  │     (Redundant: route_model_response.px does same via cascade)
  │
  └─ Pipeline ReactiveRegistry on_write(event_type:{id}, full_event)
        └→ ALL .px procedures fire on matching patterns
              ├─ track_inbound: records user message (inbound:*)
              ├─ detect_and_store_commitments: scans for promises (model_response:*)
              ├─ route_model_response: routes final responses (model_response:*)
              └─ deliver_response: sends to channel (delivery_request:*)
```

## Commits

| Hash | Phase | Description |
|------|-------|-------------|
| 097081c | cascade | Reactive cascade — procedure output writes back to registry |
| 7e5fc2f | 4 | Enrich reactive pipeline + expand trigger map |
| a7fa1c2 | 4 | CoreActionHandler + CompositeActionHandler |
| ddb27fe | 3 | ModelInvoker tier-based model selection |
| 3213bb1 | docs | REWIRING-PLAN Phase 1+2 completion |
| 196eac2 | 1+2 | Wire reactive .px pipeline end-to-end |
| ded29b2 | 1.1-1.2 | Reactive bootstrap + InboundRouter rewiring |

## Test Results (current)

```
84 spine tests pass:
- 19 reactive (patterns, registry, subscribe_result, cascade)
- 6 inbound_router (passthrough, fallback, reactive routing, procedural skip)
- 9 model_invoker (tier mapping, px metadata, messages, errors)
- 4 actions (CoreActionHandler: history, state, unknown)
- 4 bootstrap (registration, empty dirs, manual skip)
- 42 other spine (pipeline, history_recorder, commitment_detector, etc.)
```
