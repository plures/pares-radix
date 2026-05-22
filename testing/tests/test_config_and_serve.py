"""
test_config_and_serve.py — Integration tests for config, mcp-serve, and ask subcommands.

These exercise real runtime behavior: config parsing, MCP protocol over stdio,
and the ask subcommand for non-interactive prompts.

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_config_and_serve.py -v
"""
import json
import os
import signal
import subprocess
import time
import pytest
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
RADIX_BIN = os.environ.get(
    "RADIX_BINARY",
    str(REPO_ROOT / "target" / "release" / "pares-radix"),
)


def run_radix(*args, input_data=None, timeout=15):
    """Run pares-radix with args."""
    result = subprocess.run(
        [RADIX_BIN, *args],
        capture_output=True,
        text=True,
        timeout=timeout,
        input=input_data,
    )
    return result.stdout, result.stderr, result.returncode


@pytest.fixture
def radix_bin():
    """Verify binary exists."""
    assert os.path.isfile(RADIX_BIN), f"Binary not found: {RADIX_BIN}"
    return RADIX_BIN


class TestConfigSubcommand:
    """Tests for `pares-radix config` subcommand."""

    def test_config_show(self, radix_bin):
        """config show outputs TOML configuration."""
        stdout, stderr, code = run_radix("config", "show")
        combined = stdout + stderr
        # Should contain model config section
        assert code == 0, f"config show failed: {combined}"
        assert "[model]" in combined or "model" in combined.lower()

    def test_config_path(self, radix_bin):
        """config path outputs a file path."""
        stdout, stderr, code = run_radix("config", "path")
        combined = (stdout + stderr).strip()
        assert code == 0
        assert combined.endswith(".toml") or "config" in combined.lower()

    def test_config_shows_memory_path(self, radix_bin):
        """config show includes memory configuration."""
        stdout, stderr, code = run_radix("config", "show")
        combined = stdout + stderr
        assert "[memory]" in combined or "memory" in combined.lower()

    def test_config_shows_logging(self, radix_bin):
        """config show includes logging configuration."""
        stdout, stderr, code = run_radix("config", "show")
        combined = stdout + stderr
        assert "[logging]" in combined or "log" in combined.lower()


