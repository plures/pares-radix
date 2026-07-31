# ADR-0039: Spine Assembly Real Transport — Wire `pares-radix-svc` into the Reactive Spine

- **Status:** Proposed (DESIGN stage of epic `pares-radix-svc:spine-assembly-real-transport`)
- **Date:** 2026-07-30
- **Deciders:** kbristol (human gate on Risks §7), dev-lifecycle orchestrator
- **Relates:** ADR-0018 (radix-runtime-as-service), ADR-0027 (dev-lifecycle spine wiring — same class of "built but never wired" gap), ADR-0036 (praxisbot-native-task-dashboard, `.praxis/decisions/ADR-0036-praxisbot-native-task-dashboard.md`), audit PR pares-radix#589 (`docs: px-first spine audit for pares-radix + pares-agens`)
- **Blocks:** `praxisbot:px-native-task-dashboard`
- **Filename note:** ADR-0037/ADR-0038 taken by parallel work; this lands as ADR-0039 (`.praxis/decisions` currently ends at ADR-0038-deterministic-git-projection.md, verified via `git ls-tree main .praxis/decisions`).

---

## 1. Context

PR#571 shipped `pares-radix-svc` (`crates/pares-radix-svc`) as "DEV stage of ADR-0018 runtime-as-service": a thin binary that ticks `AgensRuntime::poll_events`/`process_due_timers` on a fixed interval, persists via a bare `CrdtStore` (`SledStorage`/`MemoryStorage`), and exposes a loopback HTTP surface (`/healthz`, `/readyz`, `/events` GET+POST, `/timers`). This was correct **for ADR-0018's own scope**: ADR-0018 explicitly asked for a minimal service shell around the existing timer/event primitives, "no new storage layer."

