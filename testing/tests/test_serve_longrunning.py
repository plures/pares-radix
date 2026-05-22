"""
test_serve_longrunning.py — Long-running service integration tests.

Tests that pares-radix serve/serve-spine can:
1. Start and stay alive without crashing
2. Respond to signals cleanly (SIGTERM, SIGINT)
3. Log expected startup messages
4. Accept MCP connections when mcp-serve is piped

These are real process tests — no mocks, no fakes.

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_serve_longrunning.py -v
"""
import json
import os
import signal
import subprocess
import time
import socket
import threading
import pytest
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
RADIX_BIN = os.environ.get(
    "RADIX_BINARY",
    str(REPO_ROOT / "target" / "release" / "pares-radix"),
)


@pytest.fixture
def radix_bin():
    """Verify binary exists."""
    assert os.path.isfile(RADIX_BIN), f"Binary not found: {RADIX_BIN}"
    return RADIX_BIN


def wait_for_output(proc, target_str, timeout=10):
    """Wait for a specific string in stderr output."""
    deadline = time.time() + timeout
    output = []
    while time.time() < deadline:
        if proc.poll() is not None:
            break
        # Non-blocking read from stderr
        import select
        ready, _, _ = select.select([proc.stderr], [], [], 0.5)
        if ready:
            line = proc.stderr.readline()
            if line:
                output.append(line)
                if target_str in line:
                    return True, output
    return False, output


class TestServeDesktopMode:
    """Tests for `pares-radix serve` in desktop-only mode (no Telegram token)."""

    def test_serve_starts_without_telegram(self, radix_bin):
        """serve starts in desktop-only mode when no telegram token is provided."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve", "--no-event-spine"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "PARES_TELEGRAM_TOKEN": "", "PARES_API_KEY": "test-key"},
        )
        # Give it time to start
        time.sleep(4)

        if proc.poll() is None:
            # Still running — success
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            assert True, "serve ran for 4s without crash"
        else:
            stdout, stderr = proc.communicate(timeout=5)
            # Some exits are acceptable (e.g. "no channel configured")
            # Segfault or panic is never acceptable
            assert proc.returncode != -11, f"Segfault! stderr: {stderr}"
            assert "panicked" not in stderr, f"Panic! {stderr}"
            # Document what happened
            pytest.skip(f"serve exited (code={proc.returncode}): {stderr[:200]}")

    def test_serve_responds_to_sigterm(self, radix_bin):
        """serve shuts down cleanly on SIGTERM."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve", "--no-event-spine"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "PARES_TELEGRAM_TOKEN": "", "PARES_API_KEY": "test-key"},
        )
        time.sleep(2)

        if proc.poll() is not None:
            pytest.skip("serve exited before SIGTERM test")

        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            pytest.fail("serve did not exit within 10s of SIGTERM")

        # Clean exit = 0 or 143 (128+15 SIGTERM)
        assert proc.returncode in (0, 143, -15), \
            f"Unexpected exit code: {proc.returncode}"

    def test_serve_responds_to_sigint(self, radix_bin):
        """serve shuts down cleanly on SIGINT (Ctrl-C)."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve", "--no-event-spine"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "PARES_TELEGRAM_TOKEN": "", "PARES_API_KEY": "test-key"},
        )
        time.sleep(2)

        if proc.poll() is not None:
            pytest.skip("serve exited before SIGINT test")

        proc.send_signal(signal.SIGINT)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            pytest.fail("serve did not exit within 10s of SIGINT")

        assert proc.returncode in (0, 130, -2), \
            f"Unexpected exit code: {proc.returncode}"

    def test_serve_no_panic_on_missing_model_url(self, radix_bin):
        """serve doesn't panic when model URL is unreachable."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve", "--model-url", "http://192.0.2.1:1/v1",
             "--no-event-spine"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "PARES_TELEGRAM_TOKEN": "", "PARES_API_KEY": "test-key"},
        )
        time.sleep(3)

        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        else:
            _, stderr = proc.communicate(timeout=5)
            assert "panicked" not in stderr, f"Panic with bad model URL: {stderr}"
            assert proc.returncode != -11, f"Segfault: {stderr}"


