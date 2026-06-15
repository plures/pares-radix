# Design: Conversation Threading — Channel-Agnostic Core

> **Status:** Draft v2 (revised per kbristol feedback)  
> **Date:** 2026-06-15  
> **Author:** mswork  
> **Scope:** `crates/core` (thread engine) + all channel adapters  
> **Builds on:** `praxis/procedures/topic-routing.px`, `spine/topic_routing_actions.rs`

---

## Problem Statement

pares-radix has **topic classification** (`topic-routing.px`) that detects when a user switches subjects, but it only steers model context — it doesn't actually **isolate** conversations into threads. All messages still land in one flat `chat:{id}:history` bucket in the `ConversationStore`.

Telegram's native "Forum Topics" is the inspiration, but it's clunky (admin-only, supergroup-only, all-or-nothing). We want something better that works across **all** channels.

## Design Principles

1. **Core is channel-agnostic** — the thread engine lives in `crates/core`, knows nothing about Telegram/Discord/TUI
2. **Channels express threads natively** — each adapter uses whatever UX is natural for its platform
3. **Existing `topic-routing.px` becomes the classifier** — it already detects topic shifts; now those shifts trigger thread operations instead of just steering
4. **PluresDB is the store** (C-PLURES-003) — threads, history, metadata all in PluresDB
5. **Spine events are the contract** — thread operations flow through the existing event pipeline

---

## Architecture

### Layer 1: Thread Engine (Core — `crates/core/src/threading/`)

The pure logic layer. No channel knowledge.

```rust
// crates/core/src/threading/mod.rs

/// A logical conversation thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Unique thread ID (UUID).
    pub id: String,
    /// Chat this thread belongs to.
    pub chat_id: String,
    /// Human-readable topic label.
    pub topic: String,
    /// Thread lifecycle state.
    pub state: ThreadState,
    /// When the thread was created (unix secs).
    pub created_at: u64,
    /// Last message timestamp (unix secs).
    pub last_active_at: u64,
    /// Message count (for display without loading full history).
    pub message_count: usize,
    /// Auto-generated summary (updated periodically).
    pub summary: Option<String>,
    /// Channel-specific anchor data (opaque to core).
    /// E.g., Telegram message_id, Discord thread_id, TUI tab index.
    pub channel_anchor: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreadState {
    Active,
    Paused,
    Archived,
}

/// Thread routing decision — which thread does this message belong to?
#[derive(Debug, Clone)]
pub enum ThreadDecision {
    /// Route to an existing thread.
    Existing { thread_id: String },
    /// Create a new thread with this topic.
    New { topic: String },
    /// Continue in the current/default thread (no switch).
    Continue,
}

/// Configuration for the thread engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadConfig {
    /// Enable threading (can be disabled per-chat).
    pub enabled: bool,
    /// Auto-detect topic shifts and create threads.
    pub auto_detect: bool,
    /// Confidence threshold for auto-creating a new thread.
    pub auto_create_threshold: f64,
    /// Max active threads per chat before auto-archiving oldest.
    pub max_active: usize,
    /// Seconds of inactivity before auto-archiving.
    pub archive_after_secs: u64,
    /// Messages before generating a thread summary.
    pub summarize_after: usize,
}

impl Default for ThreadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_detect: true,
            auto_create_threshold: 0.75,
            max_active: 8,
            archive_after_secs: 48 * 3600, // 48 hours
            summarize_after: 5,
        }
    }
}
```

### Layer 2: Thread-Aware Conversation Store

Replaces the flat `ConversationStore` with thread-partitioned storage:

