#!/bin/bash
# testing/run-local-tests.sh — Run tests that don't require Docker
#
# These tests validate:
#   - Binary builds and responds to subcommands
#   - Praxis .px files are well-formed
#   - MCP server protocol (if available)
#
# Prerequisites: pre-built binary at target/release/pares-radix
# Usage: ./testing/run-local-tests.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$REPO_ROOT/target/release/pares-radix"

echo "═══ Pares Radix Local Tests ═══"
echo "Binary: $BINARY"

# Check binary exists
if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: Binary not found. Run: cargo build --release -p pares-radix-cli"
    exit 1
fi

echo "Version: $($BINARY --version)"
echo ""

# Run local tests (skip Docker-only tests)
export RADIX_BINARY="$BINARY"
cd "$REPO_ROOT"

echo "Running test_local_binary.py..."
pytest testing/tests/test_local_binary.py -v --tb=short 2>&1

echo ""
echo "Running test_praxis_constraints.py..."
pytest testing/tests/test_praxis_constraints.py -v --tb=short 2>&1

echo ""
echo "Running test_px_runtime.py..."
pytest testing/tests/test_px_runtime.py -v --tb=short 2>&1

echo ""
echo "Running test_config_and_serve.py..."
pytest testing/tests/test_config_and_serve.py -v --tb=short 2>&1

echo ""
echo "Running test_pluresdb_integration.py..."
pytest testing/tests/test_pluresdb_integration.py -v --tb=short 2>&1

echo ""
echo "Running test_ask_subcommand.py..."
pytest testing/tests/test_ask_subcommand.py -v --tb=short 2>&1

echo ""
echo "Running test_serve_longrunning.py..."
pytest testing/tests/test_serve_longrunning.py -v --tb=short 2>&1

echo ""
echo "Running test_tui.py (local tests only)..."
pytest testing/tests/test_tui.py -v --tb=short -m "not docker" 2>&1

echo ""
echo "═══ All local tests passed ═══"
