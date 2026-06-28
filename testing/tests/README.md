# ⚠️ QUARANTINED: CLI-over-SSH test suite (ghost-binary era)

**Status:** quarantined 2026-06-28. NOT in any CI gate. Kept for history/reference.

## Why

These pytest files were written against a **traditional CLI binary** (`pares-radix --version`,
`pares-radix tui`, `pares-radix mcp-server`) driven over **SSH + pexpect/paramiko** inside a
Docker container. **That binary never existed.** `crates/cli` is lib-only (`pares_radix_migrate`,
no `[[bin]]`); the only Rust binary is `pares-radix-app` (svelte-Tauri). So
`./target/release/pares-radix` exited 127 and Real Integration Tests was red on every push.

pares-radix is a **svelte-Tauri** app: GUI + Svelte-TUI (`tui-css`) + svelte-ratatui (`tui-native`)
+ MCP. There is **no traditional CLI**. Per **C-TEST-002**, integration tests hit the
channel-agnostic core, not a removed adapter.

## What replaced it

`.github/workflows/real-tests.yml` now tests real surfaces only:
- **mcp** — `pnpm -C packages/mcp-dev-server test` (vitest) + native stdio JSON-RPC smoke (`smoke.mjs`).
- **workspace** — `cargo build --release --workspace` (app lib + crates compile).
- **frontend** — `pnpm test` (render-mode / tui-mappings / platform / stores).

`test_mcp_server.py` was **converted to native stdio** (spawns `tsx src/index.ts`, no SSH/CLI) and
is the only file here still wired to the real surface.

## The quarantined files

Every other `test_*.py` here still assumes `RADIX_BINARY=pares-radix` + `ssh_exec`. They are
**not deleted** (history/intent matters) but are **not run by CI**. Mutation/visual/property suites
may be revived once retargeted at MCP or a Tauri WebDriver harness. Do not add them back to a gate
until they exercise a surface that actually exists. No mocks, no ghost binaries.
