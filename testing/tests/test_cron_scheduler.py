"""
test_cron_scheduler.py — Cron/scheduler integration tests via MCP server.

Tests cover:
- cron_list: empty state, populated state
- cron_add: valid cron expressions, interval_secs, edge cases
- cron_remove: by id, non-existent id
- cron_toggle: enable/disable
- heartbeat_status: configuration retrieval
- heartbeat_configure: update settings
- Round-trip: add → list → toggle → remove → verify gone

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_cron_scheduler.py -v
"""
import json
import time
import uuid

import pytest


def _skip_if_not_configured(result):
    """Skip test if scheduler is not configured in the MCP server."""
    result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
    if "not configured" in result_str.lower():
        pytest.skip("Scheduler not configured in mcp-serve mode")


# ── cron_list ─────────────────────────────────────────────────────────────────


class TestCronList:
    """Tests for cron_list tool."""

    def test_cron_list_returns_result(self, mcp):
        """cron_list should return a result (list or object with tasks)."""
        result = mcp.call_tool("cron_list")
        assert result is not None
        _skip_if_not_configured(result)
        # Should be a list or dict with tasks key
        if isinstance(result, dict):
            assert "error" not in result or "tasks" in result
        # Empty list is valid for fresh state

    def test_cron_list_is_repeatable(self, mcp):
        """Calling cron_list twice should return consistent results."""
        r1 = mcp.call_tool("cron_list")
        _skip_if_not_configured(r1)
        r2 = mcp.call_tool("cron_list")
        assert r1 == r2


# ── cron_add ──────────────────────────────────────────────────────────────────


class TestCronAdd:
    """Tests for cron_add tool."""

    def test_add_with_cron_expression(self, mcp):
        """Add a task with a standard 5-field cron expression."""
        name = f"test-cron-{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("cron_add", {
            "name": name,
            "command": "echo hello",
            "cron": "*/5 * * * *",
        })
        assert result is not None
        _skip_if_not_configured(result)
        if isinstance(result, dict):
            # Should return success or an id
            assert "error" not in result or "id" in result or "ok" in str(result).lower()

    def test_add_with_interval_secs(self, mcp):
        """Add a task with interval_secs instead of cron expression."""
        name = f"test-interval-{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("cron_add", {
            "name": name,
            "command": "echo interval-task",
            "interval_secs": 300,
        })
        assert result is not None
        _skip_if_not_configured(result)
        if isinstance(result, dict):
            assert "error" not in result or "id" in result

    def test_add_hourly_cron(self, mcp):
        """Add an hourly cron task."""
        name = f"test-hourly-{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("cron_add", {
            "name": name,
            "command": "date >> /tmp/hourly.log",
            "cron": "0 * * * *",
        })
        assert result is not None
        _skip_if_not_configured(result)

    def test_add_missing_name_returns_error(self, mcp):
        """Adding without a name should fail."""
        result = mcp.call_tool("cron_add", {
            "command": "echo no-name",
            "cron": "* * * * *",
        })
        # Should get an error about missing required field
        if isinstance(result, dict):
            assert "error" in result or "name" in str(result).lower()

    def test_add_missing_command_returns_error(self, mcp):
        """Adding without a command should fail."""
        result = mcp.call_tool("cron_add", {
            "name": "no-command-task",
        })
        if isinstance(result, dict):
            assert "error" in result or "command" in str(result).lower()

    def test_add_appears_in_list(self, mcp):
        """A newly added task should appear in cron_list."""
        name = f"test-visible-{uuid.uuid4().hex[:6]}"
        add_result = mcp.call_tool("cron_add", {
            "name": name,
            "command": "echo visible",
            "interval_secs": 600,
        })
        _skip_if_not_configured(add_result)
        result = mcp.call_tool("cron_list")
        # The task name should appear somewhere in the result
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert name in result_str, f"Task '{name}' not found in cron_list result"


# ── cron_remove ───────────────────────────────────────────────────────────────