Separately, `crates/radix-core/src/spine/runtime.rs` (built for ADR-0027's dev-lifecycle wiring) assembles the **real spine**: `build_reactive_runtime_with_subagent` wires a `PluresDbStateStore` (`StateStore`) + `PluresConversationStore` (`ConversationStore`) + `CompositeActionHandler` (which composes `CoreActionHandler`, `TaskDashboardActionHandler`, `DevLifecycleActionHandler`, `SubagentActor`, `ToolDispatchActionHandler`) into a `Pipeline` that a `ReactiveRegistry` drives via `.px` procedures loaded from `praxis/procedures/*.px`. `task_dashboard_get` (ADR-0036) is one of those procedures: it fires on `task:cmd:dashboard:get:*` writes and calls `aggregate_task_dashboard` against the same `StateStore`.

**These two assemblies do not share a process.** `pares-radix-svc` never calls `build_reactive_runtime*`; it constructs its own bare `AgensRuntime<'static>` directly against a raw `CrdtStore`. Audit PR#589 (docs-only, no code changes) confirmed via grep that `CompositeActionHandler::new` and `register_reactive_procedures` had zero non-test callers reachable from `pares-radix-svc` — the dashboard `.px` procedure, though fully built and unit-tested, is unreachable from the running service binary. This blocks `praxisbot:px-native-task-dashboard`, which needs `task_dashboard_get` to actually fire in the deployed praxisbot process.

Three concrete gaps block a direct "swap the constructor" fix:

### Gap 1 — storage model mismatch
`pares-radix-svc::SharedState` owns a raw `&'static CrdtStore` and constructs `AgensRuntime::new(store, SERVICE_ACTOR)` directly. `build_reactive_runtime_with_subagent` instead wants a `Arc<dyn StateStore>` (backed by `PluresDbStateStore`, which itself wraps a `CrdtStore` + `SledStorage`/in-memory) and a separate `Arc<dyn ConversationStore>` (backed by `PluresConversationStore`, co-located on the *same* underlying `CrdtStore` per C-PLURES-003/004 "single durable store" discipline). There is currently no single assembly function that produces: (a) the durable `CrdtStore` open against `RADIX_SVC_DATA_DIR`, (b) a `PluresDbStateStore` and `PluresConversationStore` sharing that same store, AND (c) the bare `AgensRuntime` timer/event primitives `pares-radix-svc`'s HTTP surface (`/timers`, `GET /events`) already depends on. A naive constructor swap would either (a) open two independent `CrdtStore`s (silently splitting durable state, violating C-PLURES-003/004 the same way ADR-0027 called out), or (b) drop the existing timer/event HTTP endpoints that `pares-radix-svc`'s own smoke tests exercise.

### Gap 2 — event model mismatch
`POST /events` in `pares-radix-svc` deserializes an `AgensEvent` (the `pluresdb_procedures::agens` timer/event model) and calls `runtime.emit_event(&event)` — a CRDT node write in the `AgensRuntime`'s own namespace, nothing more. The reactive spine's `ReactiveRegistry`/`Pipeline` is driven by `SpineEvent` (`crates/radix-core/src/spine/event.rs`: `Inbound`, `ModelRequest`, `ModelResponse`, `ToolRequest`, `ToolResult`, …) flowing through `Pipeline::run`'s `on_write` dispatch, and by raw `StateStore` key writes matching the trigger-map prefixes in `spine::bootstrap::default_trigger_map()` (e.g. `task:cmd:dashboard:get:*` → `task_dashboard_get`). An `AgensEvent` POST today never becomes a `SpineEvent`, never triggers `on_write`, and never touches a `dashboard:tasks:*`/`task:cmd:dashboard:get:*` key. Even with Gap 1 fixed and a `ReactiveRuntime` constructed and spawned in-process, nothing in `pares-radix-svc`'s current HTTP surface can reach `task_dashboard_get` — there is no code path from any exposed endpoint to `registry.on_write(...)`/`pipeline.emitter()`.

### Gap 3 — no HTTP translation layer for dashboard commands or reactive results
Even assuming Gap 2 is closed generically (some endpoint can write an arbitrary `StateStore` key or emit an arbitrary `SpineEvent`), there is no dashboard-specific request/response envelope. `task_dashboard_get` is triggered by a `task:cmd:dashboard:get:{surface_id}`-shaped key write and returns via `aggregate_task_dashboard`'s reactive procedure return value (`$dashboard`), not a direct function return — reactive procedures write their result, they don't hand it back synchronously to whoever wrote the trigger key. Today `pares-radix-svc` has no route that (a) accepts a dashboard-get request over HTTP, (b) knows to write the `task:cmd:dashboard:get:{surface_id}` key with the right shape, and (c) polls/awaits the corresponding `dashboard:tasks:{surface_id}` cache write to answer the HTTP caller. This "command envelope in, reactive result out" translation does not exist for ANY `.px` procedure yet, not just the dashboard one.

## 2. Decision (proposed — DESIGN stage only, no implementation in this ADR)

Resolve all three gaps with one shared assembly plus one thin, generic HTTP translation layer, in this order:

### 2.1 Gap 1 fix — one shared durable-store assembly function

Add `pares_radix_svc::assembly::build_service_stores(data_dir: Option<&Path>) -> ServiceStores`, a single function (analogous to `build_default_reactive_runtime` in `radix-core`, but returning the raw pieces instead of a fully-spawned runtime so `pares-radix-svc` retains control of its own lifecycle/drain semantics) that:

1. Opens exactly one `CrdtStore` — reusing `pares-radix-svc`'s existing `SledStorage`/`MemoryStorage` selection logic verbatim (no new storage backend; this satisfies both ADR-0018 §"no new storage layer" and C-PLURES-003/004's single-store rule).
2. Wraps that same `CrdtStore` in a `PluresDbStateStore` (via the existing `PluresDbStateStore::open`-equivalent that accepts an already-open store rather than opening its own — a small `radix-core` addition, `PluresDbStateStore::from_store(store: CrdtStore)`, since `PluresDbStateStore::open` currently opens its own directory and would otherwise double-open the same sled path) — giving `Arc<dyn StateStore>`.
3. Constructs `PluresConversationStore::new(pdb.crdt_store())` over the SAME store handle — giving `Arc<dyn ConversationStore>`.
4. Retains a `&'static AgensRuntime` (`Box::leak`, unchanged pattern) over a shared reference to the same underlying `CrdtStore`, so the existing `/timers` and `GET /events` (poll) endpoints keep working exactly as today — these are genuinely orthogonal to the reactive spine (timer scheduling vs. `.px` reactive dispatch) and ADR-0018's scope for them stands.
5. Returns `ServiceStores { crdt_store: &'static CrdtStore, state_store: Arc<dyn StateStore>, conversation_store: Arc<dyn ConversationStore>, agens_runtime: AgensRuntime<'static> }`.

