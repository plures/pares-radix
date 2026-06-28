"""
test_mcp_server.py — MCP dev-server protocol tests via NATIVE stdio.

REWRITTEN 2026-06-28: was CLI-over-SSH (`pares-radix mcp-server` through paramiko,
testing a ghost binary). pares-radix has NO traditional CLI. The real MCP surface is
packages/mcp-dev-server (@plures/radix-mcp-server, `tsx src/index.ts`), JSON-RPC 2.0
over stdio. We spawn it directly — its native transport, the way any MCP client uses
it — and assert initialize + tools/list. No Docker, no SSH, no mocks (C-TEST-002).
"""
import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
MCP_DIR = REPO_ROOT / "packages" / "mcp-dev-server"
ENTRY = MCP_DIR / "src" / "index.ts"

pytestmark = pytest.mark.skipif(
    shutil.which("npx") is None or not ENTRY.exists(),
    reason="MCP dev server (npx tsx) not available",
)


def _rpc(*requests, timeout=30):
    """Spawn the MCP server, send newline-delimited JSON-RPC, collect responses."""
    payload = "".join(json.dumps(r) + "\n" for r in requests)
    proc = subprocess.run(
        ["npx", "tsx", str(ENTRY)],
        input=payload,
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=str(MCP_DIR),
        env={**__import__("os").environ, "RADIX_DEV": "1"},
        shell=sys.platform == "win32",
    )
    out = []
    for line in proc.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


class TestMCPProtocol:
    def test_mcp_initialize(self):
        resp = next(r for r in _rpc({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                                     "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                                                "clientInfo": {"name": "test", "version": "1.0.0"}}})
                    if r.get("id") == 1)
        assert resp["jsonrpc"] == "2.0"
        assert resp["result"]["serverInfo"]["name"] == "radix-mcp-dev"

    def test_mcp_tools_list(self):
        resp = next(r for r in _rpc(
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        ) if r.get("id") == 2)
        tools = resp["result"]["tools"]
        assert isinstance(tools, list) and len(tools) > 0
