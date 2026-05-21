#!/bin/bash
# testing/entrypoint.sh — Start SSH and MCP server for real testing
set -e

echo "[entrypoint] Starting pares-radix testing container"
echo "[entrypoint] RUST_LOG=${RUST_LOG:-info}"

# Start SSH daemon
/usr/sbin/sshd -D &
SSHD_PID=$!
echo "[entrypoint] sshd started (pid=$SSHD_PID)"

# Wait for SSH to be ready
for i in $(seq 1 10); do
    if ssh-keyscan -p 22 localhost >/dev/null 2>&1; then
        echo "[entrypoint] SSH ready"
        break
    fi
    sleep 0.5
done

# Start MCP server as the radix user (if API keys are present)
if [ -n "$OPENAI_API_KEY" ] || [ -n "$ANTHROPIC_API_KEY" ]; then
    echo "[entrypoint] Starting pares-radix serve on :3100"
    su -c "RUST_LOG=${RUST_LOG:-info} pares-radix serve --port 3100 &" radix 2>/dev/null || \
        echo "[entrypoint] serve subcommand not available (may need telegram token)"
else
    echo "[entrypoint] No API keys found, serve mode not started"
    echo "[entrypoint] Set OPENAI_API_KEY or ANTHROPIC_API_KEY to enable"
fi

echo "[entrypoint] Container ready. SSH on :22, MCP on :3100"
echo "[entrypoint] Login: ssh radix@localhost -p <mapped-port> (password: radix-test)"

# Wait for sshd (keeps container alive)
wait $SSHD_PID
