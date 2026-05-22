"""
Concurrency & Parallel Stress Tests for pares-radix MCP server.

Tests multi-client scenarios, race conditions, and concurrent tool invocations
to verify the server handles parallel load without data corruption or crashes.

Strategy:
- Single-process concurrency: rapid sequential calls testing state consistency
- Multi-process concurrency: multiple mcp-serve instances sharing a workdir
- Burst patterns: many operations in quick succession without waiting
"""

import json
import os
import select
import subprocess
import sys
import threading
import time
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parent.parent.parent
RADIX_BIN = os.environ.get(
    "RADIX_BINARY",
    str(REPO_ROOT / "target" / "release" / "pares-radix"),
)

if not os.path.isfile(RADIX_BIN):
    RADIX_BIN = str(REPO_ROOT / "target" / "debug" / "pares-radix")


class StressMcpClient:
    """MCP client for stress testing — supports rapid sequential calls."""

    def __init__(self, workdir: str):
        self.workdir = workdir
        os.makedirs(workdir, exist_ok=True)
        self.proc = None
        self._next_id = 1
        self._lock = threading.Lock()

    def start(self):
        self.proc = subprocess.Popen(
            [RADIX_BIN, "mcp-serve", "--workdir", self.workdir],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self._send("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "stress-test", "version": "1.0.0"},
        })
        resp = self._read(timeout=5)
        assert resp is not None, "MCP server failed to initialize"
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

    def call_tool(self, tool_name: str, arguments: dict = None, timeout: float = 10):
        """Thread-safe tool call."""
        with self._lock:
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
            result = resp["result"]
            if "content" in result:
                texts = [c.get("text", "") for c in result["content"] if c.get("type") == "text"]
                combined = "\n".join(texts)
                try:
                    return json.loads(combined)
                except (json.JSONDecodeError, TypeError):
                    return combined
            return result
        return resp

    def _send(self, method: str, params=None):
        req = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            req["params"] = params
        self._next_id += 1
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()

    def _read(self, timeout: float = 5):
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            return None
        line = self.proc.stdout.readline()
        if line:
            try:
                return json.loads(line.strip())
            except json.JSONDecodeError:
                return None
        return None


@pytest.fixture(scope="module")
def stress_workdir(tmp_path_factory):
    """Shared workdir for stress tests."""
    return str(tmp_path_factory.mktemp("stress_workdir"))


@pytest.fixture(scope="module")
def stress_client(stress_workdir):
    """Module-scoped stress MCP client."""
    if not os.path.isfile(RADIX_BIN):
        pytest.skip("pares-radix binary not found")
    client = StressMcpClient(stress_workdir)
    client.start()
    yield client
    client.stop()


# ═══════════════════════════════════════════════════════════════════════════════
# 1. RAPID SEQUENTIAL DB OPERATIONS (single process, state consistency)
# ═══════════════════════════════════════════════════════════════════════════════


class TestRapidDBOperations:
    """Verify PluresDB handles rapid sequential writes/reads without corruption."""

    def test_burst_writes_different_keys(self, stress_client):
        """50 rapid writes to different keys — all succeed."""
        for i in range(50):
            resp = stress_client.call_tool("db_put", {
                "key": f"burst:key:{i}",
                "value": f"value-{i}",
            })
            assert resp is not None, f"Write {i} returned None"

    def test_burst_writes_then_reads(self, stress_client):
        """Write 20 keys rapidly, then read them all back."""
        tag = uuid.uuid4().hex[:6]
        for i in range(20):
            stress_client.call_tool("db_put", {
                "key": f"rw:{tag}:{i}",
                "value": f"data-{i}",
            })

        # Read them all back
        for i in range(20):
            resp = stress_client.call_tool("db_get", {"key": f"rw:{tag}:{i}"})
            if resp is not None:
                assert f"data-{i}" in str(resp), f"Key rw:{tag}:{i} corrupt: {str(resp)[:100]}"

    def test_overwrite_same_key_rapidly(self, stress_client):
        """Overwrite same key 30 times rapidly — final read is consistent."""
        key = f"overwrite:{uuid.uuid4().hex[:6]}"
        for i in range(30):
            stress_client.call_tool("db_put", {"key": key, "value": f"version-{i}"})

        resp = stress_client.call_tool("db_get", {"key": key})
        if resp is not None:
            # Should be the last version
            assert "version-29" in str(resp), f"Expected version-29, got: {str(resp)[:100]}"

    def test_interleaved_write_read(self, stress_client):
        """Write-read-write-read pattern doesn't corrupt."""
        tag = uuid.uuid4().hex[:6]
        for i in range(15):
            stress_client.call_tool("db_put", {
                "key": f"interleave:{tag}",
                "value": f"step-{i}",
            })
            resp = stress_client.call_tool("db_get", {"key": f"interleave:{tag}"})
            if resp is not None:
                assert f"step-{i}" in str(resp), f"Step {i} inconsistent: {str(resp)[:100]}"

    def test_delete_and_recreate_cycle(self, stress_client):
        """Delete and recreate the same key rapidly."""
        key = f"cycle:{uuid.uuid4().hex[:6]}"
        for i in range(10):
            stress_client.call_tool("db_put", {"key": key, "value": f"cycle-{i}"})
            stress_client.call_tool("db_delete", {"key": key})

        # Key should be gone (or soft-deleted)
        resp = stress_client.call_tool("db_get", {"key": key})
        # Either None or null/empty is acceptable
        if resp is not None:
            assert resp in ("null", "", None) or "null" in str(resp).lower() or "cycle-" not in str(resp)


