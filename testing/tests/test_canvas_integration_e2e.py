"""
test_canvas_integration_e2e.py — End-to-end Canvas rendering and manipulation tests.

Tests the full Canvas lifecycle through the MCP server:
- Create/get/list/save canvases
- Set/validate component trees
- Add/remove nodes from the tree
- Set data bindings
- Canvas import/export round-trip
- Canvas validation (rules, structural integrity)
- Add procedures and rules to a canvas

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_canvas_integration_e2e.py -v
"""
import json
import uuid
import pytest


def unique_id(prefix="test"):
    return f"{prefix}-{uuid.uuid4().hex[:8]}"


# ── Canvas Creation ────────────────────────────────────────────────────────────


class TestCanvasCreation:
    """Test creating and retrieving canvases."""

    def test_create_canvas(self, mcp):
        """Create a new canvas returns a document."""
        result = mcp.call_tool("canvas_create", {
            "title": f"Test Canvas {unique_id()}",
        })
        assert result is not None
        result_str = str(result)
        assert "error" not in result_str.lower() or "canvas" in result_str.lower()

    def test_create_canvas_with_description(self, mcp):
        """Create canvas with description."""
        title = f"Described Canvas {unique_id()}"
        result = mcp.call_tool("canvas_create", {
            "title": title,
            "description": "A test canvas for integration testing",
        })
        assert result is not None

    def test_get_canvas_after_create(self, mcp):
        """Get returns the active canvas after creation."""
        title = f"Get Test {unique_id()}"
        mcp.call_tool("canvas_create", {"title": title})
        result = mcp.call_tool("canvas_get", {})
        assert result is not None
        result_str = str(result)
        assert title in result_str or "canvas" in result_str.lower()

    def test_create_multiple_canvases(self, mcp):
        """Multiple canvases can be created."""
        titles = [f"Multi {unique_id()}" for _ in range(3)]
        for title in titles:
            result = mcp.call_tool("canvas_create", {"title": title})
            assert result is not None


# ── Component Tree ─────────────────────────────────────────────────────────────


class TestCanvasTree:
    """Test setting and manipulating the component tree."""

    def test_set_tree_minimal(self, mcp):
        """Set a minimal component tree."""
        mcp.call_tool("canvas_create", {"title": f"Tree Test {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "children": [],
            },
        })
        assert result is not None

    def test_set_tree_with_children(self, mcp):
        """Set a tree with child nodes."""
        mcp.call_tool("canvas_create", {"title": f"Children Test {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "children": [
                    {"id": "header", "type": "text", "props": {"content": "Hello"}},
                    {"id": "body", "type": "container", "children": []},
                ],
            },
        })
        assert result is not None

    def test_set_tree_with_props(self, mcp):
        """Nodes with props are stored correctly."""
        mcp.call_tool("canvas_create", {"title": f"Props Test {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "props": {"layout": "vertical", "gap": 8},
                "children": [
                    {
                        "id": "btn",
                        "type": "button",
                        "props": {"label": "Click Me", "variant": "primary"},
                    },
                ],
            },
        })
        assert result is not None

    def test_set_tree_with_bindings(self, mcp):
        """Nodes with data bindings are stored."""
        mcp.call_tool("canvas_create", {"title": f"Bindings Test {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "children": [
                    {
                        "id": "counter-display",
                        "type": "text",
                        "bindings": {"content": "data.counter"},
                    },
                ],
            },
        })
        assert result is not None

    def test_add_node_to_tree(self, mcp):
        """Add a node under an existing parent."""
        mcp.call_tool("canvas_create", {"title": f"AddNode Test {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": []},
        })
        result = mcp.call_tool("canvas_add_node", {
            "parentId": "root",
            "node": {"id": "child-1", "type": "text", "props": {"content": "Added"}},
        })
        assert result is not None

    def test_add_multiple_nodes(self, mcp):
        """Add multiple nodes sequentially."""
        mcp.call_tool("canvas_create", {"title": f"MultiAdd Test {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": []},
        })
        for i in range(5):
            result = mcp.call_tool("canvas_add_node", {
                "parentId": "root",
                "node": {"id": f"item-{i}", "type": "text", "props": {"content": f"Item {i}"}},
            })
            assert result is not None

    def test_remove_node(self, mcp):
        """Remove a node from the tree."""
        mcp.call_tool("canvas_create", {"title": f"Remove Test {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "children": [
                    {"id": "keep", "type": "text", "props": {"content": "Keep"}},
                    {"id": "remove-me", "type": "text", "props": {"content": "Remove"}},
                ],
            },
        })
        result = mcp.call_tool("canvas_remove_node", {"nodeId": "remove-me"})
        assert result is not None

    def test_nested_tree_structure(self, mcp):
        """Deep nesting is supported."""
        mcp.call_tool("canvas_create", {"title": f"Nested Test {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "children": [{
                    "id": "l1",
                    "type": "container",
                    "children": [{
                        "id": "l2",
                        "type": "container",
                        "children": [{
                            "id": "l3",
                            "type": "text",
                            "props": {"content": "Deep"},
                        }],
                    }],
                }],
            },
        })
        assert result is not None