`ServiceLifecycle::new` calls `build_service_stores` once, keeps `agens_runtime` for the existing scheduler/timer code path unchanged, and additionally calls `pares_radix_core::spine::runtime::build_reactive_runtime_with_subagent(state_store, conversation_store, tool_dispatcher, task_manager, subagent, praxis_dir, capacity)` to obtain a `ReactiveRuntime`, spawning it (`ReactiveRuntime::spawn`) alongside the existing scheduler task in `ServiceLifecycle::run`. `tool_dispatcher` starts as a minimal `NullToolDispatcher` (returns no tools, errors on call) since `pares-radix-svc` has no LLM/tool backend yet — that limitation is scoped OUT of this ADR (task/dashboard procedures don't call tools) and tracked as follow-up debt, not silently stubbed: the dispatcher's `call_tool` returns a real `ExecutionError`-shaped JSON error, never a fabricated tool result. `task_manager`/`subagent` start as `None` — dashboard aggregation needs neither; wiring them is separately-tracked work for whichever epic needs `dispatch_task`/`spawn_subagent` live in this service.

`praxis_dir` resolves via the existing `resolve_praxis_dir()` (repo-relative `praxis/procedures`, `RADIX_PRAXIS_DIR` override) — `pares-radix-svc` must ship/mount the `.px` procedure directory alongside the binary; this is a deployment note, not new code (praxisbot's NixOS packaging already needs to include `praxis/procedures/` for `TaskDispatchActionHandler`'s existing procedures — confirm this in IMPL stage, don't assume).

### 2.2 Gap 2 fix — SpineEvent emission wiring for `POST /events`

Do NOT repurpose the existing `POST /events` (`AgensEvent`) endpoint — that is a distinct, already-tested API surface (timer/event CRDT log) with its own consumer (`GET /events` poll, smoke tests). Instead add a **new, separate** route `POST /spine/events` that:

1. Accepts a `SpineEventRequest` body — NOT a raw `SpineEvent` (the existing `radix-core::spine::event::SpineEvent` enum variants carry internal fields like `metadata`/`tool_calls` that are an internal pipeline contract, not a stable external wire format). `SpineEventRequest` is a minimal external envelope: `{ "kind": "inbound", "source": String, "chat_id": String, "sender": String, "content": String, "metadata": Value }` — mapping 1:1 onto `SpineEvent::Inbound` today, extensible to other variants later without changing the wire contract's shape philosophy.
2. Maps the request to a `SpineEvent` and sends it via the `ReactiveRuntime`'s pipeline emitter (`pipeline.emitter()` — already `Clone`, already used internally by `TaskDispatcher::with_pipeline_emitter`; `ReactiveRuntime` needs one new accessor, `ReactiveRuntime::emitter(&self) -> PipelineEmitter` alongside its existing public fields, since today the emitter is only handed to internal callers during construction).
3. Returns `202 Accepted` with the generated event id — this is fire-and-forget into the reactive loop, matching `Pipeline::run`'s async `on_write` dispatch model; it is NOT the dashboard-specific request/response endpoint (that's Gap 3).

This closes Gap 2 generically: any `SpineEvent::Inbound` (the variant `InboundRouter`/`.px` procedures key off) can now reach the pipeline from outside the process. Dashboard-specific requests still need Gap 3 because `task_dashboard_get` is triggered by a `StateStore` key write (`task:cmd:dashboard:get:*`), not a `SpineEvent`.

### 2.3 Gap 3 fix — HTTP translation layer for dashboard command envelopes

Add a third route, `POST /dashboard/tasks/:surface_id`, implemented as a thin, dashboard-specific translation handler (not a generic "write any key" endpoint — that would bypass the `task_dashboard_never_writes_source_namespaces` guard's intent of dashboard access being narrow and purpose-built):

1. Handler writes `task:cmd:dashboard:get:{surface_id}` via `state_store.set(...)` with a minimal request-marker value (e.g. `{"requested_at": <now>}`) — this is exactly the key shape `default_trigger_map()` already routes to `task_dashboard_get` (confirmed in `spine/bootstrap.rs`).
2. Because `task_dashboard_get`'s reactive procedure writes its result to `dashboard:tasks:{surface_id}` (the `task_dashboard_view` entity's cache namespace) rather than returning synchronously, the handler then polls `state_store.get("dashboard:tasks:{surface_id}")` with a short bounded retry (e.g. 5 attempts × 100ms, configurable) until the write lands or the poll budget is exhausted.
3. On success: `200 OK` with the aggregated dashboard JSON. On poll-budget exhaustion: `202 Accepted` with `{"status": "pending", "surface_id": ...}` (NOT a fabricated dashboard — an honest "not ready yet" per C-NOSTUB-001) so callers can retry or fall back to a `GET /dashboard/tasks/:surface_id` (a second, simpler route that just reads the cache without triggering aggregation — for polling after a `202`).
4. This pattern (write command key → poll derived cache key) is scoped explicitly to the dashboard's read/cache namespace pair for THIS ADR. It is not proposed as a generic "any `.px` procedure gets an HTTP command envelope" mechanism — that would need its own ADR (a generic async-command-envelope protocol, correlation ids, timeouts, and probably a `SpineEvent` variant for command completion) and is explicitly out of scope here. If a second procedure needs the same shape, extract the poll-loop helper into a small reusable function (`radix-core::spine::http_bridge` or similar) at that point — not before, per "extract on the second real use," not speculative generalization.

## 3. What this ADR does NOT decide

- Which `ToolDispatcher` implementation `pares-radix-svc` uses for real tool calls (LLM-backed, MCP-backed, etc.) — the `NullToolDispatcher` above is an honest "no tools yet," not a stub of a working dispatcher; there is no advertised tool-calling capability.
- Whether `TaskManager`/`SubagentActor` get wired into `pares-radix-svc` — separately scoped; dashboard aggregation and `SpineEvent` ingestion do not need them.
- A generic command/response envelope protocol for arbitrary `.px` procedures (§2.3.4) — deliberately deferred to its own ADR if/when a second consumer needs it.
- Whether `praxisbot`'s NixOS packaging currently bundles `praxis/procedures/` alongside the `pares-radix-svc` binary — an IMPL-stage verification item, not assumed true here.

## 4. Consequences

**Positive:**
- One durable `CrdtStore` per process, shared by the timer/event primitives AND the reactive spine — no silent state-store split (closes the C-PLURES-003/004 risk Gap 1 called out).
- `task_dashboard_get` becomes reachable from a real HTTP surface without bypassing ADR-0036's namespace-isolation guard (`task_dashboard_never_writes_source_namespaces`) — the new `/dashboard/tasks/:surface_id` route only ever writes the `task:cmd:dashboard:get:*` trigger key and reads `dashboard:tasks:*`, mirroring the same discipline the `.px` constraint already enforces internally.
- `POST /spine/events` gives future procedures (not just the dashboard) an external `SpineEvent::Inbound` entry point without overloading the existing, already-tested `AgensEvent` endpoint.

**Negative / costs:**
- Two reactive drivers now run in one process (the existing `AgensRuntime` scheduler tick AND the new `ReactiveRuntime`'s `Pipeline::run` loop) — more moving parts to drain cleanly on shutdown; `ServiceLifecycle::run`'s existing drain-grace logic must be extended to abort/await the `ReactiveRuntime`'s spawned task too (currently only aborts `scheduler_task`/`server_task`).
- `PluresDbStateStore::from_store` is a new `radix-core` API surface (small, but it's a public API change, not purely internal to `pares-radix-svc`).
- The poll-based command/response bridge (§2.3.2) has an inherent latency floor (bounded retry) rather than a push-based completion signal — acceptable for a low-frequency on-demand dashboard view, explicitly NOT acceptable if reused for a high-frequency or latency-sensitive procedure (documented as the reason §2.3.4 defers a generic mechanism).

## 5. Alternatives considered

- **Naive constructor swap** (replace `AgensRuntime::new` with a call into `build_reactive_runtime`, drop the existing timer/event HTTP surface): rejected — breaks `pares-radix-svc`'s existing, tested `/timers` and `GET/POST /events` contract, which ADR-0018 explicitly scoped in; this ADR's shared-store approach keeps both working side by side.
- **Reuse `POST /events` (`AgensEvent`) for spine ingestion** by adding a variant or a sentinel field: rejected — conflates two independent event models (timer/event log vs. spine pipeline dataflow) behind one wire type, exactly the "second authoritative copy" anti-pattern ADR-0027 named for the JS driver/`.px` duplication.
- **Generic "write any StateStore key over HTTP" endpoint** instead of the dashboard-specific route: rejected — would let any HTTP caller write into any namespace including the four source namespaces `task_dashboard_never_writes_source_namespaces` protects; the dashboard-specific handler enforces the same narrow-write discipline the `.px` constraint already models, at the transport boundary instead of only inside the procedure.

## 6. Verification plan for IMPL stage (not executed here — design only)

1. `cargo test -p pares-radix-svc` — existing smoke tests (`crates/pares-radix-svc/tests/smoke.rs`) must still pass unmodified (confirms Gap 1's shared-store change doesn't regress the timer/event surface).
2. New smoke test: start `ServiceLifecycle`, `POST /spine/events` an `Inbound` event addressed at a chat id with no matching `.px` trigger — assert `202` and no panic (confirms Gap 2's wiring doesn't require a live LLM/tool backend to not crash).
3. New smoke test: `POST /dashboard/tasks/:surface_id` against a store with at least one `task:{id}` node — assert eventual `200` with a non-empty `open_count`/`waiting_count`/etc., proving `task_dashboard_get` fired end-to-end through the real HTTP surface (this is the concrete "unblocks praxisbot:px-native-task-dashboard" proof, run against a service process, not a unit test double).
4. Drain/shutdown test: send shutdown signal while a `/dashboard/tasks/:surface_id` poll is in flight — assert the reactive runtime task is aborted within `drain_grace`, same as the existing scheduler/server drain assertions.

## 7. Open question for human gate

Should the new `/spine/events` and `/dashboard/tasks/:surface_id` routes be gated behind the SAME `RADIX_SVC_AUTH_TOKEN`/loopback-only fail-closed policy that `ServiceConfig::validate` already enforces for the existing surface (§4, ADR-0018), or does the dashboard's read path warrant a narrower read-only token scope? Recommendation: reuse the existing all-or-nothing bearer token for now (simplest, consistent with ADR-0018's existing posture) and revisit scoped tokens only if a real multi-tenant need appears — but this is a policy call, not purely technical, so flagging for explicit sign-off before IMPL stage begins.
