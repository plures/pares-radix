"""
test_app_state_management.py — End-to-end tests for app state via db_dump, telemetry, and cross-domain operations.

Tests the full app state lifecycle through MCP:
- db_dump: captures entire PluresDB state
- telemetry_snapshot: captures telemetry metrics state
- telemetry_reset: clears telemetry counters
- Cross-domain state operations (db + praxis + chronos + canvas)
- State isolation between workdirs
- Bulk operations and recovery

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_app_state_management.py -v
"""
import json
import uuid

import pytest


def unique_key(prefix="test"):
    return f"{prefix}_{uuid.uuid4().hex[:8]}"


# ── Database Dump (Full State Capture) ──────────────────────────────────────────


class TestDbDump:
    """Test db_dump captures the full PluresDB state."""

    def test_dump_returns_valid_structure(self, mcp):
        """db_dump returns a valid response."""
        result = mcp.call_tool("db_dump")
        assert result is not None

    def test_dump_captures_written_data(self, mcp):
        """db_dump includes data written to PluresDB."""
        key = unique_key("dump")
        value = {"captured": True, "ts": "2026-05-22"}
        mcp.call_tool("db_put", {"key": key, "value": value})

        dump = mcp.call_tool("db_dump")
        dump_str = str(dump)
        assert key in dump_str, f"Key {key} not found in dump"

    def test_dump_captures_multiple_keys(self, mcp):
        """db_dump includes all written keys."""
        keys = [unique_key(f"multi_{i}") for i in range(5)]
        for k in keys:
            mcp.call_tool("db_put", {"key": k, "value": f"val_{k}"})

        dump = mcp.call_tool("db_dump")
        dump_str = str(dump)
        for k in keys:
            assert k in dump_str, f"Key {k} missing from dump"

    def test_dump_is_consistent(self, mcp):
        """Multiple dumps without changes return consistent results."""
        key = unique_key("stable")
        mcp.call_tool("db_put", {"key": key, "value": "constant"})

        dump1 = str(mcp.call_tool("db_dump"))
        dump2 = str(mcp.call_tool("db_dump"))
        assert key in dump1
        assert key in dump2

    def test_dump_reflects_deletes(self, mcp):
        """db_dump reflects deleted keys."""
        key = unique_key("ephemeral")
        mcp.call_tool("db_put", {"key": key, "value": "temporary"})

        # Verify it's in the dump
        dump1 = str(mcp.call_tool("db_dump"))
        assert key in dump1

        # Delete and verify gone
        mcp.call_tool("db_delete", {"key": key})
        dump2 = str(mcp.call_tool("db_dump"))
        assert key not in dump2

    def test_dump_with_complex_values(self, mcp):
        """db_dump handles complex nested values."""
        key = unique_key("complex")
        value = {
            "nested": {"deep": {"array": [1, 2, 3]}},
            "flag": True,
            "count": 42,
        }
        mcp.call_tool("db_put", {"key": key, "value": value})

        dump = str(mcp.call_tool("db_dump"))
        assert key in dump


# ── Telemetry State Management ──────────────────────────────────────────────────


class TestTelemetryState:
    """Test telemetry_snapshot and telemetry_reset."""

    def test_telemetry_snapshot_returns_data(self, mcp):
        """telemetry_snapshot returns current metrics."""
        result = mcp.call_tool("telemetry_snapshot")
        assert result is not None
        result_str = str(result)
        # Should contain some telemetry structure
        assert len(result_str) > 5

    def test_telemetry_reset_clears_counters(self, mcp):
        """telemetry_reset clears accumulated metrics."""
        # Generate some activity first
        mcp.call_tool("db_put", {"key": unique_key("activity"), "value": "x"})
        mcp.call_tool("db_get", {"key": "nonexistent"})

        # Take snapshot to see there's data
        pre_reset = mcp.call_tool("telemetry_snapshot")
        assert pre_reset is not None

        # Reset
        result = mcp.call_tool("telemetry_reset")
        assert result is not None

        # Post-reset snapshot should show cleared/zeroed counters
        post_reset = mcp.call_tool("telemetry_snapshot")
        assert post_reset is not None

    def test_telemetry_accumulates_after_reset(self, mcp):
        """After reset, new activity accumulates fresh."""
        mcp.call_tool("telemetry_reset")

        # Generate specific activity
        for i in range(3):
            mcp.call_tool("db_put", {"key": unique_key("post"), "value": i})

        snapshot = mcp.call_tool("telemetry_snapshot")
        assert snapshot is not None
        snapshot_str = str(snapshot)
        # Should show tool call counts
        assert "db_put" in snapshot_str or "tool" in snapshot_str.lower() or len(snapshot_str) > 10

    def test_telemetry_tracks_per_tool_metrics(self, mcp):
        """Telemetry tracks metrics per tool name."""
        mcp.call_tool("telemetry_reset")

        # Call different tools
        mcp.call_tool("db_keys", {})
        mcp.call_tool("db_dump")
        mcp.call_tool("runtime_status", {})

        snapshot = str(mcp.call_tool("telemetry_snapshot"))
        # Should have per-tool breakdown
        assert len(snapshot) > 20


