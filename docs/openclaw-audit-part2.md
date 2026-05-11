# OpenClaw Innovation Audit — Part 2: Model, Memory & Runtime

## Summary

| # | Innovation | OpenClaw | pares-radix | Status |
|---|-----------|----------|-------------|--------|
| 1 | Memory integration | PluresLM plugin + markdown files | PluresDB + fastembed HNSW | **Ahead** |
| 2 | Model routing | Multi-provider via gateway | ModelRouter + inference engine | **Behind** |
| 3 | Sub-agents | Isolated child sessions | pares-agens 3-agent cognitive arch | **Behind** |
| 4 | Visual feedback | Streaming dots, thinking indicators | design-dojo streaming components | **Parity** |
| 5 | TUI mode | None | design-dojo TUI + svelte-ratatui | **Ahead** |
| 6 | Browser automation | Playwright + CDP relay | Canvas runtime (different philosophy) | **Different** |
| 7 | Device pairing | Node companion apps (iOS/Android/macOS) | Hyperswarm + LAN discovery | **Behind** |

---

## 1. Memory Integration

**OpenClaw:** PluresLM plugin with 4,200+ memories, auto-recall (injects relevant memories before each response), auto-capture (extracts facts from conversations), native embeddings (BAAI/bge-small-en-v1.5, 384-dim). Also uses flat markdown files (MEMORY.md, daily notes) injected into system prompt. Procedures engine with triggers (before_search, after_store, on_cue, cron).

**pares-radix:** PluresDB with the same fastembed model (bge-small-en-v1.5) providing HNSW vector search. Memory is graph nodes with typed relationships — not flat text. Every write is a CRDT operation that syncs via Hyperswarm. Chronos provides causal attribution ("who stored this, when, why"). No flat file layer needed.

**Verdict: Ahead.** Same embedding quality, but graph-native storage with relationships, CRDT sync, and causal attribution. OpenClaw's memory is a plugin bolted onto a file system; radix's is the foundation everything builds on.

---

## 2. Model Routing & API

**OpenClaw:** Gateway routes to 30+ model providers (OpenAI, Anthropic, Google, GitHub Copilot, Azure, Groq, etc.). Supports streaming, tool/function calling, thinking levels (off/low/medium/high), vision, PDF analysis, image/video/music generation. Provider auth managed via config profiles. Fallback chains supported.

**pares-radix:** ModelRouter in `crates/inference` with provider abstraction. Currently supports OpenAI-compatible APIs and Anthropic. Streaming via SSE in serve mode. BitNet CPU inference planned for air-gapped deployments. No vision, no media generation, no thinking level controls yet.

**Verdict: Behind.** OpenClaw's model integration is years ahead — 30+ providers, media generation, thinking controls. Radix has the architecture but thin provider coverage. The BitNet CPU inference is a unique differentiator for air-gapped scenarios, but isn't built yet.

---

## 3. Sub-agents

**OpenClaw:** Spawns isolated child sessions with independent context, configurable models and thinking levels, timeouts, and auto-announce on completion. Parent can yield and resume when children complete. Supports up to 8 concurrent sub-agents. Sub-agents inherit workspace but get fresh conversation context.

**pares-radix:** pares-agens implements a 3-agent cognitive architecture — cortex (reasoning), cerebellum (routing/tool dispatch), hippocampus (memory consolidation). Agents communicate via typed message passing, not text. The procedure engine supports delegation steps. However, the orchestration UX (spawn, monitor, collect results) is not built.

**Verdict: Behind.** OpenClaw's sub-agent system is production-tested — it's what built this audit doc. Radix has a more sophisticated agent architecture on paper (typed message passing > text relay), but the orchestration layer that makes it usable doesn't exist yet.

---

## 4. Visual Feedback

**OpenClaw:** Streaming partial responses to Telegram/Discord (configurable per channel). Thinking indicators during tool calls. Progress updates during long operations. Markdown rendering in chat.

**pares-radix:** design-dojo provides streaming text components, loading indicators, and progress bars in both GUI and TUI modes. The TUI loading screen (commit `f89fa6e`) shows progressive status during startup. Serve mode streams via SSE.

**Verdict: Parity.** Both handle streaming and progress. Radix's TUI streaming is a differentiator (OpenClaw has no terminal UI), but OpenClaw's channel-specific streaming (Telegram partial updates) is more polished for chat surfaces.

---

## 5. TUI Mode

**OpenClaw:** No terminal UI. CLI is command-only (`openclaw status`, `openclaw gateway`). No interactive terminal interface.

**pares-radix:** Full TUI via design-dojo components rendered through svelte-ratatui. Every design-dojo component has a TUI variant with box-drawing borders, terminal-safe colors, and keyboard navigation. The TUI shows chat, memory, and system status in a split-pane terminal layout. Works over SSH for headless/jumpbox deployments.

**Verdict: Ahead.** This is a capability OpenClaw simply doesn't have. The TUI enables use cases (air-gapped jumpboxes, SSH sessions, headless servers) that are impossible with OpenClaw.

---

## 6. Browser Automation

**OpenClaw:** Playwright-based browser control with snapshot/act pattern. CDP relay for connecting to user's authenticated browser. Supports screenshots, navigation, form filling, page scraping. Used for web research, portal interaction, and UI testing.

**pares-radix:** Canvas runtime — AI generates UI as structured data (reactive graph + design-dojo components) rendered natively in the app. Philosophy: don't automate browsers, replace the need for them. The agent creates interactive dashboards, forms, and visualizations directly.

**Verdict: Different philosophy.** OpenClaw automates existing web UIs. Radix generates new UIs. Both are valid — but radix can't interact with third-party websites (Azure Portal, GitHub, etc.), which is a real gap for ops work. Canvas is more powerful for custom tools; browser automation is essential for interacting with the existing web.

---

## 7. Device Pairing

**OpenClaw:** Companion apps for iOS, Android, and macOS. QR code pairing, camera/photos/screen/location/notifications access on paired devices. Paired nodes can be targeted for commands. Bootstrap token auth flow.

**pares-radix:** Hyperswarm topic-based discovery + LAN mDNS peer discovery (`crates/sync/src/lan.rs`). Peers find each other automatically on the same network. PluresDB CRDT sync replicates state across devices. No companion apps — peers are other radix instances.

**Verdict: Behind.** OpenClaw's companion apps provide phone integration (camera, photos, notifications) that radix can't match. Radix's P2P sync is architecturally stronger (CRDT > client-server), but the lack of mobile apps means no phone integration. Different target: OpenClaw pairs phones to a server; radix syncs peers as equals.

---

## Key Takeaway

Radix leads on memory (graph-native > flat files) and TUI (capability OpenClaw lacks entirely). OpenClaw leads on model coverage (30+ providers), sub-agent orchestration (production-tested), and device integration (companion apps). Browser vs canvas is a philosophical split — both needed.

The critical gap for migration: model routing needs more providers, and sub-agent orchestration needs a usable UX layer. Memory and TUI are already advantages.
