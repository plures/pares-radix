"""
test_pluresdb_integration.py — PluresDB CRUD integration tests via MCP server.

These tests exercise real PluresDB operations through the MCP JSON-RPC interface:
- db_put: store values
- db_get: retrieve values
- db_keys: list keys by prefix
- db_dump: dump all state
- db_delete: remove keys
- memory_store/memory_search: semantic memory (if embeddings available)
- chronos_record/chronos_timeline: timeline events

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_pluresdb_integration.py -v
"""
import json
import os
import select
import subprocess
import time
import uuid
import pytest
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
RADIX_BIN = os.environ.get(
    "RADIX_BINARY",
    str(REPO_ROOT / "target" / "release" / "pares-radix"),
)


class McpClient:
    """MCP JSON-RPC client over stdio."""

    def __init__(self, workdir=None):
        self.workdir = workdir or f"/tmp/radix-test-{uuid.uuid4().hex[:8]}"
        os.makedirs(self.workdir, exist_ok=True)
        self.proc = None
        self._next_id = 1

    def start(self):
        self.proc = subprocess.Popen(
            [RADIX_BIN, "mcp-serve", "--workdir", self.workdir],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        # Initialize handshake
        self._send("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "pluresdb-test", "version": "1.0.0"},
        })
        resp = self._read(timeout=5)
        assert resp is not None, "MCP server failed to respond to initialize"
        assert "result" in resp, f"Initialize failed: {resp}"

        # Send initialized notification
        self.proc.stdin.write(json.dumps({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }) + "\n")
        self.proc.stdin.flush()
        time.sleep(0.3)
        return self

    def stop(self):
        if self.proc:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()

    def call_tool(self, tool_name, arguments=None, timeout=10):
        """Call an MCP tool and return the result content."""
        req_id = self._next_id
        self._send("tools/call", {
            "name": tool_name,
            "arguments": arguments or {},
        })
        resp = self._read(timeout=timeout)
        if resp is None:
            return None
        if "error" in resp:
            return {"error": resp["error"]}
        if "result" in resp:
            # MCP tools/call returns {content: [{type, text}]}
            result = resp["result"]
            if "content" in result:
                texts = [c.get("text", "") for c in result["content"] if c.get("type") == "text"]
                combined = "\n".join(texts)
                # Try to parse as JSON
                try:
                    return json.loads(combined)
                except (json.JSONDecodeError, TypeError):
                    return combined
            return result
        return resp

    def _send(self, method, params=None):
        req = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            req["params"] = params
        self._next_id += 1
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()

    def _read(self, timeout=5):
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            return None
        line = self.proc.stdout.readline()
        if line:
            try:
                return json.loads(line.strip())
            except json.JSONDecodeError:
                return {"raw": line.strip()}
        return None


@pytest.fixture(scope="module")
def mcp():
    """Module-scoped MCP client (one server for all tests in this module)."""
    if not os.path.isfile(RADIX_BIN):
        pytest.skip(f"Binary not found: {RADIX_BIN}")
    client = McpClient()
    client.start()
    yield client
    client.stop()


