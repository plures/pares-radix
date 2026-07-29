# ADR-0037: Praxisbot Full Radix Parity (Retire Headless-Agens Shortcut)

- Status: Proposed (design only — no runtime code in this ADR)
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
   full spine/`CompositeActionHandler` assembly wired in — not the additive-only v1 scope
   that ADR-0018 explicitly limited itself to).
2. Full **Tauri GUI** (the existing `src-tauri/` desktop app), running on praxisbot's
   Cosmic desktop session (already deployed — `services.desktopManager.cosmic.enable` +
   `cosmic-greeter` + autologin are live in `hosts/praxisbot/configuration.nix` today).
3. Full **TUI** — does not exist yet in pares-radix as a standalone crate/binary (verified:
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
  — one composed binary, no separate `pares-radix-svc`, no GUI service unit, no TUI. `preStart`
  syncs `praxis/` + `plugins/` out of the Nix store into `/home/kbristol` on every start.
  Secrets via `age.secrets.pares-telegram-token` / `pares-brave-key` (agenix, root-owned
  age keys at `/etc/ssh/agenix_ed25519`).
- **`hosts/praxisbot/host.nix`**: flake overlay pulls in `inputs.pares-agens.overlays.default`
  (not `pares-radix` directly) — the running binary is agens' own package, which vendors/links
  radix-core as a library per the dependency direction already established in
  ADR-0018-procedure-native-plugin-integration.md §Context (radix does not depend on agens;
  agens links radix-core).
- **`hosts/praxisbot/configuration.nix`**: Cosmic desktop is ALREADY enabled
  (`services.xserver.enable`, `services.desktopManager.cosmic.enable`,
  `services.displayManager.cosmic-greeter.enable` + `autoLogin.user = "kbristol"`). This means
  the GUI prerequisite (a running Wayland/X session with a logged-in user) already exists —
  no new display-manager work needed, only wiring a GUI *app* into that existing session
  (see open question OQ-2).
- **pares-radix repo (`C:\Projects\pares-radix`, HEAD `80ec089`)**:
  - `Cargo.toml` workspace members: `radix-core, mcp-client, privacy, marketplace, agenda,
    praxis, sync, audit, bitnet-sys, rector, omniscient, pares-radix-svc, src-tauri`. **No
    TUI crate exists.** `packages/` (JS/TS workspace) has `canvas-runtime, create-radix-plugin,
    design-dojo, eslint-plugin-plures, mcp-dev-server` — also no TUI package.
  - `crates/pares-radix-svc/src/{lib,main}.rs` + `tests/smoke.rs` — confirms PR #571's binary
    is real and minimal, matching ADR-0018's explicit "additive, zero changes to radix-core"
    scope note (line ~255 of that ADR).
  - ADR-0018-radix-runtime-as-service.md confirms in its own text that this v1 scope is
    intentionally additive-only and does NOT construct a spine/`CompositeActionHandler` — this
    ADR (0037) is the first to require that assembly be added to `pares-radix-svc`.
  - `src-tauri/` is a full, real Tauri 2 app (icons for all platforms incl. Android/iOS,
    `tauri.conf.json`, `capabilities/default.json`) — this is the GUI to deploy on praxisbot,
    not a new build.
- **Flake pin drift**: `nixos-config/flake.lock` pins `pares-radix` input at rev
  `520cbd4239eca9b71c9f465a3b6ce34c8af15fc9`. pares-radix's own `Cargo.toml` on `main` is at
  workspace version `1.55.52`; kbristol's task brief states praxisbot's pin is 24 releases
  behind (v1.55.28). **Flagged as a prerequisite bump — not fixed in this ADR** (per task
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
   decision `praxisbot:px-native-task-dashboard`'s open question was blocked on — answered
   here as "yes, pull in the full spine assembly."
2. **`pares-agens` becomes a registered plugin**, not a systemd unit. Concretely: whatever
   channel-adapter / cognition logic `pares-agens` owns (Telegram bot, copilot model client,
   channel routing) needs to be reachable via the plugin mechanism already documented in
   `docs/architecture/plugin-system.md` / ADR-0010 (`RadixPlugin` contract: manifest, praxis
   `expectations`/`rules`/`constraints`, lifecycle hooks, `PluginContext`) — or, if that
   product-UI-plugin mechanism isn't the right fit for a channel adapter, the
   procedure-native mechanism ADR-0018-procedure-native-plugin-integration.md already
   establishes (`.px` law/constraint for policy gaps, narrow Rust/crate change for
   mechanism gaps). **Which of these two existing mechanisms pares-agens should register
   through is an open question (OQ-1)** — this ADR does not invent a third abstraction.
3. **`pares-radix-gui.nix`** (new module): a systemd **user** service (`systemd.user.services`,
   scoped to the `kbristol` user session so it has access to the already-running Cosmic
   Wayland session) that launches the Tauri app binary, pointed at the same PluresDB store
   path the `pares-radix-svc` backend uses. Desktop entry / autostart wiring TBD (OQ-2).
4. **`pares-radix-tui.nix`** (new module, later): once a TUI binary exists in pares-radix
   (new crate, dev-stage work under this program, not yet started), expose it as a plain
   package install (`environment.systemPackages`) or a `wezterm`-launched shortcut — a TUI
   does not need a systemd unit of its own, it is an interactive client of the same backend
   service's automation HTTP interface (per ADR-0018 §2.3's stated interface).
5. **Secrets**: existing `age.secrets.pares-telegram-token` / `pares-brave-key` carry over
   unchanged — they are consumed by whichever process actually opens the Telegram channel
   (today `pares-agens`; after this change, either the plugin-hosting `pares-radix-svc`
   process or a small agens-as-plugin shim, depending on OQ-1's answer). No new secrets are
   obviously required by this migration alone, but GUI auto-launch may need a session-scoped
   secret-forwarding decision (OQ-4).
6. **systemd unit sequencing**: `pares-radix-svc` (system service) must be `Wants=`/`After=`
   ordered before the GUI user service if the GUI expects the backend's HTTP port to be up
   at launch, OR the GUI must tolerate a not-yet-ready backend and retry/poll `/readyz`
   (ADR-0018 already defines this endpoint) — this ADR recommends the latter (poll/retry in
   the GUI) since systemd system-vs-user service ordering across the login boundary is
   fragile in practice (OQ-3).

## 4. Open technical questions for kbristol (need a human decision before dev stage)

1. **OQ-1 — pares-agens plugin registration mechanism.** Should pares-agens's
   channel/cognition logic register through the existing UI-oriented `RadixPlugin` contract
   (`docs/architecture/plugin-system.md`, ADR-0010/0011 — manifest + loader + marketplace,
   designed for product/domain apps with routes/nav/settings), or through the
   procedure-native mechanism ADR-0018-procedure-native-plugin-integration.md establishes
   (`.px` constraints for policy, narrow Rust changes for mechanism gaps, no new loader
   abstraction)? These are different shapes for different problems today; agens-as-plugin
   doesn't cleanly fit either as currently scoped. Getting this wrong risks building a
   plugin interface pares-agens doesn't actually need, or missing one it does.
2. **OQ-2 — Tauri GUI autostart on Cosmic.** Does the Tauri GUI need to autostart at login
   (systemd user service / Cosmic autostart `.desktop` entry) so it behaves like "the OS is
   running Radix" the same way Windows/Mac users would experience it, or is
   launch-on-demand (user opens it manually, or it's pinned in the Cosmic dock) sufficient
   for praxisbot specifically since it's a workstation kbristol also uses interactively for
   other things (dev work, remote desktop via `remmina`)? This changes whether a new
   `systemd.user.services.pares-radix-gui` unit + `wantedBy = [ "graphical-session.target" ]`
   is needed at all.
3. **OQ-3 — backend/GUI/TUI startup sequencing.** Confirm the recommendation in §3.6 (GUI
   polls `/readyz` rather than strict systemd unit ordering across the system/user service
   boundary) or state a preferred alternative (e.g., a `.timer`-based readiness gate, or
   accepting a brief GUI-shows-"connecting" state on cold boot).
4. **OQ-4 — secrets/agenix wiring for the additional services.** If the GUI or a future TUI
   process needs any of the same secrets `pares-radix-svc` reads (Telegram token, Brave key,
   or a future GitHub App token), should they get their own `age.secrets.*` entries scoped to
   the `kbristol` user session, or should the GUI/TUI only ever talk to those secrets
   indirectly through the backend's automation HTTP interface (never reading secret files
   directly)? Recommend the latter (backend-only secret access, GUI/TUI are HTTP clients) but
   flagging as a decision point since it affects whether any new `age.secrets` blocks are
   needed at all.
5. **OQ-5 — flake pin bump sequencing.** The praxisbot `pares-radix` input is 24 releases
   behind (pinned rev vs. `main` @ `1.55.52`). Should the pin bump happen (a) before any of
   this migration work starts (clean baseline), (b) as part of the same PR that lands the
   new `pares-radix-svc.nix`/`pares-radix-gui.nix` modules (since that PR needs the newer
   `pares-radix-svc`/spine code anyway), or (c) is a separate prerequisite PR preferred so
   the bump's own blast radius (24 releases of changes) is reviewed in isolation from the
   architecture change? Recommend (c) — bump first, verify praxisbot still runs clean on the
   current architecture, then do the architecture migration on top of a current pin.
6. **OQ-6 — spine assembly in `pares-radix-svc`: scope of the dev-stage work.** ADR-0018 was
   explicit that v1 is additive with "zero changes to existing `src-tauri` or `radix-core`
   public API." Wiring a full `CompositeActionHandler`/`ReactiveRuntime` spine into
   `pares-radix-svc` is real new work in `crates/pares-radix-svc` (and possibly a small,
   narrowly-scoped `radix-core` public-API surface change to expose spine construction
   helpers that today may only be reachable from `src-tauri`'s app-setup code — needs
   dev-stage investigation, not assumed here). Confirm this is acceptable scope growth
   beyond ADR-0018's original "zero radix-core changes" promise, or whether ADR-0018 itself
   needs a formal revision first.

## 5. Explicitly not done in this ADR (per design-stage discipline)

- No code changes to pares-radix, pares-agens, or nixos-config runtime behavior.
- No PR opened against any of those repos' `main`/deployable branches from this ADR alone.
- No fix to the 24-release flake pin drift (flagged only, per OQ-5).
- No new TUI crate scaffolded (flagged as required dev-stage work, not started).
- No decision made on OQ-1..OQ-6 — those are for kbristol.

## 6. Next action (dev stage, blocked on OQ-1..OQ-6 answers)

Once OQ-1..OQ-6 are answered:

1. Bump the `pares-radix` flake input pin on praxisbot (own small PR, per OQ-5 recommendation).
2. Dev-stage PR in `pares-radix`: wire full spine (`CompositeActionHandler`/`ReactiveRuntime`/
   `PluresDbStateStore`/`TaskDashboardActionHandler`) into `crates/pares-radix-svc`, closing
   the ADR-0018-vs-PR#559 gap `praxisbot:px-native-task-dashboard` was blocked on.
3. Dev-stage PR: scaffold a TUI crate/binary in pares-radix as an HTTP client of
   `pares-radix-svc`'s automation interface.
4. Dev-stage PR: resolve OQ-1 (plugin mechanism) and register pares-agens's channel/cognition
   logic through it, retiring the standalone `pares-agens serve` composed binary as the
   praxisbot deployment shape.
5. nixos-config PR (session-workspace-isolation worktree, own task): replace
   `hosts/praxisbot/pares-radix.nix` with `pares-radix-svc.nix` + `pares-radix-gui.nix` (+
   `pares-radix-tui.nix` once the TUI binary exists), per §3.
6. Test stage: smoke-test the full stack on praxisbot (backend health/readyz, GUI launches on
   Cosmic session, TUI connects, Telegram channel still responds end-to-end via the new
   plugin path) before declaring deploy/verify complete — per the mandatory
   build-the-binary-run-the-binary gate.
