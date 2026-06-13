# ADR-0005: Orchestration-as-Dataflow (.px All the Way Down)

## Status: Accepted

## Date: 2026-06-13

## Context

We're building dev-lifecycle orchestration for pares-radix. The initial approach was:
- `.px` defines the logic (stages, gates, constraints)
- TypeScript/Rust executes the side effects (spawn subagents, read results)

But a more powerful recursive pattern emerged:

**If the orchestration itself is `.px`, and the code the orchestrator writes for subagents to execute is ALSO `.px`, then we get full recursive self-improvement with zero glue code.**

## Decision

**Orchestration in pares-radix is `.px` procedures calling other `.px` procedures, with Rust actors only at the IO boundary.**

The key insight: an orchestration procedure doesn't need to "do" anything. It only needs to **write data** that triggers pre-existing procedures which already know how to:
- Build session context
- Select and call an LLM API
- Dispatch tools
- Deliver messages to channels
- Manage memory and state

## Architecture

```\n┌─────────────────────────────────────────────────────┐
│                   .px Procedures                     │
│                                                     │
│  orchestrate_task.px                                │
│    ├── write to "task_request" queue                │
│    │     └── triggers: plan_task.px                 │
│    │           └── write to "stage_ready" queue     │
│    │                 └── triggers: execute_stage.px │
│    │                       └── write to "inbound"   │
│    │                             └── triggers:      │
│    │                                 preprocess.px  │
│    │                                 context.px     │
│    │                                 generate.px    │
│    │                                 postprocess.px │
│    └── (completion bubbles back up the same way)    │
│                                                     │
│  The orchestrator doesn't call LLMs.                │
│  It writes a datum. The existing pipeline does      │
│  everything else.                                   │
└─────────────────────────────────────────────────────┘
           │ (boundary — side effects only)
┌──────────▼──────────────────────────────────────────┐
│              Rust IO Actors (dumb + generic)         │
│                                                     │
│  • llm_call — HTTP to model API                     │
│  • tool_exec — shell/process execution              │
│  • channel_send — Telegram/HTTP delivery            │
│  • file_read/write — filesystem IO                  │
│  • spawn_process — OS-level process management      │
│  • http_request — external API calls                │
│                                                     │
│  These actors know NOTHING about orchestration,     │
│  topics, tasks, or stages. They just do IO.         │
└─────────────────────────────────────────────────────┘
           │
┌──────────▼──────────────────────────────────────────┐
│              PluresDB (reactive state spine)         │
│                                                     │
│  • Stores all state (tasks, topics, memories)       │
│  • Writes trigger procedure execution              │
│  • Procedures read/write more state                 │
│  • The cycle IS the computation                     │
└─────────────────────────────────────────────────────┘
```

## Why This Is Powerful

1. **Orchestration is trivial to write.** A `.px` procedure that orchestrates only needs to write the right datum to the right key. It doesn't need to understand HTTP, LLM APIs, tool dispatch, or session management. Those are other procedures' jobs.

2. **Recursive self-improvement.** If pares-radix can write `.px` procedures, and those procedures can orchestrate work, then pares-radix can write orchestration for itself. The skill that creates orchestration code is itself orchestration code.

3. **Composability.** Procedures compose by writing to each other's trigger queues. No imports, no dependency injection, no wiring. Write a datum → procedure fires. Done.

4. **Testability.** Pure `.px` procedures can be tested by writing test data and asserting output data. No mocks needed for IO because IO doesn't exist in the procedure.

5. **Observability.** Every decision is a PluresDB write. You can replay, inspect, and audit the entire orchestration by reading the DB.

6. **The orchestrator doesn't need tools.** It doesn't call LLMs, spawn processes, or send messages. It writes data that triggers procedures that do those things. Separation of concerns at the language level.

## Implications

- **TASK-003 (topic routing)** should follow this pattern: `classify_topic.px` writes a topic decision → triggers `switch_context.px` or `steer_continuation.px` → those write to existing pipeline queues
- **Dev-lifecycle orchestration** simplifies: instead of a `spawn_subagent` actor, the orchestration procedure writes to `inbound` with the right metadata, and the existing message pipeline does the rest
- **New features** are just new `.px` files that write to existing queues. No Rust changes needed unless a new IO boundary is required.
- **Rust actors should be as dumb and generic as possible.** `llm_call` doesn't know about conversations. `channel_send` doesn't know about topics. They just do IO.

## Constraints

- C-DEV-001 already says "start with .px" — this ADR makes it structural, not aspirational
- C-PLURES-004 already says "PluresDB IS the system" — this ADR applies that to orchestration specifically
- The only time Rust code should contain decision logic is when the decision is purely about IO routing (e.g., "which HTTP endpoint for this model provider")

## References

- `praxis/procedures/dev-lifecycle.px` — first implementation of this pattern
- `praxis/procedures/topic-routing.px` — second implementation (topic classification)
- kbristol 2026-06-13: "the orchestration code shouldn't be external python, rust, or javascript... it merely needs to focus on orchestrating, not doing."
