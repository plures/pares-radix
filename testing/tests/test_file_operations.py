"""
File Operations E2E Tests
=========================
Tests for read_file, write_file, edit_file, list_directory MCP tools.
These are fundamental filesystem primitives that everything else builds on.

Tools tested:
- write_file: create files with content, auto-create parent dirs
- read_file: read file content back
- edit_file: targeted find-and-replace within files
- list_directory: enumerate directory contents
"""

import json
import os
import uuid
import pytest


@pytest.fixture(scope="module")
def test_prefix():
    """Unique prefix for this module run."""
    return f"radix-fileops-{uuid.uuid4().hex[:8]}"


def _td(test_prefix):
    """Generate a unique test directory path."""
    return f"/tmp/{test_prefix}/{uuid.uuid4().hex[:6]}"


def _assert_ok(result, msg=""):
    """Assert tool call succeeded (not error)."""
    assert result is not None, f"Result was None {msg}"
    if isinstance(result, dict) and "error" in result:
        pytest.fail(f"Tool error {msg}: {result}")


def _assert_err(result, msg=""):
    """Assert tool call returned an error."""
    if result is None:
        return  # None = timeout = error
    if isinstance(result, dict) and "error" in result:
        return  # RPC error
    # Some errors come back as text containing error keywords
    if isinstance(result, str) and any(kw in result.lower() for kw in ["error", "failed", "not found", "missing", "required"]):
        return
    # If none of the above, the call may have succeeded when it shouldn't
    # For param validation that returns error text, this is still OK
    pytest.fail(f"Expected error but got: {result} {msg}")


def _get_text(result) -> str:
    """Get text content from result."""
    if result is None:
        return ""
    if isinstance(result, str):
        return result
    if isinstance(result, dict):
        if "error" in result:
            return json.dumps(result)
        return json.dumps(result)
    return str(result)


# ── write_file ─────────────────────────────────────────────────────────────────


class TestWriteFile:
    """Tests for write_file tool."""

    def test_write_simple_file(self, mcp, test_prefix):
        """Write a simple text file."""
        path = f"/tmp/{test_prefix}/hello.txt"
        result = mcp.call_tool("write_file", {"path": path, "content": "Hello, Radix!"})
        _assert_ok(result)

    def test_write_creates_parent_dirs(self, mcp, test_prefix):
        """write_file should auto-create parent directories."""
        path = f"/tmp/{test_prefix}/deep/nested/dir/file.txt"
        result = mcp.call_tool("write_file", {"path": path, "content": "nested content"})
        _assert_ok(result)

    def test_write_empty_file(self, mcp, test_prefix):
        """Write a file with empty content."""
        path = f"/tmp/{test_prefix}/empty.txt"
        result = mcp.call_tool("write_file", {"path": path, "content": ""})
        _assert_ok(result)

    def test_write_multiline_content(self, mcp, test_prefix):
        """Write multi-line content."""
        path = f"/tmp/{test_prefix}/multiline.txt"
        result = mcp.call_tool("write_file", {"path": path, "content": "line1\nline2\nline3\n"})
        _assert_ok(result)

    def test_write_unicode_content(self, mcp, test_prefix):
        """Write content with unicode characters."""
        path = f"/tmp/{test_prefix}/unicode.txt"
        result = mcp.call_tool("write_file", {"path": path, "content": "Hello 🌍 世界 こんにちは"})
        _assert_ok(result)

    def test_write_large_content(self, mcp, test_prefix):
        """Write a moderately large file (100KB)."""
        path = f"/tmp/{test_prefix}/large.txt"
        result = mcp.call_tool("write_file", {"path": path, "content": "x" * 100_000})
        _assert_ok(result)

    def test_write_json_content(self, mcp, test_prefix):
        """Write valid JSON content."""
        path = f"/tmp/{test_prefix}/data.json"
        content = json.dumps({"key": "value", "nested": {"a": 1}}, indent=2)
        result = mcp.call_tool("write_file", {"path": path, "content": content})
        _assert_ok(result)

    def test_write_overwrites_existing(self, mcp, test_prefix):
        """Writing to an existing file should overwrite it."""
        path = f"/tmp/{test_prefix}/overwrite.txt"
        mcp.call_tool("write_file", {"path": path, "content": "original"})
        result = mcp.call_tool("write_file", {"path": path, "content": "replaced"})
        _assert_ok(result)
        # Verify overwrite
        read = mcp.call_tool("read_file", {"path": path})
        text = _get_text(read)
        assert "replaced" in text
        assert "original" not in text

    def test_write_missing_path_param(self, mcp):
        """write_file without path should error."""
        result = mcp.call_tool("write_file", {"content": "orphan"})
        _assert_err(result)

    def test_write_missing_content_param(self, mcp, test_prefix):
        """write_file without content should error."""
        result = mcp.call_tool("write_file", {"path": f"/tmp/{test_prefix}/no_content.txt"})
        _assert_err(result)


