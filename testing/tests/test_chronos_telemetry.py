"""
test_chronos_telemetry.py — Integration tests for Chronos timeline and telemetry via MCP.

Exercises REAL MCP tool calls against a running pares-radix instance:
- Chronos event recording, querying, filtering, replay
- Telemetry snapshot and reset
- Chronos log level control
- Event ordering and actor filtering

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_chronos_telemetry.py -v
"""
import json
import time
import uuid
import pytest
from conftest import McpClient


@pytest.fixture
def mcp():
    """Fresh MCP client per test."""
    client = McpClient()
    client.start()
    yield client
    client.stop()


def _is_error(result):
    """Check if result is an error string or error dict."""
    if isinstance(result, str):
        return True  # error messages come as plain strings
    if isinstance(result, dict) and "error" in result:
        return True
    return False


class TestChronosRecord:
    """Tests for chronos_record tool."""

    def test_record_basic_event(self, mcp):
        """Record a simple event and verify it returns an id."""
        result = mcp.call_tool("chronos_record", {
            "key": "test:basic",
            "actor": "pytest",
            "action": "create",
            "level": "info",
            "data": {"message": "basic event"},
        })
        assert isinstance(result, dict), f"Expected dict, got: {result}"
        assert "id" in result
        assert result["recorded"] is True
        assert result["level"] == "info"

    def test_record_all_actions(self, mcp):
        """Record events with each valid action type."""
        actions = ["create", "update", "delete", "move", "tool_invoked",
                   "message_received", "response_generated", "context_managed",
                   "model_called", "outcome_recorded"]
        for action in actions:
            result = mcp.call_tool("chronos_record", {
                "key": f"test:action:{action}",
                "actor": "pytest",
                "action": action,
                "level": "info",
            })
            assert isinstance(result, dict), f"Action {action} failed: {result}"
            assert result["recorded"] is True, f"Action {action} not recorded"

    def test_record_all_levels(self, mcp):
        """Record events at each severity level."""
        levels = ["debug", "info", "warn", "error"]
        for level in levels:
            result = mcp.call_tool("chronos_record", {
                "key": f"test:level:{level}",
                "actor": "pytest",
                "action": "create",
                "level": level,
            })
            assert isinstance(result, dict), f"Level {level} failed: {result}"
            assert result["level"] == level

    def test_record_complex_data(self, mcp):
        """Record event with nested data payload."""
        result = mcp.call_tool("chronos_record", {
            "key": "test:complex",
            "actor": "pytest",
            "action": "create",
            "level": "info",
            "data": {
                "nested": {"deep": {"value": 42}},
                "array": [1, 2, 3],
                "unicode": "日本語テスト",
                "boolean": True,
            },
        })
        assert isinstance(result, dict)
        assert result["recorded"] is True

    def test_record_with_rationale(self, mcp):
        """Record event with rationale explanation."""
        result = mcp.call_tool("chronos_record", {
            "key": "test:rationale",
            "actor": "pytest",
            "action": "update",
            "level": "info",
            "rationale": "Testing rationale field persistence",
        })
        assert isinstance(result, dict)
        assert result["recorded"] is True

    def test_record_with_constraints(self, mcp):
        """Record event with constraint references."""
        result = mcp.call_tool("chronos_record", {
            "key": "test:constraints",
            "actor": "pytest",
            "action": "create",
            "level": "info",
            "constraints": ["no-empty-key", "valid-action"],
        })
        assert isinstance(result, dict)
        assert result["recorded"] is True

    def test_record_missing_key_fails(self, mcp):
        """Missing key parameter returns error."""
        result = mcp.call_tool("chronos_record", {
            "actor": "pytest",
            "action": "create",
        })
        assert _is_error(result)
        if isinstance(result, str):
            assert "key" in result.lower()

    def test_record_invalid_action_fails(self, mcp):
        """Invalid action returns error with valid options."""
        result = mcp.call_tool("chronos_record", {
            "key": "test:bad-action",
            "action": "nonexistent_action",
        })
        assert _is_error(result)
        if isinstance(result, str):
            assert "unknown action" in result.lower() or "valid" in result.lower()

    def test_record_returns_unique_ids(self, mcp):
        """Each recorded event gets a unique id."""
        ids = set()
        for i in range(5):
            result = mcp.call_tool("chronos_record", {
                "key": f"test:unique:{i}",
                "actor": "pytest",
                "action": "create",
                "level": "info",
            })
            assert isinstance(result, dict)
            ids.add(result["id"])
        assert len(ids) == 5, "Expected 5 unique event ids"

    def test_record_timestamp_increases(self, mcp):
        """Timestamps are monotonically non-decreasing."""
        timestamps = []
        for i in range(3):
            result = mcp.call_tool("chronos_record", {
                "key": f"test:timestamp:{i}",
                "actor": "pytest",
                "action": "create",
                "level": "info",
            })
            assert isinstance(result, dict)
            timestamps.append(result["timestamp"])
        assert timestamps == sorted(timestamps), "Timestamps should be non-decreasing"


