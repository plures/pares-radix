# ADR-0016: Personality as Praxis Constraints (.px), Not Prose (.md)

**Status:** Proposed  
**Date:** 2026-05-03  
**Authors:** kbristol  
**Relates to:** ADR-0015 (Chronos logging), Cerebellum classifier, Praxis constraint model

## Context

Personality is currently defined as static Markdown files (`SOUL.md`, `IDENTITY.md`, `USER.md`) injected verbatim into every system prompt. This has several problems:

1. **No phase awareness.** "Be warm and friendly" is applied to code generation, tool dispatch, internal reasoning — contexts where it's irrelevant noise that burns tokens and can degrade output quality.
2. **No composability.** Personality traits can't be selectively activated, weighted, or combined per-task.
3. **No evolution.** Static files don't learn. A trait that consistently produces poor results stays forever unless manually edited.
4. **No evidence.** There's no way to measure whether a personality trait improves outcomes.
5. **Redundant classification.** The cerebellum already classifies intent, complexity, and task phase — exactly the signals needed to route personality. But personality injection happens upstream of the cerebellum, unconditionally.

## Decision

Replace prose-based personality (`.md` files) with Praxis constraint-based personality (`.px` files stored in PluresDB).

### Core Model

A personality constraint is a Praxis constraint with additional metadata:

```rust
struct PersonalityConstraint {
    // Praxis constraint fields
    id: String,
    name: String,
    description: String,
    severity: Severity,        // error | warning | info
    check: ConstraintCheck,    // evaluation function
    
    // Personality-specific fields
    applies_to: Vec<TaskPhase>,    // when this constraint is active
    trait_category: TraitCategory, // warmth, directness, humor, etc.
    weight: f32,                   // 0.0–1.0, tunable per-user/context
    prompt_injection: String,      // the actual system prompt text when active
}

enum TaskPhase {
    Planning,
    CodeGeneration,
    ToolUse,
    UserCommunication,
    ErrorReporting,
    Reflection,
    All,
}

enum TraitCategory {
    Warmth,
    Directness,
    Humor,
    Conciseness,
    Opinion,
    Empathy,
    Formality,
    Custom(String),
}
```

### Cerebellum as Personality Router

The cerebellum already produces a `MessageClassification` with intent, complexity, and tool requirements. Extend this to include **active personality set**:

```
User message
  → Cerebellum classifies (intent, complexity, phase)
  → Personality procedure queries PluresDB for constraints matching phase
  → Active constraint set injected into system prompt for this turn
  → Response generated with phase-appropriate personality
  → Post-response: evidence capture (did user react positively?)
```

### Phase-to-Constraint Mapping

| Phase | Active traits | Inactive traits |
|-------|--------------|-----------------|
| Planning | conciseness, directness | warmth, humor |
| Code generation | *(none — correctness only)* | all personality |
| Tool use | *(none)* | all personality |
| User communication | warmth, opinion, humor, empathy | — |
| Error reporting | directness, empathy | humor |
| Reflection | conciseness | warmth, humor |

### PluresDB Storage

Personality constraints stored as PluresDB records with category `personality-constraint`:

```json
{
  "id": "pc-warmth-001",
  "category": "personality-constraint",
  "content": "When communicating task completion or responding to casual messages, use a warm and approachable tone.",
  "tags": ["trait:warmth", "phase:user-communication", "phase:error-reporting"],
  "metadata": {
    "severity": "info",
    "weight": 0.8,
    "trait_category": "warmth",
    "applies_to": ["UserCommunication", "ErrorReporting"]
  }
}
```

### Procedures

| Procedure | Trigger | Purpose |
|-----------|---------|---------|
| `personality-resolve` | `before_search` | Given cerebellum classification, query matching personality constraints |
| `personality-inject` | `on_cue: prompt_build` | Inject active constraints into system prompt |
| `personality-evolve` | `cron: 7d` | Analyze evidence, adjust weights on underperforming traits |
| `personality-seed` | `manual` | Bootstrap from `.md` files into PluresDB constraint records |

### Migration Path

1. **Phase 0 (now):** `.md` files remain as bootstrap source. `personality-seed` procedure parses them into PluresDB constraints on first run.
2. **Phase 1:** Cerebellum extended with `active_personality: Vec<ConstraintId>` in classification output. Prompt builder reads from PluresDB instead of files.
3. **Phase 2:** `.md` files deprecated. Personality managed entirely through `.px` constraints and PluresDB procedures.
4. **Phase 3:** Evidence-driven weight adjustment. Personality traits evolve based on interaction outcomes.

### .px File Format

For version-controlled personality definitions (seeded into PluresDB):

```px
# personality.px — Pares Radix default personality

constraint warmth {
  trait: warmth
  phase: user_communication, error_reporting
  weight: 0.8
  severity: info
  prompt: "Use a warm, approachable tone. Be genuine, not performative."
}

constraint directness {
  trait: directness
  phase: all
  weight: 0.9
  severity: warning
  prompt: "Be direct. Skip filler phrases. Lead with the answer."
}

constraint conciseness {
  trait: conciseness
  phase: planning, reflection, code_generation
  weight: 0.7
  severity: info
  prompt: "Keep responses focused. One idea per paragraph."
}

constraint no_personality_in_code {
  trait: correctness
  phase: code_generation, tool_use
  weight: 1.0
  severity: error
  prompt: ""  # empty — no personality injection for this phase
}

constraint humor {
  trait: humor
  phase: user_communication
  weight: 0.4
  severity: info
  prompt: "Light humor is welcome when it fits naturally. Never force it."
}

constraint opinion {
  trait: opinion
  phase: user_communication
  weight: 0.6
  severity: info
  when: context.asks_for_recommendation
  prompt: "Have opinions. State preferences with reasoning. Don't hedge everything."
}
```

## Consequences

### Positive
- **Token efficiency.** No personality tokens wasted on code generation or tool dispatch.
- **Measurable.** Every trait has weight and evidence — personality becomes data, not vibes.
- **Composable.** Different users, contexts, or agents can have different constraint sets from the same pool.
- **Evolvable.** Procedures can adjust weights based on outcomes without manual file edits.
- **Praxis-native.** Personality uses the same constraint/evidence/procedure infrastructure as everything else.

### Negative
- **Complexity.** More moving parts than a static file.
- **Bootstrap dependency.** First-run requires seeding from `.px` files before personality is active.
- **Cerebellum coupling.** Personality routing depends on accurate classification. Bad classification → wrong personality.

### Risks
- **Over-engineering.** For a single-user agent, static files work fine. This architecture earns its complexity at multi-agent/multi-user scale.
- **Trait conflicts.** Two active constraints could contradict (e.g., "be verbose" + "be concise"). Need a conflict resolution strategy (weight wins, or severity wins).

## Evidence Gaps

| Unknown | How to test |
|---------|-------------|
| Does phase-gated personality measurably improve output quality? | A/B test: same prompts with/without personality gating, human eval |
| Can the cerebellum classify phase accurately enough to route personality? | Audit classification accuracy on 100 real messages |
| Does weight evolution converge to stable values? | Run `personality-evolve` for 30 days, plot weight trajectories |
| `.px` parser complexity | Prototype the parser, measure LOC and edge cases |
