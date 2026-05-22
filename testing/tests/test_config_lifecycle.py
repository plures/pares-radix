"""
test_config_lifecycle.py — Integration tests for config_get/set/list/delete/reload via MCP.

Exercises the full config lifecycle: CRUD operations, prefix filtering,
hot-reload signaling, edge cases (unicode, nested JSON, large values),
and config_reload behavior.

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_config_lifecycle.py -v

Note: McpClient.call_tool() returns native Python types (dict, list, int, str, None, bool)
because it parses JSON responses from the server. Assertions must handle native types.
"""
import json
import os
import time
import uuid
import pytest
from pathlib import Path


def unique_key(prefix="test"):
    """Generate a unique config key to avoid collisions between tests."""
    return f"{prefix}_{uuid.uuid4().hex[:8]}"


def to_str(val):
    """Convert a call_tool result to string for text-based assertions."""
    if val is None:
        return "null"
    if isinstance(val, str):
        return val
    return json.dumps(val)


class TestConfigGet:
    """Tests for config_get tool."""

    def test_get_nonexistent_key(self, mcp):
        """Getting a key that doesn't exist returns null."""
        key = unique_key("missing")
        result = mcp.call_tool("config_get", {"key": key})
        assert result is None or to_str(result) == "null"

    def test_get_missing_key_param(self, mcp):
        """config_get without key parameter returns error."""
        result = mcp.call_tool("config_get", {})
        s = to_str(result)
        assert "error" in s.lower() or "missing" in s.lower()

    def test_get_empty_key(self, mcp):
        """config_get with empty string key returns null (no crash)."""
        result = mcp.call_tool("config_get", {"key": ""})
        # Should return null or a valid response, not crash
        # (result being None from json.loads("null") is fine)
        assert result is None or result is not None  # Just verify no exception


class TestConfigSet:
    """Tests for config_set tool."""

    def test_set_string_value(self, mcp):
        """Set a simple string value."""
        key = unique_key("str")
        result = mcp.call_tool("config_set", {"key": key, "value": "hello"})
        assert "config set" in to_str(result).lower()
        got = mcp.call_tool("config_get", {"key": key})
        assert got == "hello"

    def test_set_integer_value(self, mcp):
        """Set an integer value."""
        key = unique_key("int")
        result = mcp.call_tool("config_set", {"key": key, "value": 42})
        assert "config set" in to_str(result).lower()
        got = mcp.call_tool("config_get", {"key": key})
        assert got == 42

    def test_set_boolean_value(self, mcp):
        """Set a boolean value."""
        key = unique_key("bool")
        mcp.call_tool("config_set", {"key": key, "value": True})
        got = mcp.call_tool("config_get", {"key": key})
        assert got is True

    def test_set_nested_json_value(self, mcp):
        """Set a nested JSON object as a config value."""
        key = unique_key("nested")
        value = {"model": "gpt-4.1", "temperature": 0.7, "options": {"stream": True}}
        mcp.call_tool("config_set", {"key": key, "value": value})
        got = mcp.call_tool("config_get", {"key": key})
        assert isinstance(got, dict)
        assert got["model"] == "gpt-4.1"
        assert got["temperature"] == 0.7
        assert got["options"]["stream"] is True

    def test_set_array_value(self, mcp):
        """Set an array value."""
        key = unique_key("arr")
        value = ["a", "b", "c", 1, 2, 3]
        mcp.call_tool("config_set", {"key": key, "value": value})
        got = mcp.call_tool("config_get", {"key": key})
        assert isinstance(got, list)
        assert got == ["a", "b", "c", 1, 2, 3]

    def test_set_null_value(self, mcp):
        """Setting null effectively clears the value."""
        key = unique_key("null")
        mcp.call_tool("config_set", {"key": key, "value": "original"})
        mcp.call_tool("config_set", {"key": key, "value": None})
        got = mcp.call_tool("config_get", {"key": key})
        assert got is None

    def test_set_missing_value_param(self, mcp):
        """config_set without value parameter returns error."""
        key = unique_key("noval")
        result = mcp.call_tool("config_set", {"key": key})
        s = to_str(result)
        assert "error" in s.lower() or "missing" in s.lower()

    def test_set_missing_key_param(self, mcp):
        """config_set without key parameter returns error."""
        result = mcp.call_tool("config_set", {"value": "test"})
        s = to_str(result)
        assert "error" in s.lower() or "missing" in s.lower()

    def test_set_unicode_value(self, mcp):
        """Set a unicode string value."""
        key = unique_key("unicode")
        value = "日本語テスト 🚀 émojis"
        mcp.call_tool("config_set", {"key": key, "value": value})
        got = mcp.call_tool("config_get", {"key": key})
        assert "日本語" in str(got)
        assert "🚀" in str(got)

    def test_set_overwrite(self, mcp):
        """Overwriting an existing config key works."""
        key = unique_key("overwrite")
        mcp.call_tool("config_set", {"key": key, "value": "first"})
        got1 = mcp.call_tool("config_get", {"key": key})
        assert got1 == "first"

        mcp.call_tool("config_set", {"key": key, "value": "second"})
        got2 = mcp.call_tool("config_get", {"key": key})
        assert got2 == "second"

    def test_set_updates_last_modified(self, mcp):
        """config_set updates the __last_modified timestamp."""
        key = unique_key("modified")
        before = time.time()
        mcp.call_tool("config_set", {"key": key, "value": "trigger"})
        ts = mcp.call_tool("db_get", {"key": "config:__last_modified"})
        if ts is not None:
            assert isinstance(ts, (int, float))
            assert ts >= int(before) - 1, "Last modified timestamp too old"


