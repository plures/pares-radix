# ADR-0017: Channel-Agnostic Agent Loop

## Status
**ACCEPTED** — 2026-05-16

## Context
pares-radix has three user-facing interfaces:
- **Telegram** (`serve` command): Full agent with model calls, tool dispatch, conversation history, memory, cerebellum routing, and Chronos audit
- **TUI** (`tui` command): Full agent (same as Telegram, different UI)
- **MCP** (`mcp-serve` command): Tool server only — exposes 56 individual tools but NO agent loop

The `ask` command is a fourth interface but single-shot (no tools, no history).

## Problem
**The communication channel determines the capability of the response.** The same prompt sent via Telegram gets a full agent response (model reasoning + tool use + memory recall), while the same prompt via MCP only gets raw tool access with no reasoning.

This violates our core principle: **the channel is a transport mechanism, not a capability boundary.**

A user asking "implement the pares-arca MCP server" via Telegram gets a full agent that reads the spec, writes code, runs tests, and pushes. The same request via MCP gets nothing — there's no `agent_ask` or `agent_chat` tool.

## Decision
**The agent loop must be accessible from every interface as a first-class capability.**

### Architecture
```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│  Telegram    │  │    TUI      │  │    MCP      │  │   ask CLI   │
│  (channel)   │  │  (channel)  │  │  (channel)  │  │  (channel)  │
└──────┬───────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                 │                │                 │
       ▼                 ▼                ▼                 ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Agent Core (unified)                          │
│  - Model client (Copilot/OpenAI)                                │
│  - Tool dispatcher                                              │
│  - Conversation history                                         │
│  - Memory (PluresLm)                                            │
│  - Cerebellum routing                                           │
│  - Chronos audit                                                │
│  - Praxis constraints                                           │
└──────────────────────────────────────────────────────────────────┘
```

### MCP Changes Required
1. Add an `agent_ask` tool to MCP that invokes the full agent loop:
   ```json
   {
     "name": "agent_ask",
     "description": "Send a prompt through the full agent loop (model + tools + memory)",
     "inputSchema": {
       "properties": {
         "prompt": { "type": "string" },
         "channel": { "type": "string", "default": "mcp" },
         "session": { "type": "string", "description": "Session ID for conversation continuity" }
       },
       "required": ["prompt"]
     }
   }
   ```
2. The MCP `agent_ask` tool shares the same `Agent` instance as `serve` and `tui`
3. Conversation history keyed by session ID, not by channel name
4. All tools available to the agent regardless of entry point

### `ask` CLI Changes Required
1. Wire full tool dispatcher into `ask` (currently model-only)
2. Support `--session` flag for conversation continuity
3. Same agent core as serve/tui/mcp

## Consequences
- **Positive**: Any MCP client (VS Code, Cursor, another agent) can use pares-radix as a full agent, not just a tool bag
- **Positive**: Testing becomes channel-independent — test the agent core once, works everywhere
- **Positive**: The DevBox migration works through MCP without Telegram
- **Negative**: MCP server startup becomes heavier (needs model client, memory, etc.)
- **Mitigation**: Lazy initialization — model client connects on first `agent_ask`, not at startup
