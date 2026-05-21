"""
conftest.py — shared fixtures for pares-radix E2E tests.

Provides SSH connection to the radix container via paramiko/pexpect.
"""
import os
import time

import paramiko
import pexpect
import pytest


SSH_HOST = os.environ.get("RADIX_SSH_HOST", "localhost")
SSH_PORT = int(os.environ.get("RADIX_SSH_PORT", "2222"))
SSH_USER = os.environ.get("RADIX_SSH_USER", "radix")
SSH_PASS = os.environ.get("RADIX_SSH_PASS", "radix-test-pw")


@pytest.fixture(scope="session")
def ssh_client():
    """Paramiko SSH client connected to the radix container."""
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    # Retry connection (container may still be starting)
    for attempt in range(10):
        try:
            client.connect(
                SSH_HOST, port=SSH_PORT, username=SSH_USER, password=SSH_PASS, timeout=5
            )
            break
        except Exception:
            if attempt == 9:
                raise
            time.sleep(2)

    yield client
    client.close()


@pytest.fixture
def ssh_exec(ssh_client):
    """Helper: execute a command via SSH and return (stdout, stderr, exit_code)."""
    def _exec(cmd: str, timeout: float = 30) -> tuple[str, str, int]:
        stdin, stdout, stderr = ssh_client.exec_command(cmd, timeout=timeout)
        exit_code = stdout.channel.recv_exit_status()
        return stdout.read().decode(), stderr.read().decode(), exit_code

    return _exec


@pytest.fixture
def tui_session():
    """
    Spawn an interactive TUI session over SSH using pexpect.
    Returns a pexpect.spawn instance connected to pares-radix tui.
    """
    cmd = (
        f"sshpass -p '{SSH_PASS}' ssh -o StrictHostKeyChecking=no "
        f"-p {SSH_PORT} {SSH_USER}@{SSH_HOST} "
        f"pares-radix tui --model-url https://models.inference.ai.azure.com"
    )
    child = pexpect.spawn(cmd, encoding="utf-8", timeout=30)
    yield child
    child.close()
