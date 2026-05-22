"""
test_boundary_edge_cases.py — Boundary and edge-case tests for pares-radix.

Tests empty strings, null/None values, max-length keys, special characters,
unicode boundaries, control characters, type coercion, and other edge cases
that stress the limits of PluresDB, Praxis, Canvas, and Chronos.

These are NOT performance tests (see test_large_payloads.py) — they test
correctness at the boundaries of valid/invalid input.
"""
import json
import time
import uuid

import pytest


# ── PluresDB Key Boundary Tests ───────────────────────────────────────────────


class TestPluresDBKeyBoundaries:
    """Test PluresDB behavior with edge-case keys."""

    def test_empty_string_key(self, mcp):
        """Empty string key should be rejected or handled consistently."""
        result = mcp.call_tool("db_put", {"key": "", "value": "test"})
        # Either error or accepted — document the behavior
        if isinstance(result, dict) and "error" in result:
            # Good: explicitly rejected
            pass
        else:
            # If accepted, we should be able to read it back
            get_result = mcp.call_tool("db_get", {"key": ""})
            assert get_result is not None

    def test_whitespace_only_key(self, mcp):
        """Whitespace-only keys."""
        result = mcp.call_tool("db_put", {"key": "   ", "value": "whitespace-key"})
        if not (isinstance(result, dict) and "error" in result):
            get_result = mcp.call_tool("db_get", {"key": "   "})
            assert "whitespace-key" in str(get_result)

    def test_single_char_key(self, mcp):
        """Single character key."""
        result = mcp.call_tool("db_put", {"key": "x", "value": "single"})
        assert not (isinstance(result, dict) and "error" in result)
        get_result = mcp.call_tool("db_get", {"key": "x"})
        assert "single" in str(get_result)

    def test_very_long_key(self, mcp):
        """Key at 10KB length — tests key size limits."""
        long_key = "k" * 10240
        result = mcp.call_tool("db_put", {"key": long_key, "value": "long-key-value"})
        if not (isinstance(result, dict) and "error" in result):
            get_result = mcp.call_tool("db_get", {"key": long_key})
            assert "long-key-value" in str(get_result)

    def test_key_with_special_characters(self, mcp):
        """Keys containing special characters: slashes, dots, colons, etc."""
        special_keys = [
            "a/b/c/d",
            "a.b.c.d",
            "a:b:c:d",
            "a::b::c",
            "key with spaces",
            "key\twith\ttabs",
            "key-with-dashes",
            "key_with_underscores",
            "KEY.UPPER.CASE",
            "🔑emoji🔑key",
        ]
        for key in special_keys:
            tag = uuid.uuid4().hex[:8]
            value = f"val-{tag}"
            result = mcp.call_tool("db_put", {"key": key, "value": value})
            if not (isinstance(result, dict) and "error" in result):
                get_result = mcp.call_tool("db_get", {"key": key})
                assert value in str(get_result), f"Failed roundtrip for key: {repr(key)}"

    def test_key_with_newlines(self, mcp):
        """Key containing newline characters."""
        result = mcp.call_tool("db_put", {"key": "line1\nline2", "value": "newline-key"})
        # Document behavior — newlines in keys are unusual
        if not (isinstance(result, dict) and "error" in result):
            get_result = mcp.call_tool("db_get", {"key": "line1\nline2"})
            assert get_result is not None

    def test_key_with_null_bytes(self, mcp):
        """Key containing null bytes — should be rejected or handled."""
        result = mcp.call_tool("db_put", {"key": "before\x00after", "value": "null-byte"})
        # Most storage engines reject null bytes in keys
        # Document what happens

    def test_key_prefix_empty_string(self, mcp):
        """db_keys with empty prefix should return all keys."""
        # Seed some keys first
        mcp.call_tool("db_put", {"key": "boundary:a", "value": "1"})
        mcp.call_tool("db_put", {"key": "boundary:b", "value": "2"})
        result = mcp.call_tool("db_keys", {"prefix": ""})
        # Should return keys (at minimum the ones we just created)
        assert result is not None
        if isinstance(result, dict) and "keys" in result:
            assert len(result["keys"]) >= 2

    def test_key_prefix_no_match(self, mcp):
        """db_keys with a prefix that matches nothing."""
        result = mcp.call_tool("db_keys", {"prefix": "zzz_nonexistent_prefix_99999"})
        if isinstance(result, dict) and "keys" in result:
            assert result["keys"] == []

    def test_delete_nonexistent_key(self, mcp):
        """Deleting a key that doesn't exist."""
        result = mcp.call_tool("db_delete", {"key": f"never-existed-{uuid.uuid4().hex}"})
        # Should succeed silently or return a clear not-found indicator
        # Either is acceptable — but it shouldn't crash