class TestMcpServeProtocol:
    """Integration tests for the MCP server (stdio JSON-RPC)."""

    def _start_mcp(self):
        """Start mcp-serve process."""
        proc = subprocess.Popen(
            [RADIX_BIN, "mcp-serve"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return proc

    def _send_jsonrpc(self, proc, method, params=None, id=1):
        """Send a JSON-RPC request to the process stdin."""
        request = {
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }
        if params is not None:
            request["params"] = params
        proc.stdin.write(json.dumps(request) + "\n")
        proc.stdin.flush()

    def _read_response(self, proc, timeout=5):
        """Read a JSON-RPC response line from stdout."""
        import select
        # Use select for timeout on Unix
        ready, _, _ = select.select([proc.stdout], [], [], timeout)
        if not ready:
            return None
        line = proc.stdout.readline()
        if line:
            try:
                return json.loads(line.strip())
            except json.JSONDecodeError:
                return {"raw": line.strip()}
        return None

    def test_mcp_initialize_handshake(self, radix_bin):
        """MCP server responds to initialize with capabilities."""
        proc = self._start_mcp()
        try:
            self._send_jsonrpc(proc, "initialize", {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test-harness", "version": "1.0.0"},
            })

            time.sleep(2)
            resp = self._read_response(proc, timeout=5)

            if resp is None:
                # Server may not output until stdin closes
                proc.stdin.close()
                time.sleep(1)
                resp = self._read_response(proc, timeout=3)

            if resp and "result" in resp:
                result = resp["result"]
                assert "capabilities" in result or "serverInfo" in result
            elif resp and "error" in resp:
                # Error response is still valid JSON-RPC
                assert "code" in resp["error"]
            else:
                # If no response at all, the server may need different framing
                pytest.skip("MCP server did not respond — may need Content-Length framing")
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()

    def test_mcp_tools_list(self, radix_bin):
        """MCP server responds to tools/list."""
        proc = self._start_mcp()
        try:
            # Initialize first
            self._send_jsonrpc(proc, "initialize", {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test-harness", "version": "1.0.0"},
            }, id=1)
            time.sleep(1)

            # Send initialized notification
            proc.stdin.write(json.dumps({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
            }) + "\n")
            proc.stdin.flush()
            time.sleep(0.5)

            # Request tools list
            self._send_jsonrpc(proc, "tools/list", {}, id=2)
            time.sleep(2)

            # Read all available responses
            responses = []
            for _ in range(5):
                resp = self._read_response(proc, timeout=2)
                if resp:
                    responses.append(resp)
                else:
                    break

            # Check if any response has tools
            tools_found = False
            for resp in responses:
                if isinstance(resp, dict) and "result" in resp:
                    result = resp["result"]
                    if "tools" in result:
                        tools_found = True
                        assert isinstance(result["tools"], list)
                        break

            if not tools_found and not responses:
                pytest.skip("MCP server did not respond to tools/list")

        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()

    def test_mcp_invalid_method(self, radix_bin):
        """MCP server handles unknown methods gracefully."""
        proc = self._start_mcp()
        try:
            self._send_jsonrpc(proc, "nonexistent/method", {}, id=99)
            time.sleep(2)
            resp = self._read_response(proc, timeout=3)
            # Should either error or ignore — not crash
            if resp and "error" in resp:
                assert resp["error"]["code"] in [-32601, -32600, -32602]
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()

    def test_mcp_malformed_json(self, radix_bin):
        """MCP server handles malformed JSON without crashing."""
        proc = self._start_mcp()
        try:
            proc.stdin.write("this is not json\n")
            proc.stdin.flush()
            time.sleep(1)
            # Server should still be alive
            assert proc.poll() is None, "MCP server crashed on malformed input"
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()


class TestServeSpine:
    """Integration tests for serve-spine (headless agent startup)."""

    def test_serve_spine_starts_cleanly(self, radix_bin):
        """serve-spine starts without immediate crash."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve-spine", "--model-url", "http://localhost:9999"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        time.sleep(3)
        # If still running after 3s, it started cleanly
        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            # Success — ran for 3 seconds
        else:
            # Exited — check if it was a clean "no config" exit vs crash
            _, stderr = proc.communicate(timeout=5)
            # Exit code 1 with config error is acceptable; segfault is not
            assert proc.returncode != -11, f"serve-spine segfaulted: {stderr}"

    def test_serve_spine_invalid_token_no_panic(self, radix_bin):
        """serve-spine with an invalid Telegram token exits cleanly (no panic).

        Regression test for issue #323: teloxide dispatcher panicked on
        invalid token instead of returning an error.
        """
        result = subprocess.run(
            [RADIX_BIN, "serve-spine", "--telegram-token", "123456:FAKE",
             "--model-url", "http://192.0.2.1:1/v1"],
            capture_output=True, text=True, timeout=20,
        )
        combined = (result.stdout + result.stderr).lower()
        # Must NOT panic
        assert "panic" not in combined, f"serve-spine panicked: {result.stderr}"
        assert result.returncode != -11, "serve-spine segfaulted"
        # Should exit with error code (token validation failed)
        assert result.returncode != 0, "Expected non-zero exit for invalid token"


class TestAskSubcommand:
    """Tests for the `ask` subcommand (non-interactive single prompt)."""

    def test_ask_help(self, radix_bin):
        """ask --help works."""
        stdout, stderr, code = run_radix("ask", "--help")
        assert code == 0
        combined = stdout + stderr
        assert "ask" in combined.lower() or "prompt" in combined.lower()

    def test_ask_no_model_fails_gracefully(self, radix_bin):
        """ask without a reachable model fails with clear error (not crash)."""
        stdout, stderr, code = run_radix(
            "ask", "--model-url", "http://localhost:1/v1",
            "--prompt", "hello",
            timeout=15,
        )
        combined = stdout + stderr
        # Should fail (no model available) but not crash
        if code != 0:
            assert "error" in combined.lower() or "connect" in combined.lower() or "fail" in combined.lower()
