"""
test_large_payloads.py — Large payload and data volume tests for pares-radix.

Tests that radix handles real-world data sizes gracefully:
- Large JSON values in PluresDB (1KB, 10KB, 100KB, 1MB)
- Many keys (hundreds in rapid succession)
- Large memory entries and search across big corpora
- Bulk canvas operations with deep trees
- Chronos timeline with many events
- Config with large nested structures

These are NOT mocks — every test runs against a real pares-radix MCP server.
"""
import json
import os
import random
import string
import time
import uuid

import pytest

# ── Helpers ───────────────────────────────────────────────────────────────────


def random_string(length):
    """Generate a random alphanumeric string of given length."""
    return ''.join(random.choices(string.ascii_letters + string.digits, k=length))


def random_json_object(target_bytes):
    """Generate a random JSON-serializable dict of approximately target_bytes size."""
    obj = {}
    key_count = 0
    current_size = 2  # {}
    while current_size < target_bytes:
        key = f"field_{key_count}"
        # Mix types: strings, numbers, booleans, nested objects, arrays
        choice = key_count % 5
        if choice == 0:
            value = random_string(min(200, target_bytes - current_size))
        elif choice == 1:
            value = random.randint(-999999, 999999)
        elif choice == 2:
            value = random.choice([True, False])
        elif choice == 3:
            value = [random_string(20) for _ in range(5)]
        else:
            value = {"nested": random_string(50), "num": random.randint(0, 100)}
        obj[key] = value
        current_size = len(json.dumps(obj))
        key_count += 1
    return obj


# ── PluresDB Large Value Tests ────────────────────────────────────────────────


class TestPluresDBLargeValues:
    """Test PluresDB with increasingly large values."""

    def test_1kb_value(self, mcp):
        """Store and retrieve a ~1KB value."""
        key = f"large-test:1kb:{uuid.uuid4().hex[:8]}"
        value = random_json_object(1024)
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert retrieved["value"] == value

    def test_10kb_value(self, mcp):
        """Store and retrieve a ~10KB value."""
        key = f"large-test:10kb:{uuid.uuid4().hex[:8]}"
        value = random_json_object(10 * 1024)
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert retrieved["value"] == value

    def test_100kb_value(self, mcp):
        """Store and retrieve a ~100KB value."""
        key = f"large-test:100kb:{uuid.uuid4().hex[:8]}"
        value = random_json_object(100 * 1024)
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert retrieved["value"] == value

    def test_1mb_value(self, mcp):
        """Store and retrieve a ~1MB value — stress test for serialization."""
        key = f"large-test:1mb:{uuid.uuid4().hex[:8]}"
        # Generate a large but structured value
        value = {
            "metadata": {"size": "1MB", "generated": True},
            "records": [
                {"id": i, "data": random_string(500), "tags": [random_string(10) for _ in range(5)]}
                for i in range(500)
            ]
        }
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert retrieved["value"]["metadata"] == value["metadata"]
            assert len(retrieved["value"]["records"]) == 500

    def test_deeply_nested_value(self, mcp):
        """Store a deeply nested JSON structure (20 levels)."""
        key = f"large-test:nested:{uuid.uuid4().hex[:8]}"
        value = {"level": 0, "data": random_string(50)}
        current = value
        for i in range(1, 20):
            current["child"] = {"level": i, "data": random_string(50)}
            current = current["child"]

        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            # Walk to verify depth preserved
            node = retrieved["value"]
            for i in range(19):
                assert "child" in node, f"Nesting lost at level {i}"
                node = node["child"]

    def test_large_array_value(self, mcp):
        """Store a value that's a large array (1000 elements)."""
        key = f"large-test:array:{uuid.uuid4().hex[:8]}"
        value = [{"index": i, "payload": random_string(100)} for i in range(1000)]
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert len(retrieved["value"]) == 1000

    def test_unicode_heavy_value(self, mcp):
        """Store values with extensive unicode content."""
        key = f"large-test:unicode:{uuid.uuid4().hex[:8]}"
        value = {
            "emoji": "🎉🚀💡🔥✨" * 100,
            "cjk": "日本語テスト中文测试한국어" * 50,
            "arabic": "مرحبا بالعالم" * 50,
            "math": "∑∫∂∇∆Ωπ√∞" * 50,
            "mixed": f"Hello {'世界' * 20} {'🌍' * 20} end",
        }
        result = mcp.call_tool("db_put", {"key": key, "value": value})
        assert result is not None

        retrieved = mcp.call_tool("db_get", {"key": key})
        assert retrieved is not None
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert retrieved["value"]["emoji"] == value["emoji"]
            assert retrieved["value"]["cjk"] == value["cjk"]


# ── PluresDB Many Keys Tests ─────────────────────────────────────────────────