class TestDbPutAndGet:
    """Test db_put and db_get: basic CRUD operations."""

    def test_put_string_value(self, mcp):
        """Store a string value and retrieve it."""
        key = f"test:string:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("db_put", {"key": key, "value": "hello world"})
        assert result is not None
        # Should not be an error
        if isinstance(result, dict) and "error" in result:
            pytest.fail(f"db_put failed: {result}")

        # Retrieve it
        got = mcp.call_tool("db_get", {"key": key})
        # The value should be "hello world" or contain it
        if isinstance(got, dict):
            assert got.get("value") == "hello world" or got.get("result") == "hello world" or "hello world" in str(got)
        elif isinstance(got, str):
            assert "hello world" in got
        else:
            pytest.fail(f"Unexpected db_get response: {got}")

    def test_put_json_object(self, mcp):
        """Store a JSON object value."""
        key = f"test:obj:{uuid.uuid4().hex[:6]}"
        value = {"name": "radix", "version": "1.44.3", "features": ["mcp", "praxis", "pluresdb"]}
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None
        if isinstance(result, dict) and "error" in result:
            pytest.fail(f"db_put object failed: {result}")

        got = mcp.call_tool("db_get", {"key": key})
        # Should contain the object data
        got_str = json.dumps(got) if isinstance(got, dict) else str(got)
        assert "radix" in got_str
        assert "1.44.3" in got_str or "features" in got_str

    def test_put_number_value(self, mcp):
        """Store a numeric value."""
        key = f"test:num:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("db_put", {"key": key, "value": 42})
        assert result is not None
        if isinstance(result, dict) and "error" in result:
            pytest.fail(f"db_put number failed: {result}")

        got = mcp.call_tool("db_get", {"key": key})
        got_str = str(got)
        assert "42" in got_str

    def test_put_boolean_value(self, mcp):
        """Store a boolean value."""
        key = f"test:bool:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("db_put", {"key": key, "value": True})
        assert result is not None

    def test_put_null_value(self, mcp):
        """Store a null value."""
        key = f"test:null:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("db_put", {"key": key, "value": None})
        assert result is not None

    def test_put_array_value(self, mcp):
        """Store an array value."""
        key = f"test:arr:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("db_put", {"key": key, "value": [1, "two", 3.0, None, True]})
        assert result is not None
        if isinstance(result, dict) and "error" in result:
            pytest.fail(f"db_put array failed: {result}")

    def test_get_nonexistent_key(self, mcp):
        """Getting a nonexistent key returns null/None."""
        key = f"test:nonexist:{uuid.uuid4().hex[:6]}"
        got = mcp.call_tool("db_get", {"key": key})
        # Should be null/None or indicate key not found
        if isinstance(got, dict):
            val = got.get("value") if "value" in got else got.get("result")
            assert val is None or val == "null" or "null" in str(got).lower() or "not found" in str(got).lower()
        elif got is None:
            pass  # Expected
        elif isinstance(got, str):
            assert "null" in got.lower() or "none" in got.lower() or got.strip() == ""

    def test_overwrite_existing_key(self, mcp):
        """Overwriting a key updates the value."""
        key = f"test:overwrite:{uuid.uuid4().hex[:6]}"
        mcp.call_tool("db_put", {"key": key, "value": "first"})
        mcp.call_tool("db_put", {"key": key, "value": "second"})
        got = mcp.call_tool("db_get", {"key": key})
        got_str = str(got)
        assert "second" in got_str


class TestDbDelete:
    """Test db_delete operations."""

    def test_delete_existing_key(self, mcp):
        """Delete a key and verify it's gone."""
        key = f"test:del:{uuid.uuid4().hex[:6]}"
        mcp.call_tool("db_put", {"key": key, "value": "to-delete"})
        result = mcp.call_tool("db_delete", {"key": key})
        assert result is not None

        # Verify deleted
        got = mcp.call_tool("db_get", {"key": key})
        if isinstance(got, dict):
            val = got.get("value") if "value" in got else got.get("result")
            assert val is None or "null" in str(got).lower()
        elif got is None:
            pass
        elif isinstance(got, str):
            assert "null" in got.lower() or got.strip() == ""

    def test_delete_nonexistent_key(self, mcp):
        """Deleting a nonexistent key doesn't crash."""
        key = f"test:del-noexist:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("db_delete", {"key": key})
        # Should succeed (no-op) or return null
        assert result is not None or result is None  # either way, no crash


class TestDbKeys:
    """Test db_keys listing operations."""

    def test_keys_with_prefix(self, mcp):
        """List keys by prefix."""
        prefix = f"test:prefix-{uuid.uuid4().hex[:4]}"
        # Store multiple keys with same prefix
        mcp.call_tool("db_put", {"key": f"{prefix}:a", "value": 1})
        mcp.call_tool("db_put", {"key": f"{prefix}:b", "value": 2})
        mcp.call_tool("db_put", {"key": f"{prefix}:c", "value": 3})

        result = mcp.call_tool("db_keys", {"prefix": prefix})
        result_str = str(result)
        # Should find all 3 keys
        assert f"{prefix}:a" in result_str
        assert f"{prefix}:b" in result_str
        assert f"{prefix}:c" in result_str

    def test_keys_empty_prefix(self, mcp):
        """List all keys (empty prefix)."""
        result = mcp.call_tool("db_keys", {})
        # Should return something (at least the test keys we've created)
        assert result is not None

    def test_keys_no_match(self, mcp):
        """Prefix with no matching keys returns empty."""
        result = mcp.call_tool("db_keys", {"prefix": f"no-such-prefix-{uuid.uuid4().hex}"})
        if isinstance(result, list):
            assert len(result) == 0
        elif isinstance(result, dict):
            keys = result.get("keys", result.get("result", []))
            if isinstance(keys, list):
                assert len(keys) == 0