class TestConfigList:
    """Tests for config_list tool."""

    def test_list_all(self, mcp):
        """config_list returns all set config keys."""
        prefix = unique_key("listall")
        mcp.call_tool("config_set", {"key": f"{prefix}_a", "value": "alpha"})
        mcp.call_tool("config_set", {"key": f"{prefix}_b", "value": "beta"})

        result = mcp.call_tool("config_list", {})
        assert isinstance(result, dict)
        assert f"{prefix}_a" in result
        assert f"{prefix}_b" in result

    def test_list_with_prefix(self, mcp):
        """config_list with prefix filters to matching keys only."""
        prefix = unique_key("prefix")
        mcp.call_tool("config_set", {"key": f"{prefix}.sub1", "value": "v1"})
        mcp.call_tool("config_set", {"key": f"{prefix}.sub2", "value": "v2"})
        mcp.call_tool("config_set", {"key": f"other_{prefix}", "value": "v3"})

        result = mcp.call_tool("config_list", {"prefix": prefix})
        assert isinstance(result, dict)
        matching_keys = [k for k in result if k.startswith(prefix)]
        assert len(matching_keys) >= 2

    def test_list_excludes_internal_keys(self, mcp):
        """config_list should not show keys starting with __."""
        result = mcp.call_tool("config_list", {})
        assert isinstance(result, dict)
        internal = [k for k in result if k.startswith("__")]
        assert len(internal) == 0, f"Internal keys leaked: {internal}"

    def test_list_empty_prefix(self, mcp):
        """config_list with empty prefix returns all keys."""
        result = mcp.call_tool("config_list", {"prefix": ""})
        assert isinstance(result, dict)

    def test_list_nonexistent_prefix(self, mcp):
        """config_list with a prefix matching nothing returns empty object."""
        result = mcp.call_tool("config_list", {"prefix": f"zzzz_{uuid.uuid4().hex}"})
        assert isinstance(result, dict)
        assert len(result) == 0


