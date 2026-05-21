#!/bin/bash
# entrypoint.sh — starts sshd + optionally pares-radix serve
set -e

echo "▸ Starting SSH daemon..."
/usr/sbin/sshd

echo "▸ pares-radix $(pares-radix --version 2>/dev/null || echo 'unknown')"

# If PARES_AUTOSTART=serve, run the serve command in background
if [ "${PARES_AUTOSTART}" = "serve" ]; then
    echo "▸ Starting pares-radix serve..."
    exec su -c "pares-radix serve \
        --model-url ${PARES_MODEL_URL:-https://models.inference.ai.azure.com} \
        --model ${PARES_MODEL:-claude-sonnet-4.5}" radix
elif [ "${PARES_AUTOSTART}" = "tui" ]; then
    echo "▸ Starting pares-radix tui..."
    exec su -c "pares-radix tui \
        --model-url ${PARES_MODEL_URL:-https://models.inference.ai.azure.com} \
        --model ${PARES_MODEL:-claude-sonnet-4.5}" radix
else
    # Default: keep container alive for SSH access, user starts TUI manually
    echo "▸ Ready. SSH into port 22 as user 'radix' to use the TUI."
    echo "▸ Or run: docker exec -it <container> pares-radix tui"
    # Keep alive
    exec tail -f /dev/null
fi