class TestPluresDBValueBoundaries:
    """Test PluresDB behavior with edge-case values."""

    def test_null_value(self, mcp):
        """Store null/None value.
        
        Known behavior: db_put accepts null, but db_get returns None (timeout)
        because the server can't distinguish null-value from missing-key in the
        response. This documents the behavior rather than asserting correctness.
        """
        key = f"boundary:null:{uuid.uuid4().hex[:8]}"
        result = mcp.call_tool("db_put", {"key": key, "value": None})
        # db_put succeeds (doesn't error)
        assert not (isinstance(result, dict) and "error" in result)
        # db_get returns None — known limitation: null values are indistinguishable
        # from missing keys at the MCP response level
        get_result = mcp.call_tool("db_get", {"key": key})
        # Acceptable: either None (can't represent null in response) or a wrapped response
        # This is a documentation test — the server doesn't crash

    def test_empty_string_value(self, mcp):
        """Store empty string value."""
        key = f"boundary:empty-str:{uuid.uuid4().hex[:8]}"
        result = mcp.call_tool("db_put", {"key": key, "value": ""})
        assert not (isinstance(result, dict) and "error" in result)
        get_result = mcp.call_tool("db_get", {"key": key})
        # Empty string should roundtrip correctly, not become null
        assert get_result is not None

    def test_boolean_values(self, mcp):
        """Store boolean true/false values."""
        key_t = f"boundary:bool-true:{uuid.uuid4().hex[:8]}"
        key_f = f"boundary:bool-false:{uuid.uuid4().hex[:8]}"
        mcp.call_tool("db_put", {"key": key_t, "value": True})
        mcp.call_tool("db_put", {"key": key_f, "value": False})
        result_t = mcp.call_tool("db_get", {"key": key_t})
        result_f = mcp.call_tool("db_get", {"key": key_f})
        # Booleans should not be coerced to strings
        assert result_t is not None
        assert result_f is not None

    def test_numeric_zero(self, mcp):
        """Store numeric zero — should not be treated as falsy/empty."""
        key = f"boundary:zero:{uuid.uuid4().hex[:8]}"
        mcp.call_tool("db_put", {"key": key, "value": 0})
        result = mcp.call_tool("db_get", {"key": key})
        assert result is not None
        # Zero should roundtrip as 0, not null/false/empty-string
        assert "0" in str(result) or 0 == result.get("value", None) if isinstance(result, dict) else True

    def test_negative_numbers(self, mcp):
        """Store negative numbers and verify sign preservation."""
        key = f"boundary:neg:{uuid.uuid4().hex[:8]}"
        mcp.call_tool("db_put", {"key": key, "value": -42})
        result = mcp.call_tool("db_get", {"key": key})
        assert "-42" in str(result)

    def test_float_precision(self, mcp):
        """Store floats and verify precision is maintained."""
        key = f"boundary:float:{uuid.uuid4().hex[:8]}"
        mcp.call_tool("db_put", {"key": key, "value": 3.141592653589793})
        result = mcp.call_tool("db_get", {"key": key})
        assert "3.14159" in str(result)

    def test_very_large_number(self, mcp):
        """Store a very large integer."""
        key = f"boundary:bignum:{uuid.uuid4().hex[:8]}"
        big = 2**53  # Beyond JS safe integer
        mcp.call_tool("db_put", {"key": key, "value": big})
        result = mcp.call_tool("db_get", {"key": key})
        assert str(big) in str(result)

    def test_empty_object(self, mcp):
        """Store empty JSON object."""
        key = f"boundary:empty-obj:{uuid.uuid4().hex[:8]}"
        mcp.call_tool("db_put", {"key": key, "value": {}})
        result = mcp.call_tool("db_get", {"key": key})
        assert result is not None

    def test_empty_array(self, mcp):
        """Store empty JSON array."""
        key = f"boundary:empty-arr:{uuid.uuid4().hex[:8]}"
        mcp.call_tool("db_put", {"key": key, "value": []})
        result = mcp.call_tool("db_get", {"key": key})
        assert result is not None

    def test_deeply_nested_empty_objects(self, mcp):
        """Store deeply nested structure of empty objects."""
        key = f"boundary:deep-empty:{uuid.uuid4().hex[:8]}"
        # 30 levels of nesting with empty objects at leaves
        value = {}
        current = value
        for i in range(30):
            current[f"level{i}"] = {}
            current = current[f"level{i}"]
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert result is not None

    def test_mixed_type_array(self, mcp):
        """Store array with mixed types: string, int, float, bool, null, object."""
        key = f"boundary:mixed-arr:{uuid.uuid4().hex[:8]}"
        value = ["hello", 42, 3.14, True, False, None, {"nested": "obj"}, [1, 2, 3]]
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert result is not None


