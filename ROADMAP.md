# pares-radix Roadmap

## Role in OASIS
Pares Radix is the end‑user desktop shell for OASIS. It hosts domain plugins, enforces UX contracts, and provides the shared intelligence/data plumbing so every OASIS workflow feels coherent across platforms.

## Current State
- Tauri 2 desktop shell port completed.
- PluresDB persistence adapter and schema‑driven UI generation merged.
- CI regressions currently open (multiple post‑merge failures).

## Phase 1 — Platform Shell Stabilization
- Fix CI regressions and lock green main.
- Solidify plugin loader (validation, dependency resolution, lifecycle hooks).
- Navigation system: persistent sidebar, breadcrumbs, and safe routing guards.
- Unified settings + help aggregation from plugins.
- Dashboard home with plugin widgets and onboarding flow.

## Phase 2 — Shared Intelligence Layer
- LLM provider configuration + token budgeting across plugins.
- Context assembly from active plugin state + PluresDB facts.
- Praxis rule enforcement + decision ledger surfaced in UI.
- UX contracts: empty state enforcement and expectation validation.

## Phase 3 — Data & Portability
- PluresDB‑backed data model for all plugin state.
- Import/export orchestration + backup/restore of full app state.
- Search across plugin data and history.
- Data migration framework for plugin schema upgrades.

## Phase 4 — Multi‑GUI + Polish
- Multi‑arch packaging (Linux, Windows, macOS; Android/iOS via Tauri where supported).
- Svelte GUI parity with Svelte TUI (design‑dojo terminal theme).
- Native terminal UX via svelte‑ratatui pipeline.
- Accessibility, performance, and command palette.
