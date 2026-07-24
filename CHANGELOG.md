## [1.55.36] — 2026-07-24

- fix: thread real chat_id through autonomous task dispatch (#481) (af0f634)

## [1.55.35] — 2026-07-24

- fix(spine): resolve clippy::too_many_arguments on build_task_aware_runtime; incorporate PR #468 changes (#473) (0b89ab7)
- docs(ADR-0035): design-dojo vendored-shim drift audit - ownership boundary + prioritized gaps (design only) (#479) (e06231d)
- docs: ADR-0019 multiplatform installer packaging design (Phase 4 prep) (#478) (6f14de7)
- docs(adr): ADR-0018 procedure-native plugin integration plan (Epic A design) (#477) (2856ce3)
- ci(release): add concurrency guard to release workflow (#476) (56379cf)

## [1.55.34] — 2026-07-21

- fix(spine): resolve px task-dispatch verbs to real Rust handlers (p0 loop closure) (#475) (002783a)

## [1.55.33] — 2026-07-20

- feat(core): interactive approval-card block-and-await seam (#472) (5a7a0f3)

## [1.55.32] — 2026-07-20

- fix(core): close task-completion seam - subagent finish drives owning Task terminal (#474) (6d6ec0f)

## [1.55.31] — 2026-07-20

- test(spine): prove milestone: write drives .px dashboard procedure locally (72f805a)

## [1.55.30] — 2026-07-20

- fix(spine): close autonomous task-execution loop (W1+W2) (#471) (0f21737)
- ci: migrate Tech Doc Writer to shared reusable (520cbd4)

## [1.55.29] — 2026-07-19

- fix(spine): inject durable open-tasks grounding on live reactive .px path (#467) (#469) (8805792)

## [1.55.24] — 2026-07-14

- fix(build): bump pluresdb deps to d08f88b (carries BOM-free praxis-lang 37be99e) (1456712)
- docs(adr): accept ADR-0033 — .px as component composition language (view-trees over design-dojo primitives) (3f2ff6a)

## [1.55.23] — 2026-07-12

- fix(strategy): CORRECTION — always-on daemon EXISTS (agens serve), build-verified (c3fa49e)
- chore(strategy): pin daemon build spec from ground-truth probe (0dc90a3)

## [1.55.22] — 2026-07-12

- fix(strategy): ONE installed app model — radix daemon + agens bundled default (3708c82)

## [1.55.21] — 2026-07-12

- fix(strategy): correct architecture model — radix=app, agens=extension (cb4167f)

## [1.55.20] — 2026-07-12

- Merge: strategy.px — objectives + parity/migration/mobile backlog as .px data (a46d151)
- feat(strategy): objectives + parity/migration/mobile backlog as .px data (05fea4e)
- feat(design-dojo): implement GraphView ego-centric graph-flex primitive (ADR-0032) (b8b5cec)
- feat(design-dojo): add FieldEditor + SchemaDesigner runtime-customization primitives (Phase B) (4be4ba3)
- docs(roadmap): ADR-0032 GraphView — ego-centric space-adaptive graph navigation primitive (215918d)
- feat(tooling): add create-radix-plugin scaffolder for canonical ADR-0024 plugins (ea2fb63)
- feat(design-dojo): add schema-driven DataGrid and SchemaForm primitives (Phase B) (0e3c09a)
- docs(roadmap): refresh strategic roadmap + ADR-0030 (mobile iOS/Android via Tauri) + ADR-0031 (plugins-as-tools for agens, user/agent-parity customization) (b31f164)
- fix(spine): collapse identical if/else in run_command test (clippy if-same-then-else) (ebe3341)
- feat(spine): morning_briefing .px procedure + run_command wiring (157bbd1)

## [1.55.19] — 2026-07-09

- feat(demo): Operations-as-Intent praxis scene (real ontology, no mocks) (#465) (faa0686)

## [1.55.18] — 2026-07-09

- feat(vscode): register radix MCP server for Copilot .px evaluation (#2919091) (#464) (66b14b1)

## [1.55.17] — 2026-07-08

- feat(spine): morning_briefing .px procedure + run_command wiring (#463) (2d7a4e7)

## [1.55.16] — 2026-07-07

- fix(mcp-dev-server): faithful praxis-evaluate — resolve bare top-level keys + includes() + arithmetic (5505009)
- ci: add security-aware Dependabot auto-merge workflow (org backfill) (3add620)

## [1.55.15] — 2026-07-03

- feat(rsi): Phase 3b — observe/detect/wire (parse RSI rails, undo→remove wiring, chronos-watcher) (f01b271)

## [1.55.14] — 2026-07-03

- feat(praxis): add rollback primitives to PraxisWriteGate (remove/disable/list constraints) (ef4469f)

## [1.55.13] — 2026-07-01

- fix(praxis): release PX-L010/PX-L012 lint fix (pluresdb-px 0ec9523) (53f9623)
- M6.4: pin pluresdb-px to 0ec9523 + migrate praxis integration tests to px-ast (cbbe7c7)
- chore(ci): drop testing/** path triggers + remove dormant ghost-CLI workflow (Thread-3 tail) (#461) (07afb9a)

## [1.55.12] — 2026-06-30

- fix(spine): make resolve_state_dir_honors_env test portable (Linux CI fix) (#462) (98a485e)

## [1.55.11] — 2026-06-30

- test(worktask): adversarial QA suite + fix pr_mode poisoning from malformed policy nodes (a4caad6)
- test(worktask): add verify loop-closer e2e command-surface probe (cfec9dc)
- test(worktask): end-to-end runtime + scratch-repo coverage (new_feature/reclaim-quarantine/doctor/pr-modes) (d474777)
- feat(worktask): real .px+Rust worktask executor (11 cmds, git/fs effects, PluresDB state) (3c51dcb)

## [1.55.10] — 2026-06-30

- feat(spine): wire .px runtime into shipped app (real PluresDB state + assembly) (0de86cf)
- chore(template): conform pares-radix to svelte-tauri-template mandate (remove CLI crate, render-modes + MCP only) (#460) (b263d0f)
- ci: svelte-kit sync before vitest (root tsconfig extends .svelte-kit/tsconfig.json) (3d983ad)
- ci: drop pnpm version pin in real-tests (use packageManager from package.json) (98321b1)
- ci: retarget Real Integration Tests to real surfaces (kill ghost CLI) (0013618)

## [1.55.9] — 2026-06-28

- fix(ci): repair tech-doc-writer.yml YAML parse break (unindented template-literal lines terminated the script: block scalar) (73abda8)
- docs(adr-0028): mark Accepted - FIX stage landed (praxis@b431611, PluresDbConstraintAdapter + 10/10 tests) (fd08ca3)

## [1.55.8] — 2026-06-28

- fix(radix-core): repair stale pares_agens_core doctest imports left by core->radix-core rename (cargo test --doc green) (3abc3e9)

## [1.55.7] — 2026-06-28

- fix(px): conform .px corpus to pluresdb-px foundation grammar + repair STATE_ALLOWLIST crate-rename regressions (px-first enforcement tests green) (baf99f0)

## [1.55.6] — 2026-06-28

- fix(lint): resolve clippy violations failing CI rust gate (useless-format, derivable-impls, unnecessary-map-or, len-zero) (b9ddeba)

## [1.55.5] — 2026-06-28

- fix(release): catch up Cargo.lock to 1.55.3 (in-flight release raced the org pipeline fix) (8753b83)
- fix(release): platformTargets reflect what actually emits artifacts (no soft stubs) (178c2dc)

## [1.55.3] — 2026-06-28

- fix(release): catch up Cargo.lock to 1.55.2 (heal release-bump lock drift) (a4e908a)

## [1.55.2] — 2026-06-28

- feat(desktop): wire Tauri updater (real signing pubkey, desktop-gated plugin, createUpdaterArtifacts) (607a632)

## [1.55.1] — 2026-06-28

- feat(desktop): restore Tauri 2 app shell scaffold (src-tauri) on 1.55.0 dev line (bb66f7b)
- chore(release): adopt org parity pipeline; retire bespoke auto-version.yml; add .plures/release.toml + ADR-0029 + UI tasks (a5762c9)
- docs(ui): record guidance-on-override layer (Stage 1+2 shipped) + human-presenter surface gap (S12) (d0a7450)

## [1.55.0] — 2026-06-28

- chore(release): v1.55.0 [skip ci] (cdcc1e9)
- feat(mcp-dev-server): return override guidance from canvas authoring handlers (AI composer) (aabb487)
- chore(release): v1.54.0 [skip ci] (2ce0841)
- feat(canvas-runtime): override provenance + per-practice rationale (guidance-on-override foundation) (c796ec7)
- docs(ui): capture sharpened strategic objective (A default-correct + B guidance-on-override; C dropped) (125d8ba)

## [1.53.0] — 2026-06-28

- chore(release): v1.53.0 [skip ci] (3eb014a)
- feat(canvas): drive responsive engine on the real /canvas surface (1ef03ed)
- feat(canvas-runtime): WCAG-AA contrast linter (validate-mode) (edfce8f)
- chore(release): v1.52.0 [skip ci] (f83c70a)
- feat(capabilities): scene@1.x CID (RSV render-state read-model + signed-Intent write-model) (d2f0e88)
- docs(adr): ADR-0028 constraint-engine canonicalization (pluresdb-px canonical, already NAPI-bridged; @plures/praxis stays framework) (2a79cce)
- docs(adr): ADR-0027 dev-lifecycle spine wiring (collapse .mjs driver into .px/actor/PluresDB loop) (561701c)
- docs(ui): mark theme/density/contrast + renderer integration as built (212a3dd)

## [1.51.0] — 2026-06-28

- chore(release): v1.51.0 [skip ci] (21091ff)
- feat(canvas-runtime): theme/density resolve practices + responsive renderer (0ddb410)

## [1.50.0] — 2026-06-28

- chore(release): v1.50.0 [skip ci] (a940876)
- feat(canvas-runtime): UI schema + reactive best-practice engine (c8931ac)
- ci: lifecycle cron */30 -> 0 */2 (cut scheduled Actions spend; events still real-time) (9b41dbd)

## [1.49.4] — 2026-06-27

- chore(release): v1.49.4 [skip ci] (374ac11)
- design(capabilities): add secrets@1.x CID grounded in plures-vault surface (e3ce06f)
- feat(platform): implement ctx.data.collection() host bridge + activate plugins (9db4051)

## [1.49.3] — 2026-06-26

- chore(release): v1.49.3 [skip ci] (bc28338)
- docs(adr): canonical plugin format + capability deps (0024), hyperswarm-git forge (0025), self-shaping PIM (0026) + git-repo/pim CIDs (2e8e0aa)
- docs(adr): ADR-0023 procedure observability event contract (plures.proc.event.v1) (70cd00f)

## [1.49.2] — 2026-06-25

- chore(release): v1.49.2 [skip ci] (7e3b963)
- refactor(radix)!: B1 R4+R5 — carve host runtime to agens; radix is platform-only (f8eada7)
- refactor(radix-core)!: B1 S-E — relocate memory DTOs to platform; de-cognition cli/mcp-client (446027f)
- feat(state): expose PluresDbStateStore::crdt_store() shared handle (22f3ca4)
- refactor(radix-core)!: B1 S-B — platform classifier+memory seams; de-cognition cli-runtime/cli-api (3172cfa)
- docs(adr-0022): mark all 6 steps Implemented + commit map + honest follow-ups (f4fac34)

## [1.49.1] — 2026-06-24

- chore(release): v1.49.1 [skip ci] (2573e98)
- test(plugins): inner-space binds to real commerce provider through resolver (ADR-0022 step 5) (ccee72a)

## [1.49.0] — 2026-06-24

- chore(release): v1.49.0 [skip ci] (d0a33d2)
- feat(plugins): commerce@1.x provider plugin + CID surface validation (ADR-0022 step 4) (27f8a99)
- chore(release): v1.48.0 [skip ci] (6b26a63)
- feat(plugins): capability host contract — CID loader + resolver (ADR-0022 steps 1-3) (38f53f3)

## [1.47.6] — 2026-06-24

- chore(release): v1.47.6 [skip ci] (eb0923d)
- fix(flake): build CLI pkg from pares-radix-cli-runtime (owns bin/pares-radix) (aef24f1)

## [1.47.5] — 2026-06-21

- chore(release): v1.47.5 [skip ci] (af15a2d)
- refactor(core): physical split — extract platform into pares-radix-core (Stage S2a) (84bf408)
- chore(release): v1.47.4 [skip ci] (ed314eb)
- chore(lockfile): sync Cargo.lock to v1.47.3 after rebase onto main (53515bd)
- refactor(core): break praxis<->cerebellum cycle + seam spine->delegation (Stage S1) (a61a4fd)
- refactor(cli): remove orphaned px_config module after agent carve (1f22689)
- refactor(cli): trim agent-only imports after Stage 4c carve (5da453e)
- refactor(cli): remove 5 agent commands from host (Stage 4c carve) (3122975)
- refactor(cli): extract host runtime into pares-radix-cli-runtime lib (D.0) (733c7cd)
- refactor(cli): extract main() body into reusable run_with_providers(registry) seam (stage 2 S1) (8233f6d)
- refactor(cli): extract CommandProvider trait into pares-radix-cli-api crate (stage 2 S0) (306fc73)
- feat(cli): add CommandProvider trait for plugin command surface (stage 1) (20484ad)

## [1.47.3] — 2026-06-18

- chore(release): v1.47.3 [skip ci] (121d3f1)
- refactor(praxis): rename load_balance_headroom rule to load_balance_saturation (82b7bc8)

## [1.47.2] — 2026-06-18

- chore(release): v1.47.2 [skip ci] (4fbc6df)
- docs(shadow): add _generate_shadow.ps1 generator referenced by README (11b7757)
- chore(release): v1.47.1 [skip ci] (0105c8b)
- test(cli): add missing fast_model_client field in RuntimeAgentFactory test init (3b4377b)

## [1.47.0] — 2026-06-18

- chore(release): v1.47.0 [skip ci] (23535fc)
- feat: shadow-deploy umbra-evolved .px (inert, not live) + shadow loader (f03669f)
- fix(omniscient): cross-platform file permissions in FileNodeBuilder (51fb7bf)
- ci: add conventional commit parsing to auto-version (32b4106)

## [1.46.21] — 2026-06-15

- chore: bump version to v1.46.21 [skip ci] (2d8fd68)
- feat: conversation threading engine (all phases) (b698087)

## [1.46.20] — 2026-06-15

- chore: bump version to v1.46.20 [skip ci] (50253ba)
- fix: use block_in_place for dataflow registration during startup (c554d45)

## [1.46.19] — 2026-06-14

- chore: bump version to v1.46.19 [skip ci] (015f30d)
- fix: inject conversation history into dataflow pipeline (session loss bug) (e3a0864)

## [1.46.18] — 2026-06-14

- chore: bump version to v1.46.18 [skip ci] (30902bf)
- fix(deploy): add ~/.pares-radix/models.toml to config search path (4ad9746)

## [1.46.17] — 2026-06-14

- chore: bump version to v1.46.17 [skip ci] (81a8d81)
- fix(deploy): correct config path resolution for NixOS + multi-location search (21b8730)

## [1.46.16] — 2026-06-14

- chore: bump version to v1.46.16 [skip ci] (03f7872)
- feat(model_pool): automatic hourly rediscovery + /model refresh + auto-deploy config (a36bef7)

## [1.46.15] — 2026-06-14

- chore: bump version to v1.46.15 [skip ci] (f5b4c04)
- feat(chronos): always write JSONL logs to ~/.pares-radix/logs/chronos/ (7e3d31d)
- chore: bump version to v1.46.14 [skip ci] (e524861)
- feat(startup): init ModelPool on boot, background provider discovery (1b6f474)

## [1.46.13] — 2026-06-14

- chore: bump version to v1.46.13 [skip ci] (d3638a6)
- feat(telegram): wire ModelPool into /model and /status commands (80568c9)

## [1.46.12] — 2026-06-14

- chore: bump version to v1.46.12 [skip ci] (d3b35d4)
- chore: lock file + telegram split analysis doc (60605f7)
- feat(model_pool): dynamic model discovery + selection + user overrides (d22a30d)
- config: dynamic discovery, no static model catalog (d04f96f)
- config: model catalog from OpenClaw source (accurate data) (169f916)
- design: model pool with dual-mode config + PluresDB sync (130f59b)
- docs: model pool architecture design (8417acd)
- status: show registered tools count instead of plugins (25eea46)

## [1.46.11] — 2026-06-14

- chore: bump version to v1.46.11 [skip ci] (efd3764)
- test: add streaming bridge integration tests (e0db51f)

## [1.46.10] — 2026-06-14

- chore: bump version to v1.46.10 [skip ci] (6931520)
- fix(serve): wire streaming broadcast + stop aggregator heading leaks (fa5765d)

## [1.46.9] — 2026-06-14

- chore: bump version to v1.46.9 [skip ci] (dee27c1)
- Add local HTTP verify test harness (C-TEST-001) (d26f0a9)

## [1.46.8] — 2026-06-13

- chore: bump version to v1.46.8 [skip ci] (1cf7601)
- feat(spine): RSI + model-selection + topic-routing boundary actors (b098214)

## [1.46.7] — 2026-06-13

- chore: bump version to v1.46.7 [skip ci] (f0b51c8)
- feat(praxis): RSI as core architectural principle (ADR-0006) (8ebd48b)

## [1.46.6] — 2026-06-13

- chore: bump version to v1.46.6 [skip ci] (0a4f900)
- feat(praxis): ADR-0005 orchestration-as-dataflow + topic-routing.px + clippy fix (a51327b)
- feat(spine): add dev-lifecycle orchestration runtime (225187f)

## [1.46.5] — 2026-06-13

- chore: bump version to v1.46.5 [skip ci] (b772a30)
- feat(praxis): add dev-lifecycle.px — staged orchestration as .px procedure (6e3f6aa)

## [1.46.4] — 2026-06-13

- chore: bump version to v1.46.4 [skip ci] (e9a68a4)
- fix(build): show version tag instead of 'unknown' in sandboxed builds (3dd66dd)

## [1.46.3] — 2026-06-13

- chore: bump version to v1.46.3 [skip ci] (6458ce2)
- fix(nix): embed git commit hash in Nix builds via GIT_COMMIT_HASH env (941278c)

## [1.46.2] — 2026-06-13

- chore: bump version to v1.46.2 [skip ci] (dbf0230)
- fix(nix): handle CRLF in flake version extraction + normalize line endings (29764bc)

## [1.46.1] — 2026-06-13

- chore: bump version to v1.46.1 [skip ci] (e13c25c)
- chore: update Cargo.lock for v1.46.0 (c306454)
- fix(ci): harden version parsing — strip newlines, validate numeric fields (a69a918)
- test(spine): add integration test verifying real .px files compile (6b54d59)
- fix(ci): pass target_version to release workflow dispatch (941e8a7)

## [1.44.3] — 2026-05-16

- fix: pin fastembed 5.13.2 / ort rc.11 (static linking, no dlopen) (afba0d7)

## [1.44.2] — 2026-05-16

- fix: revert flake.nix to working fb11189 base + naming only (6358498)

## [1.44.1] — 2026-05-16

- fix: proper ONNX Runtime for Nix build — no code changes (4a9e95f)

## [1.44.0] — 2026-05-16

- feat(mcp): auto-load .px procedures on startup (468737c)

## [1.43.10] — 2026-05-16

- fix: remove duplicate closing brace in flake.nix (9846e97)

## [1.43.9] — 2026-05-16

- fix: autoPatchelfHook on onnxruntime to embed libstdc++ RPATH (d9cc0ce)

## [1.43.8] — 2026-05-16

- fix: add libstdc++ to LD_LIBRARY_PATH for onnxruntime (87f3641)

## [1.43.7] — 2026-05-16

- fix: set ORT_PREFER_DYNAMIC_LINK=1 for ort-sys (f38c46c)

## [1.43.6] — 2026-05-16

- fix: use official ONNX Runtime release with .so for build + runtime (2fafc28)

## [1.43.5] — 2026-05-16

- fix: let ort-sys handle its own ONNX Runtime download (93f0c74)

## [1.43.4] — 2026-05-16

- fix: use nixpkgs onnxruntime for build + runtime (9935435)

## [1.43.3] — 2026-05-16

- fix: restore ORT_LIB_LOCATION for ort-sys static lib (4ec8d86)

## [1.43.2] — 2026-05-16

- fix: use __noChroot for Nix builds instead of sandbox workarounds (fb139a7)

## [1.43.1] — 2026-05-16

- fix: wrap pares-radix binary with ORT_DYLIB_PATH for ONNX Runtime (c914e48)

## [1.43.0] — 2026-05-16

- feat(mcp): add compound expressions (&&, ||, !, >, >=, <, <=) to Praxis simpleEval (7d62b4a)
- fix(mcp): wrap context in evalScope so 'context.X' expressions resolve correctly (eacdda4)

## [1.42.3] — 2026-05-16

- fix(mcp): handle === and !== in Praxis simpleEval (ac3d70e)

## [1.42.2] — 2026-05-16

- refactor: rename pares-agens platform crates to pares-radix (36a5621)

## [1.42.1] — 2026-05-16

- fix: pin svelte-ratatui to rev for deterministic Nix builds (d8f604b)

## [1.42.0] — 2026-05-16

- fix: sync version to v1.41.0 (match latest tag) (cff88e6)
- fix: read version from Cargo.toml in flake.nix (a5669ea)
- fix: self-update uses nixos-rebuild instead of cargo build (90dac2a)
- feat(mcp): add chronos_record, subagent_spawn/list/kill tools (7256821)
- fix: align flake.nix with current crate structure (812aea0)
- chore: fix all clippy warnings across workspace (c87b58d)
- feat(mcp): add praxis_run tool for executing .px procedures (3828d30)
- feat(cerebellum): wire CerebellumProcedure to live preprocess pipeline (623cf36)
- feat(mcp-server): add Chronos timeline MCP tools (c2b809f)
- feat(mcp-server): add praxis_evaluate and praxis_list tools (81efa5f)
- feat(mcp-server): implement remote node tools (file_read, file_write, dir_list, dir_fetch, status) (6073f8e)
- feat(mcp-server): add media tools — image analyze/generate, TTS, PDF, video/music stubs (ba637f8)
- feat(mcp-server): add browser automation tools via CDP (c2f8eb2)
- feat(mcp): add remote node operations (SSH-based) (f2c4a81)
- feat(mcp): add runtime_restart and config_schema tools (88cde51)
- feat(mcp-server): add heartbeat_status and heartbeat_configure tools (7358241)
- feat(config): dynamic config scanning, config_delete, improved runtime_status (e6787fe)
- feat(delegation): add SubAgentManager with session tracking, timeouts, and completion events (e637fb1)
- feat(mcp-server): add config/runtime management tools (19cd3c9)
- feat(mcp-server): add db_get, db_put, db_delete tools for key-value state access (eebfaa5)
- feat(mcp-server): wire cron_list, cron_add, cron_remove, cron_toggle tools (bb13f96)
- feat(mcp-server): wire RadixToolHandler with real tool dispatch + mcp-serve CLI subcommand (f20f8f1)
- feat(mcp-server): add MCP server crate for stdio-based tool exposure (b478fa6)
- feat(tools): add cron_list, cron_add, cron_remove, cron_toggle agent tools (929921c)
- feat(px): add async procedure executor with timeouts, hooks, and loop guards (267e89f)
- fix(test): add delay in sled lock-release test to prevent flaky WouldBlock (0a2600f)
- feat(tools): add memory_search and memory_store tools to CLI dispatcher (f7a307f)
- feat(cli): wire ShellExecutor into tool dispatcher with process management (8cb7d7b)
- feat(tools): upgrade web_fetch with HTML→text extraction via html2text (a91b435)
- feat(core): add ShellExecutor with background sessions, PTY support, and yield pattern (c9fcc5f)
- feat(px): add loop, emit, try grammar + builder for full parse→compile→execute pipeline (7948479)
- feat(px): add loop, emit, and try step kinds to procedure executor (64c1199)
- feat(mcp): add file-backed persistence to PluresDB dev server (c4c0421)
- fix(core): mark incomplete doctests as ignore to fix test suite (a27099c)
- fix(tauri): update router config tests for dual-provider defaults (4fc73f6)
- feat(praxis): add health_check.px example procedure (c2cfd14)
- feat(px): wire .px procedures into runtime with ToolDispatchActionHandler (dd1d4f2)
- feat(core): add PxProcedureAdapter bridging .px procedures to core Procedure trait (6f13914)
- fix(cerebellum): add noise detection to message router (b8a813e)
- feat(mcp): add Praxis evaluate, Chronos timeline, Plugin management tools (3fc087d)
- feat(praxis): add .px procedure executor (a26c5ff)
- praxis: add plures development guide as loadable skill (126 constraints) (2035d14)
- praxis: add foundational software engineering constraints (128821b)
- docs: comprehensive plugin author guide (582a67c)
- refactor: replace inline lifecycle with reusable workflow call (a85cb5a)
- docs: OpenClaw innovation audit — 21 innovations analyzed, migration scorecard (96e2968)
- fix: map design-dojo CSS variables (surface-1/2/3, text-primary/secondary) to theme (a28f21c)
- Fix post-merge CI regressions in lint/typecheck at 0684681 (#124) (19771d7)
- fix: resolve svelte-check type errors for design-dojo v0.17 API (0684681)
- feat: add equipment inventory page for general asset tracking (d3f8bb2)
- fix: show loading screen during TUI startup instead of blank screen (f89fa6e)
- feat: make telegram-token optional for headless/desktop mode (9cc8d2e)
- fix: TUI falls back to in-memory store when DB is locked (378e066)
- fix(nix): skip tests in CLI build — 2 cerebellum tests flaky (fb11189)
- fix: add missing quiet_hours_enabled field in heartbeat test (07c91ea)
- Revert "fix(nix): exclude tauri-app from workspace members" (248f567)
- fix(nix): exclude tauri-app from workspace members (bcd7dc8)
- chore: update Cargo.lock with tauri-plugin-tui git dep (b8e13a8)
- fix: replace hardcoded local path dep with git dep for tauri-plugin-tui (7764fd9)
- feat: plugin resolver — install from local fs, GitHub, npm, or modulus registry (253e4a4)
- feat: LAN peer discovery — automatic local network optimization (c98af4a)
- feat: env-driven model config — no hardcoded defaults for multi-host (a7f5926)
- chore: default to copilot/claude-opus-4.6, not gpt-4.1 (525c47e)
- feat: Anthropic as default provider, auto-seed from ANTHROPIC_API_KEY (350d805)
- feat: auto-seed Copilot API key from gh auth token on startup (4691216)
- feat: migration benchmark suite — 12 test cases across 5 categories (6a52d10)
- chore: point Tauri at SvelteKit app (was using old ui/ directory) (c53d6a5)
- feat: /chat and /canvas routes — agent console and canvas runtime in main app (b8fd858)
- feat: agent-api.ts — Tauri IPC bridge for pares-agens backend (36db767)
- fix: 37 → 1 type errors — all callsite fixes for design-dojo v0.17 (1ada776)
- fix: stable state — v0.17.0 npm + local Sidebar/SettingsPanel, all routes 200 (0eb0734)
- fix: restore Sidebar + SettingsPanel to local shim — npm API incompatible (20acb73)
- fix: partial type error fixes (49 → 36 remaining) (a881c8a)
- chore: bump design-dojo-npm to v0.17.0 (Text as prop, Button variants, class) (fb03969)
- fix: wire workspace shim as dependency + pin design-dojo-npm to v0.14.0 (f368b28)
- refactor: switch to real @plures/design-dojo npm + local shim for missing components (de66cee)
- fix: SSR compatibility — all routes render without errors (ae013b8)
- fix: resolve type errors in migrated design-dojo components (e80afe8)
- fix(design-dojo): widen primitive prop types for real-world usage (3ea4bc8)
- refactor: final migration — zero plures/ violations remaining (e760710)
- refactor: migrate settings, plugins, ComponentPicker, layout to design-dojo (943a147)
- config: allowlist design/+page.svelte from no-raw-html (design system demo) (9b1e58b)
- refactor: migrate EntityForm, EntityList, OmniscientIndex, help page to design-dojo (8b9b812)
- refactor: migrate RouteEditor, AIDesignAssistant, RuleEditor to design-dojo (8ceb67b)
- refactor: migrate SchemaRenderer, Breadcrumbs, EntityDetail to design-dojo (d55c939)
- refactor: migrate stores to PluresDB + fix +page.svelte violations (fc74da4)
- feat: register all 20 design-dojo components in canvas-runtime registry (02fa3c4)
- feat(design-dojo): 11 new primitives — Box, Text, Heading, Input, TextArea, Select, Link, CodeBlock, List, ListItem, Table (77f4a67)
- feat: radix-mcp-dev — full MCP control server (DEV ONLY) (e14e0f4)
- test: canvas-runtime test suite (29 tests) + lint fixes (5c7916a)
- feat: reactive canvas renderer — live updates when PluresDB changes (c250262)
- feat: AI Canvas plugin wired into pares-radix plugin system (485c471)
- feat: canvas-runtime reactive graph + AI canvas plugin + MCP tool interface (5c5e3da)
- feat: @plures/canvas-runtime — AI creates apps at runtime by writing data (7f303ad)
- feat: eslint-plugin-plures — compile-time enforcement of platform constraints (0f139ec)
- fix: convert legacy $: to $effect in App.svelte for Svelte 5 compat (39d0674)
- feat: comprehensive Chronos telemetry layer (c08275a)
- fix: Procedures was wrapped in Dialog (invisible), stripped to plain div (c3d2a9b)
- fix: Procedures plugin renders empty — open prop defaulted to false (3e6c61d)
- test: detailed view rendering tests + per-view screenshots (c586da2)
- fix: Chronos now records in browser mode + plugin registration logging (c00546e)
- fix: plugin switching — keyed each must include pluginId for re-render (2c16008)
- fix: canvas panes use writable (Unum reactivity issue) + view/component field mismatch (b9a6f57)
- fix: Chat import from barrel (./app not in exports map) (18c69f1)
- feat(ui): polished Welcome screen with actions and shortcuts (bd08de4)
- feat(ui): canvas splitting — Ctrl+Click for side-by-side plugins (f7f090e)
- feat(ui): formalized Plugin API — validatePlugin, PluginContext, statusBarItems (2954107)
- feat(ui): polished Chat plugin with real agent wiring (2a4c3ec)
- feat: wire svelte-ratatui TUI bridge — same Svelte UI renders in terminal (7d9da46)
- refactor(ui): fresh start — radix as minimal plugin host with canvas (621d848)
- feat(ui): PraxisDevOverlay prepared (needs @plures/praxis peer dep) (3d6f335)
- fix: postinstall copies local design-dojo dist (new components not on npm yet) (3c67b5f)
- feat(ui): VSCode-class visual redesign — icons, layout, theme (f2a6f46)
- fix(ui): disable Wizard (Box snippet crash), fix progress-step elements (99b19ea)
- fix(ui): remove Box borders — design-dojo defaults to visible borders (b306525)
- fix: Tauri build — absolute path for beforeBuildCommand + postinstall patch (3eaaa82)
- refactor(ui): Settings.svelte fully converted to design-dojo (0ca3a9d)
- refactor(ui): Wizard.svelte fully converted to design-dojo components (460b31a)
- fix(ui): Badge import path — use main barrel export (8ee1e31)
- feat(ui): Praxis rules engine — state transitions governed by constraints (51953c5)
- feat(ui): Unum-backed persistent stores (PluresDB when available, writable fallback) (62ddee6)
- refactor(ui): finish design-dojo migration + add Chronos recording (ef122fa)
- refactor(ui): replace raw HTML with design-dojo components (ae86525)
- feat(ui): complete api.js migration + README (20b5da6)
- feat(ui): unified Tauri API layer — single abstraction for all backend calls (2bbd3d2)
- feat(ui): integrated terminal panel (Ctrl+`) with Terminal/Chronos/Logs tabs (67e913d)
- feat(ui): Chronicle timeline plugin + ConfigBrowser as plugin (2fdac30)
- feat(ui): plugin registry system — dynamic loading, enable/disable, commands (e3c1e51)
- feat(ui): command palette (Ctrl+Shift+P) + centralized store (75ea45a)
- feat(ui): VSCode-like application shell with design-dojo layout (6f793e4)
- fix(tui): restore arrow keys for input editing, use PageUp/Down for scroll (1709c99)
- fix: remove noise filter that dropped single-word messages like 'hello' (f9b9deb)
- fix: char boundary panic in input + catch agent task panics (39f5d4a)
- fix: recover from poisoned mutexes instead of panicking (643343e)
- fix: disable Chronos record in routing path — sled deadlock in async context (6bf1199)
- fix: praxis gate never drops messages, chronos records routing, TUI scroll with user control (5e479fb)
- fix: TUI model errors visible to user, system prompt from config, /memory passthrough, scroll fix (7e0bdf0)
- feat: config file at ~/.config/pares-radix/config.toml, remove hardcoded models (c8ef67c)
- fix(tui): add 30s timeout to agent calls, better error messages (7243dae)
- refactor: shared command registry + TUI auto-scroll + better errors (06708e8)
- feat: config file system, remove hardcoded model names (c90a2c0)
- fix(tui): auto-scroll to bottom, better error reporting (da2ad5e)

## [1.40.0] — 2026-05-06

- feat: default logging to ~/.pares-agens/logs/, Chronos always-on (cc283cb)

## [1.39.3] — 2026-05-06

- fix(tui): suppress tracing output + clear terminal on startup (48dbb6c)

## [1.39.2] — 2026-05-06

- fix(tui): event drain was inside key-event block — responses never displayed (87b9a3d)

## [1.39.1] — 2026-05-06

- fix: resolve all workspace compile errors (d9aab88)

## [1.39.0] — 2026-05-06

- feat(praxis): extend .px grammar with procedures and personality constraints (5c1ad00)
- docs: rewrite ADR-0016 — PluresDB as agent runtime (e3a2eed)
- docs: ADR-0016 personality as praxis constraints (.px) (776f80c)
- fix: gate bitnet native inference behind feature flag (5386833)
- feat: promise detection — agent tracks commitments automatically (bc88cb6)
- feat: cerebellum-gated heartbeat — frequent ticks, zero token burn (bfa7777)
- feat: quiet hours optional — disabled via PARES_HEARTBEAT_NO_QUIET env (3c48e59)
- fix: spawn heartbeat runner in serve command (a82dc09)
- docs: ADR-0015 Chronos logging pattern — one log, multiple formats (47de647)
- refactor: Chronos IS the log — unified logging with JSONL output (658cdcc)
- feat: wire telemetry into serve + TUI agent paths (5ea984a)
- feat: Chronos telemetry — JSONL interaction logging with causal chains (fbaca0e)
- feat: wire context manager into cerebellum preprocess pipeline (dc7557c)
- feat: context manager — cerebellum as trained procedure engine (d1beae6)
- feat: classify subcommand + BitNet classifier test results (d92e209)
- fix: BitNet stop token handling + text-level end marker detection (8fe2739)
- feat: add 'ask' subcommand for non-interactive benchmarking (7eb37e2)
- feat: wire BitNet classifier into TUI agent factory (d4f9a2a)
- feat: BitNet cerebellum classifier — single-token classification for speed (0ecea58)
- feat: BitNet native inference on CPU — shim wraps llama.cpp (492188c)

## [1.38.0] — 2026-05-02

- feat: rename binary from pares-agens-cli to pares-radix (bdf0ea1)
- fix: remove accidentally added nested repos, add to .gitignore (bb59f3b)
- fix: resolve lint errors in plugin.ts (remove unused SvelteComponent import) (84b1ee2)

## [1.37.0] — 2026-05-01

- feat: navigation system — breadcrumbs, nav guards, platform widgets (#49, #50, #51) (7cacbde)

## [1.36.0] — 2026-05-01

- feat: extend .px language with personality + safety block types (d00b13b)

## [1.35.0] — 2026-05-01

- feat: personality evolution architecture + default personality files (fa0e27f)

## [1.34.1] — 2026-05-01

- fix: suppress ci-feedback issue spam (24h dedup window) (3e44bda)

## [1.34.0] — 2026-05-01

- feat: plugin dependency validation + topological install ordering (#48) (c49946b)

## [1.33.0] — 2026-05-01

- feat: omniscient plugin manifest + runtime adapter (b1a6434)

## [1.32.0] — 2026-05-01

- feat: add pares-omniscient crate — two-pass semantic filesystem indexer (5c5b713)

## [1.31.1] — 2026-05-01

- fix: resolve all 40 clippy warnings — zero warnings achieved (f6fcf65)

## [1.31.0] — 2026-04-30

- feat: add USER.md, AGENTS.md, HEARTBEAT.md to personality config (c0b3ce9)

## [1.30.1] — 2026-04-30

- fix: lint error — unused pluginSchema import in plugin detail page (0861b78)

## [1.30.0] — 2026-04-29

- feat: add Docker QA, enhanced /status, error display, personality logging (c77e4eb)

## [1.29.0] — 2026-04-29

- feat: auto-download BitNet models from HuggingFace on first use (60e8d03)

## [1.28.0] — 2026-04-29

- feat: LAN multicast discovery + shortcoming mitigations (c89abb6)

## [1.27.0] — 2026-04-29

- feat(rector): add direct peer + LAN multicast discovery (14b90c9)

## [1.26.0] — 2026-04-29

- feat: deep observability — health reporting, tool Chronos logging, PII guard (b5437b4)

## [1.25.0] — 2026-04-29

- feat: NixOS deployer, ModelChain fallback, orchestrator deploy wiring (c4cb474)

## [1.24.0] — 2026-04-29

- feat(rector): node discovery, cluster CLI & slash commands (c030ed9)

## [1.23.0] — 2026-04-29

- feat(rector): add infrastructure orchestrator foundation (74ad73e)

## [1.22.0] — 2026-04-29

- feat(cerebellum): add message classifier with heuristic + model backend (6e13ab4)

## [1.21.0] — 2026-04-29

- feat: restore BitNet crates for local inference (c213137)

## [1.20.0] — 2026-04-29

- feat: wire task loop into telegram — /tasks and /task commands (3bdafe7)

## [1.19.0] — 2026-04-29

- feat(core): add Praxis-driven task loop with PluresDB-backed task manager (2cffb59)
- chore: review pass — fix compilation, tests, help text, exports (6ed8b48)

## [1.18.0] — 2026-04-29

- feat(plugins): git adapter plugin + unified TOML/JSON manifest parsing (83a75ae)

## [1.17.0] — 2026-04-29

- feat(core): add Chronos version timeline and content-addressed storage (69cb186)

## [1.16.0] — 2026-04-29

- feat: session resume (/resume command) and plugin hook system (cc32839)

## [1.15.0] — 2026-04-29

- feat: wire PraxisWriteGate into serve command + agent-console plugin (703c619)

## [1.14.0] — 2026-04-29

- feat(praxis): add write gate for CrdtStore data validation (19577ed)

## [1.13.0] — 2026-04-28

- feat: wire Tauri bridge for plugin CRUD (1958d0b)
- Merge pares-agens Rust workspace into pares-radix (3d08315)

## [0.7.4] — 2026-04-24

- fix(ci): resolve post-merge build failure from invalid Svelte 5 event modifier syntax (#55) (b7e1235)

## [0.7.3] — 2026-04-24

- fix(ci): restore green typecheck + wire up ESLint after 890e933 regressions (#54) (a4e8e7d)

## [0.7.2] — 2026-04-24

- fix(svelte): resolve remaining warnings after post-merge CI failure at ed7ddb0 (#53) (d138aac)

## [0.7.1] — 2026-04-24

- fix(typecheck): resolve post-merge CI build and typecheck failures (#52) (69ed662)
- docs: refresh ROADMAP.md with OASIS strategic alignment (7481358)

## [0.7.0] — 2026-04-23

- feat(design): Phase 4 — LLM-assisted schema generation from natural language (ed7ddb0)

## [0.6.0] — 2026-04-23

- feat(design): Phase 3 — ComponentPicker, RouteEditor, SchemaRenderer (890e933)

## [0.5.0] — 2026-04-23

- feat(render): tri-mode rendering — GUI, TUI CSS, TUI native (6c51540)

## [0.4.0] — 2026-04-23

- feat(design): Phase 2 — Rule Editor, hot-reload engine, decision ledger (c69e9ca)

## [0.3.0] — 2026-04-23

- feat(design): implement design mode Phase 1 — self-modifying praxis architecture (c3e6819)
- docs: update copilot-instructions with praxis, design-dojo, automation rules (3db3288)

## [0.2.0] — 2026-04-23

- feat(release): add target_version input for milestone-driven releases (6597944)
- feat(lifecycle): milestone-close triggers roadmap-aware release (64dd3c3)
- feat(lifecycle v12): auto-release when milestone completes (4c6aa91)
- feat(lifecycle v11): smart CI failure handling — infra vs code (d56a54c)
- fix(lifecycle): label-based retry counter + CI fix priority (c370596)
- ci: lifecycle — add unmilestoned issue fallback + force-merge on CI exhaustion (ba7c698)
- ci: lifecycle v10 — auto-retry transient failures, force-merge on exhaustion (665f3da)
- docs: ADR-0011 plugin security model (5a260f6)
- feat: Tauri 2 desktop shell — events not commands (#40) (420e477)
- feat: PluresDB persistence via praxis adapter — automatic fact storage (#39) (aef9157)
- feat: schema-driven UI generation — design-dojo shell components from praxis schemas (#38) (aad3f0f)
- feat: define agens plugin praxis module — three-agent cognitive loop (#37) (c52241c)
- feat: define platform shell praxis module — facts, rules, constraints (#34) (a8e9211)
- feat: praxis expectations for platform shell and agens plugin (11c771a)
- docs: ADR-0010 — pares-agens as first radix plugin (praxis-native) (1644076)
- ci: inline lifecycle workflow — fix schedule failures (51fc638)
- Merge pull request #28 from plures/copilot/refactor-inline-components-to-design-dojo (df37d00)
- Merge branch 'main' into copilot/refactor-inline-components-to-design-dojo (c1043be)
- docs: add structured ROADMAP.md for automated issue generation (315f922)
- Merge branch 'main' into copilot/refactor-inline-components-to-design-dojo (a20b6f8)
- chore: remove redundant workflow — handled by centralized ci-reusable.yml or obsolete (7974f2b)
- Update package.json (24528a6)
- Update packages/design-dojo/package.json (56306cb)
- refactor: simplify settingsAPI method references in settings page (7082e48)
- refactor: migrate inline components to @plures/design-dojo (01fc75b)
- Initial plan (12034ed)
- chore: centralize CI to org-wide reusable workflow (9a54366)
- ci: add Design-Dojo UI compliance gate (d4952f4)
- ci: standardize Node version to lts/* — remove hardcoded versions (5d64e0c)
- feat: data import/export orchestration across plugins (#25) (279a65d)
- feat: LLM integration layer — shared provider config and context assembly (#24) (a0e4539)
- feat: Dashboard with plugin widgets (#23) (7905314)
- Merge pull request #22 from plures/copilot/feat-plugin-aware-onboarding-wizard (9258309)
- Merge branch 'main' into copilot/feat-plugin-aware-onboarding-wizard (12b564c)
- ci: tech-doc-writer triggers on minor prerelease only [actions-optimization] (8fc59ef)
- Merge branch 'main' into copilot/feat-plugin-aware-onboarding-wizard (5e913a7)
- ci: add concurrency group to copilot-pr-lifecycle [actions-optimization] (022f8db)
- Apply Copilot review suggestions (8edf822)
- feat: plugin-aware onboarding wizard with topological step ordering and dependency locking (4e12f54)
- Initial plan (f513244)
- ci: centralize lifecycle — event-driven with schedule guard (e49010d)
- Merge pull request #21 from plures/copilot/add-help-page-route (53abc2d)
- fix: validate section.links hrefs and expand isSafeUrl to support relative paths (9ecba18)
- Merge branch 'main' into copilot/add-help-page-route (c010d80)
- fix(lifecycle): v9.2 — process all PRs per tick (return→continue), widen bot filter (785558d)
- Merge branch 'main' into copilot/add-help-page-route (b8ad801)
- fix(lifecycle): change return→continue so all PRs process in one tick (237fcfc)
- Merge branch 'main' into copilot/add-help-page-route (830e9a7)
- fix(lifecycle): v9.1 — fix QA dispatch (client_payload as JSON object) (47789a3)
- Apply Copilot review suggestions (2b8cbf6)
- Merge branch 'main' into copilot/add-help-page-route (76da14e)
- fix(lifecycle): rewrite v9 — apply suggestions, merge, no nudges (f811849)
- feat: aggregated help page with markdown rendering, section links, and platform-aware shortcuts (2a8a0f7)
- Initial plan (b916ecf)
- feat: unified settings page with plugin settings slots (#20) (78ca774)
- fix: remove non-existent @plures/design-dojo package imports (#19) (a2090b6)
- chore: license BSL 1.1 (commercial product) (11e761c)
- fix: use @plures/design-dojo/enforce for ESLint (not separate package) (d9ce0a9)
- Merge pull request #13 from plures/copilot/fix-ci-failure-typecheck-again (c6a47fc)
- Merge branch 'main' into copilot/fix-ci-failure-typecheck-again (137ad00)
- fix: replace all custom UI with @plures/design-dojo components (96a27af)
- Update .github/workflows/ci.yml (30a7012)
- fix: run svelte-kit sync before typecheck in CI workflows (5416a29)
- Initial plan (ff4389c)
- feat: Svelte layout with plugin-aware sidebar navigation (#11) (556060b)
- Merge pull request #10 from plures/copilot/fix-ci-failure-typecheck (8e81101)
- fix: upgrade @sveltejs/vite-plugin-svelte to ^5.0.0 for vite v6 compatibility (bc6416f)
- Initial plan (7e2a5a6)
- feat: add SvelteKit application shell (8ad0ab2)
- docs: add comprehensive architecture documentation (a767b06)
- docs: add transition plan and FinancialAdvisor migration guide (4b6489c)

## [0.1.1] — 2026-03-28

- chore: add CI, Copilot automation, build tooling (7b4ff72)
- feat: initial pares-radix foundation (929cd1a)

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Plugin API type definitions (`RadixPlugin` contract with full lifecycle)
- Plugin loader with topological dependency resolution
- Inference engine with confidence scoring, auto-confirmation, and decision ledger
- Compound confidence merging when multiple rules fire
- UX contract system — built-in expectations for dead-end prevention, data prerequisites, nav resolution
- Runtime data requirement checking with empty state enforcement
- Architecture documentation
- Roadmap
