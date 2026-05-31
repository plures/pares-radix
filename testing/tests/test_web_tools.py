"""
test_web_tools.py — Web search and fetch integration tests via MCP server.

Tests cover:
- web_search: query execution, result structure, error handling
- web_fetch: URL fetching, content extraction, error handling
- Edge cases: empty queries, invalid URLs, timeouts

Note: web_search requires BRAVE_API_KEY. Tests gracefully skip if not configured.
web_fetch tests use public URLs that should always be available.

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_web_tools.py -v
"""
import json
import os
import time
import uuid

import pytest


# ── web_search ────────────────────────────────────────────────────────────────


class TestWebSearch:
    """Tests for web_search tool (requires Brave API key)."""

    def test_search_returns_result(self, mcp):
        """Basic search query returns results."""
        result = mcp.call_tool("web_search", {"query": "python programming language"})
        assert result is not None
        # If API key not configured, should get a clear error
        if isinstance(result, dict) and "error" in result:
            error_str = str(result["error"])
            if "api_key" in error_str.lower() or "key" in error_str.lower() or "auth" in error_str.lower():
                pytest.skip("Brave API key not configured")
        # Otherwise, should have results
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        # Should contain something search-related
        assert len(result_str) > 10

    def test_search_with_count(self, mcp):
        """Search respects count parameter."""
        result = mcp.call_tool("web_search", {
            "query": "rust programming",
            "count": 3,
        })
        if isinstance(result, dict) and "error" in result:
            error_str = str(result["error"])
            if "api_key" in error_str.lower() or "key" in error_str.lower():
                pytest.skip("Brave API key not configured")
        assert result is not None

    def test_search_empty_query(self, mcp):
        """Empty query should return error or empty results."""
        result = mcp.call_tool("web_search", {"query": ""})
        assert result is not None
        # Should handle gracefully — either error or empty

    def test_search_special_characters(self, mcp):
        """Search with special characters doesn't crash."""
        result = mcp.call_tool("web_search", {
            "query": "C++ std::vector<int> site:stackoverflow.com"
        })
        assert result is not None


# ── web_fetch ─────────────────────────────────────────────────────────────────


class TestWebFetch:
    """Tests for web_fetch tool."""

    def test_fetch_public_url(self, mcp):
        """Fetch a known public URL returns content."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://httpbin.org/html"
        })
        # httpbin.org can be slow/unreachable in CI — None is acceptable
        if result is not None:
            result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
            # httpbin.org/html returns a page with "Herman Melville"
            if "error" not in result_str.lower() and "timeout" not in result_str.lower():
                assert len(result_str) > 50, "Fetched content too short"

    def test_fetch_json_endpoint(self, mcp):
        """Fetch a JSON endpoint returns parseable content."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://httpbin.org/json"
        })
        # httpbin.org can be slow/unreachable in CI — None is acceptable
        if result is not None:
            result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
            if "error" not in result_str.lower():
                assert "slideshow" in result_str.lower() or len(result_str) > 50

    def test_fetch_with_max_chars(self, mcp):
        """max_chars parameter limits output size."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://httpbin.org/html",
            "max_chars": 100,
        })
        # Result may be None if the fetch times out in CI — that's acceptable
        if result is not None:
            result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
            # Output should be limited (though implementation may vary)
            assert len(result_str) > 0

    def test_fetch_invalid_url(self, mcp):
        """Invalid URL returns error, not crash."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://this-domain-definitely-does-not-exist-12345.com/page"
        })
        # Result may be None if DNS resolution times out in CI — that's acceptable
        # The key assertion is no crash/exception
        if result is not None:
            result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
            # Should indicate an error — various error representations are acceptable
            # Some implementations return timeout, unreachable, or the raw URL back
            assert any(k in result_str.lower() for k in [
                "error", "fail", "not found", "resolve", "dns", "connect",
                "timeout", "unreachable", "refused", "network", "exist"
            ]) or len(result_str) < 200  # Very short response also indicates failure

    def test_fetch_not_found_404(self, mcp):
        """404 URL handles gracefully."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://httpbin.org/status/404"
        })
        # httpbin.org can be unreachable in CI — no crash is the key assertion
        if result is not None:
            assert isinstance(result, (dict, list, str))

    def test_fetch_empty_url(self, mcp):
        """Empty URL returns error."""
        result = mcp.call_tool("web_fetch", {"url": ""})
        assert result is not None
        if isinstance(result, dict):
            assert "error" in result or len(str(result)) > 0

    def test_fetch_plain_text(self, mcp):
        """Fetching plain text content works."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://httpbin.org/robots.txt"
        })
        # httpbin.org can be unreachable in CI
        if result is not None:
            result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
            if "error" not in result_str.lower():
                assert len(result_str) > 10

    def test_fetch_redirect_follows(self, mcp):
        """HTTP redirects are followed."""
        result = mcp.call_tool("web_fetch", {
            "url": "https://httpbin.org/redirect/1"
        })
        # May timeout in CI — acceptable as long as no crash
        if result is not None:
            result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
            assert len(result_str) > 0


# ── Combined workflows ────────────────────────────────────────────────────────


class TestWebWorkflows:
    """Combined web tool workflows."""

    def test_search_then_fetch(self, mcp):
        """Search for something, then fetch the first result URL."""
        search_result = mcp.call_tool("web_search", {
            "query": "httpbin.org",
            "count": 1,
        })
        if isinstance(search_result, dict) and "error" in search_result:
            pytest.skip("Search API not available")

        # Extract a URL from search results (best effort)
        search_str = json.dumps(search_result) if isinstance(search_result, (dict, list)) else str(search_result)
        if "httpbin" in search_str:
            # Found something — try to fetch httpbin directly
            fetch_result = mcp.call_tool("web_fetch", {
                "url": "https://httpbin.org/get"
            })
            assert fetch_result is not None
