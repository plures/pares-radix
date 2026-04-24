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