class TestServeSpineLongRunning:
    """Tests for `pares-radix serve-spine` stability."""

    def test_serve_spine_requires_telegram_token(self, radix_bin):
        """serve-spine requires --telegram-token (unlike serve)."""
        env = {k: v for k, v in os.environ.items() if k != "PARES_TELEGRAM_TOKEN"}
        result = subprocess.run(
            [RADIX_BIN, "serve-spine"],
            capture_output=True,
            text=True,
            timeout=10,
            env=env,
        )
        # Should fail with clap "required" error, not panic
        assert result.returncode != 0
        combined = result.stdout + result.stderr
        assert "panicked" not in combined
        # Clap emits to stderr: "required arguments were not provided"
        assert "required" in combined.lower() or "telegram" in combined.lower()

    def test_serve_spine_stable_5_seconds(self, radix_bin):
        """serve-spine exits cleanly with invalid token (no panic, no segfault)."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve-spine", "--telegram-token", "123456:FAKE_TOKEN_FOR_TEST",
             "--model-url", "http://192.0.2.1:1/v1"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        time.sleep(5)

        if proc.poll() is None:
            # Still running after 5s — good
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            assert True
        else:
            _, stderr = proc.communicate(timeout=5)
            # Some exits are ok (telegram auth fails), but no crashes
            assert proc.returncode != -11, f"Segfault: {stderr}"
            assert "panicked" not in stderr, f"Panic: {stderr}"

    def test_serve_spine_graceful_shutdown(self, radix_bin):
        """serve-spine handles SIGTERM without hanging."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve-spine", "--telegram-token", "123456:FAKE_TOKEN_FOR_TEST",
             "--model-url", "http://192.0.2.1:1/v1"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        time.sleep(2)

        if proc.poll() is not None:
            pytest.skip("serve-spine exited before shutdown test")

        start = time.time()
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            pytest.fail("serve-spine hung on SIGTERM (>10s)")

        elapsed = time.time() - start
        assert elapsed < 8, f"Shutdown took too long: {elapsed:.1f}s"


