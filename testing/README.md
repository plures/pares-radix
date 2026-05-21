# Real Testing Infrastructure for pares-radix

**Philosophy:** No mocks. Real services, real keys, real terminal. Test like a human would.

## Quick Start

```bash
# 1. Copy env template and fill in real API keys
cp testing/.env.example testing/.env
# Edit .env with your keys

# 2. Build and run
cd testing/
docker compose up -d

# 3. SSH into the container (TUI access)
ssh radix@localhost -p 2222
# password: radix-test

# 4. Run automated tests
./scripts/run-tests.sh

# 5. Run TUI automation tests
pip install pexpect
python scripts/tui-test.py
```

## Architecture

```
testing/
├── Dockerfile          # Multi-stage: build from source → slim runtime + SSH
├── docker-compose.yml  # Full stack: radix + (optional) pluresdb
├── entrypoint.sh       # Starts SSH + MCP server
├── .env.example        # Required secrets template
├── scripts/
│   ├── run-tests.sh    # Shell-based E2E test runner
│   └── tui-test.py     # pexpect-based TUI automation
└── ci/
    └── real-tests.yml  # GitHub Actions workflow
```

## What Gets Tested

| Suite | What | How |
|-------|------|-----|
| Binary Verification | CLI/TUI/MCP binaries exist and respond | docker exec |
| SSH Access | Login, locale, PATH | sshpass + assertions |
| TUI Startup | TUI renders, responds to 'q', exits cleanly | pexpect over SSH |
| Praxis Loading | Constraint files present, validate command works | file checks + CLI |
| PluresDB | Put/get operations | CLI subcommands |
| MCP Server | Server starts, responds to health checks | curl/process checks |

## Adding Tests

### Shell tests (run-tests.sh)
Add a new section following the pattern:
```bash
# Test N: description
RESULT=$(docker compose exec -T pares-radix some-command 2>&1)
if echo "$RESULT" | grep -q "expected"; then
    pass "test description"
else
    fail_test "test description: $RESULT"
fi
```

### TUI tests (tui-test.py)
Add a method to `TUITestRunner`:
```python
def test_my_feature(self):
    print("\n[Suite: My Feature]")
    self.assert_output("does thing", "command", r"expected_regex")
```

## CI

The GitHub Actions workflow (`ci/real-tests.yml`) requires these repository secrets:
- `OPENAI_API_KEY` — for MCP server LLM operations
- `GITHUB_TOKEN` — auto-provided, used for repo operations

Copy to `.github/workflows/real-tests.yml` to activate.
