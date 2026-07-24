# ADR-0036: Durable task custody and atomic claims

- Status: Accepted
- Date: 2026-07-21

## Context

`TaskManager` persisted whole task JSON documents but used unconditional read/modify/write. `Unassigned` and `Self` were relative labels, so two hosts could independently select and execute the same replicated task. Chat history is not durable custody evidence.

## Decision

Keep execution status and custody orthogonal. A one-to-one custody record carries owner, target, stable handoff id, monotonic generation, digest, worker, and opaque claim token. The storage boundary exposes conditional compare-and-swap; callers never emulate CAS with get followed by put.

Export prepares `transfer_pending` once and emits a canonical integrity envelope. Import validates schema, target, and digest before atomically accepting ownership. Duplicate identical operations are idempotent; stale generations, changed payloads, and competing workers conflict. Claimed blocked/completed/failed writes require the winning token. Owner-filtered discovery excludes transfer-pending, blocked, terminal, and foreign tasks.

The first production primitive uses sled's native `compare_and_swap`, which is durable and process-safe for the host-local store used by the explicit file transfer protocol. Eventual multi-writer CRDT replication is not certified as a distributed lock; enabling shared-store claiming requires a separately modeled authority/consensus design.

## Consequences

- Restart preserves custody and the winning claim.
- Explicit canonical JSON is channel-neutral and auditable.
- Transport is outside the correctness boundary; Telegram/chat is not used.
- Agens exposes only a thin machine-readable interface over the radix service.
- CAS conflicts are typed errors, never silent no-ops.
