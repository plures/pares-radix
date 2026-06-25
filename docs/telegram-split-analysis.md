# Telegram Adapter: Transport/Intelligence Split Analysis

> **Generated:** 2026-06-14  
> **Source:** `crates/channels/src/telegram.rs` (~2875 lines, 140KB)  
> **Target architecture:** Spine pattern (see `telegram_spine.rs`, `http_spine.rs`)  
> **Purpose:** Gate the refactor that splits this monolithic adapter into a thin transport layer + .px intelligence procedures.

---

## Table of Contents

1. [Current Structure Inventory](#1-current-structure-inventory)
2. [Classification](#2-classification)
3. [MIXED Items — Extraction Details](#3-mixed-items--extraction-details)
4. [Proposed .px Procedures](#4-proposed-px-procedures)
5. [Proposed Action Handler Methods](#5-proposed-action-handler-methods)
6. [Migration Risk Assessment](#6-migration-risk-assessment)
7. [Dependency Map](#7-dependency-map)
8. [Reference: telegram_spine.rs (Target Pattern)](#8-reference-telegram_spiners-target-pattern)

---

## 1. Current Structure Inventory

### Module-Level Free Functions

| Line | Name | Signature | Description |
|------|------|-----------|-------------|
| 129 | `parse_modulus_index` | `(payload: &str) → Result<Vec<SkillMetadata>, String>` | Parses JSON marketplace index into SkillMetadata vec |
| 153 | `metadata_from_index_entry` | `(entry: &Value) → Option<SkillMetadata>` | Converts a single JSON value to SkillMetadata |
| 219 | `is_valid_sha256_hex` | `(value: &str) → bool` | Validates a 64-char hex checksum |
| 223 | `fetch_marketplace_index` | `async (index_url: &str) → Result<Vec<SkillMetadata>, String>` | HTTP fetch + parse of remote marketplace index |
| 240 | `format_index_listing` | `(skills: &[SkillMetadata]) → String` | Formats skills for human-readable Telegram display |
| 265 | `find_skill_by_id` | `(skills: &[SkillMetadata], id: &str) → Option<SkillMetadata>` | Case-insensitive skill lookup |
| 272 | `shell_single_quote` | `(value: &str) → String` | Shell-safe quoting for NixOS commands |
| 276 | `build_nixos_update_command` | `(_flake_dir: &str, _host: &str) → String` | Constructs the full nixos-rebuild shell command |
| 297 | `truncate_telegram_message` | `(content: String) → String` | Truncates to TELEGRAM_MAX_MESSAGE_CHARS with "…(truncated)" |
| 312 | `parse_logs_tail_lines` | `(args: Vec<&str>) → Result<usize, &'static str>` | Parses /logs argument (line count) with validation |
| 333 | `format_service_logs_output` | `(output: &Output) → String` | Formats journalctl output for Telegram display |
| 356 | `format_update_command_output` | `(output: &Output) → String` | Formats nixos-rebuild output for display |
| 374 | `telegram_help_text` | `() → String` | Generates /help response from TELEGRAM_HELP_COMMANDS const |
| 386 | `current_process_rss_kib` | `() → Option<u64>` | Reads /proc/self/status for memory usage |
| 405 | `is_update_authorized` | `(msg: &Message) → bool` | Checks env var allowlist for user authorization |

### Constants (Lines 45–127)

| Name | Value | Purpose |
|------|-------|---------|
| `PARES_MODULUS_INDEX_URL` | GitHub raw URL | Default marketplace index location |
| `DEFAULT_MARKETPLACE_INSTALL_DIR` | "/skills" | Where to install marketplace skills |
| `MAX_INDEX_LISTING_ITEMS` | 10 | Max items in /marketplace list |
| `DEFAULT_NIX_FLAKE_DIR` | "nixos-config" | Default NixOS flake directory |
| `DEFAULT_NIX_HOST` | "praxisbot" | Default NixOS rebuild hostname |
| `TELEGRAM_MAX_MESSAGE_CHARS` | 3900 | Message truncation threshold |
| `TELEGRAM_VERBOSE_TOOL_DETAILS_MARKER` | `"__PARES_VERBOSE_TOOL_DETAILS__:"` | Internal verbose prefix (public, used by CLI) |
| `TELEGRAM_HELP_COMMANDS` | 32-entry array | All slash command descriptions |
| `DEFAULT_LOG_TAIL_LINES` | 80 | Default journalctl tail count |
| `MAX_LOG_TAIL_LINES` | 400 | Maximum journalctl tail count |

### Structs & Enums

| Line | Name | Description |
|------|------|-------------|
| 434 | `TelegramConfig` | Builder-pattern config holding token + all optional control surfaces |
| 621 | `TelegramRuntimeConfig` | DTO for current runtime config (model, endpoint, log_level) |
| 635 | `TelegramAdapter` | Main adapter struct (config + optional EventSpineHandle) |
| 646 | `ModelCommand` | Enum: Show / SetPrimary(String) / SetDeep(String) |
| 653 | `ConfigCommand` | Enum: Show / SetModel / SetEndpoint / SetLogLevel |

### Traits (Intelligence Control Surfaces)

| Line | Name | Methods | Purpose |
|------|------|---------|---------|
| 567 | `TelegramModelControl` | `current_models`, `set_primary_model`, `set_deep_model`, `deep_escalation_enabled`, `set_deep_escalation_enabled` | Runtime model switching |
| 582 | `TelegramRuntimeControl` | `reset_runtime` | Full runtime reset |
| 589 | `TelegramConfigControl` | `current_config`, `set_model`, `set_endpoint`, `set_log_level` | Runtime config editing |
| 602 | `TelegramPersonalityControl` | `show`, `set_tone`, `add_rule`, `remove_rule`, `list_documents`, `get_document`, `set_document` | Personality layer management |

### impl TelegramAdapter (Lines 660–1196)

| Line | Method | Description |
|------|--------|-------------|
| 662 | `new` | Constructor |
| 671 | `with_event_spine` | Constructor with EventSpineHandle |
| 679 | `parse_model_command` | Parse /model args → ModelCommand |
| 693 | `parse_verbose_command` | Parse /verbose args → bool |
| 705 | `parse_reasoning_command` | Parse /reasoning args → bool |
| 717 | `parse_config_command` | Parse /config args → ConfigCommand |
| 742 | `message_to_event` | Convert teloxide Message → Event (handles text, photos, documents) |
| 814 | `escape_markdown_v2` | Escape special chars for TG MarkdownV2 |
| 832 | `build_inline_keyboard` | Build InlineKeyboardMarkup from button specs |
| 840 | `is_approval_prompt` | Detect approval language in content |
| 847 | `approval_keyboard` | Build yes/no keyboard for approval gates |
| 860 | `chunk_message` | Split text at line boundaries respecting max_len |
| 892 | `send_markdown_reply` | Send message with MarkdownV2 parsing |
| 930 | `edit_placeholder_with_response` | Edit placeholder message with final HTML-rendered response |
| 1017 | `send_html_reply` | Send multi-chunk HTML reply with fallback |
| 1100 | `send_reply_with_fallback` | Try HTML, fall back to plain text |
| 1122 | `acknowledge_message` | React with 👍 emoji to indicate processing complete |
| 1135 | `is_group_chat` | Detect group/supergroup chat type |
| 1144 | `should_respond_in_group` | Group gate logic: mentions, replies, keywords |
| 1182 | `react_contextually` | Future: contextual emoji reactions |

### impl ChannelAdapter for TelegramAdapter (Lines 1198–2414)

This is the **monolithic `run()` method** — a single ~1200-line async function containing:

| Line Range | Block | Description |
|------------|-------|-------------|
| 1203–1240 | Setup | Extract config fields, create Bot, validate token, get bot_username |
| 1240–1280 | Infrastructure | Initialize verbose_by_chat HashMap, group_context buffer, stream_tx |
| 1280–1340 | Callback query handler | Handle approval button clicks (gate:approve/reject) |
| 1340–1395 | /start, /help | Help text generation and send |
| 1395–1420 | /status, /health | Version + RSS memory + health report |
| 1420–1435 | /clear | Session clear (emit Event::SessionClear) |
| 1435–1460 | /resume, /sessions | Session management |
| 1460–1470 | /version | Version info |
| 1470–1485 | /verbose | Toggle verbose mode |
| 1485–1540 | /reasoning | Toggle deep model escalation |
| 1540–1600 | /model | Model switching (show/set primary/set deep) |
| 1600–1610 | /reset | Runtime reset |
| 1610–1680 | /config | Config management (model/endpoint/log-level) |
| 1680–1710 | /marketplace, /install | Marketplace browsing and skill installation |
| 1710–1720 | /tools | Show tool governance policies |
| 1720–1780 | /logs | journalctl tail with auth check |
| 1780–1880 | /update | NixOS self-update with sudo preflight |
| 1880–1970 | /personality | Personality CRUD (tone, rules, documents) |
| 1970–2070 | /cron | Scheduler CRUD (list/add/remove/pause/resume) |
| 2070–2170 | /plugin | Plugin management (list/install/uninstall/schema) |
| 2170–2230 | /tasks, /task | Task manager display and lifecycle |
| 2230–2310 | /praxis | Write gate inspection (constraints/log/violations) |
| 2310–2380 | /cluster | Cluster status/nodes/info/deploy/workloads |
| 2380–2395 | Unknown command | "Unknown: /{cmd}" response |
| 2395–2414 | Normal message flow | Group gate → placeholder → typing → progressive streaming → model call → response delivery |

### Tests (Lines 2415–2875)

27 unit tests covering: escape_markdown_v2, build_inline_keyboard, approval detection, model/config/verbose/reasoning command parsing, logs parsing, TelegramConfig basics, marketplace index parsing, NixOS command building, message truncation, HTML rendering integration.

---

## 2. Classification

### TRANSPORT (Stays in pares-radix channels crate)

| Item | Reason |
|------|--------|
| `TelegramConfig` struct | Bot connection configuration |
| `TelegramAdapter` struct (thinned) | Transport adapter shell |
| `message_to_event` | Message → Event conversion (pure transport) |
| `escape_markdown_v2` | Platform formatting |
| `build_inline_keyboard` | Platform UI primitive |
| `chunk_message` | Message size compliance |
| `send_markdown_reply` | Platform send method |
| `send_html_reply` | Platform send method (multi-chunk) |
| `send_reply_with_fallback` | Platform send with degradation |
| `edit_placeholder_with_response` | Progressive delivery mechanism |
| `acknowledge_message` | Platform reaction primitive |
| `is_group_chat` | Platform chat type detection |
| `truncate_telegram_message` | Platform limit enforcement |
| `approval_keyboard` / `is_approval_prompt` | Transport-level UI for approval gates |
| Token validation / Bot::get_me | Connection lifecycle |
| Polling loop / Update dispatch | Transport receive loop |
| Progressive streaming (placeholder + edit loop) | Transport delivery pattern |
| Typing indicator | Transport UX signal |
| Callback query handler (routing) | Transport event routing |
| `TELEGRAM_MAX_MESSAGE_CHARS` | Platform constant |

### INTELLIGENCE (Moves to .px procedures / pares-agens)

| Item | Reason |
|------|--------|
| `/model` command logic | Runtime model selection is agent intelligence |
| `/reasoning` command logic | Escalation strategy is agent decision |
| `/config` command logic | Runtime config is agent self-management |
| `/reset` command logic | Agent lifecycle management |
| `/personality` command logic | Personality layer is pure intelligence |
| `/cron` command logic | Scheduling is agent capability |
| `/plugin` command logic | Plugin management is agent extensibility |
| `/tasks`, `/task` command logic | Task management is agent work tracking |
| `/praxis` command logic | Write gate inspection is agent governance |
| `/cluster` command logic | Cluster orchestration is agent infrastructure |
| `/update` command logic | Self-update is agent maintenance |
| `/logs` command logic | System inspection is agent ops |
| `/marketplace`, `/install` command logic | Skill acquisition is agent intelligence |
| `/tools` command logic | Tool governance display is agent meta |
| `should_respond_in_group` (heuristics) | Group participation decision is intelligence |
| Group context injection logic | Context framing is intelligence |
| Verbose marker injection | Output detail level is intelligence decision |
| `TelegramModelControl` trait | Control surface for intelligence |
| `TelegramRuntimeControl` trait | Control surface for intelligence |
| `TelegramConfigControl` trait | Control surface for intelligence |
| `TelegramPersonalityControl` trait | Control surface for intelligence |
| `TelegramRuntimeConfig` struct | Intelligence state DTO |
| `parse_modulus_index` / `fetch_marketplace_index` / `format_index_listing` / `find_skill_by_id` | Marketplace logic |
| `build_nixos_update_command` / `shell_single_quote` | NixOS ops logic |
| `parse_logs_tail_lines` / `format_service_logs_output` / `format_update_command_output` | Ops formatting |
| `current_process_rss_kib` | System introspection |
| `is_update_authorized` | Authorization policy |
| `telegram_help_text` | Help content (intelligence decides what to show) |
| `TELEGRAM_HELP_COMMANDS` const | Intelligence command registry |
| All `parse_*_command` methods | Command interpretation |
| ModelCommand / ConfigCommand enums | Intelligence DTOs |

### MIXED (Needs Splitting)

| Item | Transport Part | Intelligence Part |
|------|---------------|-------------------|
| `run()` method (~1200 lines) | Polling, dispatch, send/receive, progressive streaming, placeholder lifecycle | All slash command handling, group gate decision, verbose injection, model dispatch routing |
| `/status` command | Sending formatted response | Deciding what to include (version, RSS, capabilities) |
| `/start`, `/help` | Sending the message | Deciding help content |
| `/clear`, `/resume`, `/sessions` | Sending confirmation | Session management decisions |
| `/verbose` | Sending toggle confirmation | Toggling state (per-chat intelligence) |
| Callback query handler | Routing button press to handler | Deciding what approval means (approve/reject gate logic) |
| Group chat gate | Detecting group type | `should_respond_in_group` heuristic + context buffer injection |
| Normal message path | Placeholder → typing → stream → edit/send | Event decoration (verbose marker), group context injection |

---

## 3. MIXED Items — Extraction Details

### 3.1 The `run()` Method (Critical — 1200+ lines)

**What stays (Transport Shell):**
```rust
// Thin run() does ONLY:
// 1. Create Bot, validate token
// 2. Set up polling handler
// 3. For each update:
//    a. Convert to SpineEvent::Inbound
//    b. Emit to pipeline
// 4. Subscribe to DeliveryRequest events
// 5. For each delivery:
//    a. Render content (HTML/fallback)
//    b. Handle chunking
//    c. Send via Bot API
//    d. Progressive streaming edits
```

**What extracts (Intelligence):**
- All 20+ slash command match arms → become .px procedures triggered by command patterns
- Group chat gate → becomes a .px procedure evaluating group policy
- Verbose state management → becomes pipeline metadata
- Model call dispatch → already in pipeline (spine handles this)
- Approval gate callback logic → becomes a .px procedure for gate resolution

### 3.2 `/status` Command

**Stays:** `send_reply_with_fallback(&bot, &msg, &reply, ...)`  
**Extracts:** Building the status string (version, RSS, uptime, capabilities). Becomes a `status.px` procedure that calls `action::system_status()`.

### 3.3 Group Chat Gate

**Stays:** `is_group_chat(msg)` detection (it's a platform fact)  
**Extracts:** 
- `should_respond_in_group` heuristic logic (mention detection, reply detection, keyword matching) → `group-gate.px`
- Context buffer injection formatting → `group-context.px`
- The passive_observe recording stays in transport (it's just buffering messages)

### 3.4 Callback Query Handler

**Stays:** Parsing callback data string, routing to handler, answering callback query  
**Extracts:** The approval gate logic (what "approve" and "reject" mean, how they affect the pipeline state) → `approval-gate.px`

### 3.5 `/verbose` Toggle

**Stays:** Per-chat state is a transport concern (it affects how messages are decorated before send)  
**Extracts:** The decision of what "verbose" means for output content → intelligence decides output detail level. However, the verbose_by_chat HashMap is a reasonable transport-level flag that the pipeline can check.

**Resolution:** Keep verbose_by_chat in transport as a simple metadata flag. The marker injection (`TELEGRAM_VERBOSE_TOOL_DETAILS_MARKER`) stays as transport metadata decoration. Intelligence procedures never see this — the pipeline attaches it.

---

## 4. Proposed .px Procedures

### 4.1 Command Router (`telegram-commands.px`)

```
trigger: spine.inbound where content starts_with "/"
priority: 100

match content:
  "/model *"       → invoke command-model.px
  "/reasoning *"   → invoke command-reasoning.px
  "/config *"      → invoke command-config.px
  "/reset"         → invoke command-reset.px
  "/personality *" → invoke command-personality.px
  "/cron *"        → invoke command-cron.px
  "/plugin *"      → invoke command-plugin.px
  "/tasks *"       → invoke command-tasks.px
  "/task *"        → invoke command-task.px
  "/praxis *"      → invoke command-praxis.px
  "/cluster *"     → invoke command-cluster.px
  "/update"        → invoke command-update.px
  "/logs *"        → invoke command-logs.px
  "/marketplace *" → invoke command-marketplace.px
  "/install *"     → invoke command-install.px
  "/tools"         → invoke command-tools.px
  "/status"        → invoke command-status.px
  "/health"        → invoke command-status.px
  "/verbose *"     → invoke command-verbose.px
  "/start"         → invoke command-help.px
  "/help"          → invoke command-help.px
  "/clear"         → invoke command-clear.px
  "/resume *"      → invoke command-session.px
  "/sessions"      → invoke command-session.px
  "/version"       → invoke command-version.px
  _               → emit delivery "Unknown command. Type /help for available commands."
```

### 4.2 Core Intelligence Procedures

| File | Trigger | Actions |
|------|---------|---------|
| `command-model.px` | `/model` inbound | Call `action::model_show`, `action::model_set_primary`, `action::model_set_deep` |
| `command-reasoning.px` | `/reasoning` inbound | Call `action::reasoning_toggle` |
| `command-config.px` | `/config` inbound | Call `action::config_show`, `action::config_set_*` |
| `command-reset.px` | `/reset` inbound | Call `action::runtime_reset` |
| `command-personality.px` | `/personality` inbound | Call `action::personality_*` (show/set_tone/add_rule/remove_rule/docs/doc) |
| `command-cron.px` | `/cron` inbound | Call `action::scheduler_*` (list/add/remove/pause/resume) |
| `command-plugin.px` | `/plugin` inbound | Call `action::plugin_*` (list/install/uninstall/schema) |
| `command-tasks.px` | `/tasks` inbound | Call `action::tasks_list` |
| `command-task.px` | `/task <id>` inbound | Call `action::task_show`, `action::task_complete`, `action::task_cancel` |
| `command-praxis.px` | `/praxis` inbound | Call `action::praxis_constraints`, `action::praxis_log`, `action::praxis_violations` |
| `command-cluster.px` | `/cluster` inbound | Call `action::cluster_*` (status/nodes/info/deploy/workloads) |
| `command-update.px` | `/update` inbound | Call `action::nixos_update` (with auth check) |
| `command-logs.px` | `/logs` inbound | Call `action::service_logs` (with auth check) |
| `command-marketplace.px` | `/marketplace` inbound | Call `action::marketplace_list` |
| `command-install.px` | `/install <id>` inbound | Call `action::marketplace_install` |
| `command-tools.px` | `/tools` inbound | Call `action::tool_policies` |
| `command-status.px` | `/status` inbound | Call `action::system_status` |
| `command-help.px` | `/help` inbound | Call `action::help_text` |
| `command-clear.px` | `/clear` inbound | Call `action::session_clear` |
| `command-session.px` | `/resume`, `/sessions` | Call `action::session_resume`, `action::session_list` |
| `command-version.px` | `/version` inbound | Call `action::version_info` |
| `command-verbose.px` | `/verbose` inbound | Call `action::verbose_toggle` |

### 4.3 Group Gate Procedure (`group-gate.px`)

```
trigger: spine.inbound where chat.type in ["group", "supergroup"]
priority: 50

conditions:
  - policy.enabled = true

evaluate:
  - mentioned = content contains "@{bot_username}"
  - replied = message.reply_to.from.is_bot AND message.reply_to.from.username == bot_username
  - keyword_match = policy.keywords.any(k => content.contains(k))

decision:
  if mentioned OR replied OR keyword_match:
    inject_context(group_buffer.recent(chat_id))
    continue_pipeline
  else:
    suppress (do not forward to model)
```

### 4.4 Approval Gate Procedure (`approval-gate.px`)

```
trigger: spine.callback_query where data starts_with "approval:" OR "gate:"
priority: 90

parse:
  action = data.split(":")[1]  // "yes", "no", "approve", "reject"
  request_id = data.split(":")[2]

execute:
  if action in ["yes", "approve"]:
    action::gate_approve(request_id)
    emit delivery "✅ Approved."
  else:
    action::gate_reject(request_id)
    emit delivery "❌ Rejected."
```

---

## 5. Proposed Action Handler Methods

These are the `AsyncActionHandler` trait methods that .px procedures will call. They represent the IO boundary — procedures declare intent, handlers execute side effects.

### System & Runtime

```rust
trait TelegramActionHandler: AsyncActionHandler {
    // System info
    async fn system_status(&self) -> ActionResult<String>;
    async fn version_info(&self) -> ActionResult<String>;
    async fn help_text(&self, channel: &str) -> ActionResult<String>;
    
    // Runtime control
    async fn runtime_reset(&self) -> ActionResult<()>;
    async fn session_clear(&self, chat_id: &str) -> ActionResult<()>;
    async fn session_list(&self) -> ActionResult<Vec<SessionInfo>>;
    async fn session_resume(&self, session_id: &str) -> ActionResult<()>;
}
```

### Model & Config

```rust
trait ModelActionHandler: AsyncActionHandler {
    async fn model_show(&self) -> ActionResult<(String, String)>;
    async fn model_set_primary(&self, model: &str) -> ActionResult<()>;
    async fn model_set_deep(&self, model: &str) -> ActionResult<()>;
    async fn reasoning_toggle(&self, enabled: Option<bool>) -> ActionResult<bool>;
    
    async fn config_show(&self) -> ActionResult<RuntimeConfig>;
    async fn config_set_model(&self, model: &str) -> ActionResult<()>;
    async fn config_set_endpoint(&self, endpoint: &str) -> ActionResult<()>;
    async fn config_set_log_level(&self, level: &str) -> ActionResult<()>;
}
```

### Personality

```rust
trait PersonalityActionHandler: AsyncActionHandler {
    async fn personality_show(&self, channel: Option<&str>) -> ActionResult<String>;
    async fn personality_set_tone(&self, tone: &str) -> ActionResult<()>;
    async fn personality_add_rule(&self, rule: &str) -> ActionResult<String>;
    async fn personality_remove_rule(&self, id: &str) -> ActionResult<()>;
    async fn personality_list_documents(&self) -> ActionResult<String>;
    async fn personality_get_document(&self, doc_type: &str) -> ActionResult<String>;
    async fn personality_set_document(&self, doc_type: &str, content: &str) -> ActionResult<()>;
}
```

### Scheduling & Tasks

```rust
trait SchedulerActionHandler: AsyncActionHandler {
    async fn scheduler_list(&self) -> ActionResult<Vec<ScheduledTask>>;
    async fn scheduler_add(&self, raw_text: &str) -> ActionResult<String>;
    async fn scheduler_remove(&self, id: &str) -> ActionResult<bool>;
    async fn scheduler_pause(&self, id: &str) -> ActionResult<bool>;
    async fn scheduler_resume(&self, id: &str) -> ActionResult<bool>;
}

trait TaskActionHandler: AsyncActionHandler {
    async fn tasks_list(&self, chat_id: &str, include_all: bool) -> ActionResult<String>;
    async fn task_show(&self, id_prefix: &str, chat_id: &str) -> ActionResult<String>;
    async fn task_complete(&self, id_prefix: &str, chat_id: &str) -> ActionResult<String>;
    async fn task_cancel(&self, id_prefix: &str, chat_id: &str) -> ActionResult<String>;
}
```

### Plugins & Marketplace

```rust
trait PluginActionHandler: AsyncActionHandler {
    async fn plugin_list(&self) -> ActionResult<String>;
    async fn plugin_install(&self, path: &str) -> ActionResult<String>;
    async fn plugin_uninstall(&self, name: &str) -> ActionResult<String>;
    async fn plugin_schema(&self, name: &str) -> ActionResult<String>;
    
    async fn marketplace_list(&self, index_url: &str) -> ActionResult<String>;
    async fn marketplace_install(&self, id: &str, index_url: &str) -> ActionResult<String>;
}
```

### Ops & Infrastructure

```rust
trait OpsActionHandler: AsyncActionHandler {
    async fn service_logs(&self, tail_lines: usize, authorized: bool) -> ActionResult<String>;
    async fn nixos_update(&self, flake_dir: &str, host: &str, authorized: bool) -> ActionResult<String>;
    async fn tool_policies(&self) -> ActionResult<String>;
}

trait ClusterActionHandler: AsyncActionHandler {
    async fn cluster_status(&self) -> ActionResult<String>;
    async fn cluster_nodes(&self) -> ActionResult<String>;
    async fn cluster_info(&self) -> ActionResult<String>;
    async fn cluster_deploy(&self, px_path: &str) -> ActionResult<String>;
    async fn cluster_workloads(&self) -> ActionResult<String>;
}
```

### Governance

```rust
trait GovernanceActionHandler: AsyncActionHandler {
    async fn praxis_constraints(&self) -> ActionResult<String>;
    async fn praxis_log(&self, n: usize) -> ActionResult<String>;
    async fn praxis_violations(&self, n: usize) -> ActionResult<String>;
    async fn gate_approve(&self, request_id: &str) -> ActionResult<()>;
    async fn gate_reject(&self, request_id: &str) -> ActionResult<()>;
    async fn verbose_toggle(&self, chat_id: &str, enabled: Option<bool>) -> ActionResult<bool>;
}
```

---

## 6. Migration Risk Assessment

### HIGH Risk

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Progressive streaming breaks** | Users see stale "⏳" that never updates | Keep streaming entirely in transport; spine procedures just emit content, transport handles progressive edit |
| **Group gate latency** | Adding a .px evaluation step before model call may add 5-50ms latency | Pre-evaluate gate synchronously in transport if policy is simple; only delegate complex heuristics to .px |
| **Approval callback race** | If callback query arrives before procedure is registered, it's dropped | Transport must buffer callback queries and replay them to procedures |
| **Session state split** | verbose_by_chat, group_context currently live in adapter memory | Move to pipeline-level state (PluresDB keyed by chat_id) |
| **20+ command regressions** | Any parsing bug in .px procedure silently breaks a command | Comprehensive integration tests for every command; run before and after migration |
| **Auth check bypass** | `is_update_authorized` is currently checked inline; if procedure forgets... | Make auth an action handler concern — handler checks auth, returns Unauthorized error |

### MEDIUM Risk

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Trait removal breaks CLI** | `main.rs` imports `TelegramModelControl` et al. | Phase: keep traits as-is but implement them on action handlers. Remove after full spine migration. |
| **Marketplace HTTP calls in procedure** | .px can't do HTTP; needs action handler | Implement `marketplace_fetch` as action handler method |
| **Test migration** | 27 tests test current struct methods directly | Tests for transport methods stay. Intelligence tests become integration tests against .px + action handlers |
| **Event format mismatch** | Current `Event::Message` vs `SpineEvent::Inbound` | Use SpineEvent exclusively in new transport; bridge if old adapter still alive |
| **Plugin install path validation** | Currently validated inline with `rt.get()` | Action handler must maintain same validation |

### LOW Risk

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Help text drift** | TELEGRAM_HELP_COMMANDS must match actual procedures | Generate help from procedure registry |
| **Emoji reaction removal** | `acknowledge_message` may be forgotten in transport | Make it automatic on every delivery |
| **Format fallback chain** | HTML→plain fallback is transport; no intelligence dependency | Already isolated in `send_reply_with_fallback` |

### Integration Testing Requirements

1. **Command parity test:** Script that sends every slash command via the HTTP spine channel and asserts the response matches the current adapter's response (golden file comparison).
2. **Progressive streaming test:** Verify placeholder → edit → final message lifecycle works identically.
3. **Group gate test:** Verify trigger/suppress behavior matches current `should_respond_in_group` logic.
4. **Approval flow test:** Send callback query with gate:approve/reject data, verify pipeline state changes.
5. **Auth boundary test:** Verify `/update` and `/logs` reject unauthorized users identically.
6. **Concurrent message test:** Verify multiple chats don't cross-contaminate state (verbose flags, group context).

---

## 7. Dependency Map

### Files that import from telegram.rs

| File | Imports | Impact of Split |
|------|---------|------------------|
| `crates/cli/src/main.rs` | `TelegramAdapter`, `TelegramConfig`, `TelegramConfigControl`, `TelegramModelControl`, `TelegramPersonalityControl`, `TelegramRuntimeConfig`, `TelegramRuntimeControl`, `TELEGRAM_VERBOSE_TOOL_DETAILS_MARKER` | **HIGH** — Main consumer of all traits. Must implement action handlers that satisfy these traits during transition. |
| `crates/channels/tests/e2e.rs` | `TelegramAdapter`, `TelegramConfig` | **MEDIUM** — Tests need updating to test transport-only adapter |
| `crates/channels/src/lib.rs` | Re-exports `pub mod telegram` | **LOW** — Module stays, just thinner |

### Internal dependencies of telegram.rs

| Dependency | Crate | Used For |
|------------|-------|----------|
| `pares_agens_core::Event` | core | Message/SessionClear/ModelResponse event types |
| `pares_agens_core::channel_contract::{ChannelContract, GroupChatPolicy}` | core | Format constraints, group policy config |
| `pares_agens_core::event_spine::EventSpineHandle` | core | Spine event emission |
| `pares_agens_core::renderers::telegram` | core | HTML rendering for Telegram |
| `pares_agens_core::task_manager::TaskManager` | core | Task CRUD |
| `pares_agens_core::model::StreamDelta` | core | Progressive streaming |
| `pares_agens_core::tool_governance::ToolGovernor` | core | /tools command |
| `pares_agens_core::praxis::write_gate::*` | core | /praxis command |
| `pares_agens_core::task::*` | core | Task types for /tasks display |
| `pares_radix_agenda::scheduler::Scheduler` | agenda | /cron command |
| `pares_radix_marketplace::*` | marketplace | /marketplace, /install |
| `pares_rector::*` | rector | /cluster command |
| `crate::adapter::{ChannelAdapter, ChannelError}` | channels | Trait implementation |
| `crate::group_context::{GroupContextBuffer, GroupMessage}` | channels | Group context buffering |
| `teloxide::*` | teloxide | Bot API |

### What moves WHERE

| Current Location | Destination | Notes |
|-----------------|-------------|-------|
| Slash command logic (lines 1340–2395) | `.px` procedures in `praxis/procedures/telegram/` | One .px per command group |
| Control traits (lines 567–618) | `pares_agens_core::action_handler` | Become action handler trait methods |
| `TelegramRuntimeConfig` | `pares_agens_core::runtime` | Already a core concept |
| `ModelCommand`, `ConfigCommand` enums | Inlined into action handler parse logic | Simple enough to not need separate types |
| `parse_*_command` methods | Into respective .px procedure logic or action handler validation | |
| Marketplace functions (129–268) | `pares_radix_marketplace` crate (already exists) | Move formatting there too |
| NixOS functions (272–371) | `pares_agens_core::ops` or new `pares_ops` module | Platform-specific ops |
| `is_update_authorized` | `pares_agens_core::auth` | Auth belongs in core |
| `current_process_rss_kib` | `pares_agens_core::health` | Health telemetry |
| `TELEGRAM_HELP_COMMANDS` | Generated from procedure registry | No more static array |

---

## 8. Reference: telegram_spine.rs (Target Pattern)

`telegram_spine.rs` (215 lines) is the **target architecture**. It demonstrates:

1. **Transport ONLY**: Receives updates → emits `SpineEvent::Inbound`. Subscribes to `SpineEvent::DeliveryRequest` → sends via Bot API.
2. **No intelligence**: No slash command handling, no group gate, no model calls, no state management.
3. **Progressive streaming**: Handled at transport level (placeholder + debounced edits) — this stays.
4. **Metadata passing**: Includes `placeholder_id` in metadata so delivery can edit vs send new.

### Gap between telegram_spine.rs and full telegram.rs

| Feature | telegram_spine.rs | telegram.rs | Plan |
|---------|-------------------|-------------|------|
| Slash commands | ❌ | ✅ (20+ commands) | .px procedures |
| Group chat gate | ❌ | ✅ | .px procedure |
| Approval buttons | ❌ | ✅ | Transport routes callback → .px procedure |
| HTML rendering | ❌ (plain text only) | ✅ | Transport concern — add to spine |
| Message chunking | ❌ | ✅ | Transport concern — add to spine |
| Verbose mode | ❌ | ✅ | Pipeline metadata flag |
| Auth checks | ❌ | ✅ | Action handler layer |
| Progressive streaming | ✅ | ✅ | Already aligned |
| Event spine emission | ❌ | ✅ | Add to spine |
| Typing indicator | ❌ | ✅ | Transport — add to spine |

### Migration Path

```
Phase 1: Extract intelligence into action handlers (keep old adapter working)
  - Move parse_* methods into standalone modules
  - Implement action handler traits
  - Wire existing control traits to delegate to action handlers
  → No user-visible change

Phase 2: Write .px procedures that call action handlers
  - One .px per command group
  - Test procedures via HTTP spine (command parity test)
  → No user-visible change (old adapter still runs)

Phase 3: Enhance telegram_spine.rs to production quality
  - Add HTML rendering + fallback
  - Add message chunking
  - Add callback query handling
  - Add typing indicator
  - Add event spine emission
  - Add group context buffering (transport-level, passive)
  → telegram_spine.rs becomes production-ready

Phase 4: Switch serve-spine to use .px procedures + enhanced spine
  - Route commands through procedure engine
  - Verify parity with golden file tests
  → serve-spine mode works identically to serve mode

Phase 5: Deprecate old adapter
  - Remove telegram.rs monolith
  - Update CLI to use spine exclusively
  - Clean up dead trait imports
  → Final state: thin transport + .px intelligence
```

---

## Summary Metrics

| Category | Lines | % of File |
|----------|-------|----------|
| **TRANSPORT (stays)** | ~450 | 16% |
| **INTELLIGENCE (extracts)** | ~1600 | 56% |
| **MIXED (split required)** | ~350 | 12% |
| **Tests** | ~460 | 16% |
| **Total** | ~2875 | 100% |

The refactor removes ~1600 lines of intelligence from the channels crate and distributes them across:
- ~25 `.px` procedure files (average 40-60 lines each)
- ~8 action handler trait definitions
- ~15 action handler implementations

The resulting `telegram.rs` (or its replacement via enhanced `telegram_spine.rs`) should be **~500 lines** — pure transport, fully interchangeable with any other channel.