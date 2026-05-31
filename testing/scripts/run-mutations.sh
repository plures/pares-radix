#!/usr/bin/env bash
# Run mutation testing on priority modules and produce a summary report.
# Usage: ./testing/scripts/run-mutations.sh [module]
# Modules: praxis-rule, praxis-factory, core-chronos, core-procedure, core-memory, all
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

TIMEOUT=120
JOBS=2
OUT_DIR="mutants.out"

declare -A MODULES=(
  ["praxis-rule"]="pares-radix-praxis:crates/praxis/src/rule.rs"
  ["praxis-factory"]="pares-radix-praxis:crates/praxis/src/factory.rs"
  ["core-chronos"]="pares-agens-core:crates/core/src/chronos.rs"
  ["core-procedure"]="pares-agens-core:crates/core/src/procedure.rs"
  ["core-memory"]="pares-agens-core:crates/core/src/memory/store.rs"
  ["privacy"]="pares-agens-privacy:crates/privacy/src/lib.rs"
  ["core-ledger"]="pares-agens-core:crates/core/src/praxis/ledger.rs"
  ["core-cerebellum"]="pares-agens-core:crates/core/src/cerebellum/pipeline.rs"
)

run_module() {
  local name="$1"
  local spec="${MODULES[$name]}"
  local package="${spec%%:*}"
  local file="${spec#*:}"
  
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "🧬 Mutation testing: $name"
  echo "   Package: $package"
  echo "   File:    $file"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  
  local module_out="$OUT_DIR/$name"
  mkdir -p "$module_out"
  
  cargo mutants \
    --package "$package" \
    -f "$file" \
    --timeout "$TIMEOUT" \
    -j "$JOBS" \
    --output "$module_out" 2>&1
  
  echo ""
  echo "📊 Results for $name:"
  if [ -f "$module_out/caught.txt" ]; then
    local caught
    caught=$(wc -l < "$module_out/caught.txt")
    echo "   ✅ Caught: $caught"
  fi
  if [ -f "$module_out/missed.txt" ]; then
    local missed
    missed=$(wc -l < "$module_out/missed.txt")
    echo "   ❌ Missed: $missed"
    if [ "$missed" -gt 0 ]; then
      echo "   Missed mutants:"
      sed 's/^/      /' "$module_out/missed.txt"
    fi
  fi
  if [ -f "$module_out/timeout.txt" ]; then
    local timeout
    timeout=$(wc -l < "$module_out/timeout.txt")
    echo "   ⏱️  Timeout: $timeout"
  fi
  if [ -f "$module_out/unviable.txt" ]; then
    local unviable
    unviable=$(wc -l < "$module_out/unviable.txt")
    echo "   🚫 Unviable: $unviable"
  fi
  echo ""
}

TARGET="${1:-all}"

if [ "$TARGET" = "all" ]; then
  for module in "${!MODULES[@]}"; do
    run_module "$module"
  done
else
  if [ -z "${MODULES[$TARGET]+x}" ]; then
    echo "Unknown module: $TARGET"
    echo "Available: ${!MODULES[*]}"
    exit 1
  fi
  run_module "$TARGET"
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✨ Mutation testing complete. Results in $OUT_DIR/"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
