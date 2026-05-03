# Chronos Logging Pattern — ADR-0015

## Status
**Accepted** — 2026-05-03

## Context
We had separate logging systems proliferating: telemetry JSONL, PluresDB memory capture, Chronos causal timeline, tracing logs. Each captured partial information in different formats. Debugging required correlating across multiple sources.

## Decision
**Chronos is the single log for all plures systems.** Every significant event flows through `ChronosTimeline.record()`. One entry format, multiple output sinks.

### The Pattern

```rust
// Every significant action → Chronos
chronos.record(&chronos.build_entry(
    key,              // what was affected
    actor,            // who did it
    ChronosAction,    // what kind of action
    &data,            // the payload (serde_json::Value)
    constraints,      // what rules were evaluated
    rationale,        // why this happened
));
```

### Output Sinks (from single record() call)

| Sink | Format | Purpose | Query |
|------|--------|---------|-------|
| PluresDB | Graph nodes + edges | Primary store, causal chains, vector search | `chronos.history(key)`, `chronos.by_actor(actor)` |
| JSONL files | One JSON line per entry | Debug, grep, git diff, cross-machine | `jq`, `grep`, `cat \| python3` |
| (future) Hyperswarm | P2P replication | Cross-node sync | PluresDB CRDT merge |
| (future) WebSocket | Live stream | Dashboard, real-time monitoring | Subscribe to topic |

### ChronosEntry Schema

```rust
pub struct ChronosEntry {
    pub id: String,              // UUID v4
    pub timestamp: u64,          // Unix seconds
    pub actor: String,           // who: "agent", "user", "cerebellum", "tool:run_command"
    pub key: String,             // what: "agent:interaction:telegram", "tool:gh", "context:managed"
    pub action: ChronosAction,   // how: Create, Update, MessageReceived, ToolInvoked, etc.
    pub data_hash: String,       // SHA-256 of data (content-addressed)
    pub parent_id: Option<String>, // causal link to previous entry for this key
    pub rationale: Option<String>, // why: human-readable reason
    pub constraint_results: Vec<String>, // praxis constraints evaluated
}
```

### ChronosAction Types

| Action | When to use |
|--------|-------------|
| `Create` | New data written to PluresDB |
| `Update` | Existing data modified |
| `Delete` | Data removed |
| `Move` | Data relocated |
| `MessageReceived` | User sent a message |
| `ResponseGenerated` | Agent produced a response |
| `ToolInvoked` | A tool was called (shell, file, web) |
| `ContextManaged` | Cerebellum adjusted the context window |
| `ModelCalled` | LLM inference happened (with model name + latency) |
| `OutcomeRecorded` | User correction or acceptance signal |

### JSONL Output

Enabled by `PARES_TELEMETRY_DIR` environment variable:

```bash
export PARES_TELEMETRY_DIR=/home/kbristol/.pares-radix/telemetry
```

Produces daily files:
```
~/.pares-radix/telemetry/2026-05-03.jsonl
```

Each line is a complete `ChronosEntry` serialized as JSON.

### Querying

```bash
# All tool invocations today
cat telemetry/2026-05-03.jsonl | jq 'select(.action == "ToolInvoked")'

# Causal chain for an interaction
cat telemetry/2026-05-03.jsonl | jq 'select(.id == "abc" or .parent_id == "abc")'

# All actions by the agent
cat telemetry/2026-05-03.jsonl | jq 'select(.actor == "agent")'

# Context management decisions
cat telemetry/2026-05-03.jsonl | jq 'select(.action == "ContextManaged") | .rationale'

# Response latencies (from data payload)
cat telemetry/2026-05-03.jsonl | jq 'select(.action == "ModelCalled") | .data_hash'
```

### Implementation Checklist

Every plures component that does significant work must log to Chronos:

- [x] **Agent response** — `ResponseGenerated` after every model completion
- [ ] **Tool execution** — `ToolInvoked` before/after every tool call
- [ ] **Context management** — `ContextManaged` when cerebellum adjusts window
- [ ] **Model calls** — `ModelCalled` with model name, latency, token counts
- [ ] **Memory writes** — `Create`/`Update` through Praxis write gate (already done)
- [ ] **Personality changes** — `Update` when rules are adopted/deprecated
- [ ] **Outcome signals** — `OutcomeRecorded` on user correction/acceptance
- [ ] **Cluster events** — `Create` when rector discovers/loses nodes
- [ ] **PluresDB sync** — `Update` when CRDT merges from peers

### Anti-patterns

1. **Don't create separate log modules.** No `telemetry.rs`, no `audit_log.rs`, no `interaction_tracker.rs`. Chronos only.
2. **Don't log to stdout/stderr for business logic.** `tracing` is for debug/ops. Chronos is for business events.
3. **Don't skip the causal chain.** Every entry should have a `parent_id` when there's a logical predecessor.
4. **Don't put secrets in data payloads.** PII guard applies before Chronos writes.

## Consequences

- One place to look for "what happened"
- Causal chains enable root-cause analysis ("this failed because that context was wrong")
- JSONL files are git-diffable across machines
- PluresDB storage enables semantic search over the log
- Training data collection becomes trivial (every interaction is already logged)