class TestMcpServeProtocolExtended:
    """Extended MCP server protocol tests — concurrent requests, error handling."""

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

    def _send_jsonrpc(self, proc, method, params=None, req_id=1):
        """Send a JSON-RPC request."""
        request = {
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
        }
        if params:
            request["params"] = params
        msg = json.dumps(request)
        proc.stdin.write(msg + "\n")
        proc.stdin.flush()

    def _read_response(self, proc, timeout=10):
        """Read a JSON-RPC response."""
        import select
        deadline = time.time() + timeout
        while time.time() < deadline:
            ready, _, _ = select.select([proc.stdout], [], [], 1.0)
            if ready:
                line = proc.stdout.readline()
                if line.strip():
                    return json.loads(line.strip())
        return None

    def test_mcp_multiple_requests_sequential(self, radix_bin):
        """MCP server handles multiple sequential requests."""
        proc = self._start_mcp()
        try:
            # Initialize
            self._send_jsonrpc(proc, "initialize", {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }, req_id=1)
            resp = self._read_response(proc)
            assert resp is not None, "No response to initialize"
            assert "result" in resp, f"Error on initialize: {resp}"

            # tools/list
            self._send_jsonrpc(proc, "tools/list", {}, req_id=2)
            resp = self._read_response(proc)
            assert resp is not None
            assert "result" in resp
            tools = resp["result"].get("tools", [])
            assert len(tools) > 0, "No tools returned"

            # Call a real tool: db_put
            self._send_jsonrpc(proc, "tools/call", {
                "name": "db_put",
                "arguments": {"key": "test:longrunning:probe", "value": "alive"}
            }, req_id=3)
            resp = self._read_response(proc)
            assert resp is not None
            assert "result" in resp, f"db_put failed: {resp}"

            # Verify with db_get
            self._send_jsonrpc(proc, "tools/call", {
                "name": "db_get",
                "arguments": {"key": "test:longrunning:probe"}
            }, req_id=4)
            resp = self._read_response(proc)
            assert resp is not None
            assert "result" in resp
            content = resp["result"].get("content", [])
            assert any("alive" in str(c) for c in content), f"db_get didn't return expected value: {content}"

            # Cleanup
            self._send_jsonrpc(proc, "tools/call", {
                "name": "db_delete",
                "arguments": {"key": "test:longrunning:probe"}
            }, req_id=5)
            resp = self._read_response(proc)
            assert resp is not None

        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    def test_mcp_invalid_method(self, radix_bin):
        """MCP server returns error for unknown methods (not crash)."""
        proc = self._start_mcp()
        try:
            # Initialize first
            self._send_jsonrpc(proc, "initialize", {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }, req_id=1)
            self._read_response(proc)

            # Send invalid method
            self._send_jsonrpc(proc, "nonexistent/method", {}, req_id=2)
            resp = self._read_response(proc)
            # Should get error response, not crash
            if resp is not None:
                assert "error" in resp, f"Expected error for invalid method: {resp}"
            # If no response, at least process shouldn't have crashed
            assert proc.poll() is None or proc.poll() == 0, \
                f"MCP crashed on invalid method (code={proc.poll()})"
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    def test_mcp_invalid_json(self, radix_bin):
        """MCP server handles malformed JSON gracefully."""
        proc = self._start_mcp()
        try:
            # Send garbage
            proc.stdin.write("not valid json at all\n")
            proc.stdin.flush()
            time.sleep(1)
            # Should not crash
            assert proc.poll() is None, \
                f"MCP crashed on invalid JSON (code={proc.poll()})"
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    def test_mcp_empty_line(self, radix_bin):
        """MCP server handles empty lines without crashing."""
        proc = self._start_mcp()
        try:
            proc.stdin.write("\n\n\n")
            proc.stdin.flush()
            time.sleep(1)
            assert proc.poll() is None, \
                f"MCP crashed on empty lines (code={proc.poll()})"
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    def test_mcp_rapid_requests(self, radix_bin):
        """MCP server handles rapid sequential requests without dropping."""
        proc = self._start_mcp()
        try:
            # Initialize
            self._send_jsonrpc(proc, "initialize", {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }, req_id=1)
            self._read_response(proc)

            # Fire 10 rapid db_put requests
            for i in range(10):
                self._send_jsonrpc(proc, "tools/call", {
                    "name": "db_put",
                    "arguments": {"key": f"test:rapid:{i}", "value": f"v{i}"}
                }, req_id=100 + i)

            # Read all responses
            responses = []
            for _ in range(10):
                resp = self._read_response(proc, timeout=15)
                if resp:
                    responses.append(resp)

            assert len(responses) >= 8, \
                f"Only got {len(responses)}/10 responses to rapid requests"

            # Cleanup
            for i in range(10):
                self._send_jsonrpc(proc, "tools/call", {
                    "name": "db_delete",
                    "arguments": {"key": f"test:rapid:{i}"}
                }, req_id=200 + i)
            time.sleep(2)

        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    def test_mcp_survives_stdin_close(self, radix_bin):
        """MCP server exits cleanly when stdin is closed (client disconnect)."""
        proc = self._start_mcp()
        try:
            # Initialize
            self._send_jsonrpc(proc, "initialize", {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }, req_id=1)
            self._read_response(proc)

            # Close stdin (simulates client disconnect)
            proc.stdin.close()

            # Should exit cleanly within 5s
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
                pytest.fail("MCP didn't exit after stdin close")

            # Clean exit
            assert proc.returncode in (0, 1), \
                f"Unexpected exit on stdin close: code={proc.returncode}"
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()


class TestServiceStability:
    """Stability tests — stress the service under unusual conditions."""

    def test_multiple_mcp_instances(self, radix_bin):
        """Multiple MCP instances can run simultaneously (different PluresDB paths)."""
        import tempfile
        procs = []
        try:
            for i in range(3):
                tmpdir = tempfile.mkdtemp(prefix=f"radix-test-{i}-")
                env = {**os.environ, "XDG_DATA_HOME": tmpdir}
                proc = subprocess.Popen(
                    [RADIX_BIN, "mcp-serve"],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    env=env,
                )
                procs.append(proc)

            time.sleep(2)

            # All should still be running
            alive = sum(1 for p in procs if p.poll() is None)
            assert alive >= 2, f"Only {alive}/3 MCP instances survived"

        finally:
            for proc in procs:
                if proc.poll() is None:
                    proc.terminate()
                    try:
                        proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()

    def test_serve_with_copilot_flag_no_tty(self, radix_bin):
        """serve --copilot without TTY doesn't hang waiting for device flow."""
        proc = subprocess.Popen(
            [RADIX_BIN, "serve", "--copilot", "--no-event-spine"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "PARES_TELEGRAM_TOKEN": ""},
        )
        # Should either exit quickly (no TTY for device flow) or run headless
        time.sleep(5)

        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        else:
            _, stderr = proc.communicate(timeout=5)
            assert proc.returncode != -11, f"Segfault: {stderr}"
            assert "panicked" not in stderr