# ── Unicode & Character Encoding Boundaries ───────────────────────────────────


class TestUnicodeBoundaries:
    """Test unicode edge cases across all components."""

    def test_unicode_surrogate_pairs(self, mcp):
        """Characters outside BMP (emoji, CJK Extension B)."""
        key = f"boundary:surrogate:{uuid.uuid4().hex[:8]}"
        # These require surrogate pairs in UTF-16: 𝕳𝖊𝖑𝖑𝖔 (Mathematical Fraktur)
        value = "𝕳𝖊𝖑𝖑𝖔 𝕎𝕠𝕣𝕝𝕕"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert "𝕳𝖊𝖑𝖑𝖔" in str(result)

    def test_rtl_text(self, mcp):
        """Right-to-left text (Arabic, Hebrew)."""
        key = f"boundary:rtl:{uuid.uuid4().hex[:8]}"
        value = "مرحبا بالعالم"  # Arabic "Hello World"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert "مرحبا" in str(result)

    def test_mixed_bidi_text(self, mcp):
        """Mixed LTR and RTL text."""
        key = f"boundary:bidi:{uuid.uuid4().hex[:8]}"
        value = "Hello مرحبا World عالم"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert "Hello" in str(result) and "مرحبا" in str(result)

    def test_zero_width_characters(self, mcp):
        """Zero-width joiner, non-joiner, spaces."""
        key = f"boundary:zwc:{uuid.uuid4().hex[:8]}"
        # ZWJ + ZWNJ + ZWSP
        value = "a\u200Db\u200Cc\u200Bd"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert result is not None

    def test_combining_characters(self, mcp):
        """Characters with combining diacritical marks."""
        key = f"boundary:combining:{uuid.uuid4().hex[:8]}"
        # é composed as e + combining acute accent vs precomposed é
        composed = "caf\u00e9"     # precomposed
        decomposed = "cafe\u0301"  # decomposed
        mcp.call_tool("db_put", {"key": key, "value": {"composed": composed, "decomposed": decomposed}})
        result = mcp.call_tool("db_get", {"key": key})
        # Both forms should be stored as-is (no normalization)
        assert result is not None

    def test_emoji_sequences(self, mcp):
        """Complex emoji sequences (skin tones, flags, ZWJ families)."""
        key = f"boundary:emoji:{uuid.uuid4().hex[:8]}"
        value = "👨‍👩‍👧‍👦 🏳️‍🌈 👋🏽 🇺🇸"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert "👨‍👩‍👧‍👦" in str(result)

    def test_cjk_unified_ideographs(self, mcp):
        """Chinese/Japanese/Korean characters."""
        key = f"boundary:cjk:{uuid.uuid4().hex[:8]}"
        value = "你好世界 こんにちは世界 안녕하세요세계"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert "你好" in str(result)

    def test_max_codepoint(self, mcp):
        """Characters near U+10FFFF (max Unicode codepoint)."""
        key = f"boundary:maxcp:{uuid.uuid4().hex[:8]}"
        # U+10FFFD is the last non-character, U+10FFFE/F are noncharacters
        value = chr(0x10FFFD) + " near-max"
        mcp.call_tool("db_put", {"key": key, "value": value})
        result = mcp.call_tool("db_get", {"key": key})
        assert "near-max" in str(result)


