# .px-First Migration Assessment — pares-radix

## Executive Summary

We have **~9,400 lines of pure logic in Rust** that violate the `.px-first` principle (C-DEV-001). These modules contain routing decisions, classification, scheduling, memory organization, and data transformation that should be `.px` procedures — with Rust only providing the IO/side-effect boundaries.

This isn't just aesthetic debt — it means:
- Logic can't be hot-reloaded (requires recompile + redeploy)
- Logic isn't inspectable by the praxis constraint system
- Logic can't be evolved by the agent itself (no self-improvement loop)
- We're maintaining two parallel systems: .px procedures AND hardcoded Rust logic

## Priority Classification

### 🔴 Critical Violations (Pure logic, zero IO, should be 100% .px)

| Module | Lines | What it does | Why it's wrong |
|--------|-------|-------------|----------------|
| `cerebellum/classifier.rs` | 431 | Intent classification, complexity scoring, topic detection | Pure keyword/rule logic. Only IO: optional LLM backend call |
| `cerebellum/router.rs` | 343 | Routes events to handlers based on classification | Match/if-else routing tree — textbook .px procedure |
| `cerebellum/context_manager.rs` | 428 | Relevance scoring, context window management | Pure math + logic, no IO |
| `praxis/guidance.rs` | 325 | Generates guidance text from constraints | Pure text generation from data — .px |
| `personality.rs` | 492 | Personality traits, response style rules | Pure config/logic, zero IO |
| `heartbeat.rs` (logic portion) | ~200 | Quiet hours, check scheduling, interval logic | Pure scheduling rules |

**Subtotal: ~2,219 lines → should be .px procedures**

### 🟠 Major Violations (Mixed logic + IO, logic portion should be .px)

| Module | Lines | Logic vs IO | Migration path |
|--------|-------|-------------|----------------|
| `cerebellum/pipeline.rs` | 1,444 | ~70% logic (stage routing, fallback chains, retry policy) / 30% IO (model calls, embedding) | Extract pipeline orchestration logic to .px, keep model/embed actors in Rust |
| `cerebellum/mod.rs` | 935 | ~60% logic (topic shift detection, autorecall strategy, context building) / 40% IO (embedding, store reads) | Extract preprocess logic to .px, keep embedding/store calls as actions |
| `memory/forgetting/engine.rs` | 1,131 | ~50% logic (retention policy evaluation, soft-delete scheduling) / 50% IO (store ops) | Extract retention policies + scheduling to .px |
| `memory/correction.rs` | 691 | ~70% logic (correction matching, dedup, quality scoring) / 30% IO (store writes) | Quality/correction logic to .px |
| `chronos.rs` | 997 | ~40% logic (level filtering, timeline queries) / 60% IO (CRDT store) | Timeline query/filter logic to .px |
| `praxis/constraints.rs` | 776 | ~80% logic (constraint evaluation, severity routing) / 20% IO (store reads) | Constraint evaluation IS what .px does natively |

**Subtotal: ~5,974 lines, ~3,500 lines of extractable logic**

### 🟡 Moderate Violations (Logic-heavy but with deeper IO coupling)

| Module | Lines | Notes |
|--------|-------|-------|
| `memory/mod.rs` | 1,616 | Heavy store interaction but consolidation/quality/decay logic is pure |
| `agent.rs` | 2,578 | Orchestration — the Agent loop itself is hard to express as .px since it IS the executor |
| `delegation/manager.rs` | 842 | Task decomposition + allocation logic is pure |
| `delegation/broker.rs` | 652 | Routing decisions for sub-agents |

### ✅ Correctly in Rust (IO boundaries, no violation)

| Module | Lines | Why it's correct |
|--------|-------|-----------------|
| `shell_executor.rs` | 1,676 | Spawns processes — pure IO boundary |
| `spine/procedures/model_invoker.rs` | 625 | Makes HTTP calls to LLM APIs |
| `spine/procedures/tool_executor.rs` | 724 | Executes external tool calls |
| `spine/conversation.rs` | PluresDB reads/writes — IO |
| `auth/copilot.rs` | 646 | OAuth flow — network IO |
| `otel.rs` / `otel_metrics.rs` | Telemetry emission — IO |
| `px_adapter.rs` | 898 | Bridge between .px and PluresDB — infrastructure |

## Migration Plan

### Phase 1: Quick wins (no architectural change needed)

These can be extracted to .px TODAY with the current executor:

1. **`classifier.rs` → `praxis/procedures/classify.px`**
   - Keyword lists, intent rules, complexity scoring → .px `when` blocks
   - Keep `ClassifierBackend` trait call as a Rust action callable from .px
   - ~431 lines of Rust → ~80 lines of .px

2. **`personality.rs` → `praxis/personality.px`** (already partially exists!)
   - Trait definitions, response style rules → .px constraints
   - ~492 lines of Rust → extends existing `personality.px` fixture

