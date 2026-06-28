# ADR-0029: Even/Odd Minor Release Gating + Tauri Platform Build Matrix

- **Status:** Accepted (DESIGN stage; FIX/scaffold stages follow)
- **Date:** 2026-06-27
- **Deciders:** kbristol (CTO, strategic directive + fork sign-off), dev-lead orchestrator
- **Relates:** ADR-0027 (dev-lifecycle spine wiring), automation-first practice, WORK-PRIORITIZATION (Level 0 Foundation Integrity)
- **Codifies:** `development-guide/practices/even-odd-release-versioning.md`, `development-guide/design/TAURI-BEST-PRACTICES.md`

---

## 1. Context

The plures org centralizes CI/release in reusable workflows under `plures/.github`
(`ci-reusable.yml`, `release-reusable.yml`). The release reusable handles version
bump + changelog + tag + GitHub Release + registry publish (npm/cargo/jsr/docker/vscode)
but **builds no platform desktop/mobile binaries**. `pares-radix`'s `auto-version.yml`
blind-minor-bumps on any `feat:` commit and triggers a release unconditionally — no
separation between noisy mid-development churn and stable, user-facing releases, and no
gating to keep expensive multi-OS builds from firing on every commit.

kbristol's directive: ship platform releases (Windows, Linux/Flatpak, NixOS, macOS,
Android, iOS) on **even** minor increments; hold Android/iOS until a build-caching
solution proves out (mobile builds historically time out compiling/downloading the full
native toolchain); research + apply Tauri best practices; codify a deterministic
versioning process org-wide.

A material finding shaped this ADR: **the Tauri desktop shell does not exist yet.** Tauri 2
deps are declared in `[workspace.dependencies]` and a frontend bridge
(`src/lib/platform/tauri.ts`) defines the command/event contract, but there is **no
`src-tauri/` scaffold in git** — no `tauri.conf.json`, no `lib.rs`, no crate consuming the
deps. The ROADMAP's "Tauri 2 desktop shell port completed" is inaccurate. "Build platform
releases" therefore has a hard prerequisite: build the Tauri app, for real.

## 2. Decision

### 2.1 Versioning: parity of the minor encodes the line type (deterministic)
- **Odd minor = development / pre-release line** (e.g. 1.51.x). Active feature dev. GitHub
  tags = pre-release. Registry publish = yes, as pre-release (`--tag next` / `-pre`).
  **Platform installers = NO.**
- **Even minor = stable release line** (e.g. 1.52.x). Bug fixes + security only. GitHub tags
  = full release. Registry publish = stable/latest. **Platform installers = YES** (subject to
  per-repo metadata + caching health).

Transitions (Conventional-Commit driven, full table in the practice doc):
feature-complete **promotion** odd→next even `.0` is gated behind an explicit
`release(scope):` commit or `workflow_dispatch promote` (NOT auto-on-every-feat, so
multi-OS CI never fires mid-dev); `fix:`/security on an even line → patch bump + stable
release; next-epic `feat:` after an even line → next odd `.0`; routine dev → patch within
the odd line. CI guard rails: never regress; never build installers off an odd line; an
**even line rejects `feat:`** with an actionable error; promote only from odd; pre-release
never tagged `latest`. This replaces `auto-version.yml`'s unconditional bump-on-feat.

### 2.2 Platform builds live in a NEW reusable, gated on parity + metadata
Tauri bundling (per-OS runners, Rust+WebView toolchains) is fundamentally different from
the ubuntu-only registry publish jobs, so it gets its own reusable
**`tauri-release-reusable.yml`** in `plures/.github`, invoked **only on even-minor tags**.
The existing `release-reusable.yml` continues to own registry publishing on every release.

A single condition gates every platform stage:
`is_even(minor) AND publishPlatformInstallers AND target ∈ platformTargets AND caching_healthy`.

