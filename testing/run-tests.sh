#!/usr/bin/env bash
# run-tests.sh — Build and run the full E2E test suite locally.
#
# Usage:
#   ./testing/run-tests.sh          # build + test
#   ./testing/run-tests.sh --skip-build  # reuse existing image
#   ./testing/run-tests.sh --teardown    # clean up after
#
# Requirements: docker, docker compose v2
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

SKIP_BUILD=false
TEARDOWN=false

for arg in "$@"; do
    case $arg in
        --skip-build) SKIP_BUILD=true ;;
        --teardown) TEARDOWN=true ;;
    esac
done

# ─── Teardown mode ───────────────────────────────────────────────────────────
if [ "$TEARDOWN" = true ]; then
    echo "▸ Tearing down test infrastructure..."
    docker compose --profile test down -v --remove-orphans
    exit 0
fi

# ─── Verify secrets ─────────────────────────────────────────────────────────
if [ ! -f testing/.env ]; then
    echo "ERROR: testing/.env not found. Copy testing/.env.example and fill in keys."
    exit 1
fi

# ─── Build ───────────────────────────────────────────────────────────────────
if [ "$SKIP_BUILD" = false ]; then
    echo "▸ Building pares-radix container..."
    docker compose build pares-radix

    echo "▸ Building test runner..."
    docker compose --profile test build test-runner
fi

# ─── Start service ───────────────────────────────────────────────────────────
echo "▸ Starting pares-radix service..."
docker compose up -d pares-radix

# ─── Wait for healthy ────────────────────────────────────────────────────────
echo "▸ Waiting for service to be healthy..."
for i in $(seq 1 30); do
    if docker compose exec pares-radix pares-radix --version &>/dev/null; then
        echo "  ✓ Service healthy after ${i}s"
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "  ✗ Service failed to become healthy"
        docker compose logs pares-radix
        docker compose down
        exit 1
    fi
    sleep 1
done

# ─── Run tests ───────────────────────────────────────────────────────────────
echo "▸ Running E2E test suite..."
docker compose --profile test run --rm test-runner
TEST_EXIT=$?

# ─── Report ──────────────────────────────────────────────────────────────────
if [ $TEST_EXIT -eq 0 ]; then
    echo ""
    echo "═══════════════════════════════════════"
    echo "  ✓ ALL TESTS PASSED"
    echo "═══════════════════════════════════════"
else
    echo ""
    echo "═══════════════════════════════════════"
    echo "  ✗ TESTS FAILED (exit code: $TEST_EXIT)"
    echo "═══════════════════════════════════════"
    echo ""
    echo "Logs:"
    docker compose logs pares-radix | tail -30
fi

# ─── Cleanup (optional) ─────────────────────────────────────────────────────
# Leave running for debugging. Use --teardown to clean up.
echo ""
echo "▸ Services still running. Use './testing/run-tests.sh --teardown' to clean up."
echo "▸ SSH access: ssh -p 2222 radix@localhost (password: radix-test-pw)"

exit $TEST_EXIT