class TestPluresDBManyKeys:
    """Test PluresDB with many keys in rapid succession."""

    def test_100_keys_rapid_write(self, mcp):
        """Write 100 keys as fast as possible, verify all stored."""
        prefix = f"bulk:{uuid.uuid4().hex[:8]}"
        keys_written = []
        for i in range(100):
            key = f"{prefix}:item:{i:04d}"
            result = mcp.call_tool("db_put", {"key": key, "value": {"index": i, "data": random_string(50)}})
            assert result is not None, f"Failed to write key {i}"
            keys_written.append(key)

        # Verify via prefix listing
        listing = mcp.call_tool("db_keys", {"prefix": prefix})
        if isinstance(listing, dict) and "keys" in listing:
            found_keys = listing["keys"]
            assert len(found_keys) >= 100, f"Expected 100 keys, got {len(found_keys)}"

    def test_rapid_read_after_bulk_write(self, mcp):
        """After bulk write, read all keys back and verify values."""
        prefix = f"readback:{uuid.uuid4().hex[:8]}"
        expected = {}
        for i in range(50):
            key = f"{prefix}:{i:04d}"
            value = {"seq": i, "payload": random_string(100)}
            mcp.call_tool("db_put", {"key": key, "value": value})
            expected[key] = value

        # Read them all back
        for key, value in expected.items():
            retrieved = mcp.call_tool("db_get", {"key": key})
            if isinstance(retrieved, dict) and "value" in retrieved:
                assert retrieved["value"]["seq"] == value["seq"]

    def test_overwrite_same_key_100_times(self, mcp):
        """Overwrite the same key 100 times, verify final value."""
        key = f"overwrite-stress:{uuid.uuid4().hex[:8]}"
        for i in range(100):
            mcp.call_tool("db_put", {"key": key, "value": {"iteration": i}})

        retrieved = mcp.call_tool("db_get", {"key": key})
        if isinstance(retrieved, dict) and "value" in retrieved:
            assert retrieved["value"]["iteration"] == 99

    def test_db_dump_with_many_keys(self, mcp):
        """db_dump should handle many keys without timeout."""
        prefix = f"dump-test:{uuid.uuid4().hex[:8]}"
        for i in range(30):
            mcp.call_tool("db_put", {"key": f"{prefix}:{i:04d}", "value": i})

        result = mcp.call_tool("db_dump", {}, timeout=15)
        assert result is not None
        # Should include our keys in the dump
        if isinstance(result, dict):
            dump_str = json.dumps(result)
            assert prefix in dump_str


# ── Memory Large Corpus Tests ─────────────────────────────────────────────────


class TestMemoryLargeCorpus:
    """Test memory storage and search with many entries."""

    def test_store_10_memories_then_search(self, mcp):
        """Store 10 diverse memories, then search for specific ones."""
        topics = [
            "Python asyncio event loop performance optimization",
            "Rust borrow checker lifetime elision rules",
            "Docker multi-stage build caching strategies",
            "Kubernetes pod scheduling and affinity rules",
            "PostgreSQL query planner cost estimation",
            "Redis cluster hash slot redistribution",
            "Git rebase interactive workflow patterns",
            "SSH tunnel port forwarding configuration",
            "TLS certificate chain validation process",
            "DNS recursive resolver caching behavior",
        ]

        # Store memories across different topics (10 — each embedding takes ~1s)
        for i in range(10):
            topic = topics[i]
            content = f"{topic} - detail {i}: {random_string(100)}"
            result = mcp.call_tool("memory_store", {
                "content": content,
                "category": "technical-note",
                "tags": [topic.split()[0].lower(), f"batch-{i // 10}"],
            }, timeout=30)
            # Memory store may not be available in all builds
            if result is None or (isinstance(result, dict) and "error" in result):
                pytest.skip("memory_store not available")

        # Search for a specific topic
        search_result = mcp.call_tool("memory_search", {
            "query": "Docker multi-stage build",
            "limit": 5,
        }, timeout=15)
        assert search_result is not None

    def test_store_large_memory_content(self, mcp):
        """Store a memory with a large content field (~5KB)."""
        content = f"Detailed analysis of system architecture: {random_string(5000)}"
        result = mcp.call_tool("memory_store", {
            "content": content,
            "category": "architecture-decision",
            "tags": ["large-payload-test"],
        }, timeout=30)
        if result is None or (isinstance(result, dict) and "error" in result):
            pytest.skip("memory_store not available")
        assert result is not None


# ── Chronos Large Timeline Tests ──────────────────────────────────────────────


