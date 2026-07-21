# ADR-0034: Autonomous Dispatch Verb Resolution Contract

- **Status:** Accepted
- **Date:** 2026-07-20
- **Deciders:** pares-radix runtime maintainers
- **Relates:** ADR-0005, ADR-0020, ADR-0027
- **Invariants:** C-DEV-001, C-PLURES-003/004, C-NOSTUB-001, C-TEST-002

## Context

`praxis/procedures/autonomous-dispatch.px` is the source of truth for autonomous task selection.
A production regression showed `.px` calling verbs that were not resolved by the Rust task-dispatch
IO boundary while Rust exposed only `dispatch_task`.

Result: heartbeat ticks repeatedly returned `no_pending_tasks` despite real Open tasks.
A second mismatch amplified this: `.px` used `pending`/`in_progress` while `TaskManager`
persisted `Open`/`InProgress`.

## Decision

1. The autonomous dispatch procedure must use handler-resolved verbs at the IO seam:
   - `read_evaluable_tasks {}`
   - `mark_task_in_progress {task_id}`
   - `dispatch_task {task_id, prompt}`
2. Status vocabulary translation happens at the boundary, not duplicated in `.px`:
   - Rust -> `.px`: `Open` -> `pending`, `InProgress` -> `in_progress`
   - `.px` -> Rust: `mark_task_in_progress` writes `TaskStatus::InProgress` and records evaluation
3. Runtime wiring must pass the same `Arc<TaskManager>` to both task-grounding and task-dispatch
   handlers (single durable task store).

## Consequences

- Non-delegated autonomous dispatch no longer silently idles.
- Loop closure is testable locally, channel-agnostic.
- New `.px` dispatch verbs require same-turn Rust handler registration or verification fails.

## Verification

- `cargo build`
- `cargo test`
- `cargo clippy -- -D warnings`
- Loop proof: Open task -> selected -> marked `InProgress` -> dispatched/re-driven -> finalized.

## References

- `crates/radix-core/src/spine/task_dispatch_actions.rs`
- `crates/radix-core/src/spine/runtime.rs`
- `praxis/procedures/autonomous-dispatch.px`
- `.praxis/expectations/C-SPINE-001-px-verbs-resolve-to-rust-handlers.md`
