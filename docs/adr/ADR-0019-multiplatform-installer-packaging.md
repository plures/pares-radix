# ADR-0019: Multiplatform Installer Packaging (Design Stage)

- Status: Proposed (design only — no implementation in this change)
- Date: 2026-07-23
- Supersedes: none
- Related: ROADMAP.md Phase 4 ("Multi-arch packaging"), `design/TAURI-BEST-PRACTICES.md`
  (development-guide), `.github/workflows/release.yml`, `plures/.github` reusable
  `release-reusable.yml`

## Context

pares-radix is a **svelte-Tauri** application (GUI + Svelte-TUI + svelte-ratatui + MCP,
<<<<<<< HEAD
no traditional CLI — see `development-guide/design/PLURES-FOUNDATION.md` → "svelte-Tauri Application Template").
=======
no traditional CLI — see `PLURES-FOUNDATION.md` → "svelte-Tauri Application Template").
>>>>>>> origin/main
ROADMAP Phase 4 calls for "Multi-arch packaging (Linux, Windows, macOS; Android/iOS via
Tauri where supported)". Today:

- `src-tauri/` is scaffolded (`tauri.conf.json`, `capabilities/`, icons for desktop +
  Android) and implements the frontend bridge contract (`navigate`,
  `get_window_state`, `set_tray_menu`, `save_window_state` + events).
- The Windows installer (NSIS + MSI) has been **built locally** but is not yet wired
  into CI (ROADMAP "Current State").
- `.github/workflows/release.yml` delegates all version/tag/changelog/publish logic to
  the org-shared `plures/.github/.github/workflows/release-reusable.yml`; it carries
  **no Tauri bundle matrix today** — no per-OS build job builds/signs/publishes
  installers.
- No `.deb`/`.rpm`/`.AppImage`/`.dmg`/APK artifacts exist yet anywhere in the repo or
  workflow definitions. iOS/Android `gen/` scaffolds are not present (only icon assets).
- `design/TAURI-BEST-PRACTICES.md` (development-guide, verified 2026-06-27 against
  v2.tauri.app) is the authoritative source for Tauri 2 packaging mechanics already
  evaluated for this repo; this ADR is the pares-radix-local decision record that
  consumes it and turns it into a concrete release-pipeline plan.

This is a **design-stage** ADR per the dev-lifecycle/dev-guide gates: it defines the
target/form-factor matrix, the cross-platform shell/bundling choice with evidence, and
the CI/release implications, and enumerates real validation paths. It intentionally
ships **no installer artifacts, no placeholder CI jobs, and no stub code** — only the
decision record and the acceptance criteria a follow-on implementation task must meet.

## Decision drivers

1. **No traditional CLI, no cross-compiled bundles.** A Tauri bundle can only be built
   on its own host OS (Tauri's bundler shells out to platform-native tooling — NSIS/
   WiX on Windows, `hdiutil`/codesign on macOS, `dpkg`/`rpmbuild`/`appimagetool` on
   Linux). There is no cross-compile path for installer formats themselves (Rust
   cross-compiles; the packaging step does not).
2. **Evented, thin Rust core** (ADR: events-not-commands, already established). The
   packaging decision must not require adding business logic to `src-tauri`.
3. **Even/odd release parity law** already governs the version/publish pipeline
   (`release-reusable.yml`); installer publishing must slot into that model
   (stable vs prerelease update feeds), not invent a second one.
4. **Cost/complexity gating is real**: Apple and Android signing require paid
   developer accounts, hardware/cert infrastructure, and dedicated runners. The
   design must explicitly gate what ships unsigned vs signed, and what is
   PR-fast vs release-only.

## Cross-platform shell choice — evaluated with evidence

Three shell options were evaluated for pares-radix specifically:

| Option | Verdict | Evidence |
|---|---|---|
| **Tauri 2** (current) | **Selected — no change** | Already the scaffolded shell (`src-tauri/`, Tauri 2 crates in `Cargo.toml`, bridge contract implemented per `TAURI-BEST-PRACTICES.md`). Uses the OS webview (WebView2/WebKitGTK/WKWebView) — no bundled Chromium, smallest binary footprint of the three, matches the "svelte-Tauri Application Template" org mandate (`PLURES-FOUNDATION.md`) that **every** new Plures app scaffolds from `svelte-tauri-template`. Supports desktop (Win/macOS/Linux) + mobile (Android/iOS) from one codebase, matching ROADMAP Phase 4 scope without a rewrite. |
| **Electron** | Rejected | Would require replacing the entire `src-tauri` Rust core and bridge (`tauri.ts` events-not-commands contract), duplicate work already done and already an org mandate violation (svelte-tauri-template is prescribed org-wide). Bundles a full Chromium+Node runtime per install (100+ MB baseline vs Tauri's OS-webview reuse) — directly opposed to the size-optimization work already landed in `TAURI-BEST-PRACTICES.md` §2. No mobile story without a second stack (Capacitor/React Native), doubling the CI matrix. No evidence of any org repo using Electron; would be a net-new, unsupported pattern. |
| **Native per-platform (WinUI/SwiftUI/GTK)** | Rejected | 3x the UI codebase (no shared Svelte GUI/TUI), abandons the already-built Svelte GUI + `svelte-ratatui` TUI parity goal in ROADMAP Phase 4 ("Svelte GUI parity with Svelte TUI"). No shared bridge/event model; would need bespoke IPC per platform. No mobile reuse. Highest total engineering cost of the three for a single-maintainer-scale project. |

**Conclusion: keep Tauri 2.** No shell migration is in scope. The remaining design
work is entirely the **packaging matrix and release pipeline**, not the app shell.

## Target / form-factor packaging matrix

| Target | Bundle format(s) | Build host | Gate | Signing | Update channel |
|---|---|---|---|---|---|
| Windows x64 desktop | `.exe` (NSIS) + `.msi` | `windows-latest` | PR: build unsigned; Release: signed | Authenticode via Azure Trusted Signing (preferred over hardware EV — no HSM on hosted runners) | `latest.json` (even/stable) or `latest-prerelease.json` (odd) |
| macOS (Apple Silicon) desktop | `.app` + `.dmg` | `macos-latest` | PR: ad-hoc signed (`signingIdentity: "-"`) fast build; Release: signed+notarized | Developer ID Application cert + `codesign` + `notarytool` (secrets-present gated) | same as above |
| macOS (Intel) desktop | `.app` + `.dmg` (`--target x86_64-apple-darwin`) | `macos-latest` (cross-arch build on Apple Silicon runner) | Release only (cost: doubles Apple build time) | Same Developer ID cert as Silicon | same as above |
| Linux x64 desktop | `.deb` + `.AppImage` (+ `.rpm` if a fedora/rhel consumer is confirmed) | `ubuntu-22.04` pinned (oldest supported glibc/webkit2gtk — **not** `ubuntu-latest`) | PR: build unsigned; Release: build + publish | No OS-level signing concept; updater `.sig` is the integrity mechanism | same as above |
| Linux Flatpak | `org.plures.Radix` Flatpak | `ubuntu-22.04`, separate `flatpak-builder` step *after* the `.deb`/binary is built (Flatpak is not a native `bundle.targets` value) | Release only, opt-in | Flatpak repo signing (deferred — not required for first ship) | Flatpak's own update mechanism (separate from Tauri updater) |
| NixOS | Flake package (`packages.<system>.pares-radix`) | N/A — consumer builds/runs via `nix build`/`nix run` from the flake | Release only; validated by `nix flake check` / `nix build` in CI, not a Tauri "bundle target" | N/A (Nix store hash is the integrity mechanism) | Flake input pin, not the Tauri updater |
| Android APK/AAB | `.apk` (sideload/testing) + `.aab` (Play Store) | `ubuntu-latest` (Gradle-driven, via `src-tauri/gen/android` which does not exist yet) | **Release/tag-only**, label-gated; not on every PR (ROADMAP + `TAURI-BEST-PRACTICES.md` §7: cold Gradle+NDK build is 20–40+ min) | Java keystore (`ANDROID_KEY_ALIAS`/`ANDROID_KEY_PASSWORD`/`ANDROID_KEY_BASE64`), secrets-present gated | Play Store internal track initially; Tauri updater not typically used for mobile |
| iOS `.ipa` | App Store / TestFlight | `macos-latest`, full Xcode (not just CLT); via `src-tauri/gen/apple` (does not exist yet) | **Release/tag-only**, label-gated | Apple Distribution cert + provisioning profile (App Store Connect API key), secrets-present gated | TestFlight / App Store review, not the Tauri updater |
| GUI (Svelte, existing) | N/A — bundled inside every desktop/mobile target above | — | — | — | — |
| TUI (svelte-ratatui) | Ships as part of the same binary (render-mode switch, not a separate installer) | same as desktop targets | Covered by the same builds | Same as parent binary | Same as parent binary |
| Headless / MCP server (`packages/mcp-dev-server`) | **No installer** — stdio JSON-RPC process, distributed as an npm/pnpm package or a thin binary via `cargo install`/CI artifact, not a Tauri bundle | Existing Node/Cargo build, no new host requirement | Already covered by existing `ci.yml`; out of scope for this ADR's installer matrix (tracked separately if a standalone headless distributable is ever requested) | N/A | npm registry versioning |

**PR-fast lane vs release-heavy lane** (mirrors `TAURI-BEST-PRACTICES.md` §8 matrix
recommendation, now scoped to pares-radix's actual repo state):

- **PR / CI build**: Windows (unsigned), macOS Silicon-only (ad-hoc signed), Linux
  `ubuntu-22.04` (deb+AppImage). No LTO profile. No mobile. Fast feedback.
- **Release / publish** (triggered by `release.yml` → `release-reusable.yml`, on
  `main` push / `v*` tag / `promote` dispatch): full desktop matrix (both macOS
  arches), Flatpak, size-optimized `[profile.release]`, updater artifacts +
  `latest.json`/`latest-prerelease.json` split by even/odd minor parity (existing
  law — no new versioning scheme introduced). Mobile (Android + iOS) gated behind a
  release label/secrets-present check, **not** run on every release by default given
  cost/flakiness (`TAURI-BEST-PRACTICES.md` §7 cold-build evidence).

## CI / release pipeline implications

1. **New workflow surface needed** (implementation task, not this ADR): a
   `platform-release` job matrix in (or called from) `.github/workflows/release.yml`,
   invoked *after* `release-reusable.yml` cuts the version/tag, so installers are
   built against the tagged commit/version — avoiding a race between version bump
   and bundle version embedding (`tauri.conf.json` version should be sourced from
   `Cargo.toml`, itself set by the reusable pipeline).
2. **Caching is mandatory before this is viable in CI wall-clock/cost terms**:
   `Swatinem/rust-cache@v2` per-OS-per-target-triple, pnpm store cache, and (for the
   deferred mobile lane) Gradle/NDK and CocoaPods/SwiftPM caches — all specified in
   `TAURI-BEST-PRACTICES.md` §3/§7. Without these, a full matrix run cold is
   30–60+ minutes and mobile alone can eat the default job timeout.
3. **Secrets-present gating, not hard failures**: every signing step (Windows Azure
   Trusted Signing, macOS cert+notarize, Android keystore, iOS Distribution cert)
   must be conditional on the relevant secret existing, so forked/PR builds still
   produce runnable (unsigned/ad-hoc-signed) artifacts.
4. **`bundle.targets` must be host-scoped explicitly** in `tauri.conf.json` (or an
   env-driven override) — `"all"` on the wrong OS silently fails or produces nothing;
   the design specifies per-runner explicit target lists (matrix table above), not a
   blanket `"all"`.
5. **Flatpak and NixOS are not Tauri `bundle.targets`** — they are post-processing
   steps consuming the already-built Linux binary/`.deb`. They must be modeled as
   separate CI steps/jobs, not folded into the Tauri bundler invocation.
6. **No `src-tauri/gen/android` or `gen/apple` exist yet.** Mobile packaging cannot
   start until `pnpm tauri android init` / `pnpm tauri ios init` are run and reviewed
   in a follow-on implementation PR — that scaffolding is itself implementation, out
   of scope here.
7. **Update manifest ownership**: `latest.json` / `latest-prerelease.json` generation
   and publish must be added to the release job, mapped to the existing even=stable /
   odd=prerelease parity law already enforced by `release-reusable.yml` — no new
   channel concept.

## Real validation paths (no placeholders)

Concrete, checkable acceptance criteria for the implementation task that follows this
ADR — each must be independently verifiable, not asserted:

1. **Windows**: `pnpm tauri build` on a `windows-latest` runner produces both a
   `.exe` (NSIS) and `.msi` under `src-tauri/target/release/bundle/`; the artifact
   installs (`Start-Process -Wait` silent install flag) and the resulting
   `pares-radix.exe` launches and emits `app-booted` (verified via existing Playwright
   e2e harness pointed at the installed binary, not the dev server).
2. **macOS**: `pnpm tauri build --target aarch64-apple-darwin` produces a `.dmg`
   whose mounted `.app` passes `spctl --assess` (ad-hoc: expected-fail with a clear
   "unsigned" reason on PR builds; expected-pass on signed release builds) and
   launches headless via `open -W`.
3. **Linux**: the `.deb` installs cleanly in a fresh `ubuntu:22.04` container
   (`dpkg -i` + `apt-get install -f`) with no missing shared-library errors, and the
   `.AppImage` runs directly (`chmod +x && ./*.AppImage --appimage-extract-and-run`)
   in a container with **no** desktop environment beyond the required webkit2gtk
   deps, proving the dependency list in `TAURI-BEST-PRACTICES.md` §8 is complete.
4. **Flatpak**: `flatpak-builder --repo=repo build-dir org.plures.Radix.yml` succeeds
   and `flatpak run org.plures.Radix` (from the local repo) launches the app.
5. **NixOS**: `nix build .#pares-radix` succeeds from a clean `nix build` (no
   pre-warmed cache) and `nix run .#pares-radix` launches the binary; `nix flake
   check` passes.
6. **Updater manifest**: a scripted round-trip — build two versions (N, N+1) with the
   updater keys, publish `latest.json` for N, point a running N binary at a local
   static server serving that manifest, and confirm the updater flow (per
   `tauri-plugin-updater` docs) reports an update available and installs N+1 without
   manual file surgery.
7. **Mobile (deferred, tag/release-only)**: after `gen/android`/`gen/apple` scaffolds
   land in a follow-on PR, validation is: `pnpm tauri android build --apk` installs
   on an emulator/device via `adb install` and boots to the same `app-booted` event;
   equivalent for iOS via `xcrun simctl install`/`launch` on a simulator. Not
   required for this ADR's acceptance — captured here so the follow-on task inherits
   a concrete bar instead of "make mobile work."

None of the above are satisfied by a CI job merely *existing* or by a bundler command
*exiting zero* with an empty output directory — each requires the artifact to actually
install/run and be functionally probed, per the org's C-NOSTUB-001 no-stub / no-ghost-
binary constraint already cited in `TAURI-BEST-PRACTICES.md`.

## Consequences

- **Positive**: single shell (Tauri) across every target keeps the GUI/TUI parity
  goal, the events-not-commands Rust core, and the org svelte-tauri-template mandate
  intact — zero migration risk. The matrix makes explicit what is PR-fast vs
  release-only vs deferred, preventing an accidental "build everything on every PR"
  cost blowup.
- **Negative / risk**: macOS Intel doubles macOS CI time; mobile is explicitly
  deferred (no Android/iOS scaffolds exist yet), so ROADMAP Phase 4's "Android/iOS
  via Tauri where supported" is only partially addressed by the first
  implementation wave — this ADR records that gap rather than papering over it with
  a placeholder mobile job.
- **Follow-on work** (tracked separately, NOT part of this design-only ADR):
  1. Add the `platform-release` CI matrix (Windows/macOS/Linux desktop only) with
     caching + secrets-gated signing.
  2. Wire the updater manifest publish step into `release-reusable.yml` consumption.
  3. Add the Flatpak post-build step.
  4. Add the NixOS flake package + `nix flake check` CI job.
  5. Scaffold `src-tauri/gen/android` + `gen/apple`, then add the tag/release-only
     mobile jobs per the deferred validation criteria above.

## References

- `design/TAURI-BEST-PRACTICES.md` (development-guide repo, verified 2026-06-27)
- `ROADMAP.md` Phase 4
- `.github/workflows/release.yml`, `plures/.github/.github/workflows/release-reusable.yml`
- Tauri 2 docs: https://v2.tauri.app (configuration, size, updater, signing, pipelines)