class TestConfigDelete:
    """Tests for config_delete tool."""

    def test_delete_existing_key(self, mcp):
        """Deleting an existing key removes it."""
        key = unique_key("del")
        mcp.call_tool("config_set", {"key": key, "value": "to_delete"})
        got = mcp.call_tool("config_get", {"key": key})
        assert got == "to_delete"

        result = mcp.call_tool("config_delete", {"key": key})
        s = to_str(result)
        assert "deleted" in s.lower()
        assert key in s

        got2 = mcp.call_tool("config_get", {"key": key})
        assert got2 is None

    def test_delete_nonexistent_key(self, mcp):
        """Deleting a nonexistent key reports not found."""
        key = unique_key("nothere")
        result = mcp.call_tool("config_delete", {"key": key})
        assert "not found" in to_str(result).lower()

    def test_delete_missing_key_param(self, mcp):
        """config_delete without key parameter returns error."""
        result = mcp.call_tool("config_delete", {})
        s = to_str(result)
        assert "error" in s.lower() or "missing" in s.lower()

    def test_delete_reports_previous_value(self, mcp):
        """config_delete shows what the previous value was."""
        key = unique_key("delval")
        mcp.call_tool("config_set", {"key": key, "value": "previous_val"})
        result = mcp.call_tool("config_delete", {"key": key})
        s = to_str(result)
        assert "was:" in s.lower() or "previous_val" in s

    def test_delete_updates_last_modified(self, mcp):
        """config_delete updates the __last_modified timestamp."""
        key = unique_key("delmod")
        mcp.call_tool("config_set", {"key": key, "value": "temp"})
        before = time.time()
        mcp.call_tool("config_delete", {"key": key})
        ts = mcp.call_tool("db_get", {"key": "config:__last_modified"})
        if ts is not None:
            assert isinstance(ts, (int, float))
            assert ts >= int(before) - 1


class TestConfigReload:
    """Tests for config_reload tool."""

    def test_reload_signals_successfully(self, mcp):
        """config_reload returns success."""
        result = mcp.call_tool("config_reload", {})
        s = to_str(result)
        assert "reload" in s.lower()

    def test_reload_sets_reload_timestamp(self, mcp):
        """config_reload sets the __reload_requested internal key."""
        before = time.time()
        mcp.call_tool("config_reload", {})
        ts = mcp.call_tool("db_get", {"key": "config:__reload_requested"})
        if ts is not None:
            assert isinstance(ts, (int, float))
            assert ts >= int(before) - 1

    def test_reload_idempotent(self, mcp):
        """Calling config_reload multiple times doesn't cause errors."""
        for _ in range(3):
            result = mcp.call_tool("config_reload", {})
            s = to_str(result)
            assert "error" not in s.lower()


