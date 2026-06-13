# ADR-0006: Recursive Self-Improvement as Core Architectural Principle

## Status: Accepted

## Date: 2026-06-13

## Context

pares-radix now has:
- Orchestration-as-dataflow (ADR-0005) — procedures orchestrate by writing data
- Model selection as a procedure — no hardcoded model config
- Topic routing as a procedure — context switching via data writes
- Dev-lifecycle orchestration — staged task execution with gates

The missing piece: the system doesn't learn from its own executions. Every task is independent. Performance signals are discarded. The same mistakes repeat.

kbristol identified the recursive paradigm: if orchestration is `.px`, and `.px` can write `.px`, then the system can improve its own orchestration based on observed results.

## Decision

**Recursive Self-Improvement (RSI) is a first-class procedure in the root orchestrator, not an add-on or future feature.**

The root orchestrator's job is not just "execute tasks" — it's "execute tasks AND get better at executing tasks."

## The Loop

```
┌─────────────────────────────────────────────────────────────┐
│                    ROOT ORCHESTRATOR                          │
│                                                              │
│  1. Receive task                                             │
│  2. Plan stages (via plan_task.px)                           │
│  3. Select model (via select_model.px)                       │
│  4. Build context (via build_session_context.px)             │
│  5. Execute (IO boundary → LLM call)                         │
│  6. Evaluate gate (via evaluate_gate.px)                     │
│  7. Loop until complete                                      │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              RSI FEEDBACK LOOP                          │  │
│  │                                                        │  │
│  │  8. Evaluate performance (what went well/badly?)       │  │
│  │  9. Identify patterns (recurring bottlenecks?)         │  │
│  │  10. Propose improvement (better .px for step 2-6?)    │  │
│  │  11. Validate (passes constraints? safe? rollbackable?)│  │
│  │  12. Apply (register new version)                      │  │
│  │  13. Monitor (regression? → auto-rollback)             │  │
│  │                                                        │  │
│  │  Next task uses the improved procedures.               │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Improvement Types

| Type | Description | Example |
|------|-------------|---------|
| **Tune** | Adjust parameters in existing procedure | Change model scoring weights after observing haiku outperforms opus on formatting |
| **Grow** | Write new sub-procedure for recurring pattern | Create `optimize_context_for_code.px` after observing code tasks need different context strategy |
| **Prune** | Remove/deprecate underperforming procedure | Deprecate a scoring heuristic that consistently picks wrong models |
| **Recombine** | Compose existing procedures in new order | Discover that running `check_memory` before `build_context` improves quality |

## Safety Model

The RSI procedure has HARD CONSTRAINTS that cannot be self-modified:

1. **Cannot modify itself** — `recursive-self-improvement.px`, `validate_improvement.px`, and constraint-checking procedures are immutable without human approval
2. **Evidence required** — Minimum 3 task observations before proposing any change
3. **Rate limited** — Maximum 3 procedure modifications per 24 hours
4. **Always rollbackable** — Previous versions stored, regression auto-triggers rollback
5. **Validated before registration** — New procedures must pass all `.px` constraints
6. **Regression detection** — If quality drops >10% after a change, automatic rollback

These constraints are the "immune system" — they prevent the system from:
- Runaway self-modification
- Optimizing for metrics over actual quality
- Removing its own safety checks
- Making changes too fast for humans to observe\n
## What RSI Can Improve (sub-procedures only)

- `select_model.px` — model scoring weights, task classification logic
- `build_session_context.px` — context assembly strategy, memory recall parameters
- `plan_task.px` — stage ordering, retry counts, timeout defaults
- `topic-routing.px` — classification confidence thresholds, reeval queue bounds
- `evaluate_gate.px` — pass/fail criteria, retry decisions
- Any NEW procedures it creates for patterns it discovers

## What RSI Cannot Improve (requires human)

- `recursive-self-improvement.px` itself
- `validate_improvement.px` (the safety validator)
- Any procedure in `praxis/constraints/` (safety boundaries)
- `px-first.px` (architectural constraints)

## Why This Is Safe

The key insight: **RSI operates on sub-procedures, never on itself or its validators.**

It's like evolution — the fitness function doesn't evolve itself. The selection pressure is fixed (task quality signals), and the adaptation happens in the population (sub-procedures). The RSI procedure is the selection pressure, not the population.

If the RSI procedure itself needs to change, that's a human decision — an architectural revision, not an optimization.

## Why This Is Powerful

1. **Compound improvement** — Each task makes the system slightly better. Over 100 tasks, the system is measurably different from where it started.
2. **Domain adaptation** — Working on code tasks? Model selection learns code model preferences. Switching to writing? It adapts. The system specializes to its actual workload.
3. **Failure recovery** — Bad procedure? Regression detection catches it, rolls back, and the system continues with the previous working version.
4. **Zero human maintenance** — Once the RSI loop is running, sub-procedures improve without human intervention. Humans only review when thresholds are exceeded or architectural changes are proposed.
5. **Observable** — Every improvement is a PluresDB write. The entire improvement history is auditable.

## Implications for the Codebase

- `praxis/procedures/recursive-self-improvement.px` — the RSI procedure (committed)
- `praxis/procedures/model-selection.px` — first target for RSI tuning
- `praxis/procedures/topic-routing.px` — second target
- PluresDB keys: `procedure:content:*`, `procedure:versions:*`, `performance:*`, `improvement:*`
- Rust boundary actors needed: `register_procedure` (hot-reload a .px into reactive registry), `validate_px_syntax` (parse without registering)

## References

- ADR-0005: Orchestration-as-Dataflow (foundation for RSI)
- kbristol 2026-06-13: "there's finally enough foundation so that we can engrain [RSI] into the heart of pares-radix by building it into the central orchestration procedure"
- C-PLURES-004: "PluresDB IS the system" — RSI state is PluresDB state