class TestCronRemove:
    """Tests for cron_remove tool."""

    def test_remove_nonexistent_id(self, mcp):
        """Removing a non-existent task should handle gracefully."""
        result = mcp.call_tool("cron_remove", {"id": "nonexistent-id-12345"})
        # Should not crash — either error message or no-op
        assert result is not None

    def test_add_then_remove(self, mcp):
        """Add a task, then remove it by id."""
        name = f"test-removable-{uuid.uuid4().hex[:6]}"
        add_result = mcp.call_tool("cron_add", {
            "name": name,
            "command": "echo removable",
            "interval_secs": 999,
        })

        # Extract id from add result
        task_id = None
        if isinstance(add_result, dict):
            task_id = add_result.get("id") or add_result.get("task_id")
        if isinstance(add_result, str):
            # Try to find an id-like string
            import re
            match = re.search(r'[a-f0-9-]{8,}', add_result)
            if match:
                task_id = match.group(0)

        if task_id is None:
            # Try to find it in the list
            list_result = mcp.call_tool("cron_list")
            list_str = json.dumps(list_result) if isinstance(list_result, (dict, list)) else str(list_result)
            if name not in list_str:
                pytest.skip("Could not determine task id from add result")
            # Parse id from list
            if isinstance(list_result, dict) and "tasks" in list_result:
                for task in list_result["tasks"]:
                    if task.get("name") == name:
                        task_id = task.get("id")
                        break

        if task_id:
            remove_result = mcp.call_tool("cron_remove", {"id": task_id})
            assert remove_result is not None
            # Verify it's gone
            list_after = mcp.call_tool("cron_list")
            list_str = json.dumps(list_after) if isinstance(list_after, (dict, list)) else str(list_after)
            assert name not in list_str, "Task still present after removal"


# ── cron_toggle ───────────────────────────────────────────────────────────────


class TestCronToggle:
    """Tests for cron_toggle tool."""

    def test_toggle_nonexistent(self, mcp):
        """Toggling a non-existent task should handle gracefully."""
        result = mcp.call_tool("cron_toggle", {
            "id": "fake-id-99999",
            "enabled": False,
        })
        assert result is not None
        _skip_if_not_configured(result)

    def test_add_disable_enable_cycle(self, mcp):
        """Add a task, disable it, re-enable it."""
        name = f"test-toggle-{uuid.uuid4().hex[:6]}"
        add_result = mcp.call_tool("cron_add", {
            "name": name,
            "command": "echo toggleable",
            "interval_secs": 120,
        })
        _skip_if_not_configured(add_result)

        # Extract id
        task_id = None
        if isinstance(add_result, dict):
            task_id = add_result.get("id") or add_result.get("task_id")

        if task_id is None:
            list_result = mcp.call_tool("cron_list")
            if isinstance(list_result, dict) and "tasks" in list_result:
                for task in list_result["tasks"]:
                    if task.get("name") == name:
                        task_id = task.get("id")
                        break

        if task_id is None:
            pytest.skip("Could not determine task id")

        # Disable
        disable_result = mcp.call_tool("cron_toggle", {
            "id": task_id,
            "enabled": False,
        })
        assert disable_result is not None

        # Re-enable
        enable_result = mcp.call_tool("cron_toggle", {
            "id": task_id,
            "enabled": True,
        })
        assert enable_result is not None

        # Cleanup
        mcp.call_tool("cron_remove", {"id": task_id})


# ── heartbeat_status ──────────────────────────────────────────────────────────


class TestHeartbeatStatus:
    """Tests for heartbeat_status tool."""

    def test_heartbeat_status_returns_config(self, mcp):
        """heartbeat_status should return configuration info."""
        result = mcp.call_tool("heartbeat_status")
        assert result is not None
        _skip_if_not_configured(result)
        if isinstance(result, dict):
            # Should have config-related fields
            result_str = json.dumps(result)
            # At minimum, should mention enabled or interval or some config
            assert any(k in result_str for k in [
                "enabled", "interval", "quiet", "config", "count", "status"
            ]), f"heartbeat_status returned unexpected structure: {result}"


