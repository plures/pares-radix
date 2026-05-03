# ADR-0016: PluresDB as Agent Runtime — Procedure-Driven Intelligence

**Status:** Proposed  
**Date:** 2026-05-03  
**Authors:** kbristol  
**Supersedes:** Static personality files (SOUL.md, IDENTITY.md)  
**Relates to:** Cerebellum, Praxis constraint model, PluresDB procedures

## Context

The current pares-agens architecture treats the LLM as the orchestrator — it receives a static system prompt, the full conversation, and decides what to do. PluresDB is passive storage. The cerebellum is a Rust function. Personality is a markdown file. Classification is compiled code.

This creates several structural problems:

1. **The model orchestrates, but can't learn.** Classification logic, personality, and context selection are frozen in code. Changing behavior requires recompilation.
2. **Personality is unconditional.** "Be warm" applies to code generation. Token waste, potential quality degradation.
3. **Classification is a function, not a procedure.** It can't evolve, capture evidence, or adjust based on outcomes.
4. **Context is assembled once.** The system prompt is built upstream and handed to the model as a monolith. There's no post-response evaluation or iterative refinement.
5. **The model sees everything or nothing.** No mechanism to selectively inject context based on task phase.

## Decision

**Invert the architecture: PluresDB is the runtime, the model is a compute step.**

### Core Loop

```
User message
  → Write to PluresDB (raw input record)
  → Triggers fire:
      classify       — determine intent, phase, complexity
      recall         — semantic search for relevant context
      personality    — resolve active constraints for this phase
      context-build  — assemble prompt from procedure outputs
  → Procedure step: call model (with constructed prompt + context)
  → Model response
  → Write to PluresDB (raw output record)
  → Triggers fire:
      evidence       — capture facts, update constraint weights
      fact-extract   — pull durable knowledge into memory
      personality-eval — did response satisfy active constraints?
      constraint-check — does output violate any active constraints?
  → If constraint violation → re-prompt with correction context (loop)
  → If tools needed → write tool request → trigger tool procedures → loop
  → Emit final output to user
```

### The Model is a Procedure Step

The LLM call is expressed as a step in a PluresDB procedure, not as the outer loop:

```px
procedure respond {
  trigger: on_write { category: "user-input" }
  
  steps:
    classify   → $classification
    recall     → $context        # semantic search, phase-aware
    personality → $traits        # active constraints for $classification.phase
    
    build_prompt {
      system: $traits.prompt_fragments
      context: $context.memories
      history: last($classification.history_depth) messages
    } → $prompt
    
    model_call {
      prompt: $prompt
      model: select_model($classification.complexity)
    } → $response
    
    evaluate {
      response: $response
      constraints: $traits.constraints
    } → $evaluation
    
    when $evaluation.violations > 0 {
      model_call {
        prompt: $prompt + correction($evaluation.violations)
        model: $prompt.model
      } → $response
    }
    
    emit { to: user, content: $response }
    
    capture_evidence {
      input: $classification
      output: $response
      constraints: $evaluation
    }
}
```

### Classification as Procedure

The cerebellum classifier becomes a `.px` procedure, not Rust code:

```px
procedure classify {
  trigger: on_write { category: "user-input" }
  
  phase: match {
    input.has_code_request || input.references_file → CodeGeneration
    input.is_question && input.complexity < 3       → UserCommunication
    input.references_error || input.tone == frustrated → ErrorReporting
    input.requests_tool || input.mentions_command    → ToolUse
    input.is_multi_step || input.complexity >= 4    → Planning
    else                                             → UserCommunication
  }
  
  confidence: semantic_similarity(input, phase.exemplars)
  
  when confidence < 0.7 {
    # Low confidence — use the model itself to classify
    model_call {
      prompt: "Classify this message phase: {input.text}"
      model: fast_model  # cheap, fast model for meta-tasks
    } → $model_phase
    phase: $model_phase
    capture_evidence(input, $model_phase, "model_classified")
  }
  
  store {
    category: "classification"
    tags: [phase, confidence]
    links: [input.id]
  }
}
```

### Personality as Phase-Gated Constraints

Personality traits are PluresDB records activated per-phase:

```px
constraint warmth {
  phase: user_communication, error_reporting
  weight: 0.8
  prompt: "Use a warm, approachable tone. Be genuine, not performative."
}

constraint no_personality {
  phase: code_generation, tool_use
  weight: 1.0
  prompt: ""  # silence — no personality injection
}

constraint directness {
  phase: all
  weight: 0.9
  prompt: "Be direct. Lead with the answer."
}
```

