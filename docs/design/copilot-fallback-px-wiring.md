# Copilot fallback .px wiring (design-stage)

Date: 2026-07-28  
Epic: `pares-radix:copilot-fallback-px-wiring`

## Scope
Design only. No production code changes in this commit.

## Evidence from current code

1. `crates/radix-core/src/auth/copilot.rs` currently owns the full fallback loop inside `CopilotModelClient::complete()`:
   - handles 421/401/5xx retries inline
   - on 4xx, iterates `self.fallback_models`
   - mutates `self.model` to each fallback and recursively calls `complete()`
   - restores primary model after each attempt
   - includes a recent HashSet dedupe patch to avoid self/duplicate fallback loops
2. `praxis/procedures/model-fallback-selection.px` already defines the intended decision owner:
   - procedure `select_fallback_model`
   - constraints: never retry failed/already-tried model, use live availability
   - explicit note that Rust should call this procedure instead of static list iteration
3. `crates/radix-core/src/pluresdb_bridge.rs` defines ownership boundaries clearly:
   - `PluresDbBridge` is platform/procedure execution infrastructure
   - cognition/model-call logic should not absorb this bridge directly
4. `crates/radix-core/src/spine/procedures/model_invoker.rs` is the model-call orchestrator in the spine pipeline; it already owns per-request context and terminal user-visible error emission.

## Options compared

### A) Inject `PluresDbBridge` into `CopilotModelClient` and execute `select_fallback_model` directly

**Pros**
- Direct and local to the existing fallback loop
- Minimal new call-site surface

**Cons**
- Violates current module boundaries: `auth/copilot.rs` becomes coupled to procedure runtime/bridge concerns
- Harder to test cleanly (network client + procedure engine coupling)
- Pushes business decision routing into a transport client that should stay provider-focused
- Makes `ModelClient` implementations uneven (Copilot gets spine/procedure dependencies others do not)

### B) Return a typed “needs fallback” signal to the caller that owns orchestration, and let caller execute fallback selection

**Pros**
- Preserves separation: `CopilotModelClient` remains HTTP/provider adapter
- Aligns with existing architecture intent in `model-fallback-selection.px`
- Caller already has request context and error reporting ownership
- Keeps fallback decision logic in .px where constraints are enforceable/testable

**Cons**
- Requires interface changes (`ModelClient` error/result typing)
- Requires explicit fallback orchestration in caller path

## Recommendation
Choose **Option B**.

The fallback choice is business decision logic, not transport logic. The provider client should report “primary failed with fallback-eligible condition” as typed state, and the spine-side caller should invoke `select_fallback_model` and decide next attempt.

---

## Proposed interface and call-chain changes (design only)

### 1) Typed fallback signal from model client

**File:** `crates/radix-core/src/model.rs`

Introduce typed error/outcome surface (names illustrative):

- `enum ModelClientError` with at least:
  - `NeedsFallback(FallbackRequestContext)`
  - `ProviderFailure { status: Option<u16>, model: String, message: String }`
  - `Cancelled`
  - `Transport(String)`
- `struct FallbackRequestContext`:
  - `failed_model: String`
  - `already_tried: Vec<String>` (or caller-maintained set + failed model)
  - `error_status: u16`
  - `task_context: serde_json::Value` (caller fills from routing metadata)

`ModelClient::complete()` returns `Result<ModelCompletion, ModelClientError>`.

### 2) Copilot client emits typed fallback-needed instead of self-iterating fallbacks

**File:** `crates/radix-core/src/auth/copilot.rs`

- Keep 421/401/5xx bounded retries as provider-local resiliency.
- On fallback-eligible 4xx, return `ModelClientError::NeedsFallback(...)`.
- Remove recursive fallback execution from `CopilotModelClient::complete()`.
- Keep model ID observation (`model_id()`/state) but do not mutate through fallback chain internally.

### 3) Spine caller owns fallback orchestration + procedure invocation

**Primary file:** `crates/radix-core/src/spine/procedures/model_invoker.rs`

Add an orchestration loop in invoker (or extracted helper) that:
1. Calls model client with current candidate model.
2. If `NeedsFallback`, executes `select_fallback_model` through spine/procedure runtime seam.
3. If procedure returns exhausted/null candidate, emit terminal model error.
4. Otherwise retry model call with selected candidate.
5. Track `already_tried` set and hard-cap attempts.

**Supporting files likely touched:**
- `crates/radix-core/src/spine/runtime.rs` (if invoker needs injected procedure executor/handle)
- `crates/radix-core/src/spine/model_selection_actions.rs` (ensure all actions used by `model-fallback-selection.px` are wired)
- `praxis/procedures/model-fallback-selection.px` (only if schema alignment tweaks are needed)

### 4) Ownership model

- `CopilotModelClient`: provider transport + provider-local retries only
- Spine invoker/runtime: request lifecycle, procedure invocation, fallback orchestration, terminal delivery
- `PluresDbBridge` / procedure engine: decision execution infra, not injected into auth provider client

---

## Error and cancellation semantics

1. **Retryable provider errors (421/401/5xx):** handled inside provider client as today (bounded).
2. **Fallback-eligible 4xx:** surfaced as `NeedsFallback`, not terminal yet.
3. **Procedure failure (cannot evaluate fallback):** fail closed with terminal error containing both provider failure and procedure failure context.
4. **Fallback exhausted (`model = null`/`exhausted = true`):** terminal provider-unavailable error; no further retries.
5. **Cancellation/timeout:** map to `Cancelled`/timeout class and stop immediately; do not launch fallback selection after cancellation signal.
6. **Loop safety:** enforce monotonic `already_tried` set + max-attempt cap (e.g., `1 + fallback_candidates`, plus optional hard ceiling) to prevent recursion/oscillation.

## Test plan (implementation-stage gate)

### Unit
1. `copilot.rs`: on 4xx returns `NeedsFallback` (not recursive call).
2. `copilot.rs`: 401/421/5xx bounded retry behavior unchanged.
3. `model_invoker.rs`: handles `NeedsFallback` by invoking selection procedure and retrying with returned candidate.
4. `model_invoker.rs`: exhausted candidate path emits terminal error once.
5. `model_invoker.rs`: cancellation short-circuits without fallback invocation.
6. Loop-safety test: duplicate/self fallback candidates never retried.

### Procedure-level
1. `model-fallback-selection.px` returns only live-available non-tried candidate.
2. Returns exhausted when all candidates excluded.
3. Constraint failure surfaces as error for invalid candidate selection.

### Integration
1. Simulated call chain: primary returns 400, procedure returns candidate, second model succeeds.
2. Simulated chain: primary 400 + no candidate => terminal error with structured reason.
3. Regression for 2026-07-28 self-fallback loop class.

## Non-goals
- No immediate changes to provider discovery/scoring algorithms.
- No changes to external API surface beyond typed internal model-client contract.
- No production implementation in this design commit.