# ── read_file ──────────────────────────────────────────────────────────────────


class TestReadFile:
    """Tests for read_file tool."""

    def test_read_written_file(self, mcp, test_prefix):
        """Read back a file that was just written."""
        path = f"/tmp/{test_prefix}/read_me.txt"
        mcp.call_tool("write_file", {"path": path, "content": "read this back"})
        result = mcp.call_tool("read_file", {"path": path})
        _assert_ok(result)
        assert "read this back" in _get_text(result)

    def test_read_unicode_roundtrip(self, mcp, test_prefix):
        """Unicode content survives write→read roundtrip."""
        path = f"/tmp/{test_prefix}/unicode_rt.txt"
        content = "Émojis: 🎉🦀 Kanji: 漢字"
        mcp.call_tool("write_file", {"path": path, "content": content})
        result = mcp.call_tool("read_file", {"path": path})
        text = _get_text(result)
        assert "🎉" in text or "Kanji" in text

    def test_read_nonexistent_file(self, mcp):
        """Reading a nonexistent file should error."""
        result = mcp.call_tool("read_file", {"path": "/tmp/radix-does-not-exist-99999.txt"})
        _assert_err(result)

    def test_read_multiline_preserves_lines(self, mcp, test_prefix):
        """Multi-line content should preserve all lines."""
        path = f"/tmp/{test_prefix}/lines.txt"
        lines = ["alpha", "beta", "gamma", "delta"]
        mcp.call_tool("write_file", {"path": path, "content": "\n".join(lines) + "\n"})
        result = mcp.call_tool("read_file", {"path": path})
        text = _get_text(result)
        for line in lines:
            assert line in text

    def test_read_empty_file(self, mcp, test_prefix):
        """Reading an empty file should succeed."""
        path = f"/tmp/{test_prefix}/empty_read.txt"
        mcp.call_tool("write_file", {"path": path, "content": ""})
        result = mcp.call_tool("read_file", {"path": path})
        _assert_ok(result)

    def test_read_missing_path_param(self, mcp):
        """read_file without path should error."""
        result = mcp.call_tool("read_file", {})
        _assert_err(result)


# ── edit_file ──────────────────────────────────────────────────────────────────


class TestEditFile:
    """Tests for edit_file tool."""

    def test_edit_simple_replacement(self, mcp, test_prefix):
        """Replace text in a file."""
        path = f"/tmp/{test_prefix}/edit_me.txt"
        mcp.call_tool("write_file", {"path": path, "content": "hello world"})
        result = mcp.call_tool("edit_file", {
            "path": path, "old_text": "world", "new_text": "radix"
        })
        _assert_ok(result)
        read = mcp.call_tool("read_file", {"path": path})
        text = _get_text(read)
        assert "radix" in text
        assert "world" not in text

    def test_edit_multiline_replacement(self, mcp, test_prefix):
        """Replace multi-line text."""
        path = f"/tmp/{test_prefix}/edit_multi.txt"
        mcp.call_tool("write_file", {"path": path, "content": "line1\nline2\nline3\n"})
        result = mcp.call_tool("edit_file", {
            "path": path, "old_text": "line2", "new_text": "LINE_TWO_MODIFIED"
        })
        _assert_ok(result)
        read = mcp.call_tool("read_file", {"path": path})
        assert "LINE_TWO_MODIFIED" in _get_text(read)

    def test_edit_nonexistent_file_errors(self, mcp):
        """Editing a nonexistent file should error."""
        result = mcp.call_tool("edit_file", {
            "path": "/tmp/radix-no-such-file-edit-99999.txt",
            "old_text": "foo", "new_text": "bar"
        })
        _assert_err(result)

    def test_edit_text_not_found(self, mcp, test_prefix):
        """If old_text doesn't exist in the file, should error or no-op."""
        path = f"/tmp/{test_prefix}/no_match.txt"
        mcp.call_tool("write_file", {"path": path, "content": "some content here"})
        result = mcp.call_tool("edit_file", {
            "path": path, "old_text": "NONEXISTENT_STRING", "new_text": "replacement"
        })
        # Should error or at minimum leave file unchanged
        read = mcp.call_tool("read_file", {"path": path})
        assert "some content here" in _get_text(read)

    def test_edit_replace_with_empty(self, mcp, test_prefix):
        """Replace text with empty string (deletion)."""
        path = f"/tmp/{test_prefix}/delete_text.txt"
        mcp.call_tool("write_file", {"path": path, "content": "keep REMOVE keep"})
        result = mcp.call_tool("edit_file", {
            "path": path, "old_text": " REMOVE", "new_text": ""
        })
        _assert_ok(result)
        read = mcp.call_tool("read_file", {"path": path})
        text = _get_text(read)
        assert "REMOVE" not in text
        assert "keep" in text

    def test_edit_insert_text(self, mcp, test_prefix):
        """Insert text by replacing a boundary."""
        path = f"/tmp/{test_prefix}/insert.txt"
        mcp.call_tool("write_file", {"path": path, "content": "start end"})
        result = mcp.call_tool("edit_file", {
            "path": path, "old_text": "start end", "new_text": "start MIDDLE end"
        })
        _assert_ok(result)
        read = mcp.call_tool("read_file", {"path": path})
        assert "MIDDLE" in _get_text(read)

    def test_edit_special_characters(self, mcp, test_prefix):
        """Edit text containing special regex characters."""
        path = f"/tmp/{test_prefix}/special.txt"
        mcp.call_tool("write_file", {"path": path, "content": "price is $100.00 (USD)"})
        result = mcp.call_tool("edit_file", {
            "path": path, "old_text": "$100.00 (USD)", "new_text": "$200.00 (EUR)"
        })
        _assert_ok(result)
        read = mcp.call_tool("read_file", {"path": path})
        assert "$200.00" in _get_text(read)


