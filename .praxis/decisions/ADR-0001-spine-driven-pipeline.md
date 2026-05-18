# ADR-0001: EventSpine as the Sole Message Processing Pipeline

## Status: Accepted

## Date: 2026-05-18

## Context

The EventSpine was designed as the message processing pipeline:

```
Channel → Spine → Procedures → Model → Spine → Delivery
```

But in practice, channel adapters (Telegram, TUI) bypass the spine entirely:

```
Channel → on_event(closure) → Model → Channel.send_reply()
```

The spine exists as an audit log, not a pipeline. All logic (slash command handling, model invocation, response formatting, progressive delivery) lives in channel adapters. This makes channels fat, the spine dead, and autonomous execution impossible because the spine's procedures — the only place where TaskLoop, Cerebellum, and DelegationManager could intercept and act — never fire meaningfully.

## Decision

**Clean break.** Remove the `on_event` callback pattern from all channel adapters. The EventSpine becomes the sole message processing path.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        EventSpine                            │
│                                                             │
│  InboundMessage → Pipeline Procedures → DeliveryRequest     │
│       ↑                    │                    ↓           │
│   Channels              TaskLoop            Channels        │
│   (input)            DelegationMgr          (output)        │
│                      Cerebellum                             │
│                      ModelClient                            │
└─────────────────────────────────────────────────────────────┘
```

### Channel Responsibilities (ONLY)

**Input side:**
- Receive platform-specific message (Telegram Update, TUI keypress, etc.)
- Parse into canonical `SpineEvent::Inbound { source, chat_id, sender, content, metadata }`
- Emit to spine
- Manage platform-specific UX (typing indicator, read receipts) via spine subscription

**Output side:**
- Subscribe to `SpineEvent::Deliver { target_channel, chat_id, content, format }`
- Render content for platform (HTML for Telegram, ANSI for TUI, etc.)
- Send via platform API
- Report `SpineEvent::DeliveryResult { success, message_id }` back to spine

### Spine Pipeline Procedures

Registered at startup, not in channels:

1. **`inbound_router`** — Receives InboundMessage, decides routing:
   - Slash command? → dispatch to command handler (registered as procedures)
   - Task-related? → TaskManager creates/updates task
   - Normal message? → emit `ModelRequest`

2. **`model_invoker`** — Receives ModelRequest:
   - Build context (history, tools, personality, constraints)
   - Call ModelClient
   - Emit `ModelResponse`

3. **`response_router`** — Receives ModelResponse:
   - Format for target channel contract
   - Emit `DeliveryRequest`

4. **`task_evaluator`** — Runs on idle (via TaskLoop):
   - Picks evaluable tasks
   - Uses Cerebellum to decide action
   - Emits `ModelRequest` with task context, or `DelegationRequest`

5. **`delegation_handler`** — Receives DelegationRequest:
   - Spawns sub-agents via DelegationManager
   - Tracks completion
   - Emits results back as spine events

### SpineEvent Taxonomy

```rust
pub enum SpineEvent {
    // Input
    Inbound { id: String, source: String, chat_id: String, sender: String, content: String, metadata: Value },
    
    // Processing
    ModelRequest { id: String, context: Vec<Message>, tools: Vec<Tool>, origin: EventOrigin },
    ModelResponse { id: String, content: String, tool_calls: Vec<ToolCall>, origin: EventOrigin },
    
    // Output
    DeliveryRequest { id: String, channel: String, chat_id: String, content: String, format: DeliveryFormat },
    DeliveryResult { id: String, success: bool, platform_message_id: Option<String> },
    
    // Tasks
    TaskCreated { id: String, task: Task },
    TaskUpdated { id: String, status: TaskStatus },
    
    // Delegation
    DelegationRequest { id: String, task_id: String, agent: String, input: String },
    DelegationComplete { id: String, task_id: String, output: String },
    
    // System
    Heartbeat { id: String, work_items: Vec<String> },
    ScheduledFire { id: String, task_id: String, command: String },
}
```

### EventOrigin

Tracks where a model request came from so responses route correctly:

```rust
pub enum EventOrigin {
    /// Direct user message — response goes back to same channel/chat
    UserMessage { channel: String, chat_id: String, message_id: Option<String> },
    /// Task evaluation — response goes to task result
    TaskEvaluation { task_id: String },
    /// Heartbeat — response may go to user or stay internal
    Heartbeat,
    /// Delegation — response goes to parent task
    Delegation { parent_task_id: String },
}
```

## Consequences

### Positive
- Channels become thin, testable, interchangeable
- TaskLoop, DelegationManager, Cerebellum all plug into the same pipeline
- Autonomous execution works because the pipeline handles system-originated events the same as user-originated ones
- Adding a new channel = implementing input parse + output render (no logic)
- Single point for logging, tracing, governance, rate limiting

### Negative
- Breaking change — existing Telegram adapter stops working until spine is real
- More complex startup (spine must be initialized before channels)
- Latency: spine indirection adds event hops (mitigated: in-process, not network)

### Risks
- If spine procedures are buggy, ALL channels break (but: better than each channel having independent bugs)
- Must handle backpressure (fast input, slow model) — spine needs a queue

## Implementation Plan

1. Define `SpineEvent` enum and the `Pipeline` trait
2. Build async spine with procedure registration and pub/sub
3. Implement the 5 core procedures (inbound_router, model_invoker, response_router, task_evaluator, delegation_handler)
4. Strip channel adapters to input/output only
5. Wire everything in main()
6. Delete the `on_event` callback pattern entirely
