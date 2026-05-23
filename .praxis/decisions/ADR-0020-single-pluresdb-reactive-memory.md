# ADR-0020: Single PluresDB — Unified State + Reactive Memory

**Status:** Accepted
**Date:** 2026-05-23
**Context:** Pares-radix migration architecture decision

## Decision

PluresDB is the ONE database. There is no separate memory system, no standalone embedding service, no PluresLM-as-external-dep.

## Architecture

```
Everything writes to PluresDB:
  - Conversations (inbound + outbound messages)
  - Model responses (full content + metadata)
  - Tool calls + results
  - System events (heartbeats, errors, state changes)

Memory = .px procedures that react to PluresDB writes:
  - on_conversation_complete → extract entities, decisions, facts
  - on_tool_result → index learnings, error patterns
  - on_session_end → summarize, compress, tag
  - periodic → consolidate, deduplicate, decay stale
```

## Principles

1. **One database.** No HashMap caches, no in-memory stores, no separate SQLite, no PluresLM running as a service. PluresDB with its CRDT store, persistence, and reactive procedures.

2. **Everything is a write.** Conversations, tool results, model responses — they all land in PluresDB as nodes. This creates the raw data layer.

3. **Memory is post-processing.** .px procedures trigger on activity and transform raw data into optimized retrieval structures. Embeddings, summaries, entity graphs, relevance scores — all computed reactively and stored back into PluresDB.

4. **Retrieval is a query.** `memory_search` doesn't call an external service — it queries PluresDB nodes that were pre-indexed by the memory procedures.

5. **.px is the logic layer.** Memory organization logic lives in .px files, not Rust code. The Rust code is only the side-effect boundary (compute embeddings, call model for summarization).

## Data Flow

```
User message arrives
  → write to PluresDB: conversation:{chat_id}:{msg_id}
  → spine pipeline processes (model → tools → response)
  → write to PluresDB: response:{chat_id}:{msg_id}
  → .px procedure fires: on_conversation_write
    → extract entities → write to PluresDB: entity:{name}
    → compute embedding → write to PluresDB: embedding:{node_id}
    → update relevance scores → write to PluresDB: relevance:{topic}
```

## What This Replaces

- PluresLM as a separate MCP service → embeddings computed inline, stored in PluresDB
- MEMORY.md as static file → PluresDB nodes with semantic index
- memory/*.md daily files → PluresDB conversation nodes with date indexes
- Separate memory_search/memory_store tools → PluresDB queries + .px procedures

## Embedding Strategy

- Use fastembed (BAAI/bge-small-en-v1.5, 384-dim) directly in-process
- Embeddings stored as PluresDB node metadata
- Similarity search via brute-force cosine over indexed nodes (fast enough for <100K entries)
- .px procedure decides WHAT gets embedded (not everything — only extracted facts/decisions/entities)

## Constraints

- C-PLURES-003 applies absolutely: ALL state through PluresDB
- C-PLURES-004 applies: pure logic in .px, IO at the boundary
- No standalone memory services
- No in-memory caches for state that should survive restarts
- The reactive procedure system must be the ONLY path for memory organization