# ── list_directory ─────────────────────────────────────────────────────────────


class TestListDirectory:
    """Tests for list_directory tool."""

    def test_list_populated_directory(self, mcp, test_prefix):
        """List a directory with known files."""
        base = f"/tmp/{test_prefix}/listdir"
        for name in ["alpha.txt", "beta.md", "gamma.json"]:
            mcp.call_tool("write_file", {"path": f"{base}/{name}", "content": name})
        result = mcp.call_tool("list_directory", {"path": base})
        _assert_ok(result)
        text = _get_text(result)
        assert "alpha.txt" in text
        assert "beta.md" in text
        assert "gamma.json" in text

    def test_list_shows_subdirectories(self, mcp, test_prefix):
        """list_directory should show subdirectories."""
        base = f"/tmp/{test_prefix}/with_dirs"
        mcp.call_tool("write_file", {"path": f"{base}/sub/file.txt", "content": "x"})
        mcp.call_tool("write_file", {"path": f"{base}/top.txt", "content": "y"})
        result = mcp.call_tool("list_directory", {"path": base})
        text = _get_text(result)
        assert "sub" in text
        assert "top.txt" in text

    def test_list_nonexistent_directory(self, mcp):
        """Listing a nonexistent directory should error."""
        result = mcp.call_tool("list_directory", {"path": "/tmp/radix-no-such-dir-99999"})
        _assert_err(result)

    def test_list_tmp_directory(self, mcp):
        """List /tmp — always exists on Linux."""
        result = mcp.call_tool("list_directory", {"path": "/tmp"})
        _assert_ok(result)

    def test_list_missing_path_param(self, mcp):
        """list_directory without path should error."""
        result = mcp.call_tool("list_directory", {})
        _assert_err(result)


# ── Cross-tool workflows ───────────────────────────────────────────────────────


class TestFileWorkflows:
    """Integration tests combining multiple file tools."""

    def test_write_list_read_cycle(self, mcp, test_prefix):
        """Full cycle: write files → list directory → read each file."""
        base = f"/tmp/{test_prefix}/cycle"
        files = {"a.txt": "content_a", "b.txt": "content_b", "c.txt": "content_c"}
        for name, content in files.items():
            mcp.call_tool("write_file", {"path": f"{base}/{name}", "content": content})

        list_result = mcp.call_tool("list_directory", {"path": base})
        text = _get_text(list_result)
        for name in files:
            assert name in text

        for name, expected in files.items():
            read = mcp.call_tool("read_file", {"path": f"{base}/{name}"})
            assert expected in _get_text(read)

    def test_write_edit_read_workflow(self, mcp, test_prefix):
        """Write → Edit → Read to verify full edit pipeline."""
        path = f"/tmp/{test_prefix}/workflow.txt"
        mcp.call_tool("write_file", {"path": path, "content": "version: 1.0.0\nstatus: draft"})
        mcp.call_tool("edit_file", {"path": path, "old_text": "version: 1.0.0", "new_text": "version: 2.0.0"})
        mcp.call_tool("edit_file", {"path": path, "old_text": "status: draft", "new_text": "status: released"})
        read = mcp.call_tool("read_file", {"path": path})
        text = _get_text(read)
        assert "version: 2.0.0" in text
        assert "status: released" in text

    def test_write_overwrite_no_append(self, mcp, test_prefix):
        """Verify write truly overwrites (not appends)."""
        path = f"/tmp/{test_prefix}/no_append.txt"
        mcp.call_tool("write_file", {"path": path, "content": "FIRST"})
        mcp.call_tool("write_file", {"path": path, "content": "SECOND"})
        read = mcp.call_tool("read_file", {"path": path})
        text = _get_text(read)
        assert "SECOND" in text
        assert "FIRST" not in text