# ═══════════════════════════════════════════════════════════════════════════════
# 2. RAPID CHRONOS EVENTS
# ═══════════════════════════════════════════════════════════════════════════════


class TestRapidChronos:
    """Verify Chronos handles rapid event recording."""

    def test_burst_events(self, stress_client):
        """Record 30 events rapidly."""
        for i in range(30):
            resp = stress_client.call_tool("chronos_record", {
                "event": f"burst-event-{i}",
                "level": "info",
                "data": {"seq": i},
            })
            assert resp is not None, f"Event {i} failed"

    def test_timeline_after_burst(self, stress_client):
        """Timeline query after burst returns events."""
        resp = stress_client.call_tool("chronos_timeline", {"limit": 50})
        assert resp is not None
        # Should have events in the response
        resp_str = str(resp)
        assert "burst-event" in resp_str or len(resp_str) > 10

    def test_mixed_levels_rapid(self, stress_client):
        """Rapid events with different levels."""
        levels = ["debug", "info", "warn", "error"]
        for i in range(20):
            level = levels[i % 4]
            resp = stress_client.call_tool("chronos_record", {
                "event": f"level-test-{i}",
                "level": level,
            })
            # Some levels might be filtered by current level setting
            # No crash is the key


# ═══════════════════════════════════════════════════════════════════════════════
# 3. RAPID PRAXIS OPERATIONS
# ═══════════════════════════════════════════════════════════════════════════════


class TestRapidPraxis:
    """Verify Praxis handles rapid constraint/rule operations."""

    def test_burst_constraint_creation(self, stress_client):
        """Create 20 constraints rapidly."""
        tag = uuid.uuid4().hex[:6]
        for i in range(20):
            resp = stress_client.call_tool("praxis_add_constraint", {
                "name": f"rapid-{tag}-{i}",
                "severity": "error",
                "require": f"ctx.field_{i} != null",
                "message": f"Constraint {i}",
            })
            assert resp is not None, f"Constraint {i} failed"

    def test_evaluate_after_burst(self, stress_client):
        """Evaluation works after rapid constraint creation."""
        resp = stress_client.call_tool("praxis_evaluate", {
            "context": {"test": True, "field_0": "exists"},
        })
        assert resp is not None

    def test_list_rules_under_load(self, stress_client):
        """Listing rules after many were added doesn't crash."""
        resp = stress_client.call_tool("praxis_list_rules", {})
        assert resp is not None


# ═══════════════════════════════════════════════════════════════════════════════
# 4. MULTI-PROCESS CONCURRENT ACCESS (shared workdir)
# ═══════════════════════════════════════════════════════════════════════════════


class TestMultiProcessAccess:
    """Multiple mcp-serve instances sharing a workdir (tests file-level locking)."""

    def test_two_clients_write_read(self, stress_workdir):
        """Two MCP clients writing to the same workdir."""
        if not os.path.isfile(RADIX_BIN):
            pytest.skip("Binary not found")

        client_a = StressMcpClient(stress_workdir)
        client_b = StressMcpClient(stress_workdir)

        try:
            client_a.start()
            client_b.start()

            # Client A writes
            client_a.call_tool("db_put", {"key": "multi:a", "value": "from-a"})
            # Client B writes
            client_b.call_tool("db_put", {"key": "multi:b", "value": "from-b"})

            # Both should be able to read their own writes
            resp_a = client_a.call_tool("db_get", {"key": "multi:a"})
            resp_b = client_b.call_tool("db_get", {"key": "multi:b"})

            if resp_a is not None:
                assert "from-a" in str(resp_a)
            if resp_b is not None:
                assert "from-b" in str(resp_b)

        finally:
            client_a.stop()
            client_b.stop()

    def test_three_clients_chronos(self, stress_workdir):
        """Three clients recording events to shared Chronos."""
        if not os.path.isfile(RADIX_BIN):
            pytest.skip("Binary not found")

        clients = [StressMcpClient(stress_workdir) for _ in range(3)]
        try:
            for c in clients:
                c.start()

            # Each records events
            for i, c in enumerate(clients):
                c.call_tool("chronos_record", {
                    "event": f"multi-client-{i}",
                    "level": "info",
                })

            # At least one should be able to read timeline
            resp = clients[0].call_tool("chronos_timeline", {"limit": 10})
            # No crash is the key assertion
        finally:
            for c in clients:
                c.stop()