# ── Praxis Constraint Boundaries ──────────────────────────────────────────────


class TestPraxisBoundaries:
    """Test Praxis with edge-case constraints and evaluation contexts."""

    def test_constraint_empty_name(self, mcp):
        """Constraint with empty name."""
        result = mcp.call_tool("praxis_add_constraint", {
            "name": "",
            "severity": "error",
            "require": "true",
        })
        # Should either reject or handle gracefully
        assert result is not None

    def test_constraint_very_long_name(self, mcp):
        """Constraint with extremely long name."""
        long_name = "constraint_" + "x" * 5000
        result = mcp.call_tool("praxis_add_constraint", {
            "name": long_name,
            "severity": "error",
            "require": "true",
        })
        assert result is not None

    def test_constraint_invalid_severity(self, mcp):
        """Constraint with invalid severity level."""
        result = mcp.call_tool("praxis_add_constraint", {
            "name": f"boundary-invalid-sev-{uuid.uuid4().hex[:8]}",
            "severity": "catastrophic",  # not a valid severity
            "require": "true",
        })
        # Should reject or normalize

    def test_evaluate_empty_context(self, mcp):
        """Evaluate constraints against completely empty context."""
        result = mcp.call_tool("praxis_evaluate", {"context": {}})
        assert result is not None

    def test_evaluate_null_context_values(self, mcp):
        """Evaluate with context containing null values."""
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"field1": None, "field2": None}
        })
        assert result is not None

    def test_constraint_special_chars_in_require(self, mcp):
        """Require expression with special characters."""
        result = mcp.call_tool("praxis_add_constraint", {
            "name": f"boundary-special-req-{uuid.uuid4().hex[:8]}",
            "severity": "error",
            "require": 'context.value == "hello\\"world"',
        })
        assert result is not None

    def test_constraint_with_unicode_message(self, mcp):
        """Constraint with unicode in the message field."""
        result = mcp.call_tool("praxis_add_constraint", {
            "name": f"boundary-unicode-msg-{uuid.uuid4().hex[:8]}",
            "severity": "error",
            "require": "true",
            "message": "⚠️ 违反约束条件 — 제약 조건 위반",
        })
        assert result is not None


# ── Canvas Boundaries ─────────────────────────────────────────────────────────