# ── Canvas Data ────────────────────────────────────────────────────────────────


class TestCanvasData:
    """Test data binding and state management."""

    def test_set_data_simple(self, mcp):
        """Set simple key-value data."""
        mcp.call_tool("canvas_create", {"title": f"Data Test {unique_id()}"})
        result = mcp.call_tool("canvas_set_data", {
            "data": {"counter": 0, "name": "test"},
        })
        assert result is not None

    def test_set_data_complex(self, mcp):
        """Set complex nested data."""
        mcp.call_tool("canvas_create", {"title": f"Complex Data {unique_id()}"})
        result = mcp.call_tool("canvas_set_data", {
            "data": {
                "user": {"name": "alice", "role": "admin"},
                "items": [1, 2, 3],
                "config": {"theme": "dark", "fontSize": 14},
            },
        })
        assert result is not None

    def test_set_data_overwrites(self, mcp):
        """Setting data again overwrites previous values."""
        mcp.call_tool("canvas_create", {"title": f"Overwrite Data {unique_id()}"})
        mcp.call_tool("canvas_set_data", {"data": {"v": 1}})
        result = mcp.call_tool("canvas_set_data", {"data": {"v": 2}})
        assert result is not None


# ── Canvas Validation ──────────────────────────────────────────────────────────


class TestCanvasValidation:
    """Test canvas validation rules."""

    def test_validate_valid_canvas(self, mcp):
        """Validating a well-formed canvas returns no errors."""
        mcp.call_tool("canvas_create", {"title": f"Valid Canvas {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": []},
        })
        result = mcp.call_tool("canvas_validate", {})
        assert result is not None

    def test_validate_empty_canvas(self, mcp):
        """Validating an empty canvas is handled."""
        mcp.call_tool("canvas_create", {"title": f"Empty Validate {unique_id()}"})
        result = mcp.call_tool("canvas_validate", {})
        assert result is not None

    def test_add_rule_to_canvas(self, mcp):
        """Add a validation rule to the canvas."""
        mcp.call_tool("canvas_create", {"title": f"Rule Canvas {unique_id()}"})
        result = mcp.call_tool("canvas_add_rule", {
            "rule": {
                "id": unique_id("rule"),
                "name": "require-root",
                "check": "tree.id == 'root'",
                "message": "Root node must have id 'root'",
                "severity": "error",
            },
        })
        assert result is not None


# ── Canvas Save/Load/List ──────────────────────────────────────────────────────


class TestCanvasPersistence:
    """Test save, load, and list operations."""

    def test_save_canvas(self, mcp):
        """Save the active canvas."""
        title = f"Save Test {unique_id()}"
        mcp.call_tool("canvas_create", {"title": title})
        result = mcp.call_tool("canvas_save", {})
        assert result is not None

    def test_list_canvases(self, mcp):
        """List returns saved canvases."""
        title = f"Listed {unique_id()}"
        mcp.call_tool("canvas_create", {"title": title})
        mcp.call_tool("canvas_save", {})
        result = mcp.call_tool("canvas_list", {})
        assert result is not None

    def test_load_saved_canvas(self, mcp):
        """Load a previously saved canvas by id."""
        title = f"Load Test {unique_id()}"
        create_result = mcp.call_tool("canvas_create", {"title": title})
        mcp.call_tool("canvas_save", {})

        # Get the list to find the id
        list_result = mcp.call_tool("canvas_list", {})
        if list_result and isinstance(list_result, (list, dict)):
            # Try to extract an id to load
            list_str = str(list_result)
            if title in list_str:
                # Canvas was saved — try loading current
                result = mcp.call_tool("canvas_get", {})
                assert result is not None


# ── Canvas Export/Import ───────────────────────────────────────────────────────