```rust
// crates/core/src/threading/store.rs

/// Thread-aware conversation store.
/// PluresDB keys:
///   thread:{chat_id}:{thread_id}:meta     → Thread struct
///   thread:{chat_id}:{thread_id}:history   → Vec<ChatMessage>
///   thread:{chat_id}:index                 → Vec<ThreadSummary> (active threads)
///   thread:{chat_id}:active                → current thread_id
///
/// Backward compat: existing chat:{chat_id}:history becomes the "default" thread.
#[async_trait]
pub trait ThreadStore: Send + Sync {
    /// Get or create the active thread for a chat.
    async fn active_thread(&self, chat_id: &str) -> Thread;

    /// Switch the active thread (returns previous active).
    async fn switch_thread(&self, chat_id: &str, thread_id: &str) -> Option<Thread>;

    /// Create a new thread and make it active.
    async fn create_thread(&self, chat_id: &str, topic: &str) -> Thread;

    /// Get history for a specific thread.
    async fn thread_history(&self, chat_id: &str, thread_id: &str) -> Vec<ChatMessage>;

    /// Add a message to the active thread.
    async fn add_message(&self, chat_id: &str, message: ChatMessage);

    /// List all threads for a chat (with filter by state).
    async fn list_threads(&self, chat_id: &str, include_archived: bool) -> Vec<Thread>;

    /// Archive a thread.
    async fn archive_thread(&self, chat_id: &str, thread_id: &str);

    /// Find thread by topic similarity (for auto-routing).
    async fn find_matching_thread(&self, chat_id: &str, topic: &str) -> Option<Thread>;
}
```

### Layer 3: Thread Router (Integrates with topic-routing.px)

The router sits in the spine pipeline between inbound and model_request:

```
Inbound → [ThreadRouter] → ModelRequest (with thread-specific history)
```

The existing `topic-routing.px` procedure's `classify_topic` output feeds into the thread router's decision:

```rust
// In spine pipeline, after topic classification:
match topic_decision {
    TopicDecision { changed: true, new_topic, confidence } 
        if confidence >= config.auto_create_threshold => {
        // Check: does a matching thread already exist?
        if let Some(existing) = store.find_matching_thread(chat_id, new_topic).await {
            store.switch_thread(chat_id, &existing.id).await;
            emit ThreadSwitched { from, to: existing.id }
        } else {
            let thread = store.create_thread(chat_id, new_topic).await;
            emit ThreadCreated { thread_id: thread.id, topic: new_topic }
        }
    }
    _ => {
        // Continue in current thread (steer_continuation path)
    }
}
```

### Layer 4: Channel Thread Adapters (per-channel UX)

Each channel adapter implements thread presentation differently:

```rust
// crates/core/src/threading/channel.rs

/// How a channel represents threads to users.
/// Channels implement this trait to provide native thread UX.
#[async_trait]
pub trait ChannelThreading: Send + Sync {
    /// Capabilities this channel supports.
    fn capabilities(&self) -> ThreadCapabilities;

    /// Present a thread creation to the user.
    async fn on_thread_created(&self, thread: &Thread, chat_id: &str) -> Result<ChannelAnchor, ThreadError>;

    /// Present a thread switch to the user.
    async fn on_thread_switched(&self, from: &Thread, to: &Thread, chat_id: &str) -> Result<(), ThreadError>;

    /// Deliver a message within thread context (may use reply-chains, thread indicators, etc.)
    async fn deliver_in_thread(&self, thread: &Thread, content: &str, chat_id: &str) -> Result<ChannelAnchor, ThreadError>;

    /// Resolve which thread an incoming message targets (from platform-native threading).
    /// E.g., Telegram reply-to-message → find which thread owns that message.
    async fn resolve_thread_from_message(&self, metadata: &serde_json::Value) -> Option<String>;

    /// Present thread list to user.
    async fn present_thread_list(&self, threads: &[Thread], chat_id: &str) -> Result<(), ThreadError>;
}

/// Channel-specific anchor data stored with each thread.
/// Opaque to core; channels write and read their own anchors.
pub type ChannelAnchor = serde_json::Value;

/// What threading features a channel supports natively.
#[derive(Debug, Clone)]
pub struct ThreadCapabilities {
    /// Can show thread indicators (topic labels, icons).
    pub indicators: bool,
    /// Can use platform reply-chains for visual threading.
    pub reply_chains: bool,
    /// Can create native platform threads (Discord threads, forum topics).
    pub native_threads: bool,
    /// Can render a thread switcher UI (tabs, sidebar, inline keyboard).
    pub thread_switcher: bool,
    /// Supports concurrent visible threads (like tabs).
    pub concurrent_display: bool,
}
```