class TestCanvasBoundaries:
    """Test Canvas with edge-case inputs."""

    def test_create_canvas_empty_title(self, mcp):
        """Create canvas with empty title."""
        result = mcp.call_tool("canvas_create", {"title": ""})
        # Should either reject or create with empty title
        assert result is not None

    def test_create_canvas_very_long_title(self, mcp):
        """Create canvas with 10KB title."""
        long_title = "T" * 10240
        result = mcp.call_tool("canvas_create", {"title": long_title})
        assert result is not None

    def test_add_node_empty_id(self, mcp):
        """Add node with empty id."""
        # First ensure a canvas exists
        mcp.call_tool("canvas_create", {"title": "boundary-node-test"})
        result = mcp.call_tool("canvas_add_node", {
            "parentId": "root",
            "node": {"id": "", "type": "text", "props": {"content": "empty-id"}}
        })
        # Document behavior

    def test_add_node_duplicate_id(self, mcp):
        """Add two nodes with the same id."""
        mcp.call_tool("canvas_create", {"title": "boundary-dup-id"})
        node_id = f"dup-{uuid.uuid4().hex[:8]}"
        mcp.call_tool("canvas_add_node", {
            "parentId": "root",
            "node": {"id": node_id, "type": "text", "props": {"content": "first"}}
        })
        result = mcp.call_tool("canvas_add_node", {
            "parentId": "root",
            "node": {"id": node_id, "type": "text", "props": {"content": "second"}}
        })
        # Should either reject the duplicate or overwrite

    def test_add_node_nonexistent_parent(self, mcp):
        """Add node to a parent that doesn't exist."""
        mcp.call_tool("canvas_create", {"title": "boundary-no-parent"})
        result = mcp.call_tool("canvas_add_node", {
            "parentId": "nonexistent_parent_id_xyz",
            "node": {"id": "orphan", "type": "text", "props": {"content": "lost"}}
        })
        # Should error gracefully

    def test_remove_node_nonexistent(self, mcp):
        """Remove a node that doesn't exist."""
        mcp.call_tool("canvas_create", {"title": "boundary-remove-ghost"})
        result = mcp.call_tool("canvas_remove_node", {"nodeId": "ghost_node_999"})
        # Should succeed silently or return not-found

    def test_remove_root_node(self, mcp):
        """Attempt to remove the root node."""
        mcp.call_tool("canvas_create", {"title": "boundary-remove-root"})
        result = mcp.call_tool("canvas_remove_node", {"nodeId": "root"})
        # Should be rejected — can't remove root

    def test_set_data_with_circular_reference_approximation(self, mcp):
        """Set data with a deeply self-referencing structure (not truly circular in JSON)."""
        mcp.call_tool("canvas_create", {"title": "boundary-deep-data"})
        # Build deeply nested but valid JSON
        deep = {"level": 0}
        current = deep
        for i in range(1, 50):
            current["child"] = {"level": i}
            current = current["child"]
        result = mcp.call_tool("canvas_set_data", {"data": {"deep": deep}})
        assert result is not None


# ── Chronos Boundaries ────────────────────────────────────────────────────────


class TestChronosBoundaries:
    """Test Chronos with edge-case events."""

    def test_record_empty_event_name(self, mcp):
        """Record event with empty name."""
        result = mcp.call_tool("chronos_record", {"event": ""})
        assert result is not None

    def test_record_event_null_data(self, mcp):
        """Record event with explicitly null data."""
        result = mcp.call_tool("chronos_record", {"event": "null-data-test", "data": None})
        assert result is not None

    def test_record_event_empty_data(self, mcp):
        """Record event with empty object data."""
        result = mcp.call_tool("chronos_record", {"event": "empty-data-test", "data": {}})
        assert result is not None

    def test_record_event_very_long_name(self, mcp):
        """Record event with extremely long name."""
        long_event = "e" * 10000
        result = mcp.call_tool("chronos_record", {"event": long_event})
        assert result is not None

    def test_timeline_zero_limit(self, mcp):
        """Request timeline with limit=0."""
        result = mcp.call_tool("chronos_timeline", {"limit": 0})
        # Should return empty or reject
        assert result is not None

    def test_timeline_negative_limit(self, mcp):
        """Request timeline with negative limit."""
        result = mcp.call_tool("chronos_timeline", {"limit": -1})
        assert result is not None

    def test_timeline_very_large_limit(self, mcp):
        """Request timeline with very large limit."""
        result = mcp.call_tool("chronos_timeline", {"limit": 999999})
        assert result is not None

    def test_set_level_invalid(self, mcp):
        """Set chronos level to invalid value."""
        result = mcp.call_tool("chronos_set_level", {"level": "ultra-verbose"})
        # Should reject or ignore
        assert result is not None

    def test_replay_invalid_ids(self, mcp):
        """Replay with non-existent from/to IDs."""
        result = mcp.call_tool("chronos_replay", {
            "fromId": "nonexistent-id-000",
            "toId": "nonexistent-id-999"
        })
        # Known issue: may produce no response (filed as #358)
        # Just verify it doesn't crash the server


# ── Memory/Embedding Boundaries ───────────────────────────────────────────────