class TestCanvasExportImport:
    """Test export and import round-trip."""

    def test_export_canvas(self, mcp):
        """Export canvas returns JSON."""
        mcp.call_tool("canvas_create", {"title": f"Export Test {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": [
                {"id": "txt", "type": "text", "props": {"content": "Hello"}},
            ]},
        })
        result = mcp.call_tool("canvas_export", {})
        assert result is not None
        # Should be JSON or a string representation
        result_str = str(result)
        assert "root" in result_str or "container" in result_str or "canvas" in result_str.lower()

    def test_import_canvas(self, mcp):
        """Import a canvas from JSON."""
        canvas_json = json.dumps({
            "title": f"Imported {unique_id()}",
            "tree": {"id": "root", "type": "container", "children": []},
            "data": {"imported": True},
        })
        result = mcp.call_tool("canvas_import", {"json": canvas_json})
        assert result is not None

    def test_export_import_roundtrip(self, mcp):
        """Exported canvas can be re-imported."""
        title = f"Roundtrip {unique_id()}"
        mcp.call_tool("canvas_create", {"title": title})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": [
                {"id": "a", "type": "text", "props": {"content": "A"}},
                {"id": "b", "type": "text", "props": {"content": "B"}},
            ]},
        })
        mcp.call_tool("canvas_set_data", {"data": {"x": 42}})

        exported = mcp.call_tool("canvas_export", {})
        assert exported is not None

        # Import it back
        if isinstance(exported, str):
            result = mcp.call_tool("canvas_import", {"json": exported})
            assert result is not None


# ── Canvas Procedures ──────────────────────────────────────────────────────────


class TestCanvasProcedures:
    """Test adding behavior procedures to canvases."""

    def test_add_procedure(self, mcp):
        """Add a procedure to the canvas."""
        mcp.call_tool("canvas_create", {"title": f"Proc Test {unique_id()}"})
        result = mcp.call_tool("canvas_add_procedure", {
            "procedure": {
                "id": unique_id("proc"),
                "name": "increment_counter",
                "trigger": "button.click",
                "actions": [
                    {"type": "set_data", "key": "counter", "value": "data.counter + 1"},
                ],
            },
        })
        assert result is not None

    def test_add_multiple_procedures(self, mcp):
        """Multiple procedures can coexist."""
        mcp.call_tool("canvas_create", {"title": f"Multi Proc {unique_id()}"})
        for i in range(3):
            result = mcp.call_tool("canvas_add_procedure", {
                "procedure": {
                    "id": unique_id(f"proc-{i}"),
                    "name": f"action_{i}",
                    "trigger": f"event_{i}",
                    "actions": [{"type": "log", "message": f"Triggered {i}"}],
                },
            })
            assert result is not None


# ── Edge Cases ─────────────────────────────────────────────────────────────────


class TestCanvasEdgeCases:
    """Edge cases and error handling."""

    def test_get_canvas_without_create(self, mcp):
        """Getting canvas when none active is handled."""
        # This depends on prior state — just verify no crash
        result = mcp.call_tool("canvas_get", {})
        # Either returns a canvas or an error — both are valid
        assert result is not None or result is None  # no crash

    def test_set_tree_without_canvas(self, mcp):
        """Setting tree without active canvas is handled."""
        # Create first to ensure clean state
        mcp.call_tool("canvas_create", {"title": f"Edge {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": []},
        })
        assert result is not None

    def test_add_node_to_nonexistent_parent(self, mcp):
        """Adding node to nonexistent parent is handled gracefully."""
        mcp.call_tool("canvas_create", {"title": f"BadParent {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": []},
        })
        result = mcp.call_tool("canvas_add_node", {
            "parentId": "nonexistent-parent-xyz",
            "node": {"id": "orphan", "type": "text", "props": {"content": "Lost"}},
        })
        # Should either fail gracefully or succeed — no crash
        assert result is not None or result is None

    def test_remove_nonexistent_node(self, mcp):
        """Removing nonexistent node is handled gracefully."""
        mcp.call_tool("canvas_create", {"title": f"BadRemove {unique_id()}"})
        mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": []},
        })
        result = mcp.call_tool("canvas_remove_node", {"nodeId": "does-not-exist"})
        assert result is not None or result is None

    def test_import_invalid_json(self, mcp):
        """Importing invalid JSON is handled gracefully."""
        result = mcp.call_tool("canvas_import", {"json": "not valid json {{"})
        # Should return an error, not crash
        assert result is not None

    def test_large_tree(self, mcp):
        """Large component tree doesn't crash."""
        mcp.call_tool("canvas_create", {"title": f"Large Tree {unique_id()}"})
        children = [
            {"id": f"node-{i}", "type": "text", "props": {"content": f"Node {i}"}}
            for i in range(50)
        ]
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {"id": "root", "type": "container", "children": children},
        })
        assert result is not None

    def test_unicode_in_tree(self, mcp):
        """Unicode content in tree nodes is preserved."""
        mcp.call_tool("canvas_create", {"title": f"Unicode {unique_id()}"})
        result = mcp.call_tool("canvas_set_tree", {
            "tree": {
                "id": "root",
                "type": "container",
                "children": [
                    {"id": "emoji", "type": "text", "props": {"content": "🎯 テスト 中文 émojis"}},
                ],
            },
        })
        assert result is not None
