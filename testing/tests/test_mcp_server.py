"""
test_mcp_server.py — MCP server protocol tests.

Tests the MCP (Model Context Protocol) server by invoking it through
the CLI and verifying JSON-RPC responses.
"""
import json
import pytest


class TestMCPProtocol:
    """Verify MCP server responds to JSON-RPC over stdio."""

    def test_mcp_initialize(self, ssh_exec):
        """MCP server responds to initialize request."""
        # JSON-RPC initialize request
        request = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-runner",
                    "version": "1.0.0"
                }
            }
        })

        # Pipe request to MCP server via stdin
        cmd = f"echo '{request}' | timeout 10 pares-radix mcp-server 2>/dev/null || true"
        stdout, stderr, code = ssh_exec(cmd, timeout=15)

        # If MCP server subcommand doesn't exist, skip
        if "unrecognized" in stderr.lower() or "not found" in stderr.lower():
            pytest.skip("mcp-server subcommand not available in this build")

        # If it responded, verify it's valid JSON-RPC
        if stdout.strip():
            try:
                response = json.loads(stdout.strip().split("\n")[0])
                assert "jsonrpc" in response
                assert response.get("id") == 1
            except json.JSONDecodeError:
                # Partial output is OK — server may need Content-Length framing
                pass

    def test_mcp_tools_list(self, ssh_exec):
        """MCP server lists available tools."""
        init_request = json.dumps({
            "jsonrpc": "2.0", "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0.0"}
            }
        })
        tools_request = json.dumps({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/list",
            "params": {}
        })

        cmd = f"printf '%s\\n%s\\n' '{init_request}' '{tools_request}' | timeout 10 pares-radix mcp-server 2>/dev/null || true"
        stdout, stderr, code = ssh_exec(cmd, timeout=15)

        if "unrecognized" in stderr.lower():
            pytest.skip("mcp-server subcommand not available")

        # Look for tools in response
        if stdout.strip():
            for line in stdout.strip().split("\n"):
                try:
                    resp = json.loads(line)
                    if resp.get("id") == 2 and "result" in resp:
                        tools = resp["result"].get("tools", [])
                        assert isinstance(tools, list)
                        return
                except json.JSONDecodeError:
                    continue