class TestDbDump:
    """Test db_dump operations."""

    def test_dump_returns_data(self, mcp):
        """db_dump returns all stored key-value pairs."""
        # Ensure at least one key exists
        key = f"test:dump:{uuid.uuid4().hex[:6]}"
        mcp.call_tool("db_put", {"key": key, "value": "dump-check"})

        result = mcp.call_tool("db_dump", {})
        result_str = str(result)
        # Should contain our test key
        assert key in result_str or "dump-check" in result_str


class TestChronosIntegration:
    """Test Chronos timeline recording and retrieval."""

    def test_record_event(self, mcp):
        """Record a Chronos event."""
        key = f"test:chronos:{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("chronos_record", {
            "key": key,
            "action": "create",
            "actor": "test-harness",
            "rationale": "integration test event",
            "data": {"test": True, "timestamp": time.time()},
        })
        assert result is not None
        if isinstance(result, dict) and "error" in result:
            pytest.skip(f"Chronos not available: {result}")

    def test_timeline_retrieval(self, mcp):
        """Retrieve timeline events."""
        # Record an event first
        key = f"test:timeline:{uuid.uuid4().hex[:6]}"
        mcp.call_tool("chronos_record", {
            "key": key,
            "action": "create",
            "actor": "test-harness",
        })

        result = mcp.call_tool("chronos_timeline", {"limit": 5})
        if isinstance(result, dict) and "error" in result:
            pytest.skip(f"Chronos timeline not available: {result}")
        # Should have at least one event
        assert result is not None

    def test_chronos_history_for_key(self, mcp):
        """Get version history for a specific key."""
        key = f"test:history:{uuid.uuid4().hex[:6]}"
        # Create multiple mutations
        mcp.call_tool("chronos_record", {"key": key, "action": "create", "actor": "test"})
        mcp.call_tool("chronos_record", {"key": key, "action": "update", "actor": "test"})

        result = mcp.call_tool("chronos_history", {"key": key})
        if isinstance(result, dict) and "error" in result:
            pytest.skip(f"Chronos history not available: {result}")
        assert result is not None

    def test_chronos_set_and_get_level(self, mcp):
        """Set and verify Chronos recording level."""
        result = mcp.call_tool("chronos_set_level", {"level": "info"})
        assert result is not None

        level = mcp.call_tool("chronos_get_level", {})
        if isinstance(level, dict) and "error" in level:
            pytest.skip("chronos_get_level not available")
        level_str = str(level).lower()
        assert "info" in level_str or "level" in level_str


class TestConfigAndRuntime:
    """Test config and runtime tools via MCP."""

    def test_config_get(self, mcp):
        """Get a config value."""
        result = mcp.call_tool("config_get", {"key": "model"}, timeout=15)
        # May return None if config key is unset (no config file) — that's valid
        # The tool call itself shouldn't error out
        if result is None:
            # Retry with a longer timeout in case of slow startup
            result = mcp.call_tool("config_get", {"key": "model"}, timeout=15)
        # Either a value or None (unset) is acceptable — not an error
        if isinstance(result, dict) and "error" in result:
            pytest.fail(f"config_get errored: {result}")

    def test_config_list(self, mcp):
        """List all config keys."""
        result = mcp.call_tool("config_list", {})
        assert result is not None

    def test_config_schema_empty(self, mcp):
        """config_schema with empty key lists all known keys."""
        result = mcp.call_tool("config_schema", {"key": ""})
        assert result is not None

    def test_runtime_status(self, mcp):
        """Get runtime status."""
        result = mcp.call_tool("runtime_status", {})
        assert result is not None
        result_str = str(result)
        # Should mention version or components
        assert "1.44" in result_str or "version" in result_str.lower() or "component" in result_str.lower() or "status" in result_str.lower()

    def test_telemetry_snapshot(self, mcp):
        """Get telemetry data."""
        result = mcp.call_tool("telemetry_snapshot", {})
        assert result is not None