# ── heartbeat_configure ───────────────────────────────────────────────────────


class TestHeartbeatConfigure:
    """Tests for heartbeat_configure tool."""

    def test_configure_quiet_hours(self, mcp):
        """Configure quiet hours."""
        result = mcp.call_tool("heartbeat_configure", {
            "quiet_hours_enabled": True,
            "quiet_hours_start": 23,
            "quiet_hours_end": 8,
        })
        assert result is not None
        _skip_if_not_configured(result)
        if isinstance(result, dict):
            assert "error" not in result

    def test_configure_interval(self, mcp):
        """Configure tick interval."""
        result = mcp.call_tool("heartbeat_configure", {
            "interval_secs": 3600,
        })
        assert result is not None
        _skip_if_not_configured(result)
        if isinstance(result, dict):
            assert "error" not in result

    def test_configure_max_proactive(self, mcp):
        """Configure max proactive messages per day."""
        result = mcp.call_tool("heartbeat_configure", {
            "max_proactive_per_day": 5,
        })
        assert result is not None
        _skip_if_not_configured(result)

    def test_configure_reflects_in_status(self, mcp):
        """Configuration changes should reflect in heartbeat_status."""
        # Set a distinctive value
        cfg_result = mcp.call_tool("heartbeat_configure", {
            "interval_secs": 7200,
        })
        _skip_if_not_configured(cfg_result)
        status = mcp.call_tool("heartbeat_status")
        status_str = json.dumps(status) if isinstance(status, (dict, list)) else str(status)
        # Should see the interval value somewhere
        assert "7200" in status_str, f"Configured interval not reflected in status: {status}"


# ── Round-trip lifecycle ──────────────────────────────────────────────────────


class TestCronLifecycle:
    """Full lifecycle tests: add → list → toggle → remove."""

    def test_full_lifecycle(self, mcp):
        """Complete cron task lifecycle."""
        name = f"lifecycle-{uuid.uuid4().hex[:6]}"

        # 1. Add
        add_result = mcp.call_tool("cron_add", {
            "name": name,
            "command": f"echo {name}",
            "cron": "30 2 * * 0",  # Weekly at 2:30 AM Sunday
        })
        assert add_result is not None
        _skip_if_not_configured(add_result)

        # 2. Verify in list
        list_result = mcp.call_tool("cron_list")
        list_str = json.dumps(list_result) if isinstance(list_result, (dict, list)) else str(list_result)
        assert name in list_str

        # 3. Extract id and toggle
        task_id = None
        if isinstance(add_result, dict):
            task_id = add_result.get("id") or add_result.get("task_id")
        if task_id is None and isinstance(list_result, dict) and "tasks" in list_result:
            for task in list_result["tasks"]:
                if task.get("name") == name:
                    task_id = task.get("id")
                    break

        if task_id:
            # 4. Disable
            mcp.call_tool("cron_toggle", {"id": task_id, "enabled": False})

            # 5. Remove
            mcp.call_tool("cron_remove", {"id": task_id})

            # 6. Verify gone
            final_list = mcp.call_tool("cron_list")
            final_str = json.dumps(final_list) if isinstance(final_list, (dict, list)) else str(final_list)
            assert name not in final_str

    def test_multiple_tasks_coexist(self, mcp):
        """Multiple cron tasks can coexist."""
        # Check if scheduler is available first
        check = mcp.call_tool("cron_list")
        _skip_if_not_configured(check)

        names = [f"multi-{i}-{uuid.uuid4().hex[:4]}" for i in range(3)]

        # Add all
        for i, name in enumerate(names):
            mcp.call_tool("cron_add", {
                "name": name,
                "command": f"echo task-{i}",
                "interval_secs": (i + 1) * 100,
            })

        # All should appear in list
        list_result = mcp.call_tool("cron_list")
        list_str = json.dumps(list_result) if isinstance(list_result, (dict, list)) else str(list_result)
        for name in names:
            assert name in list_str, f"Task '{name}' missing from list"
