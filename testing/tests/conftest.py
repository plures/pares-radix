"""
conftest.py — shared fixtures for pares-radix E2E tests.

Provides:
- SSH connection to the radix container via paramiko/pexpect (Docker tests)
- MCP client fixture for tests that exercise MCP JSON-RPC tools locally
"""
import json
import os
import select
import subprocess
import time
import uuid
from pathlib import Path

import pytest


# ── MCP client fixture (local, no Docker required) ────────────────────────────

REPO_ROOT = Path(__file__).parent.parent.parent
RADIX_BIN = os.environ.get(
    "RADIX_BINARY",
    str(REPO_ROOT / "target" / "release" / "pares-radix"),
)


class McpClient:
    """MCP JSON-RPC client over stdio."""

    def __init__(self, workdir=None):
        self.workdir = workdir or f"/tmp/radix-test-{uuid.uuid4().hex[:8]}"
        os.makedirs(self.workdir, exist_ok=True)
        self.proc = None
        self._next_id = 1

    def start(self):
        self.proc = subprocess.Popen(
            [RADIX_BIN, "mcp-serve", "--workdir", self.workdir],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        # Initialize handshake
        self._send("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "conftest-mcp", "version": "1.0.0"},
        })
        resp = self._read(timeout=5)
        assert resp is not None, "MCP server failed to respond to initialize"
        assert "result" in resp, f"Initialize failed: {resp}"

        # Send initialized notification
        self.proc.stdin.write(json.dumps({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }) + "\n")
        self.proc.stdin.flush()
        time.sleep(0.3)
        return self

    def stop(self):
        if self.proc:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()

    def call_tool(self, tool_name, arguments=None, timeout=10):
        """Call an MCP tool and return the result content."""
        self._send("tools/call", {
            "name": tool_name,
            "arguments": arguments or {},
        })
        resp = self._read(timeout=timeout)
        if resp is None:
            return None
        if "error" in resp:
            return {"error": resp["error"]}
        if "result" in resp:
            result = resp["result"]
            if "content" in result:
                texts = [c.get("text", "") for c in result["content"] if c.get("type") == "text"]
                combined = "\n".join(texts)
                try:
                    return json.loads(combined)
                except (json.JSONDecodeError, TypeError):
                    return combined
            return result
        return resp

    def _send(self, method, params=None):
        req = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            req["params"] = params
        self._next_id += 1
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()

    def _read(self, timeout=5):
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            return None
        line = self.proc.stdout.readline()
        if line:
            try:
                return json.loads(line.strip())
            except json.JSONDecodeError:
                return {"raw": line.strip()}
        return None


@pytest.fixture(scope="module")
def mcp():
    """Module-scoped MCP client (one server per test module)."""
    if not os.path.isfile(RADIX_BIN):
        pytest.skip(f"Binary not found: {RADIX_BIN}")
    client = McpClient()
    client.start()
    yield client
    client.stop()

# Guard imports — these are only needed for Docker-based tests
try:
    import paramiko
    import pexpect

    HAS_SSH_DEPS = True
except ImportError:
    HAS_SSH_DEPS = False


SSH_HOST = os.environ.get("RADIX_SSH_HOST", "localhost")
SSH_PORT = int(os.environ.get("RADIX_SSH_PORT", "2222"))
SSH_USER = os.environ.get("RADIX_SSH_USER", "radix")
SSH_PASS = os.environ.get("RADIX_SSH_PASS", "radix-test-pw")


@pytest.fixture(scope="session")
def ssh_client():
    """Paramiko SSH client connected to the radix container."""
    if not HAS_SSH_DEPS:
        pytest.skip("paramiko not installed — Docker tests require: pip install paramiko pexpect")
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    # Retry connection (container may still be starting)
    for attempt in range(3):
        try:
            client.connect(
                SSH_HOST, port=SSH_PORT, username=SSH_USER, password=SSH_PASS, timeout=3
            )
            break
        except Exception:
            if attempt == 2:
                pytest.skip(
                    f"Cannot connect to SSH at {SSH_HOST}:{SSH_PORT} — "
                    "Docker container not running (use 'make docker-test')"
                )
            time.sleep(1)

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
    if not HAS_SSH_DEPS:
        pytest.skip("pexpect not installed — Docker tests require: pip install pexpect")
    cmd = (
        f"sshpass -p '{SSH_PASS}' ssh -o StrictHostKeyChecking=no "
        f"-p {SSH_PORT} {SSH_USER}@{SSH_HOST} "
        f"pares-radix tui --model-url https://models.inference.ai.azure.com"
    )
    child = pexpect.spawn(cmd, encoding="utf-8", timeout=30)
    yield child
    child.close()
