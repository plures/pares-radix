# .px Language Guide

> The Praxis Intent Language (`.px`) is a declarative DSL for defining agent behavior, constraints, and automation procedures in pares-radix.

## Table of Contents

- [Overview](#overview)
- [Imports](#imports)
- [Facts](#facts)
- [Rules](#rules)
- [Constraints](#constraints)
- [Procedures](#procedures)
  - [Triggers](#procedure-triggers)
  - [Steps](#steps)
  - [Control Flow](#control-flow)
  - [Parallel Execution](#parallel-execution)
  - [Error Handling & Retry](#error-handling--retry)
- [Dataflow Procedures (v2)](#dataflow-procedures-v2)
- [Contracts](#contracts)
- [Functions](#functions)
- [Triggers (Event-Driven)](#triggers-event-driven)
- [Expressions](#expressions)
- [Types](#types)

---

## Overview

A `.px` file is a collection of top-level declarations. Each file can contain any combination of:

- `import` — bring in definitions from other modules
- `fact` — define structured data types
- `rule` — reactive when/then logic
- `constraint` — validation rules with severity
- `procedure` — multi-step automation workflows
- `contract` — behavioral contracts with examples
- `function` — named computations (deterministic or probabilistic)
- `trigger` — event-driven automation hooks

Comments start with `#` and extend to end of line.

```px
# This is a comment
import my_module::helpers as h

fact User:
  name: string
  role: enum(admin, member, guest)
```

---

## Imports

Import definitions from other `.px` modules:

```px
import auth::permissions
import utils::formatting as fmt
```

Paths use Rust-style `::` separators. Optional `as` alias for local name.

---

## Facts

Facts define structured data schemas:

```px
fact PullRequest:
  number: int
  title: string
  author: string
  labels: list[string]
  draft: bool
  mergeable: optional[bool]
```

### Supported Types

| Type | Description |
|------|-------------|
| `string` | UTF-8 text |
| `int` | Integer |
| `float` | Floating-point number |
| `bool` | Boolean |
| `duration` | Time duration |
| `list[T]` | List of type T |
| `optional[T]` | Nullable type T |
| `enum(a, b, c)` | One of the listed variants |

---

## Rules

Rules define reactive logic: when conditions are met, actions fire.

```px
rule auto_label_security:
  priority: 10
  when:
    - pr.files_changed contains "auth/"
    - pr.labels NOT contains "security-review"
  then:
    - action: add_label label: "security-review"
    - action: notify message: "Security review required"
  capture:
    - fact: "Security label auto-applied" category: audit tags: ["security", "automation"]
```

### Rule Structure

| Clause | Required | Description |
|--------|----------|-------------|
| `priority:` | No | Higher = fires first (default 0) |
| `when:` | Yes | List of condition expressions |
| `let` | No | Variable bindings (repeatable) |
| `then:` | Yes | List of actions to execute |
| `capture:` | No | Facts to record when rule fires |

### Conditional Actions

Actions can be conditional:

```px
rule smart_assign:
  when:
    - issue.assignee == ""
  then:
    - if issue.labels contains "bug": action: assign to: "oncall"
    - if issue.labels contains "feature": action: assign to: "backlog-owner"
    - action: add_label label: "needs-triage"
```

---

## Constraints

Constraints enforce invariants and are evaluated by the Praxis engine:

```px
constraint no_direct_push_to_main:
  scope: repository
  phase: pre-push
  when: branch == "main" && NOT is_pr_merge
  require: has_approval == true
  severity: error
  message: "Direct pushes to main are not allowed"
```

### Constraint Fields

| Field | Required | Description |
|-------|----------|-------------|
| `scope:` | No | What entity this applies to |
| `phase:` | No | Comma-separated lifecycle phases |
| `trait:` | No | Trait/capability this requires |
| `weight:` | No | Numeric weight for scoring |
| `prompt:` | No | LLM evaluation prompt |
| `when:` | No | Guard condition (constraint only evaluates when true) |
| `require:` | No | The invariant expression that must hold |
| `severity:` | Yes | `error`, `warning`, or `info` |
| `message:` | No | Human-readable violation message |

### Phases

Constraints fire at specific lifecycle points:

- `pre-commit` — before code is committed
- `pre-push` — before pushing to remote
- `pre-merge` — before merging a PR
- `on-deploy` — during deployment
- Custom phases supported via string matching

---

## Procedures

Procedures are the heart of .px automation — multi-step workflows with control flow, parallelism, and error handling.

```px
procedure deploy_service:
  trigger: manual
  given: "Deploy a service to production with health checks"
  validate_config {} -> $config
  build_container image: $config.image -> $artifact
  push_to_registry artifact: $artifact
  run_health_check endpoint: $config.health_url -> $status
  when $status.healthy == false:
    rollback artifact: $artifact
    notify channel: "ops" message: "Deploy failed, rolled back"
  end
```

### Procedure Triggers

```px
procedure on_pr_open:
  trigger: on_write {event: "pr.opened"}
  # ... steps

procedure nightly_report:
  trigger: cron {schedule: "0 0 * * *"}
  # ... steps

procedure before_reply:
  trigger: before_response
  # ... steps

procedure after_reply:
  trigger: after_response
  # ... steps

procedure manual_task:
  trigger: manual
  # ... steps
```

| Trigger | Description |
|---------|-------------|
| `manual` | Invoked explicitly |
| `on_write {event: "..."}` | Fires when a matching event is written |
| `before_response` | Runs before agent response generation |
| `after_response` | Runs after agent response generation |
| `cron {schedule: "..."}` | Cron expression schedule |

### Steps

Every line in a procedure body that isn't a control-flow keyword is a **step call**:

```px
action_name param1: value1 param2: value2
action_name {key: "value", count: 42}
action_name {} -> $result
```

- **Named params**: `key: value` pairs
- **Map params**: `{key: value}` JSON-like object
- **Output capture**: `-> $variable_name` stores the step result

Variables are scoped to the procedure and available in subsequent steps via `$name`.

### Control Flow

#### `when` (conditional)

```px
when $user.role == "admin":
  grant_access level: "full"
  log_audit action: "admin_access"
end
```

#### `loop` (iteration)

```px
# Loop over a list variable
loop over $items as item -> $results:
  process_item data: $item
end

# Loop N times
loop times 5:
  retry_connection {}
end
```

#### `match` (pattern matching)

```px
match:
  $status == "success" -> handle_success
  $status == "partial" -> handle_partial
  $status == "failed" -> handle_failure
```

#### `emit` (fire events)

```px
emit {event: "deploy.complete", service: "api", version: "1.2.3"}
emit event: "health_check.failed" service: $name
```

### Parallel Execution

Run multiple branches concurrently with `parallel`:

```px
procedure gather_data:
  trigger: manual
  parallel -> $results:
    branch users:
      fetch_users {}
    end
    branch posts:
      fetch_posts {}
    end
    branch comments:
      fetch_comments {}
    end
  end
  merge_results data: $results
```

- All branches run concurrently
- `-> $variable` captures all branch results as a combined value
- Each branch is named (used as a key in results)
- Branches can contain multiple steps

### Error Handling & Retry

#### `try/catch` (basic)

```px
procedure resilient_fetch:
  trigger: manual
  try:
    fetch_external_api url: "https://api.example.com/data" -> $data
    process_response data: $data
  catch:
    log_error message: $error
    use_cached_data {} -> $data
  end
```

- If any step in `try` fails, execution jumps to `catch`
- The `$error` variable contains the error message
- If no `catch` is provided, the error is captured as the step output

#### Per-Branch Retry (parallel blocks)

Parallel branches support automatic retry with configurable backoff:

```px
procedure resilient_fan_out:
  trigger: manual
  parallel -> $results:
    branch fetch_users retry 3 delay 500 ms backoff exponential max_delay 5000 ms jitter:
      get_users {}
    end
    branch fetch_posts retry 2 delay 100 ms backoff fixed:
      get_posts {}
    end
    branch fetch_static:
      get_static_content {}
    end
  end
```

##### Retry Options

| Option | Description | Example |
|--------|-------------|---------|
| `retry N` | Max additional attempts after first failure | `retry 3` (up to 4 total attempts) |
| `delay N ms` | Base delay between retries | `delay 500 ms` |
| `backoff fixed` | Same delay every retry | `backoff fixed` |
| `backoff exponential` | Delay doubles each retry (500, 1000, 2000, ...) | `backoff exponential` |
| `max_delay N ms` | Cap on delay (prevents exponential blowup) | `max_delay 5000 ms` |
| `jitter` | Add randomness to delay (prevents thundering herd) | `jitter` |

##### Retry Behavior

1. **First attempt** runs immediately (no delay before attempt 0)
2. On failure, waits `delay` ms then retries
3. With `backoff exponential`: delay = `base_delay × 2^(attempt-1)`, capped at `max_delay`
4. With `jitter`: actual delay is random in `[0, calculated_delay]` (full jitter)
5. If all retries exhaust, the branch reports failure
6. The `$retry_count` variable is available inside the branch (0-indexed attempt number)

##### Combining try/catch with parallel retry

```px
procedure robust_pipeline:
  trigger: manual
  try:
    parallel -> $data:
      branch api retry 3 delay 1000 ms backoff exponential max_delay 10000 ms jitter:
        call_flaky_api {}
      end
      branch db retry 2 delay 200 ms:
        query_database {}
      end
    end
    process_results data: $data
  catch:
    notify channel: "alerts" message: "Pipeline failed: " + $error
    use_fallback {}
  end
```

#### Try-Level Retry (via compiled JSON)

The async executor also supports retry on `try` blocks when procedures are constructed programmatically (via the builder or direct JSON):

```json
{
  "kind": "try",
  "retry": 3,
  "retry_delay_ms": 1000,
  "retry_backoff": "exponential",
  "retry_max_delay_ms": 10000,
  "retry_jitter": true,
  "steps": [...],
  "catch": [...]
}
```

This retries the entire `try` block up to N times before falling through to `catch`.

> **Note:** `.px` syntax for try-level retry (`try retry 3:`) is planned but not yet implemented in the parser. Use parallel branch retry or the programmatic builder for now.

---

## Dataflow Procedures (v2)

> **New in pluresdb-px 0.14+**: Dataflow procedures replace trigger-based procedures with pure functions connected by queues.

Dataflow procedures are pure functions with typed inputs and outputs. Unlike trigger-based procedures (which fire on events), dataflow procedures fire when **all input queues have data**.

### Why Dataflow?

| Trigger-based (v1) | Dataflow (v2) |
|---|---|
| Fires on event pattern | Fires when inputs ready |
| Dependencies invisible | Dependencies in signature |
| Sequential by priority | Concurrent by default |
| Reads shared state | Pure function (args → return) |
| Manual ordering | Automatic via queue topology |

### Basic Syntax

```px
# Pure function: takes message, returns classification
procedure classify_message(message: string) -> classification:
  given: "Classify an incoming message"
  detect_intent {text: $message} -> $intent
  score_complexity {text: $message} -> $complexity
  return $classification
```

### Queue Bindings

Use `from "queue_name"` to specify which queue feeds a parameter, and `into "queue_name"` for the output destination:

```px
# Reads from "inbound" queue, writes to "classification" queue
procedure classify_message(message: string from "inbound") -> classification into "classification":
  given: "Classify an incoming message"
  detect_intent {text: $message} -> $intent
  return $intent
```

Default bindings (when `from`/`into` are omitted):
- Input: queue name = parameter name
- Output: queue name = procedure name

### Multi-Input Procedures

Procedures with multiple inputs fire only when **ALL** queues have data:

```px
# Fires when BOTH classification AND context are available
procedure route_message(classification: classification from "classification", context: string from "context") -> route into "route":
  given: "Determine model tier"
  when classification.needs_deep_model:
    return {tier: "premium", reason: "high complexity"}
  end
  return {tier: "fast", reason: "simple message"}
```

### Pipeline Composition

Chain procedures by connecting output queues to input queues:

```
inbound → [classify_message] → classification → [route_message] → route → [invoke_model] → response
```

No orchestrator needed — data flows through queues automatically.

### Termination

- A procedure that returns `null`/nothing → nothing pushed downstream → propagation stops
- An empty queue means a stopped system (natural quiescence)
- Depth guard: queue rejects writes when lineage depth exceeds limit (default 25)

### Available Types

Parameter and return types support:
- Built-in: `string`, `int`, `float`, `bool`, `duration`
- Lists: `list[string]`, `list[int]`
- Optional: `string?`, `int?`
- Enums: `enum(fast, standard, premium)`
- User-defined: any identifier (e.g., `classification`, `route`, `response`)

### Effect Boundary

Procedures are pure. Side effects (model calls, tool dispatch, network IO) happen through **actions** called within steps. Actions are implemented in Rust and registered in the `CerebellumActionHandler`.

Available actions for dataflow procedures:

| Action | Purpose |
|---|---|
| `normalize_text` | Lowercase + trim |
| `detect_intent` | Classify as question/command/statement/greeting/farewell |
| `score_complexity` | Structural complexity score 0-6 |
| `detect_tools_needed` | Pattern-match for tool-requiring messages |
| `match_plugin` | Match against known plugin categories |
| `extract_topic` | Extract significant words as topic |
| `detect_topic_shift` | Compare current vs previous topic |
| `determine_model_tier` | Complexity → model tier decision |
| `model_complete` | Call LLM API |
| `compute_embedding` | Generate embedding vector |
| `read_state` / `write_state` | Persistent state access |

### Migration from Trigger-Based

```px
# OLD (trigger-based)
procedure classify:
  trigger: on_write {pattern: "inbound:*"}
  pluresdb_read {key: "inbound:latest"} -> $msg
  detect_intent {text: $msg.content} -> $intent
  pluresdb_write {key: "classification", value: $intent}

# NEW (dataflow)
procedure classify_message(message: string from "inbound") -> classification into "classification":
  detect_intent {text: $message} -> $intent
  return $intent
```

Both styles coexist — files can contain trigger-based and dataflow procedures side by side.

---

## Contracts

Contracts define behavioral expectations with concrete examples:

```px
contract classify_intent:
  given: "A user message in a support context"
  when: "The user sends a message"
  then: "Classify the intent as one of: question, complaint, praise, request"
  threshold: 0.9
  examples:
    - input: "How do I reset my password?"
      expect: "question"
    - input: "This product is terrible!"
      expect: "complaint"
    - input: "Thanks for the quick help!"
      expect: "praise"
      threshold: 0.85
```

### Contract Fields

| Field | Required | Description |
|-------|----------|-------------|
| `given:` | No | Context/precondition |
| `when:` | No | Triggering condition |
| `then:` | No | Expected behavior |
| `threshold:` | No | Global accuracy threshold (0.0–1.0) |
| `examples:` | Yes | Test cases with input/expect pairs |

Per-example `threshold:` overrides the global threshold for that case.

---

## Functions

Named computations that can be called from expressions:

```px
function calculate_risk(severity: int, likelihood: float) -> float:
  mode: deterministic
  """
  Calculate risk score as severity × likelihood.
  Returns a value between 0.0 and 10.0.
  """

function summarize_thread(messages: list[string]) -> string:
  mode: probabilistic
  """
  Summarize a conversation thread into a single paragraph.
  Focus on decisions made and action items.
  """
```

### Function Modes

| Mode | Description |
|------|-------------|
| `deterministic` | Pure computation, same input → same output |
| `probabilistic` | May involve LLM inference, output varies |
| `hybrid` | Mix of deterministic logic and probabilistic steps |

---

## Triggers (Event-Driven)

Standalone trigger declarations for hooking into system events:

```px
trigger consolidate_memories:
  on: timer
  schedule: "0 */6 * * *"
  run: memory_consolidation

trigger index_new_facts:
  on: after_store
  run: reindex_embeddings

trigger log_searches:
  on: before_search
  run: search_telemetry
```

### Trigger Events

| Event | Description |
|-------|-------------|
| `after_store` | After a fact/memory is stored |
| `before_search` | Before a semantic search executes |
| `timer` | On a cron schedule |
| `on_event("name")` | When a named event fires |

---

## Expressions

Expressions are used in `when`, `require`, conditions, and step parameters.

### Operators

| Operator | Description |
|----------|-------------|
| `==`, `!=` | Equality |
| `>`, `<`, `>=`, `<=` | Comparison |
| `&&`, `and` | Logical AND |
| `||`, `or` | Logical OR |
| `NOT` | Logical negation |
| `contains` | Membership test |

### Terms

- **Identifiers**: `name`, `status`
- **Dotted paths**: `pr.author.name`, `config.timeout`
- **Bracket access**: `items[0]`, `map["key"]`
- **Variables**: `$result`, `$error`, `$retry_count`
- **Function calls**: `len(items)`, `contains(list, item)`
- **Literals**: `"string"`, `42`, `3.14`, `true`, `[1, 2, 3]`, `{key: "val"}`
- **Parenthesized**: `(a > 0 && b < 10)`

---

## Complete Example

```px
# service-deploy.px — Production deployment procedure with safety checks

import infra::kubernetes as k8s
import monitoring::alerts

fact DeployConfig:
  service: string
  version: string
  replicas: int
  health_endpoint: string

constraint min_replicas:
  scope: deployment
  phase: pre-deploy
  require: config.replicas >= 2
  severity: error
  message: "Production services must have at least 2 replicas"

procedure safe_deploy:
  trigger: manual
  given: "Deploy a service safely with canary rollout and health checks"

  validate_config {} -> $config

  # Run pre-deploy checks in parallel
  parallel -> $checks:
    branch lint:
      run_linter service: $config.service
    end
    branch test retry 2 delay 5000 ms backoff exponential:
      run_integration_tests service: $config.service
    end
    branch security:
      run_security_scan image: $config.service version: $config.version
    end
  end

  when $checks.security.vulnerabilities > 0:
    notify channel: "security" message: "Vulnerabilities found, deploy blocked"
    emit {event: "deploy.blocked", reason: "security"}
  end

  # Canary deploy with retry
  try:
    deploy_canary service: $config.service version: $config.version -> $canary
    wait_for_health endpoint: $config.health_endpoint timeout: 300
    promote_canary deployment: $canary
  catch:
    rollback_canary deployment: $canary
    notify channel: "ops" message: "Canary failed: " + $error
  end

  emit {event: "deploy.complete", service: $config.service, version: $config.version}
```

---

## File Organization

Recommended project structure:

```
praxis/
├── facts/           # Data schemas
│   ├── domain.px
│   └── events.px
├── rules/           # Reactive rules
│   ├── automation.px
│   └── notifications.px
├── constraints/     # Validation & invariants
│   ├── security.px
│   └── quality.px
├── procedures/      # Multi-step workflows
│   ├── deploy.px
│   └── incident.px
├── contracts/       # Behavioral contracts
│   └── classification.px
└── triggers/        # Event hooks
    └── lifecycle.px
```

---

## Grammar Reference

The formal PEG grammar is in `crates/praxis/src/px/grammar.pest`. Key parsing rules:

- Indentation is **2 spaces** (not tabs)
- Blocks end with `end` keyword
- Comments: `# ...` (line comments only)
- Strings: double-quoted `"..."` or single-quoted `'...'`
- Variables: `$name` (alphanumeric + underscore)
- All keywords are lowercase