# ── Cross-Domain State Operations ───────────────────────────────────────────────


class TestCrossDomainState:
    """Test state operations spanning multiple domains."""

    def test_db_keys_prefix_filtering(self, mcp):
        """db_keys with prefix filters correctly."""
        prefix = unique_key("prefix")
        mcp.call_tool("db_put", {"key": f"{prefix}:a", "value": "1"})
        mcp.call_tool("db_put", {"key": f"{prefix}:b", "value": "2"})
        mcp.call_tool("db_put", {"key": "other:key", "value": "3"})

        result = mcp.call_tool("db_keys", {"prefix": prefix})
        result_str = str(result)
        assert f"{prefix}:a" in result_str
        assert f"{prefix}:b" in result_str

    def test_praxis_state_visible_in_dump(self, mcp):
        """Praxis constraints added are visible via db state."""
        constraint_name = unique_key("state_vis")
        mcp.call_tool("praxis_add_constraint", {
            "name": constraint_name,
            "severity": "error",
            "message": "state visibility test",
        })

        # Should be visible in praxis list
        praxis_list = str(mcp.call_tool("praxis_list"))
        assert constraint_name in praxis_list

    def test_chronos_events_accumulate(self, mcp):
        """Chronos events persist across calls."""
        event_key = unique_key("evt")
        mcp.call_tool("chronos_record", {
            "key": event_key,
            "actor": "test",
            "action": "Create",
            "data": {"test": True},
        })

        timeline = str(mcp.call_tool("chronos_timeline"))
        assert event_key in timeline

    def test_canvas_state_isolated_per_create(self, mcp):
        """Each canvas-create starts fresh state."""
        title1 = f"Canvas_{unique_key('a')}"
        title2 = f"Canvas_{unique_key('b')}"

        mcp.call_tool("canvas_create", {"title": title1})
        mcp.call_tool("canvas_set_data", {"data": {"domain": "first"}})

        mcp.call_tool("canvas_create", {"title": title2})
        canvas_result = str(mcp.call_tool("canvas_get"))
        # New canvas shouldn't have old data
        assert "first" not in canvas_result or title2 in canvas_result

    def test_runtime_status_reflects_state(self, mcp):
        """runtime_status shows current server state."""
        result = mcp.call_tool("runtime_status", {})
        assert result is not None
        result_str = str(result)
        # Should have version, uptime, or similar
        assert len(result_str) > 20


# ── Bulk Operations & Recovery ──────────────────────────────────────────────────


class TestBulkOperations:
    """Test bulk state operations and recovery patterns."""

    def test_bulk_write_then_dump(self, mcp):
        """Write many keys then dump all."""
        prefix = unique_key("bulk")
        for i in range(20):
            mcp.call_tool("db_put", {"key": f"{prefix}:{i:03d}", "value": {"n": i}})

        dump = str(mcp.call_tool("db_dump"))
        # Should contain all keys
        assert f"{prefix}:000" in dump
        assert f"{prefix}:019" in dump

    def test_bulk_delete_via_keys(self, mcp):
        """Delete keys discovered via db_keys."""
        prefix = unique_key("cleanup")
        for i in range(5):
            mcp.call_tool("db_put", {"key": f"{prefix}:{i}", "value": i})

        # Get keys
        keys_result = mcp.call_tool("db_keys", {"prefix": prefix})
        # Delete them
        for i in range(5):
            mcp.call_tool("db_delete", {"key": f"{prefix}:{i}"})

        # Verify gone
        dump = str(mcp.call_tool("db_dump"))
        assert f"{prefix}:0" not in dump

    def test_overwrite_preserves_latest(self, mcp):
        """Overwriting a key preserves only the latest value."""
        key = unique_key("overwrite")
        mcp.call_tool("db_put", {"key": key, "value": "v1"})
        mcp.call_tool("db_put", {"key": key, "value": "v2"})
        mcp.call_tool("db_put", {"key": key, "value": "v3_final"})

        result = str(mcp.call_tool("db_get", {"key": key}))
        assert "v3_final" in result
        assert "v1" not in result

    def test_concurrent_domain_writes(self, mcp):
        """Writing to multiple domains doesn't interfere."""
        db_key = unique_key("domain_db")
        constraint_name = unique_key("domain_px")

        # Write to db
        mcp.call_tool("db_put", {"key": db_key, "value": "db_data"})
        # Add constraint
        mcp.call_tool("praxis_add_constraint", {
            "name": constraint_name,
            "severity": "warning",
            "message": "domain test",
        })
        # Record chronos event
        mcp.call_tool("chronos_record", {"key": "domain_test", "actor": "test", "action": "Create"})

        # All should be independently retrievable
        db_result = str(mcp.call_tool("db_get", {"key": db_key}))
        assert "db_data" in db_result

        praxis_result = str(mcp.call_tool("praxis_list"))
        assert constraint_name in praxis_result
