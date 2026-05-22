"""
test_cluster_subcommand.py — Tests for `pares-radix cluster` subcommand.

Validates cluster status, nodes, info, and workloads without network.
"""
import os
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parent.parent.parent
RADIX_BIN = str(REPO_ROOT / "target" / "release" / "pares-radix")


def run_radix(*args, timeout=15):
    """Run pares-radix with given args, return (stdout, stderr, code)."""
    result = subprocess.run(
        [RADIX_BIN, *args],
        capture_output=True, text=True, timeout=timeout,
    )
    return result.stdout, result.stderr, result.returncode


@pytest.fixture(scope="module")
def radix_bin():
    """Ensure binary exists."""
    if not os.path.isfile(RADIX_BIN):
        pytest.skip("Binary not built — run cargo build --release first")


class TestClusterStatus:
    """Verify `cluster status` subcommand."""

    def test_cluster_status_exits_cleanly(self, radix_bin):
        """cluster status exits with code 0."""
        stdout, stderr, code = run_radix("cluster", "status")
        assert code == 0, f"cluster status failed: {stderr}"

    def test_cluster_status_shows_node_count(self, radix_bin):
        """cluster status reports at least 1 node."""
        stdout, stderr, code = run_radix("cluster", "status")
        combined = stdout + stderr
        assert "node" in combined.lower()
        # At minimum, the local node is reported
        assert "1 node" in combined or "nodes" in combined.lower()

    def test_cluster_status_shows_resources(self, radix_bin):
        """cluster status reports CPU and RAM."""
        stdout, stderr, code = run_radix("cluster", "status")
        combined = stdout + stderr
        assert "core" in combined.lower() or "cpu" in combined.lower()
        assert "ram" in combined.lower() or "gb" in combined.lower()


class TestClusterNodes:
    """Verify `cluster nodes` subcommand."""

    def test_cluster_nodes_exits_cleanly(self, radix_bin):
        """cluster nodes exits with code 0."""
        stdout, stderr, code = run_radix("cluster", "nodes")
        assert code == 0, f"cluster nodes failed: {stderr}"

    def test_cluster_nodes_shows_local_node(self, radix_bin):
        """cluster nodes lists local node with online status."""
        stdout, stderr, code = run_radix("cluster", "nodes")
        combined = stdout + stderr
        assert "online" in combined.lower()

    def test_cluster_nodes_shows_capabilities(self, radix_bin):
        """cluster nodes reports hardware (CPU, RAM)."""
        stdout, stderr, code = run_radix("cluster", "nodes")
        combined = stdout + stderr
        assert "cpu" in combined.lower() or "core" in combined.lower()


class TestClusterInfo:
    """Verify `cluster info` subcommand."""

    def test_cluster_info_exits_cleanly(self, radix_bin):
        """cluster info exits with code 0."""
        stdout, stderr, code = run_radix("cluster", "info")
        assert code == 0, f"cluster info failed: {stderr}"

    def test_cluster_info_shows_os(self, radix_bin):
        """cluster info reports OS."""
        stdout, stderr, code = run_radix("cluster", "info")
        combined = stdout + stderr
        assert "linux" in combined.lower() or "os" in combined.lower()

    def test_cluster_info_shows_cpu_count(self, radix_bin):
        """cluster info shows CPU core count as positive integer."""
        stdout, stderr, code = run_radix("cluster", "info")
        combined = stdout + stderr
        # Should contain "CPU: N cores" or similar
        assert "cpu" in combined.lower()
        # Verify it detected at least 1 core
        import re
        cores = re.findall(r"(\d+)\s*core", combined.lower())
        assert len(cores) > 0
        assert int(cores[0]) >= 1

    def test_cluster_info_shows_gpu_status(self, radix_bin):
        """cluster info reports GPU presence (even if 'none')."""
        stdout, stderr, code = run_radix("cluster", "info")
        combined = stdout + stderr
        assert "gpu" in combined.lower()


class TestClusterWorkloads:
    """Verify `cluster workloads` subcommand."""

    def test_cluster_workloads_exits_cleanly(self, radix_bin):
        """cluster workloads exits with code 0."""
        stdout, stderr, code = run_radix("cluster", "workloads")
        assert code == 0, f"cluster workloads failed: {stderr}"

    def test_cluster_workloads_reports_empty(self, radix_bin):
        """cluster workloads reports no active workloads initially."""
        stdout, stderr, code = run_radix("cluster", "workloads")
        combined = stdout + stderr
        assert "no" in combined.lower() or "0" in combined


class TestClusterDeploy:
    """Verify `cluster deploy` with a .px file."""

    def test_cluster_deploy_nonexistent_file(self, radix_bin):
        """cluster deploy with missing file exits with error."""
        stdout, stderr, code = run_radix("cluster", "deploy", "/nonexistent.px")
        assert code != 0

    def test_cluster_deploy_with_px_file(self, radix_bin, tmp_path):
        """cluster deploy parses a .px file and reports results."""
        px_file = tmp_path / "test.px"
        px_file.write_text("""
constraint "test-constraint" {
  severity = "error"
  description = "test deployment constraint"
}
""")
        stdout, stderr, code = run_radix("cluster", "deploy", str(px_file))
        # Deploy may succeed or fail based on constraints but shouldn't crash
        assert code == 0 or "error" in (stdout + stderr).lower()
        assert "panic" not in (stdout + stderr).lower()
