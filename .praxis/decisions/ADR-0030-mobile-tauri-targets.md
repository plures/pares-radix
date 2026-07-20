# ADR-0030: Mobile Targets (iOS & Android) via Tauri 2 Mobile — One Core, Two New Surfaces

- **Status:** Accepted (DESIGN stage; build-matrix/scaffold/verify stages follow)
- **Date:** 2026-07-11
- **Deciders:** kbristol (strategic directive), dev-lead orchestrator
- **Relates:** ADR-0010 (agens-first / praxis-native), ADR-0024 (canonical plugin format), ADR-0029 (Tauri platform build matrix), ROADMAP Phase C
- **Invariants:** C-PLURES-003/004 (state/logic in PluresDB, IO at boundary), C-NOSTUB-001, C-TEST-001/002

---

## 1. Context

Radix is a Tauri 2 desktop shell over a shared Rust core (`crates/radix-core`) plus a
design-dojo Svelte UI, with all decision logic in `.px`/PluresDB. Mobile (iOS/Android) has only
ever been a single Phase-4 bullet ("Android/iOS via Tauri where supported") — no design, no build
matrix, no confirmation that the core is target-clean. Tauri 2 ships first-class **mobile**
support (`tauri android`/`tauri ios`), which means the same core + same Svelte UI can run natively
on both platforms **without forking logic**. This ADR makes that the plan.

The strategic point: mobile is not a new app. It is a new **surface** over the existing core. If
we honor the invariants (logic in `.px`, state in PluresDB, IO at declared adapters, UI in
design-dojo), the delta is (a) build targets, (b) a mobile shell that satisfies the frontend
bridge contract with graceful degradation, and (c) responsive UI — not a second codebase.

## 2. Decision

1. **Shared core, no fork.** `crates/radix-core` (and its PluresDB storage adapter) MUST compile
   for `aarch64-apple-ios`, `aarch64-apple-ios-sim`, and `aarch64-linux-android` (plus
   `armv7-linux-androideabi`/`x86_64` for emulators). No desktop-only syscalls in the core; any
   desktop-only concern moves behind a declared adapter with a mobile implementation or a real
   `CapabilityUnavailable` (never a stub — C-NOSTUB-001).

2. **Tauri 2 mobile shell.** Add `tauri android init` / `tauri ios init` targets. The existing
   frontend bridge contract (navigate / get_window_state / set_tray_menu / save_window_state +
   app-booted / window-state-changed / user-navigated) is honored with **graceful degradation**:
   - No system tray on mobile → `set_tray_menu` is a no-op that returns a real "unsupported on this
     platform" result the caller handles; the feature is not advertised present.
   - Window geometry persistence degrades to app-lifecycle state (foreground/background).
   - The **system back gesture/button** maps to a `user.navigated` event (back), so navigation
     stays event-driven per ADR-0010.

3. **PluresDB is a first-class mobile replica.** The storage/CRDT adapter links and syncs on both
   targets. Mobile is local-first — a full replica that syncs, not a thin remote client. This is a
   hard acceptance criterion, not a "later."

4. **Responsive design-dojo.** The Phase-B data primitives (DataGrid/SchemaForm/EntityList/…)
   adapt to touch + narrow viewports; TUI tokens are unaffected. The command palette degrades to a
   mobile action sheet. New primitives ship GUI + TUI tokens (unchanged parity bar); "mobile" is a
   responsive mode of the GUI tokens, not a third token set.

5. **Permissions at the adapter boundary.** ADR-0011 platform-capability gates
   (network/notify/storage/system) map to native iOS/Android permission prompts inside the
   declared adapter that needs them — the gate is where the IO is, consistent with the existing
   permission model. No capability is granted implicitly by being on mobile.

6. **Packaging is gated and staged.** Dev/unsigned `.apk`/`.app` first (build-the-binary gate on an
   emulator/simulator with a real captured screenshot). Signed `.ipa`/`.aab` and any store/dev-portal
   interaction is a later step and an **external side-effect requiring explicit approval** — not
   part of the default lifecycle advance.

## 3. Consequences

**Positive** — one logic base across desktop + mobile; local-first on the phone; the plugin estate
(ADR-0024) and agens interop (ADR-0031) work on mobile for free because they ride the same
CID-mediated events and PluresDB state.

**Costs/risks** — CI grows an iOS+Android build matrix (macOS runner needed for iOS); NDK/Xcode
toolchains must be pinned (drift risk → pin, per ADR-0029's build-matrix discipline); the PluresDB
adapter must be proven on-device, not assumed (mitigation: the mobile-replica acceptance test).

## 4. Acceptance criteria (verify stage)
- `cargo build` green for iOS + Android targets in CI (build the binary).
- Tauri mobile shell launches on an emulator + simulator; real screenshot captured.
- PluresDB replica writes on device and syncs to a peer (mobile-replica test passes).
- Bridge contract honored with documented degradation; back-gesture emits `user.navigated`.
- design-dojo primitives render usable on a narrow touch viewport.
- No stubs introduced; any unsupported platform feature returns a real handled error.

## 5. References
ADR-0010, ADR-0024, ADR-0029; `development-guide/design/TAURI-BEST-PRACTICES.md`;
C-PLURES-003/004, C-NOSTUB-001, C-TEST-001/002.
