# Expectation: `.px` action verbs must resolve to real Rust handlers (C-SPINE-001)

## Status: Enforced
## Date: 2026-07-20
## Origin: task-runner loop P0 fix (non-delegated dispatch idle root cause)

## Constraint ID: C-SPINE-001

Any `.px` procedure that performs runtime IO MUST call verbs that are implemented and
registered in the corresponding Rust action handler for that seam.

For autonomous task dispatch specifically:
- Allowed IO verbs: `read_evaluable_tasks`, `mark_task_in_progress`, `dispatch_task`
- These verbs MUST be present in `TASK_DISPATCH_ACTIONS` and implemented by
  `TaskDispatchActionHandler`.

## Violations

- `.px` calls a verb that has no registered Rust handler at that seam.
- Rust removes/renames a handler verb while `.px` still calls it.
- `.px` writes durable task status/attempt fields directly instead of using the
  `TaskManager` seam handler (`mark_task_in_progress`).

## Enforcement

- Unit/integration tests must fail when autonomous-dispatch verb mapping drifts.
- Runtime should return explicit `ActionFailed` for unknown seam verbs (never silent success).
- PRs touching `.px` IO verbs must include paired Rust handler updates in the same change.

## Rationale

This prevents the exact BREAK #1 class where decision logic executes but never reaches real IO,
causing the system to appear idle despite pending work.

## References

- ADR-0034
- C-DEV-001
- C-PLURES-003/004
- C-NOSTUB-001