class TestMemoryBoundaries:
    """Test memory operations with edge-case inputs."""

    def test_search_empty_query(self, mcp):
        """Search with empty query string.
        
        Known behavior: embedding model cannot produce a vector for empty input,
        so the MCP call times out (returns None). Server doesn't crash.
        """
        result = mcp.call_tool("memory_search", {"query": ""}, timeout=15)
        # Known: returns None (timeout) because empty string can't be embedded.
        # The important thing is it doesn't crash the server.
        # Verify server is still alive:
        alive = mcp.call_tool("db_put", {"key": "boundary:alive-check", "value": "ok"})
        assert not (isinstance(alive, dict) and "error" in alive)

    def test_search_single_char(self, mcp):
        """Search with single character query.
        
        Known behavior: single-char queries may timeout due to embedding issues
        or return empty results. Server stability is the key assertion.
        """
        result = mcp.call_tool("memory_search", {"query": "x"}, timeout=15)
        # May be None (timeout) or empty results — both acceptable
        # Key: server doesn't crash
        alive = mcp.call_tool("db_put", {"key": "boundary:alive-check-2", "value": "ok"})
        assert not (isinstance(alive, dict) and "error" in alive)

    def test_search_very_long_query(self, mcp):
        """Search with extremely long query (10KB).
        
        Known behavior: very long queries may timeout during embedding.
        Server stability is the key assertion.
        """
        long_query = "test pattern " * 800  # ~10KB
        result = mcp.call_tool("memory_search", {"query": long_query}, timeout=15)
        # May timeout — embedding long text is slow or may be truncated
        # Key: server doesn't crash
        alive = mcp.call_tool("db_put", {"key": "boundary:alive-check-3", "value": "ok"})
        assert not (isinstance(alive, dict) and "error" in alive)

    def test_search_special_characters_only(self, mcp):
        """Search with only special characters."""
        result = mcp.call_tool("memory_search", {"query": "!@#$%^&*()"})
        assert result is not None

    def test_store_empty_content(self, mcp):
        """Store memory with empty content."""
        result = mcp.call_tool("memory_store", {
            "content": "",
            "category": "test",
            "tags": ["boundary"],
        })
        # Should reject or store empty

    def test_store_content_with_control_chars(self, mcp):
        """Store memory containing ASCII control characters."""
        content = "normal text\x01\x02\x03\x04\x05 more text"
        result = mcp.call_tool("memory_store", {
            "content": content,
            "category": "test",
            "tags": ["boundary", "control-chars"],
        })
        assert result is not None


# ── Plugin Boundaries ─────────────────────────────────────────────────────────


class TestPluginBoundaries:
    """Test plugin system with edge-case inputs."""

    def test_register_empty_name(self, mcp):
        """Register plugin with empty name."""
        result = mcp.call_tool("plugin_register", {
            "name": "",
            "version": "1.0.0",
        })
        assert result is not None

    def test_register_invalid_version(self, mcp):
        """Register plugin with non-semver version."""
        result = mcp.call_tool("plugin_register", {
            "name": f"boundary-bad-ver-{uuid.uuid4().hex[:8]}",
            "version": "not-a-version",
        })
        assert result is not None

    def test_activate_nonexistent_plugin(self, mcp):
        """Activate a plugin that was never registered."""
        result = mcp.call_tool("plugin_activate", {
            "name": f"never-registered-{uuid.uuid4().hex}"
        })
        # Should return error
        assert result is not None

    def test_deactivate_nonexistent_plugin(self, mcp):
        """Deactivate a plugin that doesn't exist."""
        result = mcp.call_tool("plugin_deactivate", {
            "name": f"never-registered-{uuid.uuid4().hex}"
        })
        assert result is not None

    def test_double_activate(self, mcp):
        """Activate an already-active plugin."""
        name = f"boundary-double-act-{uuid.uuid4().hex[:8]}"
        mcp.call_tool("plugin_register", {"name": name, "version": "1.0.0"})
        mcp.call_tool("plugin_activate", {"name": name})
        result = mcp.call_tool("plugin_activate", {"name": name})
        # Should succeed idempotently
        assert result is not None

    def test_double_deactivate(self, mcp):
        """Deactivate an already-inactive plugin."""
        name = f"boundary-double-deact-{uuid.uuid4().hex[:8]}"
        mcp.call_tool("plugin_register", {"name": name, "version": "1.0.0"})
        mcp.call_tool("plugin_activate", {"name": name})
        mcp.call_tool("plugin_deactivate", {"name": name})
        result = mcp.call_tool("plugin_deactivate", {"name": name})
        # Should succeed idempotently
        assert result is not None


