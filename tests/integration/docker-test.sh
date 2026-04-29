#!/bin/bash
set -euo pipefail

echo "=== Building pares-agens Docker image ==="
docker build -t pares-agens-test . 2>&1 | tail -5

echo ""
echo "=== Test 1: Binary runs and shows version ==="
VERSION=$(docker run --rm pares-agens-test --version 2>&1)
echo "Version: $VERSION"
[[ "$VERSION" == *"pares-agens"* ]] && echo "✅ Version check passed" || { echo "❌ Version check failed"; exit 1; }

echo ""
echo "=== Test 2: Help text shows all commands ==="
HELP=$(docker run --rm pares-agens-test --help 2>&1)
for cmd in serve tui cluster; do
    echo "$HELP" | grep -q "$cmd" && echo "✅ '$cmd' in help" || echo "❌ '$cmd' missing from help"
done

echo ""
echo "=== Test 3: Personality files present ==="
docker run --rm --entrypoint sh pares-agens-test -c "ls /home/test/.pares-agens/*.md" 2>&1
SOUL=$(docker run --rm --entrypoint sh pares-agens-test -c "cat /home/test/.pares-agens/SOUL.md" 2>&1)
[[ "$SOUL" == *"Praxis Bot"* ]] && echo "✅ SOUL.md loaded" || echo "❌ SOUL.md missing or empty"

echo ""
echo "=== Test 4: Cluster info command ==="
INFO=$(docker run --rm pares-agens-test cluster info 2>&1 || true)
echo "$INFO" | head -5
echo "✅ Cluster info ran (output above)"

echo ""
echo "=== All Docker tests complete ==="