---

## Channel-Specific Implementations

### Telegram (`TelegramThreading`)

| Capability | Implementation |
|---|---|
| **Indicators** | Prepend `[📎 topic]` to bot replies |
| **Reply chains** | Bot always uses `reply_to_message_id` pointing to thread anchor |
| **Native threads** | Forum topics in supergroups (optional — only if chat has forum mode) |
| **Thread switcher** | Inline keyboard on `/threads` command |
| **Concurrent display** | No (one active thread at a time; use reply-to to branch) |

**Routing from Telegram:** If user taps "Reply" on a bot message, resolve that message's thread → route there.

```rust
impl ChannelThreading for TelegramThreading {
    fn capabilities(&self) -> ThreadCapabilities {
        ThreadCapabilities {
            indicators: true,
            reply_chains: true,
            native_threads: self.is_forum_group, // only in forum supergroups
            thread_switcher: true,  // inline keyboard
            concurrent_display: false,
        }
    }

    async fn resolve_thread_from_message(&self, metadata: &Value) -> Option<String> {
        // If message has reply_to_message_id, look up which thread owns that message
        let reply_to = metadata.get("reply_to_message_id")?.as_i64()?;
        self.message_thread_index.get(&reply_to).cloned()
    }
}
```

### TUI (`TuiThreading`)

| Capability | Implementation |
|---|---|
| **Indicators** | Color-coded topic label in message gutter |
| **Reply chains** | N/A (not a chat protocol) |
| **Native threads** | Tabs — each thread is a tab in the TUI |
| **Thread switcher** | Tab bar at top, Ctrl+1-8 to switch, Ctrl+T for new |
| **Concurrent display** | Yes — split-pane mode shows two threads side by side |

```rust
impl ChannelThreading for TuiThreading {
    fn capabilities(&self) -> ThreadCapabilities {
        ThreadCapabilities {
            indicators: true,
            reply_chains: false,
            native_threads: true,  // tabs ARE threads
            thread_switcher: true, // tab bar
            concurrent_display: true, // split pane
        }
    }
}
```

### HTTP API (`HttpThreading`)

| Capability | Implementation |
|---|---|
| **Indicators** | Thread metadata in response JSON |
| **Reply chains** | N/A |
| **Native threads** | Pass `thread_id` in request JSON to target specific thread |
| **Thread switcher** | `GET /v1/threads` endpoint |
| **Concurrent display** | Inherent — API is stateless, caller picks thread per request |

Extend the HTTP API:
```
POST /v1/chat          — send to active thread (or specify thread_id)
GET  /v1/threads       — list threads for a session
POST /v1/threads       — create a new thread
PUT  /v1/threads/:id   — switch active thread
```

### Stdio (`StdioThreading`)

| Capability | Implementation |
|---|---|
| **Indicators** | `[topic] ` prefix on output lines |
| **Reply chains** | N/A |
| **Native threads** | Slash commands: `/thread new`, `/thread switch` |
| **Thread switcher** | `/threads` lists with numbers, user types number to switch |
| **Concurrent display** | No |

### Tauri Desktop (`TauriThreading`)

| Capability | Implementation |
|---|---|
| **Indicators** | Thread badge/pill in message header |
| **Reply chains** | Visual thread lines connecting related messages |
| **Native threads** | Sidebar with thread list, click to switch |
| **Thread switcher** | Sidebar panel (like Slack threads) |
| **Concurrent display** | Yes — pop-out thread windows |

---

## Spine Events (New)

```rust
// Added to SpineEvent enum:

/// A new thread was created.
ThreadCreated {
    id: String,
    chat_id: String,
    thread_id: String,
    topic: String,
    /// Channel anchor data (for the adapter to store/use).
    channel_anchor: serde_json::Value,
},

/// Active thread switched for a chat.
ThreadSwitched {
    id: String,
    chat_id: String,
    from_thread_id: String,
    to_thread_id: String,
},

/// A thread was archived (inactivity or user action).
ThreadArchived {
    id: String,
    chat_id: String,
    thread_id: String,
},
```