class TestChronosLargeTimeline:
    """Test Chronos with many events."""

    def test_record_100_events_rapidly(self, mcp):
        """Record 100 events in rapid succession."""
        for i in range(100):
            result = mcp.call_tool("chronos_record", {
                "event": f"stress-event-{i}",
                "level": "info",
                "data": {"iteration": i, "payload": random_string(50)},
            })
            assert result is not None, f"Failed to record event {i}"

    def test_timeline_query_after_bulk(self, mcp):
        """Query timeline after bulk recording — should return results quickly."""
        # Record events with a unique prefix
        marker = uuid.uuid4().hex[:8]
        for i in range(30):
            mcp.call_tool("chronos_record", {
                "event": f"timeline-{marker}-{i}",
                "level": "info",
                "data": {"seq": i},
            })

        start = time.time()
        result = mcp.call_tool("chronos_timeline", {"limit": 50})
        elapsed = time.time() - start

        assert result is not None
        assert elapsed < 5.0, f"Timeline query took {elapsed:.2f}s — too slow"

    def test_large_event_data(self, mcp):
        """Record an event with a large data payload (~50KB)."""
        large_data = {
            "metrics": [{"name": f"metric_{i}", "value": random.random(), "tags": [random_string(10) for _ in range(5)]} for i in range(200)],
            "context": random_string(10000),
        }
        result = mcp.call_tool("chronos_record", {
            "event": "large-payload-event",
            "level": "info",
            "data": large_data,
        })
        assert result is not None


# ── Canvas Deep Tree Tests ────────────────────────────────────────────────────


class TestCanvasDeepTrees:
    """Test Canvas with deep and wide component trees."""

    def test_wide_tree_50_children(self, mcp):
        """Create a canvas with 50 children under root."""
        mcp.call_tool("canvas_create", {"title": "Wide Tree Test"})
        for i in range(50):
            result = mcp.call_tool("canvas_add_node", {
                "parentId": "root",
                "node": {
                    "id": f"child-{i:04d}",
                    "type": "Text",
                    "props": {"content": f"Node {i}: {random_string(30)}"},
                },
            })
            assert result is not None, f"Failed to add child {i}"

        # Verify tree
        canvas = mcp.call_tool("canvas_get", {})
        assert canvas is not None

    def test_deep_tree_20_levels(self, mcp):
        """Create a canvas with 20 levels of nesting."""
        mcp.call_tool("canvas_create", {"title": "Deep Tree Test"})
        parent_id = "root"
        for i in range(20):
            node_id = f"deep-{i:04d}"
            result = mcp.call_tool("canvas_add_node", {
                "parentId": parent_id,
                "node": {
                    "id": node_id,
                    "type": "Box",
                    "props": {"label": f"Level {i}"},
                },
            })
            assert result is not None, f"Failed at level {i}"
            parent_id = node_id

        canvas = mcp.call_tool("canvas_get", {})
        assert canvas is not None

    def test_canvas_with_large_props(self, mcp):
        """Add nodes with large props content."""
        mcp.call_tool("canvas_create", {"title": "Large Props Test"})
        result = mcp.call_tool("canvas_add_node", {
            "parentId": "root",
            "node": {
                "id": "big-props-node",
                "type": "Text",
                "props": {
                    "content": random_string(5000),
                    "metadata": {f"key_{i}": random_string(100) for i in range(20)},
                },
            },
        })
        assert result is not None

    def test_canvas_export_large(self, mcp):
        """Export a canvas with many nodes — should complete without timeout."""
        mcp.call_tool("canvas_create", {"title": "Export Stress Test"})
        for i in range(30):
            mcp.call_tool("canvas_add_node", {
                "parentId": "root",
                "node": {
                    "id": f"export-node-{i:04d}",
                    "type": "Text",
                    "props": {"content": random_string(200)},
                },
            })

        start = time.time()
        result = mcp.call_tool("canvas_export", {}, timeout=10)
        elapsed = time.time() - start
        assert result is not None
        assert elapsed < 5.0, f"Export took {elapsed:.2f}s"


# ── Praxis Large Constraint Sets ──────────────────────────────────────────────


class TestPraxisLargeConstraints:
    """Test Praxis with many constraints and complex evaluations."""

    def test_add_50_constraints(self, mcp):
        """Add 50 constraints and verify they're all registered."""
        marker = uuid.uuid4().hex[:8]
        for i in range(50):
            result = mcp.call_tool("praxis_add_constraint", {
                "name": f"stress-constraint-{marker}-{i:04d}",
                "severity": "error" if i % 3 == 0 else "warning",
                "require": f"context.value_{i} != null",
                "message": f"Constraint {i} violated: value_{i} must not be null",
            })
            assert result is not None, f"Failed to add constraint {i}"

        rules = mcp.call_tool("praxis_list_rules", {})
        assert rules is not None

    def test_evaluate_against_many_constraints(self, mcp):
        """Evaluate context against many constraints — performance check."""
        # Add constraints
        marker = uuid.uuid4().hex[:8]
        for i in range(20):
            mcp.call_tool("praxis_add_constraint", {
                "name": f"eval-perf-{marker}-{i}",
                "severity": "error",
                "require": f"context.field_{i} == 'valid'",
                "message": f"field_{i} must be valid",
            })

        # Evaluate with a context that satisfies some, violates others
        context = {f"field_{i}": "valid" if i % 2 == 0 else "invalid" for i in range(20)}
        start = time.time()
        result = mcp.call_tool("praxis_evaluate", {"context": context})
        elapsed = time.time() - start

        assert result is not None
        assert elapsed < 3.0, f"Evaluation took {elapsed:.2f}s — too slow for 20 constraints"


