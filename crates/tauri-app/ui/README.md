# pares-radix GUI

## Quick Start (Browser Dev Mode)

```bash
npm install
npm run dev
```

Opens at http://localhost:5173. All features work with mock data.

## Full Tauri Desktop Build

```bash
cd ../..  # repo root
cargo tauri dev
```

Requires: Rust toolchain, Tauri CLI (`cargo install tauri-cli`), system deps.

## Architecture

- Activity bar (left) — plugin-driven navigation
- Main area — active plugin view
- Sidebar (right) — memory, search
- Bottom panel (Ctrl+`) — terminal, chronos, logs
- Command palette (Ctrl+Shift+P) — all commands

## Plugins

Plugins register via `src/lib/plugins/registry.js`. Built-in:

- 💬 Chat — AI assistant
- ⚡ Procedures — Praxis procedure editor
- 📜 Timeline — Chronos event viewer
- 🖥️ Config Browser — datacenter config tree
- ⚙️ Settings — app configuration
- 🧩 Extensions — plugin manager
