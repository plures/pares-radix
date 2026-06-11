#!/usr/bin/env bash
# sync-dev-guide-procedures.sh — Pull development-guide procedures to local praxis dir
#
# Run on praxisbot to sync the latest .px procedures from the development-guide repo.
# Called during deploy or via heartbeat when dev-guide repo changes.
#
# Usage:
#   ./scripts/sync-dev-guide-procedures.sh [target-dir]
#
# Default target: ~/.pares-radix/praxis/procedures/org/

set -euo pipefail

TARGET="${1:-$HOME/.pares-radix/praxis/procedures/org}"
REPO_URL="https://github.com/plures/development-guide.git"
REPO_DIR="${TMPDIR:-/tmp}/dev-guide-sync"

echo "==> Syncing dev-guide procedures to $TARGET"

# Clone/pull the repo
if [ -d "$REPO_DIR/.git" ]; then
  cd "$REPO_DIR" && git pull --ff-only origin main
else
  rm -rf "$REPO_DIR"
  git clone --depth 1 --branch main "$REPO_URL" "$REPO_DIR"
fi

# Ensure target exists
mkdir -p "$TARGET"

# Copy .px files
cp "$REPO_DIR/procedures/"*.px "$TARGET/"

# Count what we synced
COUNT=$(ls "$TARGET"/*.px 2>/dev/null | wc -l)
echo "==> Synced $COUNT .px files to $TARGET"

# Also copy the manifest for tooling
mkdir -p "$(dirname "$TARGET")/constraints"
cp "$REPO_DIR/constraints/plures-dev-guide.json" "$(dirname "$TARGET")/constraints/" 2>/dev/null || true

echo "==> Done"
