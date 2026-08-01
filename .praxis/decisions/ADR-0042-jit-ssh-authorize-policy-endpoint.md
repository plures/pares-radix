# ADR-0042: JIT-SSH `/v1/ssh/authorize` policy endpoint (design stage)

- Status: Proposed (design stage — px procedure defined here; Rust wiring follows in this ADR's implementation commit)
- Date: 2026-08-01
- Program: `jit-ssh-cert-signing-program`, Track C (radix policy extension)

## Context

`jit-ssh-jitd` (crate `jit-ssh-jitd`, `src/policy.rs`) already implements a `PolicyClient` that calls out to a not-yet-built policy server at `POST {base_url}/v1/ssh/authorize`, fail-closed: any network error, non-2xx, `allowed: false`, missing/zero TTL, or empty `principals` list results in `jitd` refusing to sign. `jitd` deliberately contains no allowlist and no decision logic — that logic is Track C's job, to be built as a `.px`-first extension of the existing radix policy/settings engine (ADR-0038).

### Confirmed wire contract (from `jit-ssh-jitd/src/policy.rs`, ground truth — not paraphrased)

Request (`AuthorizeRequest`, JSON body of the POST):
```
{ "pubkey": string, "target_host": string, "role": string, "user": string }
```

Response (`AuthorizeResponse`):
```
{
  "allowed": bool,
  "ttl_seconds": u64?,        // required when allowed=true (missing => jitd refuses)
  "principals": [string],     // required non-empty when allowed=true (empty => jitd refuses)
  "extensions": [string],     // reserved, not yet consumed by jitd, must still round-trip
  "deny_reason": string?      // surfaced in jitd's error message when allowed=false
}
```

`jitd` treats the endpoint as fail-closed at every layer: unreachable server, non-2xx status, JSON parse failure, `allowed: false`, absent TTL, zero TTL, or empty principals are ALL treated as "do not sign." The radix side must never emit a response that could be misread as an implicit allow (e.g. omitting `allowed` — the field is mandatory in every reply, never a bare 200 with no body).

### What already exists in pares-radix (reused, not reinvented)

- ADR-0038 policy/settings model: `policy:<scope>:<name>` keyspace, `resolve_policy` (host > plugin > global > secure-default precedence), `kind: constraint | setting`, `overridable`/`secure_default` fields, `emit {type: "policy.updated", ...}` audit trail. This ADR's procedures are pure consumers of that model — no new storage primitive.
- `pares-radix-svc` (`crates/pares-radix-svc/src/lib.rs`): axum `Router` built in `build_router`, `SharedState` holding the `&'static CrdtStore` + `AgensRuntime`, existing routes `/healthz`, `/readyz`, `/events`, `/timers`. New route follows the exact same `State<Arc<SharedState>>` + `Json<Req> -> impl IntoResponse` handler pattern already used by `post_event`/`post_timer`.
- `crates/radix-core/src/px_adapter.rs`: the bridge that lets Rust call named `.px` procedures against the live `CrdtStore` and get a structured result back. The new HTTP handler calls into this bridge rather than reimplementing policy logic in Rust — Rust is IO/transport only, per the project's px-first gate.

## Decision

### 1. `.px` procedure: `authorize_ssh` (design-stage, this file is the spec; procedure file follows in the same commit)

```
procedure authorize_ssh(pubkey: string, target_host: string, role: string, user: string) -> ssh_authorization:
  given: "Fail-closed authZ decision for a JIT SSH cert request, per ADR-0038 policy scopes (host > plugin > global > secure-default)"

  # Scope resolution mirrors resolve_policy from ADR-0038 §5, specialized
  # to this program's plugin id ("jit-ssh") and the requested target_host
  # as the host scope.
  resolve_policy {name: "ssh-authorize-role:" role, plugin_id: "jit-ssh", host_id: target_host} -> $role_policy
  resolve_policy {name: "ssh-authorize-default-ttl", plugin_id: "jit-ssh", host_id: target_host} -> $ttl_policy

  when role_policy == null:
    emit {type: "ssh.authorize.denied", pubkey: $pubkey, target_host: $target_host, role: $role, user: $user, reason: "no_policy_for_role"}
    return {allowed: false, deny_reason: "no policy registered for role '" role "' on host '" target_host "'"}
  end

  when role_policy.entry.kind != "setting":
    emit {type: "ssh.authorize.denied", pubkey: $pubkey, target_host: $target_host, role: $role, user: $user, reason: "malformed_policy"}
    return {allowed: false, deny_reason: "policy entry for role is not kind=setting"}
  end

  # role_policy.entry.value is expected shape:
  #   { allowed_users: [string], principals: [string], ttl_seconds: int?, extensions: [string]? }
  when NOT (user in role_policy.entry.value.allowed_users):
    emit {type: "ssh.authorize.denied", pubkey: $pubkey, target_host: $target_host, role: $role, user: $user, reason: "user_not_allowed"}
    return {allowed: false, deny_reason: "user '" user "' is not permitted for role '" role "' on host '" target_host "'"}
  end

  when role_policy.entry.value.principals == null OR role_policy.entry.value.principals == []:
    emit {type: "ssh.authorize.denied", pubkey: $pubkey, target_host: $target_host, role: $role, user: $user, reason: "no_principals_configured"}
    return {allowed: false, deny_reason: "policy for role '" role "' has no principals configured"}
  end

  # TTL: role-specific override wins, else scoped default, else secure-default
  # floor (short-lived by construction — never "unbounded" as a fallback).
  when role_policy.entry.value.ttl_seconds:
    return {
      allowed: true,
      ttl_seconds: role_policy.entry.value.ttl_seconds,
      principals: role_policy.entry.value.principals,
      extensions: role_policy.entry.value.extensions
    } into "ssh_authorize_granted"
  end

  when ttl_policy != null:
    return {
      allowed: true,
      ttl_seconds: ttl_policy.entry.value,
      principals: role_policy.entry.value.principals,
      extensions: role_policy.entry.value.extensions
    } into "ssh_authorize_granted"
  end

  # Secure-default floor per ADR-0038 §4: absence of an explicit TTL setting
  # falls back to a short, hardcoded ceiling rather than an unbounded grant.
  return {
    allowed: true,
    ttl_seconds: 300,
    principals: role_policy.entry.value.principals,
    extensions: role_policy.entry.value.extensions
  } into "ssh_authorize_granted"
```

Every exit path emits an audit event (`ssh.authorize.denied` or the `ssh_authorize_granted` queue, which a subscriber emits `ssh.authorize.granted` from) — mirroring the ADR-0038 §4 audit requirement (every policy decision observable by construction).

### 2. Policy entries this procedure reads (seeded as `policy:global:ssh-authorize-role:<role>` `kind: setting` entries, populated out-of-band by whoever administers the role; NOT authored by this ADR)

```
policy:global:ssh-authorize-role:<role>
  kind: setting
  value: { allowed_users: [string], principals: [string], ttl_seconds: int?, extensions: [string]? }
  secure_default: false        # no role is granted by default; absence = deny
  overridable: true            # plugin/host scopes may narrow or grant per-host roles
```

Absence of any entry for a requested `<role>` is a hard deny (`no_policy_for_role`), matching ADR-0038 §3.4 (absence falls back to secure-default baseline, and this program's secure-default baseline for SSH authorization is "nothing is granted").

### 3. HTTP transport: `POST /v1/ssh/authorize` on `pares-radix-svc`

New axum route registered in `build_router` alongside the existing routes:

```rust
.route("/v1/ssh/authorize", post(post_ssh_authorize))
```

Handler contract (Rust is transport-only; all decision logic lives in the `.px` procedure above via `px_adapter`):

- Request body: `SshAuthorizeRequest { pubkey: String, target_host: String, role: String, user: String }` — field names match `jit-ssh-jitd`'s `AuthorizeRequest` exactly (no renaming/remapping).
- Response body: `SshAuthorizeResponse { allowed: bool, ttl_seconds: Option<u64>, principals: Vec<String>, extensions: Vec<String>, deny_reason: Option<String> }` — matches `AuthorizeResponse` exactly, `allowed` always present.
- Calls `px_adapter` to invoke `authorize_ssh` against the live `CrdtStore` with the request fields as procedure args.
- **Fail-closed on the Rust side too**: if the px_adapter call errors (procedure not found, store error, malformed result), the handler returns HTTP 200 with `{"allowed": false, "deny_reason": "<error class, no internal detail leaked>"}` — never a 5xx-with-empty-body that `jitd` might misinterpret, and never `allowed: true` on any internal failure path. This is a deliberate divergence from typical REST error conventions, required because `jitd`'s fail-closed contract only inspects the JSON body's `allowed` field, not just HTTP status, for its `allowed: false` branch — but non-2xx status is ALSO treated as deny by `jitd`, so an alternate acceptable failure mode is a plain 5xx with a short text body; this ADR chooses 200+`allowed:false` for consistency of the response schema across all decision paths (uniform client-side parsing), and because it produces a `deny_reason` message that surfaces cleanly in `jitd`'s error chain instead of an opaque status-code-only failure.
- No auth token required beyond whatever `pares-radix-svc`'s existing loopback/bearer-token gate already enforces (ADR-0018 §4) — this endpoint does not introduce a new trust boundary, it rides the same one as `/events`/`/timers`.

## Consequences

- **Positive**: Zero new decision logic in Rust — `authorize_ssh` is a straight extension of the ADR-0038 `resolve_policy` pattern; Rust only marshals HTTP <-> px_adapter call <-> HTTP.
- **Positive**: Wire-compatible with `jit-ssh-jitd`'s `PolicyClient` as already shipped — no changes needed in `jit-ssh-jitd` for this to work end-to-end.
- **Risk**: Role policy entries (`policy:global:ssh-authorize-role:*`) are NOT seeded by this ADR — until an operator populates at least one role's entry, every `authorize_ssh` call denies with `no_policy_for_role`. This is correct fail-closed behavior, not a bug, but must be documented in the PR so it isn't mistaken for a broken deploy.
- **Deferred**: per-host (`policy:host:<id>:ssh-authorize-role:*`) overrides are supported by the scope mechanism from day one but no specific host content is authored here, consistent with ADR-0038's "mechanism now, per-host content later" stance.

## Next steps (this ADR's own implementation, same PR)

1. Add `praxis/procedures/ssh-authorize.px` implementing `authorize_ssh` per §1 (real procedure file, not the illustrative sketch above — though the sketch here IS the intended real logic, just needs the exact `.px` grammar verified against the parser).
2. Add `post_ssh_authorize` handler + `SshAuthorizeRequest`/`SshAuthorizeResponse` structs to `crates/pares-radix-svc/src/lib.rs`, wired through `crates/radix-core/src/px_adapter.rs`.
3. Add an integration test in `pares-radix-svc` that seeds a `policy:global:ssh-authorize-role:<role>` entry, POSTs `/v1/ssh/authorize`, and asserts both the allow and no-policy-deny paths.
4. Open PR against `pares-radix` `origin/main` from `feature/jit-ssh-policy-server`.
