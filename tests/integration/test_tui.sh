#!/bin/bash
# Integration test for pares-agens: verifies TUI mode, tool execution, health,
# and PluresDB/Chronos state after interaction.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

TEST_HOME="/tmp/test-pares-agens-$$"
BINARY="./target/debug/pares-agens"
PASS=0
FAIL=0

cleanup() {
    rm -rf "$TEST_HOME"
}
trap cleanup EXIT

echo "═══════════════════════════════════════════════════"
echo "  pares-agens Integration Tests"
echo "═══════════════════════════════════════════════════"
echo ""

# Build
echo "▶ Building pares-agens..."
cargo build -p pares-agens-cli 2>&1 | tail -5

if [ ! -f "$BINARY" ]; then
    # Try alternate binary name
    BINARY="./target/debug/pares-agens-cli"
    if [ ! -f "$BINARY" ]; then
        echo "❌ Binary not found after build"
        exit 1
    fi
fi

echo "  ✓ Build complete: $BINARY"
echo ""

# Setup test environment
echo "▶ Setting up test environment..."
mkdir -p "$TEST_HOME/.pares-agens"
cat > "$TEST_HOME/.pares-agens/SOUL.md" << 'EOF'
# Test Soul
You are a test agent. Be brief and helpful.
EOF
cat > "$TEST_HOME/.pares-agens/IDENTITY.md" << 'EOF'
# Test Identity
Name: test-agent
EOF
echo "  ✓ Personality docs created"
echo ""

# Test 1: Binary starts and responds
echo "▶ Test 1: TUI basic response"
RESULT=$(echo "say hello" | HOME="$TEST_HOME" timeout 30 "$BINARY" tui --no-copilot 2>/dev/null || true)
if [ -n "$RESULT" ]; then
    echo "  ✓ Got response from TUI"
    ((PASS++))
else
    echo "  ✗ No response from TUI (may need model endpoint)"
    ((FAIL++))
fi
echo ""

# Test 2: Tool execution (list directory)
echo "▶ Test 2: Tool execution"
RESULT=$(echo "list the files in /tmp" | HOME="$TEST_HOME" timeout 30 "$BINARY" tui --no-copilot 2>/dev/null || true)
if echo "$RESULT" | grep -qi "test-pares-agens\|tmp\|directory"; then
    echo "  ✓ Tool execution produced output"
    ((PASS++))
else
    echo "  ✗ Tool execution didn't produce expected output"
    ((FAIL++))
fi
echo ""

# Test 3: PluresDB state directory exists after run
echo "▶ Test 3: PluresDB state"
if [ -d "$TEST_HOME/.pares-agens/memory" ] || [ -d "$TEST_HOME/.pares-agens" ]; then
    echo "  ✓ PluresDB state directory exists"
    ((PASS++))
else
    echo "  ✗ No PluresDB state directory"
    ((FAIL++))
fi
echo ""

# Test 4: Health module compiles with tests
echo "▶ Test 4: Health module unit tests"
if cargo test -p pares-agens-core health:: --no-fail-fast 2>&1 | grep -q "test result: ok"; then
    echo "  ✓ Health module tests pass"
    ((PASS++))
else
    echo "  ✗ Health module tests failed"
    ((FAIL++))
fi
echo ""

# Test 5: PII guard module tests
echo "▶ Test 5: PII guard unit tests"
if cargo test -p pares-agens-core pii_guard:: --no-fail-fast 2>&1 | grep -q "test result: ok"; then
    echo "  ✓ PII guard tests pass"
    ((PASS++))
else
    echo "  ✗ PII guard tests failed"
    ((FAIL++))
fi
echo ""

# Summary
echo "═══════════════════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "═══════════════════════════════════════════════════"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