---

## Integration with Existing `topic-routing.px`

The existing `.px` procedure stays. We add a new procedure that consumes its output:

```praxis
# thread-management.px — reacts to topic_decision to manage threads

procedure route_to_thread(decision: object from "topic_decision") -> write into "thread_routed":
  given: "Route messages to the correct thread based on topic classification"

  get_field {object: $decision, field: "changed"} -> $changed
  get_field {object: $decision, field: "new_topic"} -> $new_topic
  get_field {object: $decision, field: "confidence"} -> $confidence

  # Get threading config
  read_state {key: "thread:config"} -> $config
  get_field {object: $config, field: "auto_create_threshold", default: 0.75} -> $threshold

  branch $changed:
    true:
      # Check confidence meets threshold for thread creation
      evaluate_topic_confidence {confidence: $confidence, threshold: $threshold} -> $eval
      get_field {object: $eval, field: "above_threshold"} -> $meets_threshold

      branch $meets_threshold:
        true:
          # Find existing thread or create new
          find_or_create_thread {
            chat_id: $decision.chat_id,
            topic: $new_topic
          } -> $thread_result
          return $thread_result
        false:
          # Not confident enough — mark for reeval, stay in current thread
          return {action: "continue", reason: "low_confidence"}

    false:
      return {action: "continue", reason: "same_topic"}
```

---

## Migration Path

1. **Backward compat:** Existing `chat:{id}:history` data becomes the "default" thread when threading is first enabled for a chat
2. **Gradual rollout:** `ThreadConfig.enabled` defaults to `true` but `auto_detect` can be off per-chat
3. **No breaking changes to adapters:** `ConversationStore` trait gets a compat wrapper that delegates to `ThreadStore` using the active thread

---

## Implementation Phases

### Phase 1: Core Thread Engine
- [ ] `crates/core/src/threading/mod.rs` — Thread, ThreadState, ThreadConfig structs
- [ ] `crates/core/src/threading/store.rs` — PluresDB-backed ThreadStore
- [ ] `crates/core/src/threading/router.rs` — ThreadRouter (decision logic)
- [ ] `crates/core/src/threading/channel.rs` — ChannelThreading trait + ThreadCapabilities
- [ ] New SpineEvents: ThreadCreated, ThreadSwitched, ThreadArchived
- [ ] `praxis/procedures/thread-management.px` — orchestration procedure
- [ ] Backward-compat wrapper for existing ConversationStore callers

### Phase 2: Channel Adapters
- [ ] `TelegramThreading` — reply chains + inline keyboard thread switcher + indicators
- [ ] `TuiThreading` — tab bar + split pane + Ctrl+T keybinds
- [ ] `HttpThreading` — thread_id in API + /v1/threads endpoints
- [ ] `StdioThreading` — slash commands + prefix indicators
- [ ] `TauriThreading` — IPC events for sidebar thread list

### Phase 3: Intelligence Integration
- [ ] Wire `topic-routing.px` output → `thread-management.px` → ThreadRouter
- [ ] Thread-specific history passed to model (only active thread's messages in context)
- [ ] Auto-archive timer (via spine Timer events)
- [ ] Auto-summarize on archive (model call)

### Phase 4: Advanced
- [ ] Thread search (semantic search across all thread histories)
- [ ] Thread merge/split operations
- [ ] Cross-thread context injection ("what did we say about X in the other thread?")
- [ ] Per-thread personality/tone overrides
- [ ] Thread-aware group context (in groups, each participant's active thread)

---

## Non-Goals

- Not implementing Telegram Forum Topics API (that's a Telegram platform feature, not ours)
- Not requiring any platform-specific settings changes (works with default chat settings everywhere)
- Not replacing the session system (threads are below sessions in the hierarchy)
- Not coupling any channel's native thread mechanism to our threading model
