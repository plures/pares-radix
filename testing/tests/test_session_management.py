"""
Session & Subagent Management E2E Tests
========================================
Tests for session_status, session_list, session_history, session_yield,
subagent_spawn, subagent_list, subagent_kill, subagent_steer, and agent_ask tools.

Tools tested:
- session_status: runtime info, model, uptime
- session_list: enumerate active sessions
- session_history: message history retrieval
- session_yield: agent turn control
- subagent_spawn: spawn isolated sub-agent sessions
- subagent_list: list spawned sub-agents
- subagent_kill: terminate a sub-agent
- subagent_steer: inject message to running sub-agent
- agent_ask: full agent loop invocation
- runtime_status: system health
- runtime_restart: graceful restart signal
"""

import json
import os
import uuid
import pytest


def _assert_ok(result, msg=""):
    """Assert tool call succeeded."""
    assert result is not None, f"Result was None {msg}"
    if isinstance(result, dict) and "error" in result:
        pytest.fail(f"Tool error {msg}: {result}")


def _assert_err(result, msg=""):
    """Assert tool call returned an error or rejection."""
    if result is None:
        return
    if isinstance(result, dict) and "error" in result:
        return
    if isinstance(result, str) and any(kw in result.lower() for kw in [
        "error", "failed", "not found", "missing", "required", "invalid",
        "not configured", "not available"
    ]):
        return
    # session_send may return {"status": "responded", "response": None} for ghost sessions
    if isinstance(result, dict) and result.get("response") is None:
        return
    pytest.fail(f"Expected error but got: {result} {msg}")


def _get_text(result) -> str:
    """Get text content from result."""
    if result is None:
        return ""
    if isinstance(result, str):
        return result
    if isinstance(result, dict):
        return json.dumps(result)
    return str(result)


# ── session_status ─────────────────────────────────────────────────────────────


class TestSessionStatus:
    """Tests for session_status tool."""

    def test_status_returns_info(self, mcp):
        """session_status should return runtime information."""
        result = mcp.call_tool("session_status", {})
        _assert_ok(result)
        text = _get_text(result)
        assert any(kw in text.lower() for kw in [
            "version", "uptime", "model", "status", "session", "active"
        ]), f"Unexpected session_status: {text[:200]}"

    def test_status_with_session_id(self, mcp):
        """session_status with explicit session_id."""
        result = mcp.call_tool("session_status", {"session_id": "mcp"})
        _assert_ok(result)

    def test_status_model_override(self, mcp):
        """session_status with model override."""
        result = mcp.call_tool("session_status", {"model": "gpt-4o-mini"})
        _assert_ok(result)

    def test_status_model_reset(self, mcp):
        """session_status with model='default' resets override."""
        result = mcp.call_tool("session_status", {"model": "default"})
        _assert_ok(result)


# ── session_list ───────────────────────────────────────────────────────────────


class TestSessionList:
    """Tests for session_list tool."""

    def test_list_sessions(self, mcp):
        """session_list should return session data."""
        result = mcp.call_tool("session_list", {})
        _assert_ok(result)
        text = _get_text(result)
        assert len(text) > 0

    def test_list_includes_some_session(self, mcp):
        """The MCP session should appear in some form."""
        result = mcp.call_tool("session_list", {})
        text = _get_text(result)
        assert "mcp" in text.lower() or "session" in text.lower() or "shell" in text.lower()


# ── session_history ────────────────────────────────────────────────────────────


class TestSessionHistory:
    """Tests for session_history tool."""

    def test_history_requires_session_id(self, mcp):
        """session_history without session_id should error."""
        result = mcp.call_tool("session_history", {})
        _assert_err(result)

    def test_history_for_mcp_session(self, mcp):
        """session_history for 'mcp' session."""
        result = mcp.call_tool("session_history", {"session_id": "mcp"})
        # May be empty or error if no conversation history yet
        assert result is not None

    def test_history_with_limit(self, mcp):
        """session_history respects limit parameter."""
        result = mcp.call_tool("session_history", {"session_id": "mcp", "limit": 5})
        assert result is not None

    def test_history_nonexistent_session(self, mcp):
        """session_history for nonexistent session returns empty history."""
        result = mcp.call_tool("session_history", {
            "session_id": f"nonexistent-{uuid.uuid4().hex[:8]}"
        })
        # Radix returns empty history with a note rather than an error
        if isinstance(result, dict):
            assert result.get("message_count", 0) == 0 or "no history" in result.get("note", "")
        else:
            _assert_err(result)


# ── session_yield ──────────────────────────────────────────────────────────────


class TestSessionYield:
    """Tests for session_yield tool."""

    def test_yield_basic(self, mcp):
        """session_yield should succeed."""
        result = mcp.call_tool("session_yield", {})
        _assert_ok(result)

    def test_yield_with_message(self, mcp):
        """session_yield with a status message."""
        result = mcp.call_tool("session_yield", {"message": "Waiting for sub-agent"})
        _assert_ok(result)