3. **`praxis/guidance.rs` → `praxis/procedures/guidance.px`**
   - Constraint-to-guidance text generation → .px procedure
   - ~325 lines of Rust → ~50 lines of .px

4. **`heartbeat.rs` logic → `praxis/spine/heartbeat.px`** (already exists!)
   - Quiet hours, interval logic → extend existing `heartbeat.px`
   - Keep timer/tokio scheduling in Rust as IO boundary

### Phase 2: Core cerebellum extraction

5. **`cerebellum/router.rs` → `praxis/procedures/routing.px`**
   - Event-to-handler routing decisions → .px when/return pattern
   - ~343 lines → ~40 lines of .px

6. **`cerebellum/context_manager.rs` → `praxis/procedures/context-window.px`**
   - Relevance scoring weights, window sizing, eviction policy → .px
   - Keep `cosine_similarity()` as Rust action
   - ~428 lines → ~100 lines of .px

7. **`cerebellum/mod.rs` preprocess logic → `praxis/spine/preprocess.px`**
   - Autorecall strategy, topic shift policy, fallback decisions → .px
   - Keep embedding computation + store access as Rust actions
   - ~500 lines extractable → ~80 lines of .px

### Phase 3: Memory system

8. **`memory/forgetting/engine.rs` retention logic → `praxis/procedures/retention.px`**
   - Retention policies, decay curves, soft-delete scheduling → .px
   - Keep store operations as Rust actions
   - ~600 lines extractable → ~120 lines of .px

9. **`memory/correction.rs` → `praxis/procedures/memory-correction.px`**
   - Correction matching, quality scoring, dedup logic → .px
   - ~500 lines extractable → ~80 lines of .px

### Phase 4: Constraint system (meta — constraints evaluating constraints)

10. **`praxis/constraints.rs` → native pluresdb-px constraint evaluation**
    - This is the most ironic violation: the constraint system is written in Rust
      instead of being expressed AS constraints that pluresdb-px evaluates natively
    - ~776 lines → self-hosting: constraints evaluate themselves

## What Stays in Rust

- **All IO boundaries** (HTTP, filesystem, shell, PluresDB, embedding model inference)
- **The executor itself** (pluresdb-px is Rust — it runs .px procedures)
- **Channels** (Telegram, HTTP, stdio adapters)
- **Serialization** (serde, protocol buffers)
- **Concurrency primitives** (tokio runtime, channels, locks)
- **The spine pipeline skeleton** (InboundRouter→HistoryRecorder→ModelInvoker→ToolExecutor→ResponseRouter)
  - But the LOGIC within each stage should be .px
  - The Rust code becomes: "receive event, call .px procedure, emit result"

## New Rust Actions Required

For .px to express this logic, we need these built-in actions registered:

```
action compute_embedding(text: string) -> vec<f32>    # Already exists via fastembed
action cosine_similarity(a: vec<f32>, b: vec<f32>) -> f32
action classify_llm(system: string, message: string) -> string  # Optional LLM classifier
action get_current_time() -> i64                       # Unix timestamp
action read_state(key: string) -> json                 # PluresDB read
action write_state(key: string, value: json)           # PluresDB write
action emit_event(type: string, payload: json)         # Spine event emission
```

Most of these already exist in some form — they just need to be registered as .px-callable actions.

## Impact Assessment

| Metric | Before | After |
|--------|--------|-------|
| Pure-logic Rust lines | ~9,400 | ~2,000 (IO glue only) |
| .px procedure lines | ~75KB | ~95KB |
| Hot-reloadable logic | ~30% | ~85% |
| Self-improvable by agent | No | Yes (agent writes .px, reloads) |
| Recompile required for logic changes | Always | Only for IO boundary changes |
| Constraint coverage | ~20% (can't inspect Rust) | ~85% (all .px is inspectable) |

## Recommended Execution Order

1. ✅ Phase 1 (classifier, personality, guidance, heartbeat logic) — **1-2 days**
2. 🔜 Phase 2 (cerebellum extraction) — **2-3 days**  
3. 📅 Phase 3 (memory system) — **2-3 days**
4. 📅 Phase 4 (constraint meta-hosting) — **1-2 days**

**Total: ~8-10 days of focused work to achieve 85% .px coverage.**

## The Self-Improvement Payoff

Once logic lives in .px:
- Praxisbot can modify its own classification rules without recompilation
- New personality traits are a .px file drop — no Rust change
- Memory retention policies evolve through usage patterns (agent writes .px adjustments)
- The constraint system becomes self-referential: constraints checking constraints, all in .px
- Deployment becomes: update .px files → auto-reload → done (no `nix flake update`, no binary rebuild)

This is the architecture we designed. Time to actually use it.
