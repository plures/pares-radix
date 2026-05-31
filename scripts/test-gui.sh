#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "═══════════════════════════════════════════════════════"
echo "  Layer 1: Vitest (component/unit tests)"
echo "═══════════════════════════════════════════════════════"
pnpm run test

echo ""
echo "═══════════════════════════════════════════════════════"
echo "  Layer 2: Playwright (E2E tests)"
echo "═══════════════════════════════════════════════════════"
pnpm run test:e2e

echo ""
echo "═══════════════════════════════════════════════════════"
echo "  Layer 3: Cargo (Rust unit tests)"
echo "═══════════════════════════════════════════════════════"
cd src-tauri
cargo test

echo ""
echo "═══════════════════════════════════════════════════════"
echo "  ✅ All test layers passed"
echo "═══════════════════════════════════════════════════════"
