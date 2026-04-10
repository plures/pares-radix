# ADR-0011: Plugin Security Model

## Status: Approved

## Date: 2026-04-10

## Context

Pares-radix is a praxis-native plugin platform. Plugins extend the platform with new Facts, Events, Rules, and Constraints. Any plugin — first-party or third-party — can declare rules that process events and mutate state. This creates a security surface: a malicious or buggy plugin could exfiltrate data, corrupt state, or perform unauthorized I/O.

ADR-0012 (Cognitive Architecture) established a 5-level brain-first authorization gate for subagent actions. This ADR extends that model to plugins: all plugin actions pass through the same gate, with additional restrictions on the runtime environment.

## Decision

### Principle: Same Rules for Everyone

First-party and third-party plugins follow identical security rules. No exceptions. No "trusted" plugins that skip validation. If a rule is worth enforcing, it's worth enforcing universally.

### 1. Plugins Are Praxis Modules

Every plugin is a set of praxis primitives:

| Primitive | Security Property |
|-----------|-------------------|
| **Facts** | Inspectable — all state is visible to the platform |
| **Events** | Observable — all actions are logged |
| **Rules** | Auditable — all logic is declarative, no hidden behavior |
| **Constraints** | Enforceable — violations are blocked, not warned |

If it can't be expressed as praxis primitives, it doesn't belong in a plugin.

### 2. Platform Capability Adapters

Plugins cannot perform I/O directly. No `fetch()`, no `fs`, no `process`, no `eval()`.

Instead, plugins emit events that the platform mediates:

```
Plugin rule emits: { type: "http.request", url: "https://api.example.com/data" }
Platform adapter: validates URL against allowlist → performs fetch → returns result as event
Plugin receives: { type: "http.response", status: 200, body: {...} }
```

Platform capability adapters:
- **Network** — HTTP requests (allowlisted URLs only)
- **Storage** — PluresDB reads/writes (namespaced per plugin)
- **UI** — design-dojo component rendering (sandboxed)
- **Notifications** — user-facing alerts (rate-limited)
- **LLM** — model inference (token-budgeted)
- **System** — clipboard, shell, files (requires explicit user approval per session)

### 3. Capability Manifest

Every plugin declares its required capabilities:

```json
{
  "id": "com.example.my-plugin",
  "version": "1.0.0",
  "capabilities": {
    "network": {
      "urls": ["https://api.example.com/*"],
      "reason": "Fetches weather data"
    },
    "storage": {
      "namespaces": ["weather-cache"],
      "reason": "Caches API responses"
    },
    "llm": {
      "maxTokensPerDay": 10000,
      "reason": "Summarizes forecasts"
    }
  },
  "praxis": {
    "facts": ["weather.current", "weather.forecast"],
    "events": ["weather.refresh.requested"],
    "rules": ["weather.auto-refresh"],
    "constraints": ["weather.api-rate-limit"]
  }
}
```

On install, the platform shows the manifest to the user:

> **My Weather Plugin** wants to:
> - Access https://api.example.com/* (weather data)
> - Store data in "weather-cache"
> - Use up to 10,000 LLM tokens/day
>
> [Allow] [Deny]

No silent capabilities. No ambient permissions.

### 4. Restricted Rule Runtime

Rule `evaluate` functions run in a restricted context:

**Allowed:**
- Read facts from praxis state
- Emit events (which the platform mediates)
- Return new facts
- Call pure functions (math, string, date)

**Blocked:**
- `fetch`, `XMLHttpRequest`, `WebSocket`
- `require`, `import()` (dynamic)
- `process`, `child_process`, `fs`
- `eval`, `Function()`, `setTimeout`, `setInterval`
- `globalThis` mutation
- Any reference outside the sandbox

Enforcement: Rules execute in a frozen context. The platform wraps execution with a capability boundary that intercepts blocked APIs.

### 5. Trust Tiers

Trust tiers affect **visibility**, not **privileges**. All plugins have the same security rules.

| Tier | Verification | Badge | Install UX |
|------|-------------|-------|-----------|
| **Verified** | Source audited, maintainer identity confirmed, signed | ✅ Verified | One-click install, manifest shown |
| **Community** | Published to registry, passes automated gates | 🏷️ Community | Manifest shown, "unverified" warning |
| **Local** | Loaded from filesystem, no registry | 🔧 Local | Full manifest review, explicit path confirmation |

Gates for registry submission:
1. Manifest schema validation
2. No blocked API usage in rule code (static analysis)
3. Size audit (<5MB)
4. Type check passes
5. License declared

### 6. Authorization Gate (from ADR-0012)

Plugin actions pass through the same 5-level brain-first gate:

1. **Hard constraint violation** → auto-BLOCK (e.g., plugin tries to access URL not in manifest)
2. **Already done** → auto-SKIP
3. **Known failures** → inject warning context
4. **Destructive/external** → surface to HUMAN (e.g., plugin wants System capability)
5. **Everything else** → brain auto-APPROVES

The `System` capability (clipboard, shell, files) always requires Level 4 (human approval) on first use per session.

### 7. Plugin Data Isolation

- Each plugin gets a namespaced PluresDB partition
- Plugins cannot read other plugins' data without explicit `data-sharing` capability
- Platform facts are readable by all plugins (they're the shared state)
- Plugin-emitted facts are tagged with origin for audit

## Consequences

**Positive:**
- Plugins are inspectable by design — praxis primitives are transparent
- No ambient capabilities — everything declared and approved
- Same security model scales from "I wrote this" to "stranger on the internet wrote this"
- Brain-first gate provides defense-in-depth beyond static analysis

**Negative:**
- Capability adapters add latency to I/O operations
- Some legitimate patterns (WebSocket streaming, background timers) need adapter support
- Restricted runtime may frustrate developers expecting full JS/TS access
- Static analysis for blocked APIs isn't perfect — runtime enforcement is the real boundary

**Risks:**
- Escape from restricted runtime via prototype pollution or WASM
- Social engineering via misleading manifest descriptions
- Plugin collusion (two plugins sharing data via platform facts)

**Mitigations:**
- Runtime enforcement is the primary boundary, static analysis is a gate
- Manifest review before install with clear capability descriptions
- Audit log tracks all plugin I/O through adapters
- Plugin-emitted facts tagged with origin prevents spoofing
