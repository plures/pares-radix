#!/bin/bash
# testing/scripts/doctor.sh — Diagnose testing environment readiness
#
# Reports which components are available and what's missing.
# Exits 0 if at least local tests can run, 1 if nothing works.
#
# Usage: ./testing/scripts/doctor.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTING_DIR="$(dirname "$SCRIPT_DIR")"
REPO_DIR="$(dirname "$TESTING_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; }
info() { echo -e "  ${BLUE}ℹ${NC} $1"; }

CAN_LOCAL=false
CAN_DOCKER=false
CAN_SSH=false
ISSUES=0

echo "═══ pares-radix Testing Infrastructure Doctor ═══"
echo ""

# ─── Binary ───────────────────────────────────────────────────────────────────
echo "[Binary]"
BINARY="$REPO_DIR/target/release/pares-radix"
if [ -f "$BINARY" ]; then
    VERSION=$("$BINARY" --version 2>&1 || echo "ERROR")
    if echo "$VERSION" | grep -q "pares-radix"; then
        ok "Release binary: $VERSION"
        CAN_LOCAL=true
    else
        fail "Binary exists but --version failed: $VERSION"
        ISSUES=$((ISSUES + 1))
    fi
else
    warn "No release binary at $BINARY"
    info "Build with: cargo build --release -p pares-radix-cli"
    ISSUES=$((ISSUES + 1))
fi

# ─── Python / pytest ──────────────────────────────────────────────────────────
echo ""
echo "[Python Test Dependencies]"

if command -v python3 &>/dev/null; then
    ok "python3: $(python3 --version 2>&1)"
else
    fail "python3 not found"
    ISSUES=$((ISSUES + 1))
fi

if command -v pytest &>/dev/null; then
    ok "pytest: $(pytest --version 2>&1 | head -1)"
else
    warn "pytest not found (pip install pytest pytest-timeout pexpect paramiko hypothesis)"
    ISSUES=$((ISSUES + 1))
fi

for pkg in pexpect paramiko hypothesis; do
    if python3 -c "import $pkg" 2>/dev/null; then
        ok "$pkg available"
    else
        warn "$pkg missing (pip install $pkg)"
    fi
done

# ─── Docker ───────────────────────────────────────────────────────────────────
echo ""
echo "[Docker]"

if command -v docker &>/dev/null; then
    ok "docker client: $(docker --version 2>&1)"
    if docker info &>/dev/null 2>&1; then
        ok "Docker daemon running"
        CAN_DOCKER=true
    else
        warn "Docker daemon NOT running (can't build/run containers)"
        info "Start with: sudo systemctl start docker"
        if [ ! -S /var/run/docker.sock ]; then
            info "Socket /var/run/docker.sock does not exist"
        fi
        if ! command -v dockerd &>/dev/null; then
            info "dockerd binary not found — may need full Docker Engine install"
        fi
    fi
else
    warn "docker not installed"
    info "Install: https://docs.docker.com/engine/install/"
fi

if command -v docker-compose &>/dev/null || docker compose version &>/dev/null 2>&1; then
    ok "docker compose available"
else
    warn "docker compose not available"
fi

# ─── SSH Tools ────────────────────────────────────────────────────────────────
echo ""
echo "[SSH/TUI Automation]"

if command -v ssh &>/dev/null; then
    ok "ssh client available"
    CAN_SSH=true
else
    warn "ssh client not found"
fi

if command -v sshpass &>/dev/null; then
    ok "sshpass available (for automated SSH login)"
else
    warn "sshpass not installed (apt install sshpass)"
    info "Needed for Docker TUI tests over SSH"
fi

# ─── .env Configuration ──────────────────────────────────────────────────────
echo ""
echo "[Configuration]"

if [ -f "$TESTING_DIR/.env" ]; then
    ok ".env file exists"
    # Check for API key (without revealing it)
    if grep -q "^PARES_API_KEY=.\+" "$TESTING_DIR/.env" 2>/dev/null; then
        ok "PARES_API_KEY configured"
    else
        warn "PARES_API_KEY empty (live inference tests will skip)"
    fi
else
    warn ".env file not found"
    info "Copy: cp $TESTING_DIR/.env.example $TESTING_DIR/.env"
fi

# ─── Rust Toolchain ──────────────────────────────────────────────────────────
echo ""
echo "[Rust Toolchain]"

if command -v cargo &>/dev/null; then
    ok "cargo: $(cargo --version 2>&1)"
    ok "rustc: $(rustc --version 2>&1)"
else
    warn "Rust toolchain not found"
    info "Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

if command -v cargo-mutants &>/dev/null; then
    ok "cargo-mutants: $(cargo-mutants --version 2>&1)"
else
    info "cargo-mutants not installed (for mutation testing)"
fi

# ─── Summary ─────────────────────────────────────────────────────────────────
echo ""
echo "═══ Summary ═══"

if [ "$CAN_LOCAL" = true ]; then
    ok "LOCAL tests: READY (make test)"
else
    fail "LOCAL tests: NOT READY (need binary + pytest)"
fi

if [ "$CAN_DOCKER" = true ]; then
    ok "DOCKER E2E tests: READY (make test-docker)"
else
    warn "DOCKER E2E tests: NOT AVAILABLE (daemon not running)"
fi

if [ "$CAN_LOCAL" = true ] && [ "$CAN_DOCKER" = true ]; then
    echo ""
    ok "Full test suite available!"
    echo "  Run: cd testing && make test        # local only"
    echo "  Run: cd testing && make test-docker  # full E2E"
    exit 0
elif [ "$CAN_LOCAL" = true ]; then
    echo ""
    ok "Local tests available (Docker tests unavailable)"
    echo "  Run: cd testing && make test"
    exit 0
else
    echo ""
    fail "No tests can run. Fix issues above."
    exit 1
fi
