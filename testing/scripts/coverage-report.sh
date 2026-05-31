#!/usr/bin/env bash
# Generate a mutation testing coverage report from existing results.
# Usage: ./testing/scripts/coverage-report.sh [mutants-dir]
# Defaults to mutants.out/ in the repo root.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/mutants.out}"

if [ ! -d "$OUT_DIR" ]; then
  echo "No mutation results found at $OUT_DIR"
  echo "Run ./testing/scripts/run-mutations.sh first."
  exit 1
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 Mutation Testing Coverage Report"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

TOTAL_CAUGHT=0
TOTAL_MISSED=0
TOTAL_TIMEOUT=0
TOTAL_UNVIABLE=0

printf "%-20s %8s %8s %8s %8s %10s\n" "Module" "Caught" "Missed" "Timeout" "Unviable" "Kill Rate"
printf "%-20s %8s %8s %8s %8s %10s\n" "────────────────────" "────────" "────────" "────────" "────────" "──────────"

for dir in "$OUT_DIR"/*/; do
  [ -d "$dir" ] || continue
  name="$(basename "$dir")"
  
  caught=0
  missed=0
  timeout=0
  unviable=0
  
  [ -f "$dir/caught.txt" ] && caught=$(wc -l < "$dir/caught.txt")
  [ -f "$dir/missed.txt" ] && missed=$(wc -l < "$dir/missed.txt")
  [ -f "$dir/timeout.txt" ] && timeout=$(wc -l < "$dir/timeout.txt")
  [ -f "$dir/unviable.txt" ] && unviable=$(wc -l < "$dir/unviable.txt")
  
  viable=$((caught + missed))
  if [ "$viable" -gt 0 ]; then
    kill_rate=$(echo "scale=1; $caught * 100 / $viable" | bc)
    kill_str="${kill_rate}%"
  else
    kill_str="N/A"
  fi
  
  printf "%-20s %8d %8d %8d %8d %10s\n" "$name" "$caught" "$missed" "$timeout" "$unviable" "$kill_str"
  
  TOTAL_CAUGHT=$((TOTAL_CAUGHT + caught))
  TOTAL_MISSED=$((TOTAL_MISSED + missed))
  TOTAL_TIMEOUT=$((TOTAL_TIMEOUT + timeout))
  TOTAL_UNVIABLE=$((TOTAL_UNVIABLE + unviable))
done

echo ""
printf "%-20s %8d %8d %8d %8d" "TOTAL" "$TOTAL_CAUGHT" "$TOTAL_MISSED" "$TOTAL_TIMEOUT" "$TOTAL_UNVIABLE"

TOTAL_VIABLE=$((TOTAL_CAUGHT + TOTAL_MISSED))
if [ "$TOTAL_VIABLE" -gt 0 ]; then
  TOTAL_KILL=$(echo "scale=1; $TOTAL_CAUGHT * 100 / $TOTAL_VIABLE" | bc)
  printf " %10s\n" "${TOTAL_KILL}%"
else
  printf " %10s\n" "N/A"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Report missed mutants in detail if any
if [ "$TOTAL_MISSED" -gt 0 ]; then
  echo ""
  echo "❌ MISSED MUTANTS (gaps needing tests):"
  echo ""
  for dir in "$OUT_DIR"/*/; do
    [ -d "$dir" ] || continue
    if [ -f "$dir/missed.txt" ] && [ -s "$dir/missed.txt" ]; then
      name="$(basename "$dir")"
      echo "  [$name]"
      sed 's/^/    /' "$dir/missed.txt"
      echo ""
    fi
  done
fi
