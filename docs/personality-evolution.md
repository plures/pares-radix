# Personality Evolution Architecture

## Overview

The cerebellum manages personality adaptation through a gated pipeline that ensures safety while allowing the system to learn from user interactions.

## Architecture

```
User Interaction
       │
       ▼
┌─────────────────────┐
│  Pattern Detector   │  ← BitNet classifier (cerebellum)
│  (cerebellum)       │
└─────────────────────┘
       │
       ▼ PersonalitySignal { kind, confidence, evidence[] }
┌─────────────────────┐
│  Confidence Gate    │  ← Praxis write gate
│  (praxis)           │
│                     │
│  ≥ 0.90: auto-adopt │
│  0.70–0.89: propose │
│  < 0.70: log only   │
└─────────────────────┘
       │
       ▼
┌─────────────────────┐
│  Safety Check       │  ← HARDCODED, not in PluresDB
│  (inviolable)       │
│                     │
│  Reject if:         │
│  - Overrides safety │
│  - Enables harm     │
│  - Removes gates    │
└─────────────────────┘
       │
       ▼
┌─────────────────────┐
│  PluresDB Write     │  ← PersonalityRule node
│  (graph store)      │
│                     │
│  category: "rule"   │
│  confidence: f32    │
│  source: "explicit" │
│  evidence: [...]    │
│  status: "active"   │
│  │"proposed"|"log"  │
└─────────────────────┘
       │
       ▼
┌─────────────────────┐
│  Chronos Log        │  ← Decision record
│  (timeline)         │
└─────────────────────┘
```

## Signal Types

### Explicit (highest confidence)
User directly states a rule:
- "Always push to GitHub without asking" → confidence 0.95
- "Never modify my config file" → confidence 0.98
- "I prefer short answers" → confidence 0.90

### Corrective (high confidence after pattern)
User corrects behavior repeatedly:
- 1st correction: log, confidence 0.40
- 2nd same correction: propose, confidence 0.70
- 3rd same correction: auto-adopt, confidence 0.90

### Frustration (medium confidence, triggers investigation)
User expresses frustration with outcome:
- "You told me you fixed this!" → investigate what went wrong
- Doesn't directly create a rule, but:
  - Creates a constraint: "verify fix before claiming done"
  - Confidence 0.80 (frustration is strong signal)

### Implicit (low confidence, accumulates)
Detected from behavior without explicit statement:
- User always reformats your output → maybe they prefer a different format
- User always adds context you missed → maybe you should check more sources
- Confidence starts at 0.30, accumulates +0.15 per instance

## PluresDB Schema

```rust
PersonalityRule {
    id: Uuid,
    rule: String,           // The actual rule text
    category: String,       // "communication" | "engineering" | "safety" | "workflow"
    confidence: f32,        // 0.0–1.0
    status: String,         // "active" | "proposed" | "logged" | "rejected" | "deprecated"
    source: String,         // "explicit" | "corrective" | "frustration" | "implicit"
    evidence: Vec<Evidence>, // What led to this rule
    created_at: DateTime,
    last_reinforced: DateTime,
    reinforcement_count: u32,
    conflicts_with: Vec<Uuid>, // Other rules this conflicts with
}

Evidence {
    timestamp: DateTime,
    interaction: String,     // What happened
    signal_type: String,     // The signal that created this
    raw_confidence: f32,     // Confidence of this individual evidence
}
```

## Praxis Procedures

### `personality-signal-evaluate`
Trigger: `on_cue: personality_signal`
1. Receive PersonalitySignal
2. Check against safety axioms (REJECT if violation)
3. Check for conflicts with existing active rules
4. If conflicts: trigger `personality-arbitrate`
5. Score confidence
6. If ≥ 0.90: write as "active"
7. If 0.70–0.89: write as "proposed", notify user
8. If < 0.70: write as "logged"

### `personality-arbitrate`
Trigger: `on_cue: personality_conflict`
1. Load conflicting rules
2. Compare evidence strength (count × confidence)
3. If clear winner: deprecate loser, activate winner
4. If ambiguous: propose both to user for resolution

### `personality-decay`
Trigger: `cron: 7d`
1. Find active rules not reinforced in 30 days
2. Reduce confidence by 0.10
3. If confidence drops below 0.50: move to "proposed" (ask user if still valid)
4. If below 0.30: deprecate

### `personality-frustration-handler`
Trigger: `on_cue: user_frustration`
1. Analyze what went wrong (check Chronos for recent actions)
2. Identify the gap (what should have been different?)
3. Propose a corrective constraint
4. Store evidence linking frustration to the gap

## Safety Invariants (HARDCODED)

These live in Rust code, NOT in PluresDB. They cannot be modified at runtime:

```rust
const SAFETY_AXIOMS: &[&str] = &[
    "Never take irreversible destructive action without confirmation",
    "Always create rollback plans for multi-step operations",
    "Verify with fresh facts rather than relying on stale memory",
    "Proactively minimize user friction within ethical bounds",
];

fn safety_check(proposed_rule: &PersonalityRule) -> Result<(), SafetyViolation> {
    // Check if the proposed rule would:
    // 1. Override or weaken a safety axiom
    // 2. Enable harmful actions without gates
    // 3. Remove confirmation requirements
    // 4. Disable logging or audit trails
    // If any of the above: REJECT unconditionally
}
```

## User Interface

```
/personality list              — Show active rules with confidence
/personality proposed          — Show pending proposals
/personality approve <id>      — Accept a proposed rule
/personality reject <id>       — Reject a proposed rule
/personality history           — Show evolution timeline
/personality explain <id>      — Show evidence for a rule
```

## Example Flow

1. User: "You should always run tests before saying something is fixed"
2. Cerebellum detects: explicit instruction, category=engineering
3. Confidence: 0.95 (explicit + imperative + clear rule)
4. Safety check: PASS (doesn't weaken any axiom, actually strengthens "verify before claiming")
5. Write to PluresDB: PersonalityRule { rule: "Run tests before confirming a fix", confidence: 0.95, status: "active", source: "explicit" }
6. Chronos log: "Adopted personality rule from explicit instruction"
7. From now on: before saying "fixed", run relevant tests

## Integration with BitNet

The cerebellum uses BitNet (local, offline) for:
- **Signal classification**: Is this an explicit rule? Correction? Frustration?
- **Confidence scoring**: How strong is this signal?
- **Conflict detection**: Does this contradict existing rules?
- **Frustration analysis**: What went wrong and what's the corrective constraint?

BitNet runs at the classifier tier — always available, even offline. This means personality evolution works without cloud connectivity.
