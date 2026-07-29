# ADR-0037: Praxisbot Full Radix Parity (Retire Headless-Agens Shortcut)

- Status: Proposed / design in progress (OQ-1..OQ-4, OQ-6 resolved 2026-07-29; OQ-5 open;
  .px-first decomposition begun in §7 — no runtime code in this ADR)
- Date: 2026-07-29
- Epic: `program:praxisbot-full-radix-parity` (memory/epic-registry.json, plures/nixos-config repo scope)
- Stage: DESIGN (per `pares-radix-dev-lifecycle` staged lifecycle: analyze → **design** → fix → test → deploy → verify)
- Coordinates with: `pares-radix:runtime-as-service` (ADR-0018-radix-runtime-as-service.md),
  `pares-radix:plugin-integration` (ADR-0018-procedure-native-plugin-integration.md, PR #477),
  `praxisbot:px-native-task-dashboard` (PR #559 / PR #571)

## 1. Decision

kbristol decided 2026-07-29: praxisbot's current headless-`pares-agens`-only deployment is
a shortcut/debt and is retired. Praxisbot must run the **same architecture already planned
for Windows/Mac**:

1. Full **pares-radix-svc** backend service (the ADR-0018 runtime-as-service, but with the
   full spine/`CompositeActionHandler` assembly wired in - not the additive-only v1 scope
   that ADR-0018 explicitly limited itself to).
2. Full **Tauri GUI** (the existing `src-tauri/` desktop app), running on praxisbot's
   Cosmic desktop session (already deployed - `services.desktopManager.cosmic.enable` +
   `cosmic-greeter` + autologin are live in `hosts/praxisbot/configuration.nix` today).
3. Full **TUI** - does not exist yet in pares-radix as a standalone crate/binary (verified:
   no `crates/*/src/main.rs` outside `src-tauri`; `crates/pares-radix-svc` is HTTP-only per
   ADR-0018). Building it is in scope for this initiative's dev stage, not a prerequisite
   that already exists elsewhere.
4. **pares-agens as a plugin inside pares-radix**, not a standalone systemd unit running a
   composed `pares-agens` binary as today.

This also resolves the open question in `praxisbot:px-native-task-dashboard` (PR #559 vs
PR #571): the dashboard lives in the full Radix spine assembly, because that assembly is
now the mandatory praxisbot deployment shape, not a special-case add-on. PR #559's
`TaskDashboardActionHandler` (spine/`ReactiveRuntime` path, tested) is the dashboard
implementation to carry forward; PR #571's `pares-radix-svc` binary gets the spine wired
into it (see §3) rather than getes a parallel minimal route.

## 2. Ground truth verified for this ADR (2026-07-29)

Read directly, not assumed:

- **`hosts/praxisbot/pares-radix.nix`** (nixos-config, canonical checkout, current `HEAD`
  c9cebfd): a single `systemd.services.pares-radix` unit runs
  `${pares-agens-package}/bin/pares-agens serve --copilot --telegram-token ... --brave-api-key
  ... --model gpt-4.1 --deep-model gpt-5.2`. This is the entire "backend" on praxisbot today
  - one composed binary, no separate `pares-radix-svc`, no GUI service unit, no TUI. `preStart`
  syncs `praxis/` + `plugins/` out of the Nix store into `/home/kbristol` on every start.
  Secrets via `age.secrets.pares-telegram-token` / `pares-brave-key` (agenix, root-owned
  age keys at `/etc/ssh/agenix_ed25519`).
- **`hosts/praxisbot/host.nix`**: flake overlay pulls in `inputs.pares-agens.overlays.default`
  (not `pares-radix` directly) - the running binary is agens' own package, which vendors/links
  radix-core as a library per the dependency direction already established in
  ADR-0018-procedure-native-plugin-integration.md §Context (radix does not depend on agens;
  agens links radix-core).
- **`hosts/praxisbot/configuration.nix`**: Cosmic desktop is ALREADY enabled
  (`services.xserver.enable`, `services.desktopManager.cosmic.enable`,
  `services.displayManager.cosmic-greeter.enable` + `autoLogin.user = "kbristol"`). This means
  the GUI prerequisite (a running Wayland/X session with a logged-in user) already exists -
  no new display-manager work needed, only wiring a GUI *app* into that existing session
  (see open question OQ-2).
- **pares-radix repo (`C:\Projects\pares-radix`, HEAD `80ec089`)**:
  - `Cargo.toml` workspace members: `radix-core, mcp-client, privacy, marketplace, agenda,
    praxis, sync, audit, bitnet-sys, rector, omniscient, pares-radix-svc, src-tauri`. **No
    TUI crate exists.** `packages/` (JS/TS workspace) has `canvas-runtime, create-radix-plugin,
    design-dojo, eslint-plugin-plures, mcp-dev-server` - also no TUI package.
  - `crates/pares-radix-svc/src/{lib,main}.rs` + `tests/smoke.rs` - confirms PR #571's binary
    is real and minimal, matching ADR-0018's explicit "additive, zero changes to radix-core"
    scope note (line ~255 of that ADR).
  - ADR-0018-radix-runtime-as-service.md confirms in its own text that this v1 scope is
    intentionally additive-only and does NOT construct a spine/`CompositeActionHandler` - this
    ADR (0037) is the first to require that assembly be added to `pares-radix-svc`.
  - `src-tauri/` is a full, real Tauri 2 app (icons for all platforms incl. Android/iOS,
    `tauri.conf.json`, `capabilities/default.json`) - this is the GUI to deploy on praxisbot,
    not a new build.
- **Flake pin drift**: `nixos-config/flake.lock` pins `pares-radix` input at rev
  `520cbd4239eca9b71c9f465a3b6ce34c8af15fc9`. pares-radix's own `Cargo.toml` on `main` is at
  workspace version `1.55.52`; kbristol's task brief states praxisbot's pin is 24 releases
  behind (v1.55.28). **Flagged as a prerequisite bump - not fixed in this ADR** (per task
  scope: design only).

## 3. Proposed target architecture

Replace the single `pares-radix.nix` module (one systemd unit, one binary) with a small
module family, all under `hosts/praxisbot/`:

1. **`pares-radix-svc.nix`** (renamed/rewritten from today's `pares-radix.nix`): a systemd
   service running `pares-radix-svc` (the crate in `crates/pares-radix-svc`) instead of
   `pares-agens serve`. This requires a **dev-stage** change in pares-radix itself (tracked
   as a follow-up task under this same program epic, not this ADR): `pares-radix-svc`'s
   `main.rs`/`lib.rs` must construct the full spine (`CompositeActionHandler` +
   `ReactiveRuntime` + `PluresDbStateStore` + `TaskDashboardActionHandler` from PR #559),
   not just the bare `AgensRuntime` driver loop ADR-0018 describes. This is the same
   decision `praxisbot:px-native-task-dashboard`'s open question was blocked on - answered
   here as "yes, pull in the full spine assembly."
2. **`pares-agens` becomes a registered plugin**, not a systemd unit. Concretely: whatever
   channel-adapter / cognition logic `pares-agens` owns (Telegram bot, copilot model client,
   channel routing) needs to be reachable via the plugin mechanism already documented in
   `docs/architecture/plugin-system.md` / ADR-0010 (`RadixPlugin` contract: manifest, praxis
   `expectations`/`rules`/`constraints`, lifecycle hooks, `PluginContext`) - or, if that
   product-UI-plugin mechanism isn't the right fit for a channel adapter, the
   procedure-native mechanism ADR-0018-procedure-native-plugin-integration.md already
   establishes (`.px` law/constraint for policy gaps, narrow Rust/crate change for
   mechanism gaps). **Which of these two existing mechanisms pares-agens should register
   through is an open question (OQ-1)** - this ADR does not invent a third abstraction.
3. **`pares-radix-gui.nix`** (new module): a systemd **user** service (`systemd.user.services`,
   scoped to the `kbristol` user session so it has access to the already-running Cosmic
   Wayland session) that launches the Tauri app binary, pointed at the same PluresDB store
   path the `pares-radix-svc` backend uses. Desktop entry / autostart wiring TBD (OQ-2).
4. **`pares-radix-tui.nix`** (new module, later): once a TUI binary exists in pares-radix
   (new crate, dev-stage work under this program, not yet started), expose it as a plain
   package install (`environment.systemPackages`) or a `wezterm`-launched shortcut - a TUI
   does not need a systemd unit of its own, it is an interactive client of the same backend
   service's automation HTTP interface (per ADR-0018 §2.3's stated interface).
5. **Secrets**: existing `age.secrets.pares-telegram-token` / `pares-brave-key` carry over
   unchanged - they are consumed by whichever process actually opens the Telegram channel
   (today `pares-agens`; after this change, either the plugin-hosting `pares-radix-svc`
   process or a small agens-as-plugin shim, depending on OQ-1's answer). No new secrets are
   obviously required by this migration alone, but GUI auto-launch may need a session-scoped
   secret-forwarding decision (OQ-4).
6. **systemd unit sequencing**: `pares-radix-svc` (system service) must be `Wants=`/`After=`
   ordered before the GUI user service if the GUI expects the backend's HTTP port to be up
   at launch, OR the GUI must tolerate a not-yet-ready backend and retry/poll `/readyz`
   (ADR-0018 already defines this endpoint) - this ADR recommends the latter (poll/retry in
   the GUI) since systemd system-vs-user service ordering across the login boundary is
   fragile in practice (OQ-3).

## 4. Open technical questions - resolved 2026-07-29 (except OQ-5)

kbristol answered OQ-1, OQ-2, OQ-3, OQ-4, and OQ-6 on 2026-07-29 (see
`memory/2026-07-29.md`, 12:20-12:51 PDT). OQ-5 is explicitly NOT answered and remains
genuinely open - do not assume an ordering for it; only escalate if it becomes actually
blocking to the .px-first decomposition or dev-stage work below.

1. **OQ-1 - RESOLVED.** pares-agens is a **special-privilege plugin** - the same category
   as GitHub Copilot inside VS Code: deeply integrated, given elevated platform access, but
   still a plugin, NOT a standalone app with its own systemd unit/binary composition. This
   points toward the `RadixPlugin` contract (`docs/architecture/plugin-system.md`) as the
   base mechanism, extended with a privileged capability tier (channel ownership, model
   invocation authority) that ordinary UI-domain plugins don't get - the .px-first
   decomposition below (§7) treats "what does special-privilege mean as decision logic"
   as the first thing to model, not the plugin-loader plumbing.
2. **OQ-2 - RESOLVED.** GUI launches **on demand by default**, user-configurable (an
   autostart-at-login mode remains available as a setting, not the default). No
   `systemd.user.services.pares-radix-gui` unit with `wantedBy = [ "graphical-session.target" ]`
   by default; the GUI is launched like a normal desktop app (dock pin / manual launch),
   with a config toggle to opt into autostart later.
3. **OQ-3 - RESOLVED with a scope note.** Explore backend/GUI/TUI running on **separate
   machines**, not just same-machine. The backend should be abstracted to the frontend as a
   **local synchronous resource** (i.e. the frontend's mental model and code shape should
   look like calling a local function, not plumbing async network fetches), even when the
   backend happens to be remote. Same-machine remains the default deployment shape
   (praxisbot todaay). **Explicit escape hatch, decided in advance:** if this
   synchronous-local abstraction proves leaky (retries, partial failure, latency, and
   cache-invalidation surface through anyway), ABANDON the abstraction in favor of a
   traditional local-first design rather than compromise the local-first ideal to preserve
   the abstraction. Purity of "looks synchronous" is subordinate to the actual local-first
   guarantee.
4. **OQ-4 - RESOLVED, same caveat as OQ-3.** PluresDB is confirmed as the heart of
   everything, including the secrets plugin - backend-only secret access holds (GUI/TUI
   are HTTP/local-resource clients, never direct secret-file readers). Same caveat as OQ-3:
   kbristol is willing to sacrifice architectural purity elsewhere (e.g. a slightly less
   "clean" secrets-access path) to preserve local-first simplicity if the pure abstraction
   turns out to be impractical.
5. **OQ-5 - STILL OPEN, not resolved.** Flake pin bump sequencing (bump-first vs.
   bundled-with-migration vs. separate-prerequisite-PR) has no decision yet. Do not assume
   an ordering. Flag it as a blocking dependency only if/when the dev-stage work actually
   needs the pin bumped to proceed - do not resolve it by assumption here.
6. **OQ-6 - RESOLVED.** The spine's physical location is **NOT necessarily** `pares-radix`
   core. It could be hoisted into PluresDB instead - not necessarily part of radix-core.
   This changes §3's framing: the "spine assembly in `pares-radix-svc`" work item is no
   longer assumed to live in `crates/pares-radix-svc`/`radix-core`; where the spine
   physically lives is now an open architectural question to be answered AFTER the
   .px-first decomposition (§7) - per the org-wide sequencing directive (decompose
   procedures first, define side-effect handlers second, decide physical location third).

## 5. Explicitly not done in this ADR (per design-stage discipline)

- No code changes to pares-radix, pares-agens, or nixos-config runtime behavior.
- No PR opened against any of those repos' `main`/deployable branches from this ADR alone.
- No fix to the 24-release flake pin drift (flagged only, per OQ-5, still open).
- No new TUI crate scaffolded (flagged as required dev-stage work, not started).
- No physical placement decision for the spine (radix-core vs. PluresDB, per OQ-6) -
  deferred until AFTER the .px-first decomposition in §7, per the org-wide sequencing
  directive.

## 6. Next action (dev stage, blocked only on OQ-5 + §7 decomposition completion)

OQ-1..OQ-4 and OQ-6 are resolved (§4). Per the org-wide sequencing directive (.px-first:
decompose procedures -> define side-effect handlers -> THEN decide physical placement), the
immediate next action is §7's decomposition, not code. Once §7 is far enough along to name
concrete procedures and handler seams:

1. Resolve OQ-5 (flake pin bump sequencing) if/when it actually blocks a dev-stage PR -
   do not resolve it by assumption before then.
2. Dev-stage PR in `pares-radix`: implement the side-effect handlers identified in §7.2-7.4
   (e.g. `plugin_check_privilege`, `gui_resolve_launch_mode`, `backend_resource_call`) as
   thin Rust IO boundaries, per the spine principle "Rust exists ONLY at IO boundaries."
3. Decide physical placement of the spine assembly (radix-core `crates/pares-radix-svc` vs.
   hoisted into PluresDB, per OQ-6) using the decomposed procedures from §7 as the input -
   placement follows behavior, not the reverse.
4. Dev-stage PR: scaffold a TUI crate/binary in pares-radix as a client of the resolved
   backend resource abstraction (OQ-3).
5. Dev-stage PR: register pares-agens as a special-privilege plugin (OQ-1) using the
   procedures drafted in §7.2, retiring the standalone `pares-agens serve` composed binary
   as the praxisbot deployment shape.
6. nixos-config PR (session-workspace-isolation worktree, own task): replace
   `hosts/praxisbot/pares-radix.nix` with `pares-radix-svc.nix` + `pares-radix-gui.nix`
   (on-demand launch per OQ-2, config-toggle for autostart) + `pares-radix-tui.nix` once the
   TUI binary exists.
7. Test stage: smoke-test the full stack on praxisbot (backend health/readyz, GUI
   on-demand launch on Cosmic session, TUI connects, Telegram channel still responds
   end-to-end via the new plugin path) before declaring deploy/verify complete - per the
   mandatory build-the-binary-run-the-binary gate.

## 7. .px-first decomposition (begun 2026-07-29, per C-DEV-001 / C-PLURES-004)

Per the org-wide sequencing directive: fully decompose existing spine/orchestration logic
into procedures FIRST, define side-effect handlers SECOND, and only THEN decide where the
spine physically lives (OQ-6). This section is the START of that decomposition, not the
end - it identifies the seams and drafts the first procedures; it does not claim the
decomposition is complete.

### 7.1 What already exists (ground truth, 2026-07-29)

- `praxis/spine/spine.px` (this worktree) already defines the v3 dataflow queue topology:
  `inbound -> classify_and_route -> route_decision -> assemble_context/dispatch_steered_task
  -> model_request -> invoke_model -> model_response -> route_model_response -> delivery ->
  deliver_response`. IO boundaries are explicitly listed as: channel adapters, model client,
  tool executor, heartbeat timer, TaskDispatcher. This IS the spine's existing
  procedure-native decomposition for the conversation/task loop - it does NOT yet cover
  (a) plugin activation/lifecycle, (b) GUI launch decision logic, or (c) backend-as-
  local-resource abstraction. Those three are the NEW seams this ADR's scope requires, and
  are what §7.2-7.4 draft.
- `docs/architecture/plugin-system.md` (pares-radix repo) defines the existing `RadixPlugin`
  TypeScript contract (manifest, routes, `onActivate`/`onDeactivate` lifecycle,
  `PluginContext` with `settings`/`data`/`llm`/`inference`/`navigation`/`notify` APIs) - this
  is Rust/TS plumbing, NOT a `.px` procedure. Per OQ-1's resolution (agens = special-
  privilege plugin), the DECISION logic of "is this plugin allowed to do X" (invoke a model
  directly, own a channel, bypass normal budget/`llm.available()` gating) is business/policy
  logic that belongs in `.px`, not scattered through `onActivate` TypeScript - this is the
  gap §7.2 targets.
- No existing `.px` procedure governs GUI launch-on-demand-vs-autostart decision logic
  (OQ-2), or backend-resource-call abstraction leak detection (OQ-3/OQ-4). These are
  drafted fresh in §7.3-7.4.

### 7.2 New procedure file: `praxis/procedures/agens-plugin-lifecycle.px`

Captures the special-privilege plugin decision logic from OQ-1: what elevates pares-agens
above an ordinary `RadixPlugin`, and what side effects that privilege triggers when it is
granted or checked.

### 7.3 New procedure file: `praxis/procedures/gui-launch-policy.px`

Captures the on-demand-vs-autostart GUI launch decision (OQ-2) and the readiness-poll
sequencing decision (OQ-3, §3.6) as pure decision logic, independent of whether the GUI
ends up talking to a local or remote backend.

### 7.4 New procedure file: `praxis/procedures/backend-resource-abstraction.px`

Captures the OQ-3/OQ-4 decision logic: how a frontend (GUI or TUI) decides whether it is
talking to a local in-process backend or a remote one, the explicit abandon-the-
abstraction escape hatch if leakiness is detected (retries surfacing, partial failure
surfacing, cache-invalidation surfacing to the caller), and OQ-4's backend-only
secrets-access rule as a constraint.

### 7.5 Drafted procedures

See the three new files committed alongside this ADR (in this worktree,
`praxis/procedures/`):
- `agens-plugin-lifecycle.px`
- `gui-launch-policy.px`
- `backend-resource-abstraction.px`

These are DESIGN-STAGE drafts: procedure signatures, queue bindings, and constraints are
proposed and internally consistent with `praxis/spine/spine.px`'s existing conventions, but
no Rust side-effect handler exists yet for any of the new actions they call (e.g.
`plugin_check_privilege`, `gui_resolve_launch_mode`, `backend_resource_call`,
`backend_leak_detected`). Writing those handlers is the NEXT step (dev stage), not part of
this design-stage commit.

### 7.6 OQ-5 status re: this decomposition

OQ-5 (flake pin sequencing) did NOT become blocking during this decomposition pass - the
`.px` procedures drafted here are language-level artifacts independent of the pares-radix
flake pin version; they can be written and reviewed regardless of which pin is live on
praxisbot. OQ-5 will only become blocking once dev-stage work needs to actually build
against a specific `pares-radix-svc` API surface that differs across the 24-release gap.
No assumption made; revisit at that point.