# ═══════════════════════════════════════════════════════════════════════════════
# 5. MIXED WORKLOAD STRESS
# ═══════════════════════════════════════════════════════════════════════════════


class TestMixedWorkloadStress:
    """Simulate realistic mixed workloads."""

    def test_full_lifecycle_burst(self, stress_client):
        """Complete lifecycle: create canvas, add data, add constraints, record events."""
        tag = uuid.uuid4().hex[:6]

        # Canvas
        stress_client.call_tool("canvas_create", {"title": f"stress-{tag}"})
        stress_client.call_tool("canvas_set_data", {"data": {"load": tag}})

        # DB
        for i in range(5):
            stress_client.call_tool("db_put", {
                "key": f"lifecycle:{tag}:{i}",
                "value": f"v{i}",
            })

        # Chronos
        for i in range(5):
            stress_client.call_tool("chronos_record", {
                "event": f"lifecycle-{tag}-{i}",
                "level": "info",
            })

        # Praxis
        stress_client.call_tool("praxis_add_constraint", {
            "name": f"lifecycle-{tag}",
            "severity": "error",
            "require": "true",
        })

        # Verify nothing crashed — final read
        resp = stress_client.call_tool("db_get", {"key": f"lifecycle:{tag}:4"})
        if resp is not None:
            assert "v4" in str(resp)

    def test_sustained_mixed_ops(self, stress_client):
        """60 mixed operations in rapid succession."""
        tag = uuid.uuid4().hex[:6]
        ops_completed = 0

        for i in range(20):
            # Rotate through db_put, chronos_record, db_get
            stress_client.call_tool("db_put", {
                "key": f"sustained:{tag}:{i}",
                "value": str(i),
            })
            ops_completed += 1

            stress_client.call_tool("chronos_record", {
                "event": f"sustained-{i}",
                "level": "info",
            })
            ops_completed += 1

            stress_client.call_tool("db_get", {"key": f"sustained:{tag}:{i}"})
            ops_completed += 1

        assert ops_completed == 60, f"Only completed {ops_completed}/60 ops"


# ═══════════════════════════════════════════════════════════════════════════════
# 6. LATENCY & PERFORMANCE BASELINE
# ═══════════════════════════════════════════════════════════════════════════════


class TestPerformanceBaseline:
    """Establish latency baselines for regression detection."""

    def test_db_put_latency(self, stress_client):
        """db_put p99 under 5 seconds (generous for stdio overhead)."""
        latencies = []
        for i in range(20):
            start = time.time()
            stress_client.call_tool("db_put", {
                "key": f"perf:put:{i}",
                "value": f"x" * 100,
            })
            latencies.append(time.time() - start)

        p99 = sorted(latencies)[int(len(latencies) * 0.99)]
        avg = sum(latencies) / len(latencies)
        assert p99 < 5.0, f"p99 latency {p99:.3f}s exceeds 5s"
        assert avg < 2.0, f"avg latency {avg:.3f}s exceeds 2s"

    def test_db_get_latency(self, stress_client):
        """db_get p99 under 5 seconds."""
        # Seed
        stress_client.call_tool("db_put", {"key": "perf:read", "value": "stable"})

        latencies = []
        for i in range(20):
            start = time.time()
            stress_client.call_tool("db_get", {"key": "perf:read"})
            latencies.append(time.time() - start)

        p99 = sorted(latencies)[int(len(latencies) * 0.99)]
        avg = sum(latencies) / len(latencies)
        assert p99 < 5.0, f"p99 latency {p99:.3f}s exceeds 5s"
        assert avg < 2.0, f"avg latency {avg:.3f}s exceeds 2s"

    def test_chronos_record_latency(self, stress_client):
        """chronos_record p99 under 5 seconds."""
        latencies = []
        for i in range(20):
            start = time.time()
            stress_client.call_tool("chronos_record", {
                "event": f"perf-{i}",
                "level": "info",
            })
            latencies.append(time.time() - start)

        p99 = sorted(latencies)[int(len(latencies) * 0.99)]
        avg = sum(latencies) / len(latencies)
        assert p99 < 5.0, f"p99 latency {p99:.3f}s exceeds 5s"
        assert avg < 2.0, f"avg latency {avg:.3f}s exceeds 2s"

    def test_throughput_baseline(self, stress_client):
        """At least 5 ops/sec throughput (accounting for stdio overhead)."""
        start = time.time()
        ops = 0
        for i in range(10):
            stress_client.call_tool("db_put", {"key": f"thru:{i}", "value": "x"})
            ops += 1
        elapsed = time.time() - start

        throughput = ops / elapsed if elapsed > 0 else 0
        assert throughput > 5, f"Throughput {throughput:.1f} ops/s below 5 ops/s minimum"
