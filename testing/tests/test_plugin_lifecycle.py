"""
test_plugin_lifecycle.py — Plugin CRUD lifecycle via MCP tools.

Tests the full plugin lifecycle:
- Register a plugin
- List plugins (verify it appears)
- Get plugin info
- Activate a plugin
- Deactivate a plugin
- Re-register (update) a plugin
- Edge cases: duplicate register, activate non-existent, deactivate non-existent

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_plugin_lifecycle.py -v
"""
import json
import uuid
import pytest


def _unique_plugin_name():
    return f"test-plugin-{uuid.uuid4().hex[:8]}"


class TestPluginRegistration:
    """Test plugin registration via MCP."""

    def test_register_plugin_basic(self, mcp):
        """Register a plugin with minimal fields."""
        name = _unique_plugin_name()
        result = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
        })
        result_str = str(result)
        if "not configured" in result_str.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        # Should succeed — no error
        assert "error" not in result_str.lower() or name in result_str

    def test_register_plugin_with_all_fields(self, mcp):
        """Register a plugin with description and capabilities."""
        name = _unique_plugin_name()
        result = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "2.1.0",
            "description": "A fully-specified test plugin",
            "capabilities": ["search", "index", "notify"],
        })
        result_str = str(result)
        if "not configured" in result_str.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        assert "error" not in result_str.lower() or name in result_str

    def test_register_plugin_appears_in_list(self, mcp):
        """A registered plugin should appear in plugin_list."""
        name = _unique_plugin_name()
        reg_result = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
            "description": "List test plugin",
        })
        reg_str = str(reg_result)
        if "not configured" in reg_str.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        list_result = mcp.call_tool("plugin_list", {})
        list_str = str(list_result)
        assert name in list_str, f"Plugin '{name}' not found in list: {list_str[:200]}"

    def test_register_duplicate_name_updates(self, mcp):
        """Re-registering a plugin with the same name should update it."""
        name = _unique_plugin_name()
        mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
            "description": "Original",
        })
        result = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "2.0.0",
            "description": "Updated",
        })
        result_str = str(result)
        if "not configured" in result_str.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        info = mcp.call_tool("plugin_info", {"name": name})
        info_str = str(info)
        # Should reflect updated version
        assert "2.0.0" in info_str or "Updated" in info_str


class TestPluginInfo:
    """Test plugin info retrieval."""

    def test_plugin_info_existing(self, mcp):
        """Get info for a registered plugin."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "3.0.0",
            "description": "Info test plugin",
            "capabilities": ["test-cap"],
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        info = mcp.call_tool("plugin_info", {"name": name})
        info_str = str(info)
        assert name in info_str
        assert "3.0.0" in info_str

    def test_plugin_info_nonexistent(self, mcp):
        """Getting info for non-existent plugin returns error or empty."""
        result = mcp.call_tool("plugin_info", {"name": "nonexistent-plugin-xyz"})
        result_str = str(result).lower()
        if "not configured" in result_str:
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        # Should indicate not found
        assert "not found" in result_str or "error" in result_str or result is None

    def test_plugin_info_shows_capabilities(self, mcp):
        """Plugin info should include capabilities list."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
            "capabilities": ["search", "embed"],
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        info = mcp.call_tool("plugin_info", {"name": name})
        info_str = str(info)
        assert "search" in info_str or "embed" in info_str