class TestChronosQuery:
    """Tests for chronos_history, chronos_recent, chronos_timeline."""

    def test_recent_returns_list(self, mcp):
        """chronos_recent returns a list of events."""
        # Seed
        mcp.call_tool("chronos_record", {
            "key": "test:query:seed",
            "actor": "pytest",
            "action": "create",
            "level": "info",
        })

        result = mcp.call_tool("chronos_recent", {"limit": 5})
        assert isinstance(result, list), f"Expected list, got: {type(result)} = {result}"
        assert len(result) > 0

    def test_recent_contains_seeded_event(self, mcp):
        """Seeded event appears in recent results."""
        marker = f"marker-{uuid.uuid4().hex[:8]}"
        mcp.call_tool("chronos_record", {
            "key": f"test:query:{marker}",
            "actor": "pytest",
            "action": "create",
            "level": "info",
            "data": {"marker": marker},
        })

        result = mcp.call_tool("chronos_recent", {"limit": 10})
        assert isinstance(result, list)
        keys = [e.get("key", "") for e in result]
        assert any(marker in k for k in keys), f"Marker not found in: {keys}"

    def test_recent_limit_respected(self, mcp):
        """Limit parameter caps the number of results."""
        # Seed several events
        for i in range(5):
            mcp.call_tool("chronos_record", {
                "key": f"test:limit:{i}",
                "actor": "pytest",
                "action": "create",
                "level": "info",
            })

        result = mcp.call_tool("chronos_recent", {"limit": 2})
        assert isinstance(result, list)
        assert len(result) <= 2

    def test_history_query(self, mcp):
        """chronos_history returns event history for a specific key."""
        mcp.call_tool("chronos_record", {
            "key": "test:history:data",
            "actor": "pytest",
            "action": "create",
            "level": "info",
        })

        result = mcp.call_tool("chronos_history", {"key": "test:history:data", "limit": 5})
        assert result is not None
        assert not _is_error(result)
        # Should be a list with our event
        if isinstance(result, list):
            assert len(result) >= 1
            assert any(e.get("key") == "test:history:data" for e in result)

    def test_timeline_query(self, mcp):
        """chronos_timeline returns events."""
        mcp.call_tool("chronos_record", {
            "key": "test:timeline:data",
            "actor": "pytest",
            "action": "create",
            "level": "info",
        })

        result = mcp.call_tool("chronos_timeline", {"limit": 5})
        assert result is not None
        assert not _is_error(result)

    def test_by_actor_filter(self, mcp):
        """chronos_by_actor filters by actor field."""
        unique_actor = f"actor-{uuid.uuid4().hex[:6]}"
        mcp.call_tool("chronos_record", {
            "key": "test:actor:filter",
            "actor": unique_actor,
            "action": "create",
            "level": "info",
        })

        result = mcp.call_tool("chronos_by_actor", {"actor": unique_actor})
        assert result is not None
        assert not _is_error(result)
        if isinstance(result, list):
            assert len(result) >= 1
            assert all(e.get("actor") == unique_actor for e in result)


class TestChronosLevel:
    """Tests for chronos_set_level and chronos_get_level."""

    def test_get_level(self, mcp):
        """Get current Chronos recording level."""
        result = mcp.call_tool("chronos_get_level", {})
        assert isinstance(result, dict)
        assert "level" in result
        assert result["level"] in ("debug", "info", "warn", "error")

    def test_set_level_to_warn(self, mcp):
        """Set level to warn and verify."""
        result = mcp.call_tool("chronos_set_level", {"level": "warn"})
        assert not _is_error(result)

        get_result = mcp.call_tool("chronos_get_level", {})
        assert get_result["level"] == "warn"

    def test_set_level_roundtrip(self, mcp):
        """Set each level and verify roundtrip."""
        for level in ["debug", "info", "warn", "error"]:
            mcp.call_tool("chronos_set_level", {"level": level})
            get_result = mcp.call_tool("chronos_get_level", {})
            assert get_result["level"] == level, f"Expected {level}, got {get_result}"

    def test_level_filtering_suppresses_low_priority(self, mcp):
        """Events below the threshold level are not recorded."""
        # Set to error — should suppress info/debug/warn
        mcp.call_tool("chronos_set_level", {"level": "error"})

        # Record debug event
        debug_result = mcp.call_tool("chronos_record", {
            "key": "test:filtered:debug",
            "actor": "pytest",
            "action": "create",
            "level": "debug",
            "data": {"should_be": "filtered"},
        })

        # Debug event might not be recorded
        if isinstance(debug_result, dict):
            # If implementation still records but marks unrecorded:
            if "recorded" in debug_result:
                assert debug_result["recorded"] is False, \
                    "Debug event should be filtered at error level"

        # Error event should pass through
        error_result = mcp.call_tool("chronos_record", {
            "key": "test:filtered:error",
            "actor": "pytest",
            "action": "create",
            "level": "error",
        })
        if isinstance(error_result, dict) and "recorded" in error_result:
            assert error_result["recorded"] is True

        # Reset
        mcp.call_tool("chronos_set_level", {"level": "info"})