The `personality` procedure queries constraints matching the current phase, sorted by weight, and produces prompt fragments injected into the model call.

### Evidence-Driven Evolution

Every response cycle captures evidence:

```px
procedure personality_evolve {
  trigger: cron { every: 7d }
  
  steps:
    query_evidence { 
      category: "personality-evidence"
      window: 7d 
    } → $evidence
    
    for each $trait in active_constraints {
      success_rate: $evidence.where(trait == $trait).positive / total
      
      when success_rate < 0.5 {
        update $trait.weight -= 0.1
        capture "trait {$trait.name} underperforming, weight reduced"
      }
      
      when success_rate > 0.9 {
        update $trait.weight += 0.05
        capture "trait {$trait.name} performing well, weight increased"
      }
    }
}
```

### What Rust Becomes

The Rust binary becomes a thin event loop:

```rust
loop {
    let event = receive_event().await;       // Telegram, CLI, HTTP
    pluresdb.write(event).await;             // triggers fire automatically
    // ... that's it. Procedures handle everything.
}
```

The Rust code owns:
- **I/O adapters** — Telegram, CLI, HTTP, WebSocket (these can't be procedures)
- **PluresDB engine** — the procedure executor itself
- **Model clients** — HTTP calls to LLM APIs (called by procedure steps)
- **Tool executors** — file I/O, shell, web fetch (called by procedure steps)

Everything else — classification, personality, context assembly, prompt construction, response evaluation, evidence capture — is `.px` procedures in PluresDB.

### .px File Format

SudoLang-inspired constraint and procedure definitions. Seeded into PluresDB on first run, then live as mutable records:

```px
# agent.px — Core agent procedures

procedure classify { ... }
procedure recall { ... }
procedure personality { ... }
procedure respond { ... }
procedure evidence { ... }

constraint warmth { ... }
constraint directness { ... }
constraint conciseness { ... }

# Model selection
model_selector {
  complexity < 3  → fast_model    # gpt-4.1-mini, local BitNet
  complexity < 5  → default_model # gpt-4.1
  complexity >= 5 → deep_model    # gpt-5.2, o3
}
```

### Migration Path

| Phase | What changes | What stays |
|-------|-------------|------------|
| **0 (now)** | ADR accepted. `.px` parser prototyped. | Everything current |
| **1** | Classification procedure replaces Rust cerebellum classifier. Personality constraints replace `.md` files. | Model call still in Rust orchestration |
| **2** | Full procedure chain: input → classify → recall → personality → prompt → model → evaluate → emit. Rust becomes thin event loop. | I/O adapters, tool executors in Rust |
| **3** | Evidence-driven evolution. Procedures self-modify weights and match patterns based on outcomes. | Rust I/O + PluresDB engine |

## Consequences

### Positive
- **Living intelligence.** Classification, personality, and context selection evolve without recompilation.
- **Phase-aware everything.** Code gen gets no personality. User replies get no tool noise. Each phase gets exactly the context it needs.
- **Observable.** Every procedure step writes to PluresDB. Full audit trail of why the agent did what it did.
- **Composable.** New behaviors added by writing `.px` files, not Rust code.
- **Self-improving.** Evidence procedures adjust weights and patterns over time.

### Negative
- **Performance.** Procedure execution adds latency vs. direct Rust function calls. Mitigated by PluresDB being in-process.
- **Complexity.** Debugging procedure chains is harder than debugging Rust functions.
- **Bootstrap.** First message requires procedures to already be seeded. Cold start problem.
- **Parser investment.** `.px` needs a real parser — not trivial.

### Risks
- **Procedure loops.** A procedure that triggers itself via writes could infinite-loop. Need cycle detection.
- **Model dependency for classification.** If the fast model is unavailable, fallback classification must still work (heuristic procedures).
- **Constraint conflicts.** Two procedures writing contradictory context. Need a merge/priority strategy.

## Evidence Gaps

| Unknown | Test |
|---------|------|
| Procedure execution latency vs. Rust classifier | Benchmark: 1000 messages through each path |
| `.px` parser complexity | Prototype parser, count LOC and edge cases |
| Can procedures express all current cerebellum logic? | Port existing classifier 1:1 to `.px`, verify identical outputs |
| Does evidence-driven weight evolution converge? | Run 30-day simulation with synthetic feedback |
| Cold start: how fast can procedures seed from `.px` files? | Measure seed time for full personality + classifier set |
