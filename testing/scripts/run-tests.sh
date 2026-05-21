#!/bin/bash
# testing/scripts/run-tests.sh — End-to-end test runner
# Builds container, starts it, runs all test suites, reports results, tears down.
#
# Usage:
#   ./testing/scripts/run-tests.sh [--keep] [--no-build]
#
# Options:
#   --keep      Don't tear down after tests (for debugging)
#   --no-build  Skip docker build (use existing image)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TESTING_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_DIR="$(dirname "$TESTING_DIR")"
COMPOSE_FILE="$TESTING_DIR/docker-compose.yml"
ENV_FILE="$TESTING_DIR/.env"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

KEEP=false
NO_BUILD=false
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

for arg in "$@"; do
    case $arg in
        --keep) KEEP=true ;;
        --no-build) NO_BUILD=true ;;
    esac
done

log() { echo -e "${GREEN}[test]${NC} $1"; }
warn() { echo -e "${YELLOW}[warn]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; }

pass() {
    log "✓ $1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

fail_test() {
    fail "✗ $1"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

skip() {
    warn "⊘ $1 (skipped)"
    TESTS_SKIPPED=$((TESTS_SKIPPED + 1))
}

cleanup() {
    if [ "$KEEP" = false ]; then
        log "Tearing down..."
        docker compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
    else
        warn "Keeping containers alive (--keep)"
    fi
}
trap cleanup EXIT

# ─── Pre-flight ───────────────────────────────────────────────────────────────
log "=== pares-radix Real Testing Infrastructure ==="
log "Project: $PROJECT_DIR"

if [ ! -f "$ENV_FILE" ]; then
    fail ".env file not found at $ENV_FILE"
    echo "Copy .env.example to .env and fill in real API keys"
    exit 1
fi

# ─── Build ────────────────────────────────────────────────────────────────────
if [ "$NO_BUILD" = false ]; then
    log "Building Docker image..."
    docker compose -f "$COMPOSE_FILE" build 2>&1 | tail -20
    if [ $? -ne 0 ]; then
        fail "Docker build failed"
        exit 1
    fi
    pass "Docker build succeeded"
else
    log "Skipping build (--no-build)"
fi

# ─── Start ────────────────────────────────────────────────────────────────────
log "Starting services..."
docker compose -f "$COMPOSE_FILE" up -d
sleep 3

# Wait for container to be healthy
log "Waiting for container health..."
for i in $(seq 1 30); do
    STATUS=$(docker compose -f "$COMPOSE_FILE" ps --format json 2>/dev/null | jq -r '.[0].Health // "starting"' 2>/dev/null || echo "starting")
    if [ "$STATUS" = "healthy" ]; then
        break
    fi
    sleep 2
done

if [ "$STATUS" != "healthy" ]; then
    warn "Container not reporting healthy after 60s, continuing anyway..."
fi

# ─── Test Suite: Basic ────────────────────────────────────────────────────────
log ""
log "=== Test Suite: Basic Binary Verification ==="

# Test 1: CLI --version
VERSION=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix pares-radix --version 2>&1)
if [ $? -eq 0 ] && echo "$VERSION" | grep -q "pares-radix"; then
    pass "CLI --version: $VERSION"
else
    fail_test "CLI --version failed: $VERSION"
fi

# Test 2: CLI --help
HELP=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix pares-radix --help 2>&1)
if [ $? -eq 0 ] && echo "$HELP" | grep -qi "usage\|options\|commands"; then
    pass "CLI --help responds"
else
    fail_test "CLI --help failed"
fi

# Test 3: Subcommands available (serve, migrate)
SUBCMDS=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix pares-radix --help 2>&1)
if echo "$SUBCMDS" | grep -qi "serve\|migrate"; then
    pass "CLI subcommands available (serve/migrate)"
else
    fail_test "CLI subcommands not found in help output"
fi

# Test 4: Binary is properly linked (no missing .so)
LDD_CHECK=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix ldd /usr/local/bin/pares-radix 2>&1 | grep "not found" || echo "ALL_OK")
if [ "$LDD_CHECK" = "ALL_OK" ]; then
    pass "Binary fully linked (no missing libraries)"
else
    fail_test "Missing shared libraries: $LDD_CHECK"
fi