class TestChronosReplay:
    """Tests for chronos_replay."""

    def test_replay_returns_result(self, mcp):
        """Replay invocation returns without error."""
        # Seed an event
        mcp.call_tool("chronos_record", {
            "key": "test:replay:source",
            "actor": "pytest",
            "action": "create",
            "level": "info",
        })

        result = mcp.call_tool("chronos_replay", {})
        assert result is not None
        assert not _is_error(result)


class TestTelemetry:
    """Tests for telemetry_snapshot and telemetry_reset."""

    def test_snapshot_structure(self, mcp):
        """Telemetry snapshot returns expected fields."""
        result = mcp.call_tool("telemetry_snapshot", {})
        assert isinstance(result, dict), f"Expected dict: {result}"
        assert "total_calls" in result
        assert "top_tools" in result
        assert "uptime_secs" in result
        assert "started_at_unix" in result

    def test_snapshot_reflects_calls(self, mcp):
        """Telemetry counts tool calls accurately."""
        # Make 3 db_keys calls
        mcp.call_tool("db_keys", {})
        mcp.call_tool("db_keys", {})
        mcp.call_tool("db_keys", {})

        result = mcp.call_tool("telemetry_snapshot", {})
        assert isinstance(result, dict)
        # Find db_keys in top_tools
        db_keys_entry = next(
            (t for t in result["top_tools"] if t["name"] == "db_keys"), None
        )
        assert db_keys_entry is not None, f"db_keys not in top_tools: {result['top_tools']}"
        assert db_keys_entry["calls"] >= 3
        assert db_keys_entry["successes"] >= 3

    def test_snapshot_tracks_failures(self, mcp):
        """Telemetry distinguishes successes from failures."""
        # Trigger a known failure: db_get without key
        mcp.call_tool("db_get", {})

        result = mcp.call_tool("telemetry_snapshot", {})
        db_get_entry = next(
            (t for t in result["top_tools"] if t["name"] == "db_get"), None
        )
        assert db_get_entry is not None
        assert db_get_entry["failures"] >= 1

    def test_snapshot_latency(self, mcp):
        """Telemetry records average latency."""
        mcp.call_tool("db_keys", {})

        result = mcp.call_tool("telemetry_snapshot", {})
        assert "avg_latency_ms" in result
        assert isinstance(result["avg_latency_ms"], (int, float))
        # Latency should be non-negative
        assert result["avg_latency_ms"] >= 0

    def test_reset_clears_counters(self, mcp):
        """Telemetry reset zeros all counters."""
        # Make calls
        mcp.call_tool("db_keys", {})
        mcp.call_tool("db_keys", {})

        # Reset
        reset_result = mcp.call_tool("telemetry_reset", {})
        assert not _is_error(reset_result)

        # Snapshot after reset — only the snapshot call itself counted
        result = mcp.call_tool("telemetry_snapshot", {})
        assert isinstance(result, dict)
        # total_calls should be very low (0 or 1 for the snapshot itself)
        assert result["total_calls"] <= 1
        assert result["unique_tools_used"] <= 1

    def test_unique_tools_counted(self, mcp):
        """unique_tools_used tracks distinct tool names."""
        mcp.call_tool("db_keys", {})
        mcp.call_tool("db_put", {"key": "test:telem", "value": "x"})
        mcp.call_tool("db_get", {"key": "test:telem"})

        result = mcp.call_tool("telemetry_snapshot", {})
        assert result["unique_tools_used"] >= 3


class TestRuntimeStatus:
    """Tests for runtime_status tool."""

    def test_returns_version(self, mcp):
        """runtime_status contains version string."""
        result = mcp.call_tool("runtime_status", {})
        assert isinstance(result, dict), f"Expected dict: {result}"
        assert "version" in result
        assert result["version"]  # non-empty

    def test_returns_status_running(self, mcp):
        """runtime_status shows running state."""
        result = mcp.call_tool("runtime_status", {})
        assert result["status"] == "running"

    def test_returns_components(self, mcp):
        """runtime_status lists component statuses."""
        result = mcp.call_tool("runtime_status", {})
        assert "components" in result
        components = result["components"]
        # At minimum, state_store should be active
        assert "state_store" in components
        assert components["state_store"] == "active"

    def test_returns_workdir(self, mcp):
        """runtime_status shows the working directory."""
        result = mcp.call_tool("runtime_status", {})
        assert "workdir" in result
        assert result["workdir"].startswith("/tmp/radix-test-")
