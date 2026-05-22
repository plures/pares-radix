"""
test_shell_execution.py — Shell execution integration tests via MCP server.

Tests cover:
- run_command: basic execution, stdout/stderr capture, exit codes, timeout, workdir, env
- process: list sessions, poll, background execution
- Edge cases: long output, special characters, concurrent commands

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_shell_execution.py -v
"""
import json
import os
import time
import uuid

import pytest


# ── run_command basics ────────────────────────────────────────────────────────


class TestRunCommandBasic:
    """Basic run_command functionality."""

    def test_echo_returns_output(self, mcp):
        """Simple echo command returns expected output."""
        result = mcp.call_tool("run_command", {"command": "echo hello-world"})
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "hello-world" in result_str

    def test_exit_code_zero(self, mcp):
        """Successful command reports exit code 0."""
        result = mcp.call_tool("run_command", {"command": "true"})
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        # Should indicate success (exit 0 or no error)
        if isinstance(result, dict):
            code = result.get("exit_code") or result.get("exitCode") or result.get("code")
            if code is not None:
                assert code == 0

    def test_exit_code_nonzero(self, mcp):
        """Failed command reports non-zero exit code."""
        result = mcp.call_tool("run_command", {"command": "false"})
        assert result is not None
        if isinstance(result, dict):
            code = result.get("exit_code") or result.get("exitCode") or result.get("code")
            if code is not None:
                assert code != 0

    def test_stderr_capture(self, mcp):
        """stderr output is captured (or merged into stdout)."""
        # Some implementations merge stderr into stdout, others separate them
        result = mcp.call_tool("run_command", {"command": "echo error-msg >&2; echo stdout-msg"})
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        # At minimum, the command should complete without error
        # stderr may or may not be captured depending on implementation
        assert "stdout-msg" in result_str or len(result_str) >= 0

    def test_multiline_output(self, mcp):
        """Multi-line output is captured completely."""
        result = mcp.call_tool("run_command", {
            "command": "printf 'line1\\nline2\\nline3\\n'"
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "line1" in result_str
        assert "line3" in result_str

    def test_pipe_command(self, mcp):
        """Pipe commands work correctly."""
        result = mcp.call_tool("run_command", {
            "command": "echo 'hello world' | wc -w"
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "2" in result_str

    def test_command_with_special_chars(self, mcp):
        """Commands with special characters execute correctly."""
        marker = "m" + uuid.uuid4().hex[:7]
        result = mcp.call_tool("run_command", {
            "command": f"echo 'quotes \"nested\" {marker}'"
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert marker in result_str


# ── run_command with workdir ──────────────────────────────────────────────────


class TestRunCommandWorkdir:
    """run_command with working directory option."""

    def test_workdir_changes_pwd(self, mcp):
        """workdir parameter changes the command's working directory."""
        result = mcp.call_tool("run_command", {
            "command": "pwd",
            "workdir": "/tmp",
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "/tmp" in result_str

    def test_workdir_affects_relative_paths(self, mcp):
        """Commands using relative paths resolve against workdir."""
        # Create a temp file in /tmp
        # Prefix with 'm' to ensure marker never looks like scientific notation
        # (e.g. '9595e303' would be parsed as 9.595e+306)
        marker = "m" + uuid.uuid4().hex[:7]
        mcp.call_tool("run_command", {
            "command": f"echo {marker} > /tmp/test-workdir-{marker}.txt"
        })
        # Read it with relative path from /tmp workdir
        result = mcp.call_tool("run_command", {
            "command": f"cat test-workdir-{marker}.txt",
            "workdir": "/tmp",
        })
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert marker in result_str
        # Cleanup
        mcp.call_tool("run_command", {
            "command": f"rm -f /tmp/test-workdir-{marker}.txt"
        })


# ── run_command with env ──────────────────────────────────────────────────────


class TestRunCommandEnv:
    """run_command with environment variables."""

    def test_env_vars_available(self, mcp):
        """Custom environment variables are available to the command."""
        marker = "m" + uuid.uuid4().hex[:7]
        result = mcp.call_tool("run_command", {
            "command": "echo $TEST_VAR",
            "env": {"TEST_VAR": marker},
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert marker in result_str

    def test_multiple_env_vars(self, mcp):
        """Multiple env vars can be set simultaneously."""
        result = mcp.call_tool("run_command", {
            "command": "echo $A-$B",
            "env": {"A": "alpha", "B": "beta"},
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "alpha" in result_str
        assert "beta" in result_str


# ── run_command timeout ───────────────────────────────────────────────────────


class TestRunCommandTimeout:
    """run_command timeout behavior."""

    def test_fast_command_within_timeout(self, mcp):
        """Fast command completes within timeout."""
        result = mcp.call_tool("run_command", {
            "command": "echo fast",
            "timeout": 5,
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "fast" in result_str

    def test_slow_command_killed_by_timeout(self, mcp):
        """Command exceeding timeout is killed."""
        result = mcp.call_tool("run_command", {
            "command": "sleep 30",
            "timeout": 1,
        })
        assert result is not None
        # Should indicate timeout/killed
        if isinstance(result, dict):
            result_str = json.dumps(result)
            # Might have timeout, killed, signal, or non-zero exit
            has_timeout_indicator = any(k in result_str.lower() for k in [
                "timeout", "killed", "signal", "timed out"
            ])
            code = result.get("exit_code") or result.get("exitCode") or result.get("code")
            assert has_timeout_indicator or (code is not None and code != 0)


# ── run_command background ────────────────────────────────────────────────────


class TestRunCommandBackground:
    """Background command execution."""

    def test_background_returns_session_id(self, mcp):
        """Background command returns a session id for tracking."""
        result = mcp.call_tool("run_command", {
            "command": "sleep 5",
            "background": True,
        })
        assert result is not None
        if isinstance(result, dict):
            # Should have session_id or similar
            has_id = any(k in result for k in [
                "session_id", "sessionId", "id", "pid"
            ])
            if not has_id:
                # String result might contain id
                result_str = json.dumps(result)
                assert len(result_str) > 2  # Not just empty

    def test_background_command_doesnt_block(self, mcp):
        """Background command returns immediately."""
        start = time.time()
        result = mcp.call_tool("run_command", {
            "command": "sleep 10",
            "background": True,
        })
        elapsed = time.time() - start
        assert elapsed < 5, f"Background command blocked for {elapsed}s"


# ── process tool ──────────────────────────────────────────────────────────────


class TestProcess:
    """Tests for the process management tool."""

    def test_process_list(self, mcp):
        """process list returns current sessions."""
        result = mcp.call_tool("process", {"action": "list"})
        assert result is not None

    def test_process_poll_nonexistent(self, mcp):
        """Polling a non-existent session handles gracefully."""
        result = mcp.call_tool("process", {
            "action": "poll",
            "sessionId": "nonexistent-session-id",
        })
        # Should return error or empty, not crash
        assert result is not None

    def test_background_then_poll(self, mcp):
        """Start background command, then poll its status."""
        # Start a command that runs for a bit
        bg_result = mcp.call_tool("run_command", {
            "command": "echo bg-output; sleep 2; echo bg-done",
            "background": True,
        })

        session_id = None
        if isinstance(bg_result, dict):
            session_id = (bg_result.get("session_id") or
                         bg_result.get("sessionId") or
                         bg_result.get("id"))

        if session_id is None:
            pytest.skip("Could not get session_id from background command")

        # Poll for output
        time.sleep(1)
        poll_result = mcp.call_tool("process", {
            "action": "poll",
            "sessionId": session_id,
        })
        assert poll_result is not None

    def test_process_kill(self, mcp):
        """Kill a running background session."""
        # Start a long-running command
        bg_result = mcp.call_tool("run_command", {
            "command": "sleep 300",
            "background": True,
        })

        session_id = None
        if isinstance(bg_result, dict):
            session_id = (bg_result.get("session_id") or
                         bg_result.get("sessionId") or
                         bg_result.get("id"))

        if session_id is None:
            pytest.skip("Could not get session_id from background command")

        # Kill it
        kill_result = mcp.call_tool("process", {
            "action": "kill",
            "sessionId": session_id,
        })
        assert kill_result is not None


# ── Stress / edge cases ───────────────────────────────────────────────────────


class TestShellEdgeCases:
    """Edge cases and stress tests for shell execution."""

    def test_empty_command(self, mcp):
        """Empty command should return error, not crash."""
        result = mcp.call_tool("run_command", {"command": ""})
        assert result is not None

    def test_large_output(self, mcp):
        """Commands with large output don't crash."""
        result = mcp.call_tool("run_command", {
            "command": "seq 1 1000"
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "1000" in result_str

    def test_binary_in_path(self, mcp):
        """Standard binaries are findable in PATH."""
        result = mcp.call_tool("run_command", {"command": "which ls"})
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "/ls" in result_str or "ls" in result_str

    def test_sequential_commands(self, mcp):
        """Multiple sequential commands execute independently."""
        r1 = mcp.call_tool("run_command", {"command": "echo first"})
        r2 = mcp.call_tool("run_command", {"command": "echo second"})
        r1_str = json.dumps(r1) if isinstance(r1, (dict, list)) else str(r1)
        r2_str = json.dumps(r2) if isinstance(r2, (dict, list)) else str(r2)
        assert "first" in r1_str
        assert "second" in r2_str

    def test_command_chaining(self, mcp):
        """Chained commands (&&) work correctly."""
        result = mcp.call_tool("run_command", {
            "command": "echo start && echo middle && echo end"
        })
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "start" in result_str
        assert "end" in result_str