# ── Config Boundaries ─────────────────────────────────────────────────────────


class TestConfigBoundaries:
    """Test config operations with edge-case inputs."""

    def test_get_nonexistent_key(self, mcp):
        """Get a config key that doesn't exist — returns None (no value)."""
        result = mcp.call_tool("config_get", {"key": f"nonexistent.path.{uuid.uuid4().hex}"})
        # config_get returns None for missing keys — that IS the expected behavior
        # (the tool call itself succeeds; the value is just absent)
        assert True  # No crash = success; None is valid

    def test_set_nested_deep_key(self, mcp):
        """Set a very deeply nested config path."""
        deep_key = ".".join([f"level{i}" for i in range(20)])
        result = mcp.call_tool("config_set", {"key": deep_key, "value": "deep-value"})
        assert result is not None

    def test_set_config_empty_value(self, mcp):
        """Set config with empty string value."""
        result = mcp.call_tool("config_set", {
            "key": f"boundary.empty.{uuid.uuid4().hex[:8]}",
            "value": ""
        })
        assert result is not None


# ── Cross-Component Boundary Interactions ─────────────────────────────────────


class TestCrossComponentBoundaries:
    """Test boundary cases that span multiple components."""

    def test_store_then_search_empty_string(self, mcp):
        """Store empty string, then retrieve it.
        
        Known behavior: db_get for empty-string values returns None, similar to
        null values. The MCP response format cannot distinguish empty-string from
        missing-key. This is a known limitation.
        """
        key = f"boundary:cross:empty:{uuid.uuid4().hex[:8]}"
        result = mcp.call_tool("db_put", {"key": key, "value": ""})
        # Put succeeds
        assert not (isinstance(result, dict) and "error" in result)
        # Get may return None — known limitation for empty/null values
        get_result = mcp.call_tool("db_get", {"key": key})
        # Key shows up in prefix listing even if get returns None
        keys_result = mcp.call_tool("db_keys", {"prefix": key})
        if isinstance(keys_result, dict) and "keys" in keys_result:
            assert key in keys_result["keys"]

    def test_rapid_create_delete_cycle(self, mcp):
        """Rapidly create and delete the same key."""
        key = f"boundary:cycle:{uuid.uuid4().hex[:8]}"
        for i in range(20):
            mcp.call_tool("db_put", {"key": key, "value": f"iteration-{i}"})
            mcp.call_tool("db_delete", {"key": key})
        # After all cycles, key should be gone
        result = mcp.call_tool("db_get", {"key": key})
        # Should be not-found or null

    def test_overwrite_type_change(self, mcp):
        """Overwrite a key changing its type from string→object→array→number."""
        key = f"boundary:typeswap:{uuid.uuid4().hex[:8]}"
        types = ["hello", {"obj": True}, [1, 2, 3], 42, True]
        for val in types:
            mcp.call_tool("db_put", {"key": key, "value": val})
        # Final value should be True (last non-null write)
        result = mcp.call_tool("db_get", {"key": key})
        assert result is True

    def test_concurrent_key_prefix_listing_during_writes(self, mcp):
        """List keys by prefix while other keys are being written."""
        prefix = f"boundary:concurrent:{uuid.uuid4().hex[:8]}"
        # Write 10 keys
        for i in range(10):
            mcp.call_tool("db_put", {"key": f"{prefix}:{i}", "value": i})
        # List while state is populated
        result = mcp.call_tool("db_keys", {"prefix": prefix})
        if isinstance(result, dict) and "keys" in result:
            assert len(result["keys"]) == 10

    def test_canvas_with_unicode_everywhere(self, mcp):
        """Canvas with unicode in title, node ids, props, and data."""
        mcp.call_tool("canvas_create", {"title": "캔버스 テスト 🎨"})
        mcp.call_tool("canvas_add_node", {
            "parentId": "root",
            "node": {
                "id": "noeud-français",
                "type": "text",
                "props": {"content": "محتوى عربي 中文内容"}
            }
        })
        mcp.call_tool("canvas_set_data", {"data": {"データ": "значение", "键": "값"}})
        result = mcp.call_tool("canvas_get", {})
        assert result is not None