### 2.3 Per-repo opt-in via `.plures/release.toml`
Every repo carries `.plures/release.toml` (`publishPlatformInstallers` bool,
`platformTargets` array over `["windows","macos","linux-flatpak","nixos","android","ios"]`,
plus caching flags). The **same** reusable CI runs everywhere; platform stages auto-skip
when installers aren't requested or a target is absent. Foundation/library repos
(pluresdb, praxis, pedantic-rs) set `false`/`[]` and stay registry-only, but **still obey the
even/odd parity convention**. `pares-radix` starts with
`platformTargets=["windows","macos","linux-flatpak","nixos"]`; android/ios added after
caching is proven and Apple creds exist.

### 2.4 Tauri scaffold is built for real (no stubs — C-NOSTUB-001)
A real `src-tauri/` implements the contract the frontend already calls: commands
`navigate`/`get_window_state`/`set_tray_menu`/`save_window_state`; events
`app-booted`/`window-state-changed`/`user-navigated`; `tray-icon`; window geometry persisted
to a real store (survives restart, not an in-memory map). Binding facts from the research:
`frontendDist = ../build` (SvelteKit adapter-static writes `build/`), `identifier` set once and
never changed (changing it orphans the updater + installs), CSP never `null`, v2 ACL
capabilities (least-privilege `core:*` per window) replace the v1 allowlist.

### 2.5 Caching uses internal tooling first, measured
Per Decision B: prefer **pares-cache** (our Nix substituter) for Nix/NixOS targets; layer
`Swatinem/rust-cache@v2` (per-OS, per-target-triple keys) for the Rust app crate and
Gradle/SDK-NDK/CocoaPods/SwiftPM caches for mobile. Caching is only "done" when it
**persists across builds, is accessible on subsequent builds, and shows a measurable,
tracked build-time improvement** (`trackBuildMetrics=true`). The app crate stays thin
(events-not-commands) so `rust-cache` — which by design recaches only deps, never the top
crate — yields maximum benefit.

### 2.6 Target-runner mapping (no cross-compiling bundles)
windows-latest → nsis+msi; macos-latest → dmg (both arches) **+ iOS**; ubuntu-22.04 →
deb/rpm/appimage **+ Flatpak + Android**. Flatpak is a post-step (`flatpak-builder`), not a
Tauri bundle target. **NixOS ships as a flake package, not an installer format** (built +
pushed to pares-cache). Ubuntu pinned to old glibc for portable `.deb`/`.AppImage`.

### 2.7 Updater channels map to parity
Two manifests: `latest.json` (even/stable) and `latest-prerelease.json` (odd/prerelease).
A running build picks its feed by its own minor parity. Updater signing
(`TAURI_SIGNING_PRIVATE_KEY`) is mandatory and **separate** from OS code signing; all OS
signing is **skip-if-secret-absent** so forks/PRs stay green.

## 3. Consequences

**Positive:** deterministic, automation-first releases; noisy dev separated from stable
user-facing releases; multi-OS CI cost controlled (installers only on even lines, mobile
gated on proven caching); one org-wide pipeline + a 1-file per-repo opt-in; an honest,
working Tauri app instead of an aspirational ROADMAP claim.

**Costs / blockers:**
- **iOS/macOS signing needs a paid Apple Developer cert + macOS runner** — hard external
  blocker for notarized macOS + distributable iOS (pending kbristol). Desktop unsigned +
  Linux/Windows/NixOS proceed without it.
- Mobile is the expensive, flaky part (20–40+ min cold) — release-only, ABI-trimmed on PRs,
  raised step timeouts, and gated behind caching health.
- Building the Tauri shell is real net-new work (Stage 1), not just CI wiring.

**Neutral:** ROADMAP must be corrected to reflect that the desktop shell is being built now.

## 4. Execution (staged, gated)
Stage 0 research + this ADR + dev-guide codification (lands on odd 1.51.x — allowed) →
Stage 1 real `src-tauri/` scaffold (local `pnpm tauri build` Windows installer + `cargo build`
green) → Stage 2 caching layer with tracked metrics → Stage 3 `tauri-release-reusable.yml`
desktop matrix (Win/macOS/Flatpak/NixOS), even-minor-gated, pares-radix opt-in → Stage 4
mobile (android/ios) after caching proven + Apple creds → Stage 5 verify (real draft/pre-release
artifact set; confirm odd does NOT trigger installers) + ROADMAP truth + back-brief. Each
stage gates the next; no platform-release workflow merges to an even line.
