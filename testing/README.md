# pares-radix Real Testing Infrastructure

Real-world integration testing. No mocks. Docker container with SSH, TUI automation via pexpect.

## Quick Start

```bash
# 1. Configure secrets
cp testing/.env.example testing/.env
# Edit testing/.env — add your API key

# 2. Build and run tests
./testing/run-tests.sh

# 3. SSH into the running container for manual testing
ssh -p 2222 radix@localhost
# password: radix-test-pw

# 4. Start TUI manually inside container
pares-radix tui --model-url https://models.inference.ai.azure.com

# 5. Teardown
./testing/run-tests.sh --teardown
```

## Architecture

```
┌─────────────────────────────────────┐
│  docker-compose.yml                 │
├─────────────────────────────────────┤
│  pares-radix service                │
│  ├── SSH server (port 2222)         │
│  ├── pares-radix binary             │
│  └── UTF-8 locale + terminfo        │
├─────────────────────────────────────┤
│  test-runner (--profile test)       │
│  ├── pytest + pexpect + paramiko    │
│  ├── test_smoke.py (binary, env)    │
│  ├── test_tui.py (interactive TUI)  │
│  └── test_mcp_server.py (protocol) │
└─────────────────────────────────────┘
```

## Test Categories

| File | What it tests | Requires API key? |
|------|---------------|-------------------|
| `test_smoke.py` | Binary works, env correct, SSH access | No |
| `test_tui.py` | TUI launches, responds to keys, exits cleanly | Partially (may skip) |
| `test_mcp_server.py` | MCP JSON-RPC protocol compliance | No |

## Secrets

All secrets are injected via `testing/.env` (never baked into the image).
Required for full test suite:
- `PARES_API_KEY` — GitHub Models / OpenAI-compatible API key

Smoke tests run without any API keys.

## CI

Copy `testing/ci/real-tests.yml` to `.github/workflows/` to enable on push:

```bash
cp testing/ci/real-tests.yml .github/workflows/real-tests.yml
```

## Design Principles

1. **No mocks** — tests hit real binaries, real SSH, real protocol
2. **SSH for TUI** — same way a human would access it remotely
3. **pexpect for automation** — industry standard for terminal automation
4. **Secrets at runtime** — `.env` file, never in image layers
5. **Profile isolation** — test runner only starts with `--profile test`
