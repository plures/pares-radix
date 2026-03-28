# Roadmap — pares-radix

## Vision
The Praxis base application: a plugin-driven platform with inference engine, UX contracts, and LLM integration. All plures domain apps (FinancialAdvisor, vault, sprint-log, netops, pares-agens) become plugins to this base.

## Phase 1 — Foundation (Q2 2026)
- [x] Plugin API type definitions (`RadixPlugin` contract)
- [x] Plugin loader with dependency resolution
- [x] Inference engine with confidence scoring + decision ledger
- [x] UX contract system (journey expectations)
- [ ] Layout with persistent sidebar nav (plugin-aware)
- [ ] Unified settings page (aggregates plugin settings)
- [ ] Aggregated help page
- [ ] Onboarding wizard (dependency-ordered plugin steps)
- [ ] Dashboard with plugin widgets
- [ ] Data import/export orchestration
- [ ] LLM integration layer (provider config, context assembly)
- [ ] Package as `@plures/pares-radix` npm

## Phase 2 — Inference + UX (Q2–Q3 2026)
- [ ] PluresDB-backed inference table + decision ledger
- [ ] Confidence gating in UI (auto-confirm ≥0.90, user gate <0.70)
- [ ] Build-time UX expectation validation
- [ ] Runtime empty state enforcement
- [ ] Breadcrumb navigation with back-tracking

## Phase 3 — Plugin Migration (Q3 2026)
- [ ] Port FinancialAdvisor as first plugin (proof of concept)
- [ ] Port remaining apps to pares-modulus
- [ ] Cross-plugin data awareness
- [ ] Shared LLM context assembly

## Phase 4 — Subconscious + Federation (Q3–Q4 2026)
- [ ] Background inference preprocessing
- [ ] Rule generation from user behavior
- [ ] Anonymized rule sharing (federated intelligence)
- [ ] Decision→outcome mapping