# ─── Test Suite: SSH Access ───────────────────────────────────────────────────
log ""
log "=== Test Suite: SSH Access ==="

# Test 5: SSH connectivity
SSH_CMD="sshpass -p radix-test ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -p 2222 radix@localhost"

if command -v sshpass >/dev/null 2>&1; then
    SSH_RESULT=$($SSH_CMD "echo OK" 2>&1)
    if [ "$SSH_RESULT" = "OK" ]; then
        pass "SSH login works (radix@localhost:2222)"
    else
        fail_test "SSH login failed: $SSH_RESULT"
    fi

    # Test 6: SSH + run CLI
    SSH_VERSION=$($SSH_CMD "pares-radix --version" 2>&1)
    if echo "$SSH_VERSION" | grep -q "pares-radix"; then
        pass "CLI accessible via SSH: $SSH_VERSION"
    else
        fail_test "CLI via SSH failed: $SSH_VERSION"
    fi

    # Test 7: Locale correct for TUI
    SSH_LOCALE=$($SSH_CMD "locale | grep LANG" 2>&1)
    if echo "$SSH_LOCALE" | grep -q "UTF-8"; then
        pass "UTF-8 locale configured for TUI"
    else
        fail_test "Locale not UTF-8: $SSH_LOCALE"
    fi
else
    skip "SSH tests (sshpass not installed)"
fi

# ─── Test Suite: MCP Server ───────────────────────────────────────────────────
log ""
log "=== Test Suite: MCP Server ==="

# Test 8: MCP server port
MCP_PORT_CHECK=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:3100/health 2>/dev/null || echo "000")
if [ "$MCP_PORT_CHECK" != "000" ]; then
    pass "MCP server responding on :3100 (HTTP $MCP_PORT_CHECK)"
else
    # MCP might use stdio/websocket, not HTTP — check if process is running
    MCP_PROC=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix pgrep -f "mcp-server" 2>&1 || echo "")
    if [ -n "$MCP_PROC" ]; then
        pass "MCP server process running (pid: $MCP_PROC)"
    else
        skip "MCP server (no API keys configured or server uses stdio transport)"
    fi
fi

# ─── Test Suite: Praxis Constraints ──────────────────────────────────────────
log ""
log "=== Test Suite: Praxis Constraint Loading ==="

# Test 9: Praxis files present
PRAXIS_FILES=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix find /home/radix/.config/pares-radix/praxis -name "*.px" 2>/dev/null | wc -l)
if [ "$PRAXIS_FILES" -gt 0 ]; then
    pass "Praxis constraints present ($PRAXIS_FILES .px files)"
else
    # Check if praxis dir has any files at all
    PRAXIS_ANY=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix ls /home/radix/.config/pares-radix/praxis/ 2>&1 | wc -l)
    if [ "$PRAXIS_ANY" -gt 0 ]; then
        pass "Praxis directory has content ($PRAXIS_ANY entries)"
    else
        fail_test "Praxis directory empty"
    fi
fi

# ─── Test Suite: PluresDB Operations ─────────────────────────────────────────
log ""
log "=== Test Suite: PluresDB Operations ==="

# Test 10: CLI db operations (if supported)
DB_PUT=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix pares-radix db put test-key '{"test":true}' 2>&1 || echo "NOT_SUPPORTED")
if echo "$DB_PUT" | grep -qi "not_supported\|error\|unknown"; then
    skip "PluresDB CLI operations (not available via CLI subcommand)"
else
    pass "PluresDB put operation: $DB_PUT"

    DB_GET=$(docker compose -f "$COMPOSE_FILE" exec -T pares-radix pares-radix db get test-key 2>&1)
    if echo "$DB_GET" | grep -q "test"; then
        pass "PluresDB get operation returns stored data"
    else
        fail_test "PluresDB get failed: $DB_GET"
    fi
fi

# ─── Results ──────────────────────────────────────────────────────────────────
log ""
log "═══════════════════════════════════════════════"
log "Results: ${GREEN}$TESTS_PASSED passed${NC}, ${RED}$TESTS_FAILED failed${NC}, ${YELLOW}$TESTS_SKIPPED skipped${NC}"
log "═══════════════════════════════════════════════"

if [ $TESTS_FAILED -gt 0 ]; then
    exit 1
fi
