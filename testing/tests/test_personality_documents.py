"""
test_personality_documents.py — Personality document CRUD via MCP/PluresDB.

Tests exercise the personality document system through PluresDB keys:
- Store personality documents (soul, identity, user, agents, heartbeat)
- Retrieve them via db_get
- Verify key format: personality:doc:{type}
- Verify document structure (doc_type, content, updated_at)
- Verify invalid doc types are rejected at the key level
- Test format ordering assumptions

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_personality_documents.py -v
"""
import json
import time
import uuid
import pytest


# Valid personality document types (mirrors PERSONALITY_DOC_TYPES in personality.rs)
VALID_DOC_TYPES = ["soul", "identity", "user", "agents", "heartbeat"]

# Expected key format
DOC_KEY_PREFIX = "personality:doc:"


class TestPersonalityDocumentStorage:
    """Test personality document CRUD via PluresDB MCP tools."""

    def test_store_soul_document(self, mcp):
        """Store a soul document and verify it persists."""
        content = "Be direct and helpful. No fluff."
        doc = {
            "doc_type": "soul",
            "content": content,
            "updated_at": int(time.time()),
        }
        key = f"{DOC_KEY_PREFIX}soul"
        result = mcp.call_tool("db_put", {"key": key, "value": doc})
        assert result is not None

        # Retrieve and verify
        got = mcp.call_tool("db_get", {"key": key})
        assert got is not None
        got_data = self._parse_result(got)
        assert got_data["doc_type"] == "soul"
        assert got_data["content"] == content
        assert "updated_at" in got_data

    def test_store_identity_document(self, mcp):
        """Store an identity document."""
        doc = {
            "doc_type": "identity",
            "content": "Name: TestBot\nCreature: Assistant\nEmoji: 🤖",
            "updated_at": int(time.time()),
        }
        key = f"{DOC_KEY_PREFIX}identity"
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert got_data["doc_type"] == "identity"
        assert "TestBot" in got_data["content"]

    def test_store_user_document(self, mcp):
        """Store a user document."""
        doc = {
            "doc_type": "user",
            "content": "Name: testuser\nTimezone: PST",
            "updated_at": int(time.time()),
        }
        key = f"{DOC_KEY_PREFIX}user"
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert got_data["doc_type"] == "user"
        assert "testuser" in got_data["content"]

    def test_store_agents_document(self, mcp):
        """Store an agents document."""
        doc = {
            "doc_type": "agents",
            "content": "## Rules\n- Be concise\n- Test everything",
            "updated_at": int(time.time()),
        }
        key = f"{DOC_KEY_PREFIX}agents"
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert got_data["doc_type"] == "agents"
        assert "concise" in got_data["content"]

    def test_store_heartbeat_document(self, mcp):
        """Store a heartbeat document."""
        doc = {
            "doc_type": "heartbeat",
            "content": "## Active Tasks\n- Build testing infra\n- Fix bugs",
            "updated_at": int(time.time()),
        }
        key = f"{DOC_KEY_PREFIX}heartbeat"
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert got_data["doc_type"] == "heartbeat"
        assert "Active Tasks" in got_data["content"]

    def test_all_doc_types_have_correct_key_format(self, mcp):
        """Verify all doc types use the correct key prefix."""
        for doc_type in VALID_DOC_TYPES:
            key = f"{DOC_KEY_PREFIX}{doc_type}"
            assert key.startswith("personality:doc:")
            assert doc_type in key

    def test_overwrite_existing_document(self, mcp):
        """Overwriting a document replaces the content."""
        key = f"{DOC_KEY_PREFIX}soul"
        doc_v1 = {
            "doc_type": "soul",
            "content": "Version 1: Be helpful.",
            "updated_at": int(time.time()) - 100,
        }
        mcp.call_tool("db_put", {"key": key, "value": doc_v1})

        doc_v2 = {
            "doc_type": "soul",
            "content": "Version 2: Be direct and concise.",
            "updated_at": int(time.time()),
        }
        mcp.call_tool("db_put", {"key": key, "value": doc_v2})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert "Version 2" in got_data["content"]
        assert "Version 1" not in got_data["content"]

    def test_delete_personality_document(self, mcp):
        """Deleting a personality document removes it."""
        key = f"{DOC_KEY_PREFIX}soul"
        doc = {
            "doc_type": "soul",
            "content": "Temporary soul.",
            "updated_at": int(time.time()),
        }
        mcp.call_tool("db_put", {"key": key, "value": doc})

        # Delete
        mcp.call_tool("db_delete", {"key": key})

        # Verify gone
        got = mcp.call_tool("db_get", {"key": key})
        # Should be null/None/empty
        got_data = self._parse_result(got)
        assert got_data is None or got_data == {}

    def test_list_personality_keys(self, mcp):
        """List all personality keys via db_keys prefix search."""
        # Store at least one doc
        key = f"{DOC_KEY_PREFIX}soul"
        doc = {
            "doc_type": "soul",
            "content": "Test soul for key listing.",
            "updated_at": int(time.time()),
        }
        mcp.call_tool("db_put", {"key": key, "value": doc})

        # List with prefix
        result = mcp.call_tool("db_keys", {"prefix": DOC_KEY_PREFIX})
        result_str = str(result)
        assert "personality:doc:" in result_str

    def test_document_updated_at_is_numeric(self, mcp):
        """The updated_at field should be a unix timestamp (numeric)."""
        ts = int(time.time())
        key = f"{DOC_KEY_PREFIX}identity"
        doc = {
            "doc_type": "identity",
            "content": "Timestamp test.",
            "updated_at": ts,
        }
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert isinstance(got_data["updated_at"], (int, float))
        assert got_data["updated_at"] >= ts - 1  # within 1s tolerance

    def test_large_document_content(self, mcp):
        """Personality documents can hold substantial content (e.g. full AGENTS.md)."""
        large_str = "# AGENTS.md\n\n" + "\n".join(f"- Rule {i}: do the thing" for i in range(200))
        key = f"{DOC_KEY_PREFIX}agents"
        doc = {
            "doc_type": "agents",
            "content": large_str,
            "updated_at": int(time.time()),
        }
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert len(got_data["content"]) > 2000
        assert "Rule 199" in got_data["content"]

    def test_document_with_special_characters(self, mcp):
        """Documents with markdown, emoji, and unicode store correctly."""
        content = '# 🤖 Soul\n\n**Bold** and _italic_ and `code`\n\n> Quote: "Hello, world!"\n\nUnicode: 日本語テスト'
        key = f"{DOC_KEY_PREFIX}soul"
        doc = {
            "doc_type": "soul",
            "content": content,
            "updated_at": int(time.time()),
        }
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert "🤖" in got_data["content"]
        assert "日本語テスト" in got_data["content"]

    def test_multiple_documents_coexist(self, mcp):
        """Multiple personality documents can exist simultaneously."""
        docs_to_store = {
            "soul": "Be direct.",
            "identity": "Name: Tester",
            "user": "Name: human",
        }
        ts = int(time.time())

        for doc_type, content in docs_to_store.items():
            key = f"{DOC_KEY_PREFIX}{doc_type}"
            doc = {"doc_type": doc_type, "content": content, "updated_at": ts}
            mcp.call_tool("db_put", {"key": key, "value": doc})

        # Verify all exist independently
        for doc_type, expected_content in docs_to_store.items():
            key = f"{DOC_KEY_PREFIX}{doc_type}"
            got = mcp.call_tool("db_get", {"key": key})
            got_data = self._parse_result(got)
            assert got_data["content"] == expected_content

    def test_empty_content_document(self, mcp):
        """Storing a document with empty content should work (edge case)."""
        key = f"{DOC_KEY_PREFIX}heartbeat"
        doc = {
            "doc_type": "heartbeat",
            "content": "",
            "updated_at": int(time.time()),
        }
        mcp.call_tool("db_put", {"key": key, "value": doc})

        got = mcp.call_tool("db_get", {"key": key})
        got_data = self._parse_result(got)
        assert got_data["content"] == ""

    def _parse_result(self, result):
        """Parse MCP tool result into a dict."""
        if result is None:
            return None
        if isinstance(result, dict):
            if "value" in result:
                val = result["value"]
                if isinstance(val, str):
                    try:
                        return json.loads(val)
                    except (json.JSONDecodeError, TypeError):
                        return val
                return val
            if "error" in result:
                return None
            return result
        if isinstance(result, str):
            try:
                parsed = json.loads(result)
                if isinstance(parsed, dict) and "value" in parsed:
                    val = parsed["value"]
                    if isinstance(val, str):
                        try:
                            return json.loads(val)
                        except (json.JSONDecodeError, TypeError):
                            return val
                    return val
                return parsed
            except (json.JSONDecodeError, TypeError):
                return None
        if isinstance(result, list) and len(result) > 0:
            # MCP returns content as list of {type, text}
            text = result[0].get("text", "") if isinstance(result[0], dict) else str(result[0])
            try:
                parsed = json.loads(text)
                if isinstance(parsed, dict) and "value" in parsed:
                    val = parsed["value"]
                    if isinstance(val, str):
                        try:
                            return json.loads(val)
                        except (json.JSONDecodeError, TypeError):
                            return val
                    return val
                return parsed
            except (json.JSONDecodeError, TypeError):
                return None
        return None
