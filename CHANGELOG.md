## [1.40.1] — 2026-05-06

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
