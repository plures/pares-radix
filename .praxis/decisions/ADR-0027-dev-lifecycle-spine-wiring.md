# ADR-0027: Dev-Lifecycle Spine Wiring - Collapse the `.mjs` Driver into the `.px`/Actor/PluresDB Loop

- **Status:** Proposed (DESIGN stage of TASK-2026-06-25-001; FIX stage gated on this ADR)
- **Date:** 2026-06-25
- **Deciders:** kbristol (human gate on Risks §9), dev-lifecycle orchestrator
- **Relates:** ADR-0005 (orchestration-as-dataflow), ADR-0020 (single-PluresDB reactive memory), ADR-0001 (spine-driven pipeline), ADR-0022 (capability-host-contract)
- **Filename note:** Task brief requested `ADR-dev-lifecycle-spine-wiring.md`. Numbered to dir convention `ADR-NNNN-kebab-title.md` at apply time: ADR-0023 through ADR-0026 were taken by parallel work, so this landed as **ADR-0027**.

---

## 1. Context

The dev-lifecycle is meant to run as **pure logic in `.px` + Rust IO actors + a PluresDB reactive loop**:

```
task_request:{id}      -> plan_task        -> writes devtask:{id}
                                           -> spawn_subagent (first stage)
(stage runs) stage_complete:{id}:{stg}     -> evaluate_gate   -> spawn_subagent (next stage)
                                           -> ... until last stage ...
task_complete:{id}     -> report_result    -> writes notification:{id}
```

This topology is already designed and **partly built**:

- `praxis/procedures/dev-lifecycle.px` defines `plan_task`, `evaluate_gate`, `report_result` as pure dataflow (no side effects). *(observed)*
- `spine/dev_lifecycle_actions.rs` implements the 7 pure-compute action handlers the procedures call (`get_default_stages`, `merge_stage_config`, `find_next_stage`, `get_stage`, `update_stage_status`, `format_stage_brief`, `collect_stage_outputs`). *(observed: `DEV_LIFECYCLE_ACTIONS`, dev_lifecycle_actions.rs:455-470)*
- `spine/subagent_actor.rs` implements `spawn_subagent`: it spawns via `Arc<dyn SubAgentSpawner>`, then a background task polls `manager.get(session_id)` and, on terminal status, writes `stage_complete:{task_id}:{stage_name}` back into the `ReactiveRegistry` - closing the loop into `evaluate_gate`. *(observed: subagent_actor.rs:120-235)*
- `spine/actions.rs::CompositeActionHandler` already composes `CoreActionHandler` + `DevLifecycleActionHandler` + optional `SubagentActor` + `ToolDispatchActionHandler` and resolves action names to the right sub-handler. *(observed: actions.rs:160-216)*
- `spine/bootstrap.rs::register_reactive_procedures(praxis_dir, registry, handler)` loads every `.px` file, maps each to a trigger pattern (`plan_task -> task_request:*`, `evaluate_gate -> stage_complete:*`, `report_result -> task_complete:*`), and registers them in the `ReactiveRegistry`. *(observed: bootstrap.rs:39-103, 116-180)*

**The problem:** none of that is wired up at runtime, and a parallel JS orchestrator does the real work.

`C:\Users\kbristol\.openclaw\workspace\scripts\dev-lifecycle.mjs` contains a full **`DevLifecycleDriver`** state machine plus a **disk ledger** (`memory/dev-lifecycle-runs/{taskId}.json`) that re-implements stage ordering, gate evaluation, retry bounds, escalation, and completion - the exact logic in `dev-lifecycle.px`/`evaluate_gate`. *(observed)* The `.mjs` even documents *why* it duplicates the logic: "A standalone `node` process cannot call sessions_spawn... so [we compute] ALL decisions HERE, deterministically, in code. The script is the orchestrator." *(observed: DRIVER comment block)*

That violates two first-principles constraints:

- **C-PLURES-003 (single source of orchestration truth):** orchestration logic must live in `.px`, not be re-implemented in a side-effect driver. The `.mjs` `DevLifecycleDriver`/`evaluateGate` is a second, authoritative copy of `evaluate_gate`.
- **C-PLURES-004 (PluresDB reactive loop is the runtime):** stage transitions must be driven by PluresDB writes through the `ReactiveRegistry`, not by a JS ledger file the LLM hand-feeds.

**The collapse this ADR designs:** wire the already-built actor/`.px` machinery into a real runtime entry point (Gap A), supply the one missing IO actor - a production `SubAgentSpawner` (Gap B), and delete the duplicate JS orchestrator (Gap C).

---

## 2. Evidence Table

Marker key: **[observed]** = read directly this session; **[grep]** = repo-wide search; **[UNKNOWN]** = not determinable from the repo, needs FIX-stage discovery or human input.

| # | Assertion | Evidence | Marker |
|---|-----------|----------|--------|
| E1 | `.px` dev-lifecycle procedures are pure dataflow (no IO); Rust actions do side effects. | `praxis/procedures/dev-lifecycle.px` header + `plan_task`/`evaluate_gate`/`report_result` bodies | [observed] |
| E2 | The 7 dev-lifecycle compute actions exist and are matched by `is_dev_lifecycle_action`. | `dev_lifecycle_actions.rs:455-470` | [observed] |
| E3 | `spawn_subagent` writes `stage_complete:{task_id}:{stage_name}` on terminal status (Completed/Failed/TimedOut/Killed). | `subagent_actor.rs:131-220` (`tokio::spawn` poll loop + `registry.on_write`) | [observed] |
| E4 | `SubagentActor` depends only on `Arc<dyn SubAgentSpawner>` + `Arc<ReactiveRegistry>`; no cognition/channel dep. | `subagent_actor.rs:30-50` | [observed] |
| E5 | `SubAgentSpawner` is a platform-owned trait: `spawn(agent,prompt,opts)->String`, `get(session_id)->Option<SpawnedInfo>`; DTOs `SpawnOptions`/`SessionStatus`/`SpawnedInfo`. | `subagent_spawn.rs:113-130` (+ DTOs 17-107) | [observed] |
| E6 | **No production implementor of `SubAgentSpawner` exists.** Only `impl` is `MockSpawner` inside `subagent_actor.rs` tests; `delegation::SubAgentManager` appears only in doc comments. | `grep impl SubAgentSpawner` -> `subagent_actor.rs` MockSpawner (`#[cfg(test)]`); doc refs `subagent_spawn.rs:6,109`, `actions.rs:208` | [grep] |
| E7 | **No `delegation` module / `SubAgentManager` struct exists.** Zero `.rs` match `delegation`; only an aspirational test path `crates/core/src/delegation/manager.rs` (wrong crate name - it's `radix-core`). | `grep` over `crates/**/*.rs`; `px_first_enforcement.rs:53` | [grep] |
| E8 | `CompositeActionHandler` routes by `CORE_ACTIONS` / `is_dev_lifecycle_action` / `is_subagent_action` / else->tool, and exposes `set_subagent_actor(Arc<SubagentActor>)` to break the construction cycle. | `actions.rs:160-216` | [observed] |
| E9 | **`CompositeActionHandler::new` is never called outside its own `impl`.** | `grep CompositeActionHandler::new` -> only actions.rs def | [grep] |
| E10 | **`register_reactive_procedures` is never called in a runtime path** - only in `bootstrap.rs` + `shadow.rs` tests. | `grep register_reactive_procedures` | [grep] |
| E11 | Its signature is `(praxis_dir:&Path, registry:&ReactiveRegistry, handler:Arc<dyn AsyncActionHandler>)` and maps `plan_task->task_request:*`, `evaluate_gate->stage_complete:*`, `report_result->task_complete:*`. | `bootstrap.rs:116-119, 80-82` | [observed] |
| E12 | `Pipeline::with_reactive(capacity, reactive)` exists and `run()` forwards events to `reactive.on_write(...)`. | `pipeline.rs:78-92, 150-160` | [observed] |
| E13 | **No serve/daemon binary exists** that builds the spine pipeline + reactive registry for production. No `#[tokio::main]` in `crates/cli`; `radix-core` is a lib; `Pipeline::new` only in `pipeline.rs` tests. | `grep #[tokio::main]` (only secrets/mcp doctests); `grep Pipeline::new`; `cli/src` = {lib,migrate,openclaw}.rs | [grep] |
| E14 | `cli/src/openclaw.rs` is an **installation reader** (`OpenClawInstallation::load` parses `memories.json`/`config.json`/`crons.json`/`*.md`). No session-spawn / gateway-call capability. | `openclaw.rs` full read | [observed] |
| E15 | `dev-lifecycle.mjs` contains a duplicate orchestrator: `DevLifecycleDriver` (start/resume/next/record), `evaluateGate`, `parseStageResult`, disk ledger under `memory/dev-lifecycle-runs/`. | `dev-lifecycle.mjs` | [observed] |
| E16 | `CoreActionHandler::write_state` is a **stub** (`debug!("write_state (stub - will be PluresDB)"); Ok(Null)`); `read_state` only special-cases `chat_history:`. So `.px` `write_state {key:"devtask:{id}"}` currently no-ops. | `actions.rs:121-135` (write_state), `:96-117` (read_state) | [observed] |
| E17 | The registry the **`SubagentActor` writes into** MUST be the **same** registry the `.px` procedures are **registered in**, else `stage_complete` won't trigger `evaluate_gate`. `SubagentActor::new(manager, registry)` takes it explicitly. | `subagent_actor.rs:48-50` + `:160` | [observed] |
| E18 | How a real OpenClaw sub-agent session is created from inside `pares-radix` (MCP? gateway HTTP? in-proc cognition?) is **not present in this repo**. `crates/mcp-client` exists but its spawn-session surface was not confirmed. | `grep` (mcp-client exists; no spawn API traced) | [UNKNOWN] |
| E19 | Whether a PluresDB-backed `StateStore` is already constructed/reachable at the (missing) serve entry - i.e. whether Gap A also requires standing up PluresDB - is not determinable without the serve binary E13 says doesn't exist. | derived E13+E16 | [UNKNOWN] |

**Net:** the loop is ~85% built. The missing 15% is exactly three seams: (A) nobody constructs+registers the composite handler at runtime, (B) there is no real `SubAgentSpawner`, (C) a JS driver fills the vacuum and must be removed.

---

## 3. Decision (summary)

1. **Gap A -** Add a single runtime assembly function in `radix-core` (`spine::bootstrap::wire_dev_lifecycle`) that builds the `Arc<ReactiveRegistry>`, constructs `CompositeActionHandler` (injecting a real `SubagentActor` bound to that registry via `set_subagent_actor`), and calls `register_reactive_procedures` with the **same** registry + composite handler. Call it from the serve entry. Match `CompositeActionHandler`'s existing resolution pattern; do not invent a new dispatch mechanism.
2. **Gap B -** Implement a production `SubAgentSpawner` behind a **new channel-agnostic seam**, because no real session-spawn API exists in-repo today (E6/E7/E18). Ship a concrete adapter over the transport the serve binary owns (preferred: the existing `crates/mcp-client`; fallback: a `SpawnTransport` trait the binary injects). **No mock in the shipped path** (C-NOSTUB-001): if the transport is genuinely absent at build time, the binary wires `None`, `spawn_subagent` returns the existing "actor not wired" error (actions.rs:208), and the feature reports unavailable rather than pretending.
3. **Gap C -** Delete `DevLifecycleDriver`, `evaluateGate`, `parseStageResult`, and the ledger from `dev-lifecycle.mjs`. Keep only `buildBrief` iff a thin CLI shim still needs it; otherwise delete the file and let `.px` own brief construction.

---

## 4. Gap A Design - Bootstrap Wiring (the crux)

### 4.1 Action-name -> handler resolution (already built)

There **is** a composite: `CompositeActionHandler` (actions.rs:160). Its `call()` resolves in priority order *(observed actions.rs:195-216)*:

```
if CORE_ACTIONS.contains(action)        -> core.call           // read_state/write_state/read_history/append_history
else if is_dev_lifecycle_action(action) -> dev_lifecycle.call  // the 7 stage-compute actions
else if is_subagent_action(action)      -> subagent.call       // spawn_subagent (errors if actor unset)
else                                     -> tool_handler.call   // everything else = tool dispatch
```

So the executor the `ReactiveRegistry` uses resolves actions by handing the name to **one** `Arc<dyn AsyncActionHandler>` (the composite), which fans out internally. **We match this** - we do not add a registry-of-handlers.

### 4.2 The construction cycle (and its built-in escape hatch)

`SubagentActor::new(manager, registry)` needs the `Arc<ReactiveRegistry>` (E17). The composite needs the `SubagentActor`. The registry needs the handler (to register `.px` against it). 3-way cycle. `actions.rs` already provides the escape: build the composite **without** the subagent, then `set_subagent_actor` *(observed actions.rs:181-183)*. Because `set_subagent_actor(&mut self, ...)` takes `&mut self`, the composite must be mutated **before** being wrapped in the `Arc<dyn AsyncActionHandler>` passed to `register_reactive_procedures`.

### 4.3 Insertion point (precise)

No runtime caller exists today (E10/E13). Add a function in `spine/bootstrap.rs`, immediately after `register_reactive_procedures` (ends **bootstrap.rs:180**, before the `#[cfg(test)]` mod at :181):

```rust
/// Wire the full dev-lifecycle reactive loop into ONE registry + ONE composite handler.
/// Returns (registry, handler) ready to attach to a Pipeline via `with_reactive`.
pub async fn wire_dev_lifecycle(
    praxis_dir: &Path,
    conversation_store: Arc<dyn ConversationStore>,
    tool_handler: Arc<ToolDispatchActionHandler>,
    spawner: Option<Arc<dyn SubAgentSpawner>>,   // None => spawn_subagent reports unavailable (no mock)
) -> (Arc<ReactiveRegistry>, Arc<dyn AsyncActionHandler>) {
    let registry = Arc::new(ReactiveRegistry::new());

    // Build composite, then inject the subagent actor bound to THIS registry (E17).
    let mut composite = CompositeActionHandler::new(conversation_store, tool_handler);
    if let Some(spawner) = spawner {
        let actor = Arc::new(SubagentActor::new(spawner, Arc::clone(&registry)));
        composite.set_subagent_actor(actor);
    }
    let handler: Arc<dyn AsyncActionHandler> = Arc::new(composite);

    // Register every .px (incl. plan_task/evaluate_gate/report_result) against the SAME registry+handler.
    register_reactive_procedures(praxis_dir, &registry, Arc::clone(&handler)).await;
    (registry, handler)
}
```

(The exact constructor signature of `CompositeActionHandler::new` - whether arg 1 is a `ConversationStore` or a `CoreActionHandler` - is **[UNKNOWN-A1]**; FIX must read actions.rs:167 and match it. The shape above is the contract, not the literal args.)

**Call site (serve):** wherever the spine pipeline is stood up for production it must (a) `wire_dev_lifecycle(...)` then (b) `Pipeline::with_reactive(cap, registry)` *(observed pipeline.rs:78)*. Per E13 that binary **does not exist**; FIX must first re-grep `crates/cli-api` + any `bin/`. If a daemon assembly exists, insert the two calls there; otherwise creating the serve binary is itself a FIX deliverable (see Risk R1).

### 4.4 Why this is correct (not merely plausible)

- The **same `Arc<ReactiveRegistry>`** flows into both `register_reactive_procedures` (so `evaluate_gate` is subscribed to `stage_complete:*`, bootstrap.rs:81) and `SubagentActor::new` (so its `stage_complete` write, subagent_actor.rs:160, lands on that subscription). This single fact (E17) is the current cause of the dead loop.
- The composite is the lone handler the registry executes; its routing (E8) already covers all dev-lifecycle + subagent actions, so no per-action registration is needed.
- `write_state`/`read_state` for `devtask:{id}` are stubs today (E16). For the loop to persist task state across a process bounce, `CoreActionHandler` must back them with the real PluresDB `StateStore`. **In scope for Gap A**, flagged as a FIX sub-task; reachability of a `StateStore` at the serve entry is E19 [UNKNOWN].

---

## 5. Gap B Design - Real `SubAgentSpawner` (production, non-mock)

### 5.1 Finding: the real session-creation API is NOT in this repo

Per E6/E7/E18 there is no `delegation::SubAgentManager`, no other `impl SubAgentSpawner`, and no traced in-repo API that "creates a session and returns its final output." `cli/src/openclaw.rs` is a file reader, not a spawner (E14). Therefore, per the task contract, **the seam must be added** and I propose the channel-agnostic interface (C-TEST-002) rather than a mock in the shipped path (C-NOSTUB-001).

### 5.2 The seam to add

A transport trait owned by the platform, plus a production `SubAgentSpawner` adapter mapping the platform contract onto it. New file:

`crates/radix-core/src/spine/subagent_spawner_impl.rs`

```rust
//! Production SubAgentSpawner over a pluggable, channel-agnostic transport.
//! No Telegram/Discord/UI dependency (C-TEST-001/002). No mocks in shipped path (C-NOSTUB-001).

/// Channel-agnostic transport for creating + tracking an agent session.
/// Implemented by whatever the serve binary owns (MCP client today; gateway HTTP later).
#[async_trait]
pub trait SpawnTransport: Send + Sync {
    async fn create_session(&self, agent: &str, prompt: &str, opts: &SpawnOptions)
        -> Result<String, SpawnError>;            // -> opaque session id
    async fn poll_session(&self, session_id: &str)
        -> Result<SpawnedInfo, SpawnError>;       // map transport state -> platform SpawnedInfo
}

/// Production spawner: adapts SpawnTransport -> SubAgentSpawner (the platform seam, subagent_spawn.rs:113).
pub struct TransportSubAgentSpawner<T: SpawnTransport> { transport: Arc<T> }

#[async_trait]
impl<T: SpawnTransport> SubAgentSpawner for TransportSubAgentSpawner<T> {
    async fn spawn(&self, agent: &str, prompt: &str, options: SpawnOptions) -> String {
        // create_session; on transport error return a sentinel id whose poll yields Failed(..)
        // so the actor writes stage_complete{status:failed} instead of hanging forever.
    }
    async fn get(&self, session_id: &str) -> Option<SpawnedInfo> {
        // poll_session -> Some(info); transport "unknown id" -> None
        // (matches the actor's "session lost?" branch, subagent_actor.rs:228).
    }
}
```

This keeps `SubagentActor` (E4) and the `.px` loop entirely transport-agnostic: the actor only ever sees `spawn`/`get` (subagent_spawn.rs:113-130), never a channel.

### 5.3 The actual call path (mapped), in priority of what the repo supports

1. **MCP transport (preferred).** `crates/mcp-client` already exists. The serve binary builds an MCP client to the OpenClaw gateway and implements `SpawnTransport` over it: `create_session` -> the gateway's spawn-session tool; `poll_session` -> its status/result call. **Exact MCP method + param names are E18 [UNKNOWN-B1]** - FIX must confirm them against `crates/mcp-client`. This is also the channel-agnostic entry point the test strategy (§7) drives.
2. **Gateway HTTP transport (fallback).** If MCP lacks a spawn verb, implement `SpawnTransport` over the gateway's HTTP API (`POST /sessions`, `GET /sessions/{id}`). Same adapter, different transport impl. **[UNKNOWN-B2]:** whether such an endpoint exists.
3. **In-proc cognition (only if pares-radix ever hosts the agent loop itself).** Out of scope now; noted so the trait isn't accidentally MCP-shaped.

The serve binary owns the transport choice and injects `Some(Arc::new(TransportSubAgentSpawner{ transport }))` into `wire_dev_lifecycle` (§4.3). Nothing channel-specific leaks below the binary.

### 5.4 Completion -> `stage_complete` write (already handled by the actor)

The actor already closes the loop (E3, subagent_actor.rs:131-220): after `spawn`, a background task polls `get(session_id)` until a terminal `SessionStatus`, then writes `stage_complete:{task_id}:{stage_name}` with the session's final output as the value into the **same** registry (E17). **Gap B does not re-implement completion** - it only has to make `get()` return truthful `SpawnedInfo{status,output}` from the real transport. The `task_id`/`stage_name` correlation is carried in the spawn metadata the actor already threads (subagent_actor.rs spawn call site); FIX confirms the exact metadata field names **[UNKNOWN-B3]**.

### 5.5 Why no mock ships (C-NOSTUB-001 compliance)

- The mock (`MockSpawner`) stays `#[cfg(test)]` in `subagent_actor.rs` - a test double at a real seam, never in a shipped path.
- If no transport is wired, `wire_dev_lifecycle` is called with `spawner: None`; `spawn_subagent` then returns the **existing real error** "subagent actor not wired" (actions.rs:208) instead of a fake success. The capability is simply *absent*, honestly reported - not stubbed.

---

## 6. Gap C Plan - Delete the JS Orchestrator

`dev-lifecycle.mjs` (E15) must lose everything that duplicates `.px`/actor logic:

**Delete:**
- `class DevLifecycleDriver` (start/resume/list/next/record/_*Message) - duplicate of `plan_task`+`evaluate_gate`+`report_result`.
- `evaluateGate(...)` - duplicate of `evaluate_gate` in dev-lifecycle.px (violates C-PLURES-003).
- `parseStageResult(...)` - gate logic; the `.px` owns pass/fail/blocked classification.
- Ledger machinery: `ledgerPath`, `loadLedger`, `saveLedger`, `LEDGER_DIR`, and the `memory/dev-lifecycle-runs/` store - replaced by PluresDB `devtask:{id}` records written by `plan_task` via the now-real `write_state` (E16 fix). This is the C-PLURES-004 correction.
- `STAGE_ORDER` / `STAGE_DEFAULTS` constants **iff** not imported elsewhere - canonical copies are `get_default_stages`/`merge_stage_config` in Rust (E2). Grep before deleting.

**Possibly keep (only if a thin CLI shim needs it):**
- `buildBrief(...)` *may* survive transiently as a convenience, **but** the canonical brief builder is `.px` `format_stage_brief`/`build_stage_brief` (E2). Preference: delete `buildBrief` too and have any CLI shim call the engine. If kept, mark it "presentation-only, not orchestration."

**Net outcome:** ideally the file is deleted entirely. If a human-facing "kick off / check status" CLI is still wanted, it shrinks to a shim that (a) writes `task_request:{id}` into PluresDB to start, and (b) reads `devtask:{id}`/`notification:{id}` to report - a *client* of the reactive loop holding **zero** orchestration logic. That shim is **not** built this stage (design only).

---

## 7. Test Strategy - Channel-Agnostic End-to-End Proof

Goal: prove the `.px`-driven loop runs **without** any Telegram/Discord adapter (C-TEST-001/002). Three layers:

**T1 - Rust integration test (in-repo, deterministic; primary gate).**
New `crates/radix-core/tests/dev_lifecycle_loop.rs`:
1. `wire_dev_lifecycle(praxis_dir, store, tool_handler, Some(Arc::new(scripted_spawner)))` where `scripted_spawner` is a **test** `SubAgentSpawner` (the existing MockSpawner pattern, `#[cfg(test)]`) returning canned terminal outputs per stage.
2. Drive `registry.on_write("task_request:{id}", {task def})`.
3. Assert the loop advances analyze->fix->test->deploy->verify by observing successive `devtask:{id}` state writes and `spawn_subagent` invocations, terminating in `task_complete:{id}` -> `report_result` -> `notification:{id}`.
4. Assert a forced FAIL output triggers retry/escalate exactly as `evaluate_gate` specifies.
This proves the **wiring + `.px` logic** with no transport and no channel. It is the gate FIX/TEST must pass.

**T2 - Channel-agnostic transport smoke (the real entry point).**
Drive the loop through the **MCP entry point** named in §5.3(1): start the serve binary, issue the MCP "spawn dev-lifecycle task" call (or write `task_request` via the MCP state tool), and assert `notification:{id}` appears. Entry point = **the MCP server surface of the serve binary**, explicitly *not* a chat adapter. Blocked until UNKNOWN-B1 resolves; until then T2 is specified-but-skipped.

**T3 - CLI/HTTP equivalence (optional).** If the gateway HTTP transport (§5.3(2)) lands, repeat T2 over `curl POST /sessions` to prove transport-independence.

`MockSpawner` is confined to T1 (`#[cfg(test)]`); T2/T3 exercise the real transport (C-NOSTUB-001).

---

## 8. Risks / Open Questions (human decision before FIX)

- **R1 - No serve binary exists (E13).** Wiring Gap A needs a production process that builds the pipeline. **Decision for kbristol:** does FIX create a new `radix serve`/daemon binary (and PluresDB bring-up), or is there an existing OpenClaw-side host that should own `wire_dev_lifecycle`? Biggest scope fork.
- **R2 - Real spawn transport is out-of-repo (E18/UNKNOWN-B1/B2).** Confirm the MCP (or HTTP) method that creates a session and returns its final output. If neither exists, Gap B's `SpawnTransport` is correct but FIX cannot ship a working spawner until the gateway exposes one - the loop wires with `spawner: None` and honestly reports unavailable.
- **R3 - `write_state`/`read_state` stubs (E16).** Backing them with PluresDB `StateStore` is required for persistence and may be its own ADR (relates to ADR-0020). Confirm scope: fold in or split.
- **R4 - `.px` `report_result` delivery.** It writes `notification:{id}`; whether a delivery procedure routes that to a human channel is unverified. Acceptable for loop correctness (the write is the contract), but flagged.
- **R5 - Constructor/metadata unknowns (UNKNOWN-A1/B3).** Exact `CompositeActionHandler::new` args and the spawn->`stage_complete` correlation metadata field names are mechanical FIX-time reads, not design blockers.

None of R1-R5 blocks *writing this ADR*; R1 and R2 are the decisions that shape FIX.

---

## 9. Consequences

- **Positive:** one source of orchestration truth (`.px`); the reactive loop becomes the runtime (C-PLURES-003/004 satisfied); `dev-lifecycle.mjs` stops being a shadow engine; the lifecycle becomes testable in-repo (T1) without a chat channel.
- **Negative / cost:** requires standing up a serve host (R1) and a real spawn transport (R2); `write_state` must become real (R3). Until R2 lands, the loop is wired but spawn-incapable (honest `None`), so the `.mjs` deletion (Gap C) should land **only after** T1 passes and a transport is available, to avoid a capability gap.

RESULT: PASS