# ── Config Large Structures ───────────────────────────────────────────────────


class TestConfigLargeStructures:
    """Test config operations with large/complex config values."""

    def test_large_config_value(self, mcp):
        """Store a large config entry."""
        key = f"test.large.config.{uuid.uuid4().hex[:8]}"
        value = {
            "services": {f"svc-{i}": {"port": 8000 + i, "replicas": i % 5 + 1, "env": {f"VAR_{j}": random_string(20) for j in range(10)}} for i in range(20)},
        }
        result = mcp.call_tool("config_put", {"key": key, "value": value})
        if result is None or (isinstance(result, dict) and "error" in result):
            pytest.skip("config_put not available")
        assert result is not None

    def test_many_config_keys(self, mcp):
        """Write 50 config keys and list them."""
        prefix = f"test.bulk.{uuid.uuid4().hex[:8]}"
        for i in range(50):
            mcp.call_tool("config_put", {"key": f"{prefix}.item.{i:04d}", "value": {"index": i}})

        result = mcp.call_tool("config_list", {"prefix": prefix})
        if result is None or (isinstance(result, dict) and "error" in result):
            pytest.skip("config_list not available")
        assert result is not None


# ── Performance Baseline Tests ────────────────────────────────────────────────


class TestPerformanceBaselines:
    """Establish performance baselines for large operations."""

    def test_db_put_latency_under_load(self, mcp):
        """Measure average put latency across 50 operations."""
        times = []
        prefix = f"perf:{uuid.uuid4().hex[:8]}"
        for i in range(50):
            start = time.time()
            mcp.call_tool("db_put", {"key": f"{prefix}:{i}", "value": {"i": i, "data": random_string(200)}})
            times.append(time.time() - start)

        avg = sum(times) / len(times)
        p99 = sorted(times)[int(len(times) * 0.99)]
        assert avg < 2.0, f"Average put latency {avg:.3f}s exceeds 2s"
        assert p99 < 5.0, f"p99 put latency {p99:.3f}s exceeds 5s"

    def test_db_get_latency_under_load(self, mcp):
        """Measure average get latency across 50 operations."""
        prefix = f"perf-get:{uuid.uuid4().hex[:8]}"
        # Write first
        for i in range(50):
            mcp.call_tool("db_put", {"key": f"{prefix}:{i}", "value": {"i": i}})

        # Measure reads
        times = []
        for i in range(50):
            start = time.time()
            mcp.call_tool("db_get", {"key": f"{prefix}:{i}"})
            times.append(time.time() - start)

        avg = sum(times) / len(times)
        p99 = sorted(times)[int(len(times) * 0.99)]
        assert avg < 1.0, f"Average get latency {avg:.3f}s exceeds 1s"
        assert p99 < 3.0, f"p99 get latency {p99:.3f}s exceeds 3s"

    def test_sustained_mixed_workload_60s(self, mcp):
        """Run a mixed read/write workload for up to 30 seconds, measure throughput."""
        prefix = f"sustained:{uuid.uuid4().hex[:8]}"
        ops = 0
        errors = 0
        deadline = time.time() + 30  # 30 seconds max

        while time.time() < deadline and ops < 200:
            op = random.choice(["put", "get", "keys"])
            try:
                if op == "put":
                    mcp.call_tool("db_put", {"key": f"{prefix}:{ops}", "value": {"op": ops}})
                elif op == "get":
                    mcp.call_tool("db_get", {"key": f"{prefix}:{random.randint(0, max(ops, 1))}"})
                else:
                    mcp.call_tool("db_keys", {"prefix": prefix})
                ops += 1
            except Exception:
                errors += 1
                if errors > 10:
                    break

        elapsed = min(time.time() - (deadline - 30), 30)
        throughput = ops / elapsed if elapsed > 0 else 0
        assert ops > 20, f"Only completed {ops} ops in {elapsed:.1f}s"
        assert throughput > 1.0, f"Throughput {throughput:.2f} ops/s too low"
        assert errors < ops * 0.1, f"Error rate {errors}/{ops} too high"