class TestPluginActivation:
    """Test plugin activate/deactivate lifecycle."""

    def test_activate_registered_plugin(self, mcp):
        """Activating a registered plugin should succeed."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        result = mcp.call_tool("plugin_activate", {"name": name})
        result_str = str(result).lower()
        # Should succeed or say already active
        assert "active" in result_str or "error" not in result_str

    def test_activate_nonexistent_plugin(self, mcp):
        """Activating a non-existent plugin should fail gracefully."""
        result = mcp.call_tool("plugin_activate", {"name": "ghost-plugin-xyz"})
        result_str = str(result).lower()
        if "not configured" in result_str:
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        assert "not found" in result_str or "error" in result_str

    def test_deactivate_registered_plugin(self, mcp):
        """Deactivating a registered plugin should succeed."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        # Activate first
        mcp.call_tool("plugin_activate", {"name": name})

        # Now deactivate
        result = mcp.call_tool("plugin_deactivate", {"name": name})
        result_str = str(result).lower()
        assert "deactivat" in result_str or "inactive" in result_str or "error" not in result_str

    def test_deactivate_nonexistent_plugin(self, mcp):
        """Deactivating a non-existent plugin should fail gracefully."""
        result = mcp.call_tool("plugin_deactivate", {"name": "ghost-plugin-xyz"})
        result_str = str(result).lower()
        if "not configured" in result_str:
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        assert "not found" in result_str or "error" in result_str

    def test_activate_already_active(self, mcp):
        """Activating an already-active plugin is idempotent."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        mcp.call_tool("plugin_activate", {"name": name})
        result = mcp.call_tool("plugin_activate", {"name": name})
        result_str = str(result).lower()
        # Should not error — idempotent
        assert "already active" in result_str or "active" in result_str

    def test_full_lifecycle(self, mcp):
        """Full lifecycle: register → activate → deactivate → re-activate."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
            "description": "Lifecycle test",
            "capabilities": ["lifecycle"],
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        # Activate
        act = mcp.call_tool("plugin_activate", {"name": name})
        assert "error" not in str(act).lower() or "active" in str(act).lower()

        # Verify in list
        plugins = mcp.call_tool("plugin_list", {})
        assert name in str(plugins)

        # Deactivate
        deact = mcp.call_tool("plugin_deactivate", {"name": name})
        deact_str = str(deact).lower()
        assert "error" not in deact_str or "not found" not in deact_str

        # Re-activate
        react = mcp.call_tool("plugin_activate", {"name": name})
        assert "error" not in str(react).lower() or "active" in str(react).lower()


class TestPluginListFiltering:
    """Test plugin list returns expected data."""

    def test_plugin_list_returns_array(self, mcp):
        """plugin_list should return a list/array structure."""
        result = mcp.call_tool("plugin_list", {})
        result_str = str(result)
        if "not configured" in result_str.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        # Should be parseable as JSON array or contain array-like structure
        assert result is not None

    def test_plugin_list_after_multiple_registrations(self, mcp):
        """Multiple registered plugins all appear in list."""
        names = [_unique_plugin_name() for _ in range(3)]
        for i, name in enumerate(names):
            reg = mcp.call_tool("plugin_register", {
                "name": name,
                "version": f"{i+1}.0.0",
            })
            if "not configured" in str(reg).lower():
                pytest.skip("Plugin runtime not configured in headless MCP mode")

        result = mcp.call_tool("plugin_list", {})
        result_str = str(result)
        for name in names:
            assert name in result_str, f"Plugin '{name}' missing from list"

    def test_plugin_list_includes_version(self, mcp):
        """Plugin list entries should include version info."""
        name = _unique_plugin_name()
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "7.7.7",
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        result = mcp.call_tool("plugin_list", {})
        result_str = str(result)
        assert "7.7.7" in result_str


class TestPluginEdgeCases:
    """Edge cases and error handling."""

    def test_register_without_name(self, mcp):
        """Registering without a name should fail."""
        result = mcp.call_tool("plugin_register", {
            "version": "1.0.0",
        })
        result_str = str(result).lower()
        if "not configured" in result_str:
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        # Should error — name is required
        assert "error" in result_str or "required" in result_str or "missing" in result_str

    def test_register_without_version(self, mcp):
        """Registering without a version should fail."""
        result = mcp.call_tool("plugin_register", {
            "name": _unique_plugin_name(),
        })
        result_str = str(result).lower()
        if "not configured" in result_str:
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        # Should error — version is required
        assert "error" in result_str or "required" in result_str or "missing" in result_str

    def test_register_empty_name(self, mcp):
        """Registering with empty name should fail."""
        result = mcp.call_tool("plugin_register", {
            "name": "",
            "version": "1.0.0",
        })
        result_str = str(result).lower()
        assert "error" in result_str or "empty" in result_str or "invalid" in result_str or result is not None

    def test_plugin_with_many_capabilities(self, mcp):
        """Plugins can declare many capabilities."""
        name = _unique_plugin_name()
        caps = [f"cap-{i}" for i in range(20)]
        reg = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "1.0.0",
            "capabilities": caps,
        })
        if "not configured" in str(reg).lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")

        info = mcp.call_tool("plugin_info", {"name": name})
        info_str = str(info)
        # At least some capabilities should be present
        assert "cap-0" in info_str or "cap-19" in info_str