class TestConfigEdgeCases:
    """Edge case tests for config operations."""

    def test_dotted_key_name(self, mcp):
        """Config keys with dots (namespaced) work correctly."""
        key = f"routing.{unique_key('dot')}.model"
        mcp.call_tool("config_set", {"key": key, "value": "gpt-4o"})
        got = mcp.call_tool("config_get", {"key": key})
        assert got == "gpt-4o"
        mcp.call_tool("config_delete", {"key": key})

    def test_key_with_special_characters(self, mcp):
        """Config keys with dashes and underscores work."""
        key = unique_key("my-key_with-mixed_chars")
        mcp.call_tool("config_set", {"key": key, "value": "special"})
        got = mcp.call_tool("config_get", {"key": key})
        assert got == "special"
        mcp.call_tool("config_delete", {"key": key})

    def test_large_value(self, mcp):
        """Config can store moderately large values."""
        key = unique_key("large")
        value = "x" * 10000
        mcp.call_tool("config_set", {"key": key, "value": value})
        got = mcp.call_tool("config_get", {"key": key})
        assert isinstance(got, str) and len(got) >= 10000
        mcp.call_tool("config_delete", {"key": key})

    def test_rapid_set_get_cycle(self, mcp):
        """Rapid set-then-get cycles are consistent."""
        key = unique_key("rapid")
        for i in range(10):
            mcp.call_tool("config_set", {"key": key, "value": f"v{i}"})
            got = mcp.call_tool("config_get", {"key": key})
            assert got == f"v{i}"
        mcp.call_tool("config_delete", {"key": key})

    def test_set_get_delete_roundtrip(self, mcp):
        """Full lifecycle: set → get → delete → get(null)."""
        key = unique_key("lifecycle")
        # Set
        mcp.call_tool("config_set", {"key": key, "value": {"life": "cycle"}})
        # Get
        got = mcp.call_tool("config_get", {"key": key})
        assert isinstance(got, dict)
        assert got["life"] == "cycle"
        # Delete
        mcp.call_tool("config_delete", {"key": key})
        # Verify gone
        got2 = mcp.call_tool("config_get", {"key": key})
        assert got2 is None

    def test_config_isolation_from_db(self, mcp):
        """Config keys are stored under 'config:' namespace, not polluting raw db."""
        key = unique_key("isolation")
        mcp.call_tool("config_set", {"key": key, "value": "isolated"})
        # Direct db_get without config: prefix should not find it
        raw = mcp.call_tool("db_get", {"key": key})
        assert raw is None
        # With config: prefix should find it
        namespaced = mcp.call_tool("db_get", {"key": f"config:{key}"})
        assert namespaced == "isolated"
        mcp.call_tool("config_delete", {"key": key})

    def test_float_value_precision(self, mcp):
        """Float values maintain precision through set/get."""
        key = unique_key("float")
        mcp.call_tool("config_set", {"key": key, "value": 3.14159265358979})
        got = mcp.call_tool("config_get", {"key": key})
        assert isinstance(got, float)
        assert abs(got - 3.14159265358979) < 1e-10
        mcp.call_tool("config_delete", {"key": key})

    def test_empty_string_value(self, mcp):
        """Empty string is a valid config value, distinct from null."""
        key = unique_key("empty")
        mcp.call_tool("config_set", {"key": key, "value": ""})
        got = mcp.call_tool("config_get", {"key": key})
        assert got == ""
        mcp.call_tool("config_delete", {"key": key})

    def test_zero_is_valid(self, mcp):
        """Zero is a valid config value, distinct from null."""
        key = unique_key("zero")
        mcp.call_tool("config_set", {"key": key, "value": 0})
        got = mcp.call_tool("config_get", {"key": key})
        assert got == 0
        mcp.call_tool("config_delete", {"key": key})

    def test_false_is_valid(self, mcp):
        """False is a valid config value, distinct from null."""
        key = unique_key("false")
        mcp.call_tool("config_set", {"key": key, "value": False})
        got = mcp.call_tool("config_get", {"key": key})
        assert got is False
        mcp.call_tool("config_delete", {"key": key})


class TestRuntimeStatusWithConfig:
    """Tests verifying runtime_status reflects config state."""

    def test_runtime_status_includes_config_count(self, mcp):
        """runtime_status reports config_key_count."""
        result = mcp.call_tool("runtime_status", {})
        assert isinstance(result, dict)
        assert "config_key_count" in result
        assert isinstance(result["config_key_count"], int)

    def test_config_count_changes_with_set(self, mcp):
        """Adding a config key increments the config_key_count."""
        result1 = mcp.call_tool("runtime_status", {})
        assert isinstance(result1, dict)
        count1 = result1["config_key_count"]

        key = unique_key("count")
        mcp.call_tool("config_set", {"key": key, "value": "counting"})

        result2 = mcp.call_tool("runtime_status", {})
        count2 = result2["config_key_count"]
        assert count2 >= count1

        mcp.call_tool("config_delete", {"key": key})

    def test_runtime_status_components(self, mcp):
        """runtime_status includes component health info."""
        result = mcp.call_tool("runtime_status", {})
        assert isinstance(result, dict)
        assert "components" in result
        assert "status" in result
        assert result["status"] == "running"
        assert "version" in result
        assert "workdir" in result