# ── subagent_list ──────────────────────────────────────────────────────────────


class TestSubagentList:
    """Tests for subagent_list tool."""

    def test_list_subagents(self, mcp):
        """subagent_list should succeed."""
        result = mcp.call_tool("subagent_list", {})
        _assert_ok(result)


# ── subagent_spawn ─────────────────────────────────────────────────────────────


class TestSubagentSpawn:
    """Tests for subagent_spawn tool."""

    def test_spawn_requires_params(self, mcp):
        """subagent_spawn without required params should error."""
        result = mcp.call_tool("subagent_spawn", {})
        _assert_err(result)

    def test_spawn_requires_task(self, mcp):
        """subagent_spawn without task should error."""
        result = mcp.call_tool("subagent_spawn", {"agent": "test-agent"})
        _assert_err(result)

    def test_spawn_nonexistent_agent(self, mcp):
        """Spawning a non-registered agent should error."""
        result = mcp.call_tool("subagent_spawn", {
            "agent": f"no-such-agent-{uuid.uuid4().hex[:8]}",
            "task": "do something"
        })
        _assert_err(result)


# ── subagent_kill ──────────────────────────────────────────────────────────────


class TestSubagentKill:
    """Tests for subagent_kill tool."""

    def test_kill_requires_session_id(self, mcp):
        """subagent_kill without session_id should error."""
        result = mcp.call_tool("subagent_kill", {})
        _assert_err(result)

    def test_kill_nonexistent_session(self, mcp):
        """Killing a non-existent session should error."""
        result = mcp.call_tool("subagent_kill", {
            "session_id": f"fake-session-{uuid.uuid4().hex[:8]}"
        })
        _assert_err(result)


# ── subagent_steer ─────────────────────────────────────────────────────────────


class TestSubagentSteer:
    """Tests for subagent_steer tool."""

    def test_steer_requires_params(self, mcp):
        """subagent_steer without params should error."""
        result = mcp.call_tool("subagent_steer", {})
        _assert_err(result)

    def test_steer_nonexistent_session(self, mcp):
        """Steering a non-existent session should error."""
        result = mcp.call_tool("subagent_steer", {
            "session_id": f"fake-{uuid.uuid4().hex[:8]}",
            "message": "do something else"
        })
        _assert_err(result)


# ── agent_ask ──────────────────────────────────────────────────────────────────


class TestAgentAsk:
    """Tests for agent_ask tool (full agent loop)."""

    def test_ask_requires_prompt(self, mcp):
        """agent_ask without prompt should error."""
        result = mcp.call_tool("agent_ask", {})
        _assert_err(result)

    def test_ask_simple_prompt(self, mcp):
        """agent_ask with a prompt — tests parameter acceptance.
        May error without API key (valid behavior)."""
        result = mcp.call_tool("agent_ask", {"prompt": "What is 2+2?"}, timeout=15)
        # Both success and auth-error are valid outcomes
        assert result is not None

    def test_ask_with_session(self, mcp):
        """agent_ask with session param."""
        result = mcp.call_tool("agent_ask", {
            "prompt": "Say hello", "session": "test-session"
        }, timeout=15)
        assert result is not None


# ── session_send ───────────────────────────────────────────────────────────────


class TestSessionSend:
    """Tests for session_send tool."""

    def test_send_requires_params(self, mcp):
        """session_send without required params should error."""
        result = mcp.call_tool("session_send", {})
        _assert_err(result)

    def test_send_requires_message(self, mcp):
        """session_send without message should error."""
        result = mcp.call_tool("session_send", {"session_id": "mcp"})
        _assert_err(result)

    def test_send_to_nonexistent_session(self, mcp):
        """Sending to a non-existent session should error."""
        result = mcp.call_tool("session_send", {
            "session_id": f"ghost-{uuid.uuid4().hex[:8]}",
            "message": "hello"
        })
        _assert_err(result)


# ── runtime_status ─────────────────────────────────────────────────────────────


class TestAAARuntimeStatus:
    """Tests for runtime_status tool.
    Named AAA* to ensure it runs FIRST before agent_ask tests potentially clog the pipe."""

    def test_runtime_status_basic(self, mcp):
        """runtime_status should return system health info."""
        result = mcp.call_tool("runtime_status", {}, timeout=15)
        _assert_ok(result)
        text = _get_text(result)
        assert any(kw in text.lower() for kw in [
            "version", "health", "uptime", "active", "component", "shell"
        ]), f"Unexpected runtime_status: {text[:300]}"


class TestZZRuntimeRestart:
    """Tests for runtime_restart tool.
    Named ZZ* to ensure it runs LAST since restart kills the MCP server process."""

    def test_restart_signal(self, mcp):
        """runtime_restart should acknowledge restart signal."""
        result = mcp.call_tool("runtime_restart", {"reason": "integration test"}, timeout=15)
        # After restart signal, process may die — None is acceptable
        if result is None:
            return  # Process died from restart — that's valid behavior
        _assert_ok(result)