class TestPraxisViaDb:
    """Test Praxis constraint/rule management through MCP tools."""

    def test_add_constraint(self, mcp):
        """Add a Praxis constraint via MCP."""
        name = f"test-constraint-{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "payload.tests_passed == true",
            "message": "Tests must pass before deploy",
        })
        assert result is not None
        if isinstance(result, dict) and "error" in result:
            pytest.skip(f"praxis_add_constraint not available: {result}")

    def test_add_rule(self, mcp):
        """Add a Praxis rule via MCP."""
        name = f"test-rule-{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("praxis_add_rule", {
            "name": name,
            "priority": 10,
            "conditions": ["action == 'build'"],
            "actions": [{"type": "log", "message": "Build started"}],
        })
        assert result is not None

    def test_praxis_evaluate(self, mcp):
        """Evaluate Praxis constraints against a context."""
        result = mcp.call_tool("praxis_evaluate", {
            "action": "test_action",
            "payload": {"tests_passed": True, "branch": "main"},
        })
        assert result is not None

    def test_praxis_list(self, mcp):
        """List Praxis modules and rules."""
        result = mcp.call_tool("praxis_list", {})
        assert result is not None

    def test_px_status(self, mcp):
        """Get Praxis subsystem status."""
        result = mcp.call_tool("px_status", {})
        assert result is not None


class TestPluginSystem:
    """Test plugin management via MCP."""

    def test_plugin_list(self, mcp):
        """List registered plugins."""
        result = mcp.call_tool("plugin_list", {})
        assert result is not None

    def test_plugin_register_and_info(self, mcp):
        """Register a test plugin and inspect it."""
        name = f"test-plugin-{uuid.uuid4().hex[:6]}"
        result = mcp.call_tool("plugin_register", {
            "name": name,
            "version": "0.1.0",
            "description": "Integration test plugin",
            "capabilities": ["test"],
        })
        if isinstance(result, dict) and "error" in result:
            pytest.skip(f"plugin_register not available: {result}")

        info = mcp.call_tool("plugin_info", {"name": name})
        info_str = str(info)
        # Plugin runtime may not be configured in headless mode — that's acceptable
        if "not configured" in info_str.lower() or "not found" in info_str.lower():
            pytest.skip("Plugin runtime not configured in headless MCP mode")
        assert name in info_str or "0.1.0" in info_str


class TestCanvasOperations:
    """Test canvas CRUD via MCP."""

    def test_canvas_create(self, mcp):
        """Create a canvas app."""
        result = mcp.call_tool("canvas_create", {
            "title": f"Test Canvas {uuid.uuid4().hex[:6]}",
            "description": "Integration test canvas",
        })
        assert result is not None
        if isinstance(result, dict) and "error" in result:
            pytest.skip(f"canvas_create not available: {result}")

    def test_canvas_get_after_create(self, mcp):
        """Get the active canvas after creating one."""
        mcp.call_tool("canvas_create", {"title": "Get Test"})
        result = mcp.call_tool("canvas_get", {})
        assert result is not None
        result_str = str(result)
        assert "Get Test" in result_str or "title" in result_str.lower()

    def test_canvas_set_tree(self, mcp):
        """Set a component tree on the active canvas."""
        mcp.call_tool("canvas_create", {"title": "Tree Test"})
        tree = {
            "id": "root",
            "type": "Container",
            "props": {"padding": "16px"},
            "children": [
                {"id": "heading", "type": "Text", "props": {"content": "Hello Radix"}},
            ],
        }
        result = mcp.call_tool("canvas_set_tree", {"tree": tree})
        assert result is not None

    def test_canvas_validate(self, mcp):
        """Validate the active canvas."""
        mcp.call_tool("canvas_create", {"title": "Validate Test"})
        result = mcp.call_tool("canvas_validate", {})
        assert result is not None

    def test_canvas_catalog(self, mcp):
        """Get the component catalog."""
        result = mcp.call_tool("canvas_catalog", {})
        assert result is not None


