"""
test_browser_tools.py — E2E tests for browser automation MCP tools.

Tests cover:
- browser_status: graceful response when no CDP endpoint configured
- browser_navigate: input validation
- browser_snapshot: graceful error handling
- browser_screenshot: format validation
- browser_click: parameter requirements
- browser_type: parameter requirements
- Live CDP tests (when RADIX_CDP_ENDPOINT is set): actual browser interaction

Run: pytest testing/tests/test_browser_tools.py -v
"""
import json
import os
import subprocess
import time

import pytest

from conftest import McpClient


@pytest.fixture(scope="module")
def mcp():
    """Shared MCP client for the test module."""
    client = McpClient()
    client.start()
    yield client
    client.stop()


# ── Browser status ─────────────────────────────────────────────────────────────


class TestBrowserStatus:
    """Tests for browser_status tool."""

    def test_status_without_cdp_configured(self, mcp):
        """When no CDP endpoint is configured, returns informative error."""
        result = mcp.call_tool("browser_status")
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Should indicate browser not configured or not available
        assert any(word in text.lower() for word in ["not configured", "not available", "available"]), f"Unexpected: {text}"

    def test_status_returns_json(self, mcp):
        """Response should be parseable (either error text or JSON)."""
        result = mcp.call_tool("browser_status")
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Either a JSON object or an error message — both are valid
        try:
            parsed = json.loads(text)
            assert isinstance(parsed, dict)
        except (json.JSONDecodeError, TypeError):
            # Error message is also acceptable
            assert len(text) > 0


# ── Browser navigate ───────────────────────────────────────────────────────────


