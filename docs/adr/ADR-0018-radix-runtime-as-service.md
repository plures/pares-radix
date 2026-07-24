# ADR-0018: Radix Runtime as a Standalone Service

- Status: Proposed (design only — no code in this ADR)
- Date: 2026-07-23
- Epic: `pares-radix:runtime-as-service`
- Stage: DESIGN (per `pares-radix-dev-lifecycle` staged lifecycle: analyze → **design** → fix → test → deploy → verify)

## 1. Context

### 1.1 What exists today

pares-radix currently ships as a **Tauri desktop app** (`src-tauri/`, workspace
member `src-tauri`). The cognition/runtime logic lives in `crates/radix-core`
(a library, no `fn main`), and is driven entirely by the Tauri process
lifecycle — there is no standalone service binary, no HTTP/health endpoint,
and no headless entrypoint anywhere in the workspace (`crates/*/src/*.rs`
audited: zero `fn main` outside `src-tauri/src/main.rs`, zero `tokio::main`,
zero long-running `loop {}` driving events).

Event/timer plumbing already exists **as a library**, one layer down, in
`pluresdb-procedures` (consumed via the `pluresdb` git dependency, pinned by
rev in `crates/radix-core/Cargo.toml`, `crates/agenda/Cargo.toml`,
`crates/audit/Cargo.toml`, `crates/praxis/Cargo.toml`):

- **`AgensEvent`** (`pluresdb-procedures/src/agens.rs`): 8 event variants
  (`Message`, `Timer`, `StateChange`, `ModelResponse`, `ToolResult`,
  `PraxisAnalysisReady`, `PraxisAnalysisFailed`, `PraxisGuidanceUpdated`).
  Persisted as CRDT nodes (`agens:command` type), so events are durable and
  sync across peers via Hyperswarm. Idempotent emission (`emit_praxis_event`)
  uses deterministic node keys (`praxis:cmd:{id}`) for the three Praxis
  lifecycle events.
- **`TimerTable`**: schedule/cancel/list interval, cron, and one-shot timers,
  persisted as `agens:timer` CRDT nodes. Cron via the `cron` crate (5-field
  normalized to 6-field). `due_timers(now)` is **O(n)** over the whole store
  (documented limitation — no timer-specific index/prefix).
- **`AgensRuntime`**: procedure handler registry (`register_procedure` /
  `execute_procedure`), event emission/polling (`emit_event` /
  `poll_events(since)` — also O(n), timestamp-cursor based, **at-least-once**,
  consumers must dedupe on logical id), and `process_due_timers(now)` which
  fires due timers through the `"timer"` handler and reschedules them.

**The gap:** nothing in the pares-radix workspace *calls*
`process_due_timers` / `poll_events` on a cadence. There is no driver loop,
no scheduler task, no health surface, and no way to run this outside the
Tauri desktop shell (e.g., on a server, in CI, or as a background daemon on a
headless box). `crates/radix-core/src/health.rs` defines `SystemHealth` /
`HealthReport` types but they're an in-process report struct with no HTTP
exposure — nothing polls them externally today.

### 1.2 Why this matters now

Running only inside a Tauri desktop process means:
- The agent is only "alive" while a human has the desktop app open.
- No way to host pares-radix on a server/VM/container for 24/7 automation
  (crons, Telegram bot responsiveness, timer-driven procedures).
- No external health check surface for monitoring (the existing
  `praxisbot-deployment-drift` / `release-pipeline-health` crons referenced
  in the `pares-radix-dev-lifecycle` skill watch *releases*, not a running
  service).
- `TimerTable`/`AgensEvent` — real, tested infrastructure — sits unused for
  its primary purpose (driving scheduled/reactive procedures) because there's
  no host process to run the poll loop.

## 2. Decision (proposed)

Introduce a new **`pares-radix-svc`** binary crate (workspace member) that
hosts the existing `radix-core` runtime headlessly, with:

1. A **lifecycle-managed run loop** driving `AgensRuntime::poll_events` +
   `process_due_timers` on a fixed tick.
2. **Persistence exclusively through PluresDB** (no new storage layer — reuse
   `CrdtStore` opened from a configurable on-disk path, matching how
   `radix-core`/Tauri already persist state today via `pluresdb_bridge.rs` /
   `state.rs`).
3. An **automation interface** (HTTP, loopback-bound by default) exposing:
   health, readiness, event emission, timer CRUD, and procedure invocation —
   a thin REST wrapper over the existing `AgensRuntime` / `PluresDbBridge`
   APIs, not a rewrite of them.