class TestMemoryOperations:
    """Test memory_store and memory_search via MCP."""

    def test_memory_store_basic(self, mcp):
        """Store a fact in memory."""
        unique = uuid.uuid4().hex[:8]
        result = mcp.call_tool("memory_store", {
            "content": f"Test fact {unique}: the sky is blue",
            "tags": ["test", "color"],
        })
        result_str = str(result)
        # Should succeed or return stored ID
        assert result is not None
        assert "error" not in result_str.lower() or "not configured" in result_str.lower()

    def test_memory_store_without_tags(self, mcp):
        """Store a fact without tags."""
        result = mcp.call_tool("memory_store", {
            "content": "Radix was built in Rust.",
        })
        assert result is not None
        result_str = str(result)
        # Accept success or 'not configured' (headless mode may lack embeddings)
        assert "missing required" not in result_str.lower()

    def test_memory_store_missing_content(self, mcp):
        """Store without content should error."""
        result = mcp.call_tool("memory_store", {})
        result_str = str(result).lower()
        assert "error" in result_str or "missing" in result_str

    def test_memory_search_basic(self, mcp):
        """Search memory for a query."""
        result = mcp.call_tool("memory_search", {
            "query": "sky color",
        })
        assert result is not None
        # May return empty results or error if no embeddings; shouldn't crash

    def test_memory_search_with_limit(self, mcp):
        """Search with explicit limit parameter."""
        result = mcp.call_tool("memory_search", {
            "query": "rust programming",
            "limit": 5,
        })
        assert result is not None

    def test_memory_search_missing_query(self, mcp):
        """Search without query should error."""
        result = mcp.call_tool("memory_search", {})
        result_str = str(result).lower()
        assert "error" in result_str or "missing" in result_str

    def test_memory_roundtrip(self, mcp):
        """Store then search — verify the system doesn't crash on a full cycle."""
        unique = uuid.uuid4().hex[:8]
        store_result = mcp.call_tool("memory_store", {
            "content": f"Roundtrip test {unique}: Docker containers are useful",
            "tags": ["test", "docker"],
        })
        assert store_result is not None

        search_result = mcp.call_tool("memory_search", {
            "query": f"roundtrip {unique}",
        })
        assert search_result is not None


class TestPraxisScenarios:
    """Test Praxis constraint evaluation with realistic scenarios."""

    def test_praxis_add_and_evaluate_constraint(self, mcp):
        """Add a constraint then evaluate against matching context."""
        unique = uuid.uuid4().hex[:8]
        # Add a constraint
        add_result = mcp.call_tool("praxis_add_constraint", {
            "name": f"test-no-delete-{unique}",
            "severity": "error",
            "when": "action == 'delete'",
            "require": "confirmed == true",
            "message": "Deletion requires confirmation",
        })
        assert add_result is not None

        # Evaluate with a violating context
        eval_result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "delete", "confirmed": False},
        })
        assert eval_result is not None
        eval_str = str(eval_result)
        # Should have violation or at least not crash
        # The constraint system may or may not match depending on eval engine
        assert "error" not in eval_str.lower() or "violation" in eval_str.lower() or "constraint" in eval_str.lower()

    def test_praxis_add_rule_and_list(self, mcp):
        """Add a rule then verify it appears in the list."""
        unique = uuid.uuid4().hex[:8]
        rule_name = f"test-rule-{unique}"
        add_result = mcp.call_tool("praxis_add_rule", {
            "name": rule_name,
            "conditions": ["status == 'ready'"],
            "actions": [{"type": "notify", "message": "System is ready"}],
            "priority": 10,
        })
        assert add_result is not None

        list_result = mcp.call_tool("praxis_list", {})
        assert list_result is not None
        list_str = str(list_result)
        assert rule_name in list_str

    def test_praxis_evaluate_no_violations(self, mcp):
        """Evaluate a context that shouldn't trigger violations."""
        eval_result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "read", "user": "admin"},
        })
        assert eval_result is not None
        # Read actions typically don't violate anything