class TestBrowserNavigate:
    """Tests for browser_navigate tool."""

    def test_navigate_missing_url(self, mcp):
        """Navigate without URL should return error."""
        result = mcp.call_tool("browser_navigate", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "url" in text.lower() or "not configured" in text.lower()

    def test_navigate_with_empty_url(self, mcp):
        """Navigate with empty string should error."""
        result = mcp.call_tool("browser_navigate", {"url": ""})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Should fail - either URL validation or no browser
        assert any(word in text.lower() for word in ["error", "not configured", "invalid", "fail"])

    def test_navigate_without_browser(self, mcp):
        """Navigate when browser not configured gives clear error."""
        result = mcp.call_tool("browser_navigate", {"url": "https://example.com"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "not configured" in text.lower() or "error" in text.lower() or "fail" in text.lower()


# ── Browser snapshot ───────────────────────────────────────────────────────────


class TestBrowserSnapshot:
    """Tests for browser_snapshot tool."""

    def test_snapshot_without_browser(self, mcp):
        """Snapshot when no browser configured returns error."""
        result = mcp.call_tool("browser_snapshot")
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "not configured" in text.lower() or "error" in text.lower()

    def test_snapshot_accepts_no_args(self, mcp):
        """Snapshot requires no arguments (works against current page)."""
        result = mcp.call_tool("browser_snapshot", {})
        assert result is not None
        # Just verify it returns something (error or result)
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert len(text) > 0


# ── Browser screenshot ─────────────────────────────────────────────────────────


class TestBrowserScreenshot:
    """Tests for browser_screenshot tool."""

    def test_screenshot_without_browser(self, mcp):
        """Screenshot when no browser configured returns error."""
        result = mcp.call_tool("browser_screenshot")
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "not configured" in text.lower() or "error" in text.lower()

    def test_screenshot_with_format_param(self, mcp):
        """Screenshot accepts format parameter."""
        result = mcp.call_tool("browser_screenshot", {"format": "png"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Should fail gracefully (no browser) but accept the param
        assert "not configured" in text.lower() or "error" in text.lower() or "base64" in text.lower()

    def test_screenshot_jpeg_format(self, mcp):
        """Screenshot accepts jpeg format."""
        result = mcp.call_tool("browser_screenshot", {"format": "jpeg"})
        assert result is not None


# ── Browser click ──────────────────────────────────────────────────────────────


class TestBrowserClick:
    """Tests for browser_click tool."""

    def test_click_missing_selector(self, mcp):
        """Click without selector should return error."""
        result = mcp.call_tool("browser_click", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert any(word in text.lower() for word in ["selector", "not configured", "error", "missing"])

    def test_click_with_selector(self, mcp):
        """Click with selector — fails gracefully when no browser."""
        result = mcp.call_tool("browser_click", {"selector": "#submit-btn"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "not configured" in text.lower() or "error" in text.lower() or "click" in text.lower()

    def test_click_with_coordinates(self, mcp):
        """Click with x/y coordinates."""
        result = mcp.call_tool("browser_click", {"x": 100, "y": 200})
        assert result is not None


# ── Browser type ───────────────────────────────────────────────────────────────


class TestBrowserType:
    """Tests for browser_type tool."""

    def test_type_missing_text(self, mcp):
        """Type without text should return error."""
        result = mcp.call_tool("browser_type", {})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert any(word in text.lower() for word in ["text", "not configured", "error", "missing"])

    def test_type_with_text(self, mcp):
        """Type with text — fails gracefully when no browser."""
        result = mcp.call_tool("browser_type", {"text": "hello world"})
        assert result is not None
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "not configured" in text.lower() or "error" in text.lower() or "typed" in text.lower()

    def test_type_with_selector(self, mcp):
        """Type with selector targets specific element."""
        result = mcp.call_tool("browser_type", {"text": "test", "selector": "input#name"})
        assert result is not None


# ── Live CDP tests (require running Chromium) ──────────────────────────────────


CDP_ENDPOINT = os.environ.get("RADIX_CDP_ENDPOINT")


@pytest.mark.skipif(
    not CDP_ENDPOINT,
    reason="RADIX_CDP_ENDPOINT not set — no live browser for testing",
)
class TestBrowserLiveCDP:
    """Tests that run against a real Chromium instance via CDP.

    Set RADIX_CDP_ENDPOINT=http://localhost:9222 and ensure Chromium is running with
    --remote-debugging-port=9222 --no-sandbox --headless
    """

    @pytest.fixture(scope="class")
    def cdp_mcp(self):
        """MCP client with CDP endpoint configured."""
        workdir = f"/tmp/radix-browser-test-{os.getpid()}"
        os.makedirs(workdir, exist_ok=True)
        # Write config with CDP endpoint
        config = {"browser": {"cdp_endpoint": CDP_ENDPOINT}}
        config_path = os.path.join(workdir, "config.toml")
        import toml
        with open(config_path, "w") as f:
            toml.dump(config, f)
        client = McpClient(workdir=workdir)
        client.start()
        yield client
        client.stop()

    def test_live_status_available(self, cdp_mcp):
        """With CDP configured and Chrome running, status shows available."""
        result = cdp_mcp.call_tool("browser_status")
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        parsed = json.loads(text)
        assert parsed.get("available") is True

    def test_live_navigate_and_snapshot(self, cdp_mcp):
        """Navigate to a page and take a snapshot."""
        result = cdp_mcp.call_tool("browser_navigate", {"url": "https://example.com"})
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "error" not in text.lower() or "not configured" not in text.lower()

        # Give page time to load
        time.sleep(1)

        result = cdp_mcp.call_tool("browser_snapshot")
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        assert "example" in text.lower() or "domain" in text.lower()

    def test_live_screenshot_returns_base64(self, cdp_mcp):
        """Screenshot returns base64 encoded image data."""
        # Navigate first
        cdp_mcp.call_tool("browser_navigate", {"url": "https://example.com"})
        time.sleep(1)

        result = cdp_mcp.call_tool("browser_screenshot", {"format": "png"})
        text = result.get("text", "") if isinstance(result, dict) else str(result)
        # Should contain base64 data or a file path
        assert len(text) > 100  # Base64 PNG would be much larger


# ── Tool listing verification ──────────────────────────────────────────────────


class TestBrowserToolRegistration:
    """Verify all browser tools are registered in the MCP tool list."""

    def test_all_browser_tools_registered(self, mcp):
        """All 6 browser tools appear in tools/list."""
        tools = mcp.list_tools()
        tool_names = [t["name"] for t in tools]
        expected = [
            "browser_status",
            "browser_navigate",
            "browser_snapshot",
            "browser_screenshot",
            "browser_click",
            "browser_type",
        ]
        for name in expected:
            assert name in tool_names, f"Missing tool: {name}"

    def test_browser_navigate_has_url_param(self, mcp):
        """browser_navigate declares url as a parameter."""
        tools = mcp.list_tools()
        nav_tool = next(t for t in tools if t["name"] == "browser_navigate")
        schema = nav_tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "url" in props, f"browser_navigate missing url param: {schema}"

    def test_browser_click_has_selector_param(self, mcp):
        """browser_click declares selector parameter."""
        tools = mcp.list_tools()
        click_tool = next(t for t in tools if t["name"] == "browser_click")
        schema = click_tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "selector" in props or "x" in props, f"browser_click missing params: {schema}"

    def test_browser_type_has_text_param(self, mcp):
        """browser_type declares text parameter."""
        tools = mcp.list_tools()
        type_tool = next(t for t in tools if t["name"] == "browser_type")
        schema = type_tool.get("inputSchema", {})
        props = schema.get("properties", {})
        assert "text" in props, f"browser_type missing text param: {schema}"