4. A **health check** endpoint built directly on the existing
   `crates/radix-core/src/health.rs` `SystemHealth`/`HealthReport` types
   (reuse, don't duplicate).
5. **Local QA** harness: a scriptable smoke-test path (`cargo run --bin
   pares-radix-svc -- --once` style dry-run, plus an integration test crate)
   that exercises the full loop against a temp `CrdtStore` without needing
   Tauri, Hyperswarm peers, or a live model backend.

This keeps `radix-core` as the shared library (Tauri desktop **and** the new
service both depend on it) and adds one new thin binary crate rather than
forking runtime logic.

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ pares-radix-svc (new binary crate)                           │
│                                                               │
│  main() -> ServiceLifecycle::run()                           │
│                                                               │
│  ┌─────────────┐   ┌──────────────────┐   ┌────────────────┐ │
│  │ Automation   │   │  Scheduler loop   │   │ Health/Ready   │ │
│  │ HTTP surface │   │  (tokio interval) │   │ endpoint       │ │
│  │ (axum, bind:  │   │  every N sec:     │   │ GET /healthz   │ │
│  │ 127.0.0.1)    │  │   poll_events()   │   │ GET /readyz    │ │
│  │ POST /events │   │   process_due_    │   │ (SystemHealth) │ │
│  │ GET/POST/DEL │   │     timers()      │   └────────────────┘ │
│  │  /timers     │   └──────────────────┘                     │
│  │ POST /procs/ │                                            │
│  │   {name}/run │                                            │
│  └─────────────┘                                             │
│           │                    │                    │        │
│           └────────────────────┴────────────────────┘        │
│                             │                                │
│                    AgensRuntime<'_>  (existing, unmodified)  │
│                    PluresDbBridge     (existing, unmodified) │
│                             │                                │
└─────────────────────────────┼────────────────────────────────┘
                              ▼
                    CrdtStore (PluresDB, on-disk)
                    -- SAME store path Tauri app uses when
                       PARES_RADIX_DATA_DIR is shared, or a
                       dedicated service data dir otherwise --
```

Key point: **no new persistence layer**. The service opens the same
`CrdtStore` type via the same `pluresdb` crate already vendored. If run
side-by-side with the desktop app against the *same* store path, CRDT
merge semantics (already relied on for Hyperswarm multi-peer sync) make
that safe by construction — the service is just another "actor" writing to
the store, exactly like a second desktop peer would be.

### 3.1 Lifecycle states

Explicit state machine, mirroring the ADR-0017 channel-agnostic pattern and
the `pares-radix-dev-lifecycle` skill's own staged/gated philosophy:

```
Starting -> Ready -> Running -> Draining -> Stopped
              │                     ▲
              └──────── Degraded ───┘  (health check fails, keep serving
                                        but readyz=503; do not crash-loop)
```

- **Starting**: open `CrdtStore` at configured path, construct
  `AgensRuntime`, register procedure handlers (reuse
  `radix-core::procedures::load_default_procedures` + any custom
  registrations), bind HTTP listener. Failure here → process exits non-zero
  (fail fast, let the process supervisor — systemd/Windows service/Docker —
  restart per its own backoff policy; the service does **not** implement its
  own crash-loop backoff).
- **Ready**: HTTP surface up, `/healthz` returns 200 but `/readyz` gates on
  first successful scheduler tick (avoids serving traffic before the
  first timer/event pass proves the store is reachable).
- **Running**: steady-state — scheduler tick + HTTP requests interleaved on
  the tokio runtime.
- **Degraded**: a scheduler tick errors (e.g., store I/O failure). Log +
  record `HealthReport` via `SystemHealth::record`, keep retrying on the next
  tick (bounded exponential backoff capped at e.g. 30s), `/readyz` flips to
  503 until a tick succeeds again. Never panics the process for a transient
  store hiccup — that decision is deliberate: a service that crash-loops on
  every blip is worse than one that reports itself unready.
- **Draining**: on SIGTERM/Ctrl-C, stop accepting new scheduler ticks and new
  mutating HTTP requests (still serve `/healthz`), finish any in-flight
  procedure execution up to a grace deadline (default 10s, configurable),
  then flush/close the store cleanly.
- **Stopped**: process exit. Exit code 0 for a clean drain, non-zero for a
  Starting-phase failure, so it composes with standard service supervisors.

## 4. Automation interface (design, not final API contract)

Local-first, loopback-bound HTTP (default `127.0.0.1:8730` — new port,
picked to avoid the existing `pares-radix` ecosystem's known ports; final
port TBD at implementation time and should be config/env driven, not
hardcoded, e.g. `RADIX_SVC_BIND_ADDR`). No auth by default when bound to
loopback; bearer-token auth required when `RADIX_SVC_BIND_ADDR` is
non-loopback (fail closed: refuse to bind to `0.0.0.0`/non-loopback without
a configured token — this is a hard gate, not a warning).

| Endpoint | Method | Purpose | Backing call |
|---|---|---|---|
| `/healthz` | GET | Liveness — process is up | `SystemHealth::report()` |
| `/readyz` | GET | Readiness — store reachable, first tick done | scheduler tick state |
| `/events` | POST | Emit an `AgensEvent` (idempotent variants use `emit_praxis_event`) | `AgensRuntime::emit_event` / `emit_praxis_event` |
| `/events` | GET | Poll events since a cursor (for external orchestrators) | `AgensRuntime::poll_events` |
| `/timers` | GET | List scheduled timers | `TimerTable::list` |
| `/timers` | POST | Schedule interval/cron/once timer | `TimerTable::schedule_*` |
| `/timers/{id}` | DELETE | Cancel a timer | `TimerTable::cancel` |
| `/procedures/{name}/run` | POST | Manually trigger a named procedure (bypasses timer/event dispatch, for QA/ops) | `AgensRuntime::execute_procedure` or `PluresDbBridge::run_procedure` |

This is a **thin transport wrapper**: every handler is a direct pass-through
to an existing `radix-core` / `pluresdb-procedures` API. No new business
logic in the HTTP layer — keeps the "logic in `.px`/library, IO at the edge"
principle from the dev-lifecycle skill.

## 5. Persistence through PluresDB (explicit, per requirement)

- No new database, no new schema layer. `CrdtStore::open(path)` (existing
  API surface used by `pluresdb_bridge.rs` today) is the single source of
  truth.
- Service config (bind address, tick interval, data dir, procedure toggles)
  is **not** stored in PluresDB — it's process config (env/CLI/file), since
  it must be readable before the store is even open. Runtime-mutable state
  (timers, events, praxis lifecycle events, `SetupConfig`-equivalent
  settings) stays in PluresDB via the existing `StateStore`/`TimerTable`
  patterns already in `radix-core`/`pluresdb-procedures`.
- Multi-instance safety relies on existing CRDT merge + Hyperswarm sync
  semantics — no new distributed-locking design needed for a first version.
  (Explicitly out of scope: leader election for the scheduler if multiple
  service instances point at synced stores — call out as a **known
  limitation**, see §7.)

## 6. Local QA plan (no code yet — plan only)

1. **Unit-level**: existing `pluresdb-procedures` unit tests already cover
   `TimerTable`/`AgensRuntime` correctness — no duplication needed.
2. **Service integration test crate** (`pares-radix-svc/tests/`):
   - Spin up the service against a `tempdir()`-backed `CrdtStore`, bind to
     `127.0.0.1:0` (OS-assigned port).
   - Assert `/healthz` returns 200 immediately, `/readyz` flips to 200 only
     after the first tick.
   - POST a one-shot timer with `run_at = now`, wait one tick interval,
     assert the registered handler fired exactly once (dedupe check via
     logical id) and `/timers` shows it inactive afterward.
   - POST an `AgensEvent::Message`, GET `/events?since=...`, assert it comes
     back.
   - Send SIGTERM-equivalent (drop the server task / call shutdown), assert
     graceful drain: in-flight procedure completes, `/healthz` stops
     responding only after drain deadline.
3. **Manual smoke script** (`scripts/smoke-radix-svc.mjs` or `.ps1`, TBD at
   implementation): start service, curl the endpoints above, confirm exit
   code semantics on Ctrl-C.
4. **CI hook**: add a `cargo test -p pares-radix-svc` job to the existing
   GitHub Actions workflow (`.github/workflows/`), gated the same way other
   crates are — this becomes part of the `test` stage in the dev-lifecycle
   loop, not a one-off.

None of this requires Hyperswarm peers, a live LLM backend, or the Tauri
shell — that's the point: the service must be independently QA-able.

## 7. Known limitations / explicitly out of scope for v1

- No leader election across multiple service replicas sharing a synced
  store (single-instance-per-store assumption for v1; document, don't
  solve).
- `poll_events`/`due_timers` remain O(n) over the whole CRDT store — fine at
  current scale, called out as a follow-up ADR trigger if store size grows
  (the existing doc comments already flag this; the service design inherits
  the limitation rather than papering over it).
- Automation HTTP interface has no schema versioning story yet — v1 ships
  unversioned (`/events`, not `/v1/events`); revisit before any external
  (non-loopback) consumer depends on it.
- Auth model is bearer-token-only, no RBAC/scopes in v1.

## 8. Migration / rollout

- Additive: new `pares-radix-svc` workspace member, zero changes to existing
  `src-tauri` or `radix-core` public API required to ship v1 (both consume
  the same crates unmodified).
- Desktop app is unaffected; can continue running standalone or pointed at
  the same store as the service.
- Follow-on epics (post-DESIGN): FIX (implement `pares-radix-svc` per this
  ADR) → TEST (crate above) → DEPLOY (systemd unit / Docker image / Windows
  service wrapper — TBD which target first) → VERIFY (close the loop per
  `pares-radix-dev-lifecycle`, mandatory, cannot be skipped).

## 9. Open questions for follow-up FIX stage

1. Which process supervisor is the deploy target first — systemd (Linux),
   Windows Service, or Docker/container orchestrator? Affects signal
   handling details in §3.1.
2. Should `/procedures/{name}/run` be gated behind an explicit "QA/debug
   only" flag in production, given it bypasses normal event/timer dispatch?
3. Shared-store-with-desktop-app scenario: do we want a documented warning
   in the desktop UI when the service is also running against the same data
   dir, or is silent CRDT-merge coexistence sufficient?
