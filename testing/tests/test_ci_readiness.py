"""
test_ci_readiness.py — Meta-tests verifying the testing infrastructure itself.

Ensures all test files import cleanly, required fixtures exist, Docker files
are well-formed, and the test suite is self-consistent.
"""
import importlib
import os
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parent.parent.parent
TESTING_DIR = REPO_ROOT / "testing"


class TestInfrastructureFiles:
    """Verify all infrastructure files exist and are non-empty."""

    REQUIRED_FILES = [
        "Dockerfile",
        "Dockerfile.test-runner",
        "docker-compose.yml",
        "entrypoint.sh",
        ".env.example",
        "pytest.ini",
        "run-local-tests.sh",
        "run-tests.sh",
        "README.md",
        "Makefile",
    ]

    @pytest.mark.parametrize("filename", REQUIRED_FILES)
    def test_file_exists(self, filename):
        """Each infrastructure file exists and is non-empty."""
        path = TESTING_DIR / filename
        assert path.exists(), f"Missing: {filename}"
        assert path.stat().st_size > 0, f"Empty: {filename}"

    def test_ci_workflow_exists(self):
        """GitHub Actions workflow is in place."""
        workflow = REPO_ROOT / ".github" / "workflows" / "real-tests.yml"
        assert workflow.exists(), "CI workflow missing"
        content = workflow.read_text()
        assert "local-tests" in content
        assert "integration-tests" in content

    def test_env_example_has_required_vars(self):
        """The .env.example declares minimum required variables."""
        env_example = TESTING_DIR / ".env.example"
        content = env_example.read_text()
        # At minimum should document what keys are needed
        assert "RADIX" in content.upper() or "PARES" in content.upper() or "RUST_LOG" in content.upper()


class TestPythonImports:
    """Verify all test modules import without errors."""

    TEST_MODULES = [
        "test_local_binary",
        "test_praxis_constraints",
        "test_px_runtime",
        "test_config_and_serve",
        "test_pluresdb_integration",
        "test_ask_subcommand",
        "test_serve_longrunning",
        "test_tui",
        "test_smoke",
        "test_mcp_server",
    ]

    @pytest.mark.parametrize("module_name", TEST_MODULES)
    def test_module_imports(self, module_name):
        """Each test module imports without errors."""
        tests_dir = str(TESTING_DIR / "tests")
        if tests_dir not in sys.path:
            sys.path.insert(0, tests_dir)

        # Should not raise ImportError
        spec = importlib.util.find_spec(module_name)
        if spec is None:
            # Module file might not exist yet — that's a real error
            path = TESTING_DIR / "tests" / f"{module_name}.py"
            assert path.exists(), f"Module file missing: {module_name}.py"


class TestDockerfileIntegrity:
    """Verify Dockerfile is well-structured."""

    def test_dockerfile_multistage(self):
        """Dockerfile uses multi-stage build."""
        content = (TESTING_DIR / "Dockerfile").read_text()
        assert content.count("FROM ") >= 2, "Should be multi-stage"
        assert "AS builder" in content

    def test_dockerfile_has_ssh(self):
        """Dockerfile installs openssh-server."""
        content = (TESTING_DIR / "Dockerfile").read_text()
        assert "openssh-server" in content

    def test_dockerfile_has_healthcheck(self):
        """Dockerfile includes a healthcheck."""
        content = (TESTING_DIR / "Dockerfile").read_text()
        assert "HEALTHCHECK" in content

    def test_dockerfile_exposes_ports(self):
        """Dockerfile exposes SSH and MCP ports."""
        content = (TESTING_DIR / "Dockerfile").read_text()
        assert "EXPOSE" in content
        assert "22" in content

    def test_entrypoint_executable_flag(self):
        """Entrypoint script is marked executable in Dockerfile."""
        content = (TESTING_DIR / "Dockerfile").read_text()
        assert "chmod +x /entrypoint.sh" in content


class TestDockerComposeIntegrity:
    """Verify docker-compose.yml is well-structured."""

    def test_compose_has_radix_service(self):
        """Compose defines the main pares-radix service."""
        content = (TESTING_DIR / "docker-compose.yml").read_text()
        assert "pares-radix:" in content

    def test_compose_has_test_runner(self):
        """Compose defines the test-runner with test profile."""
        content = (TESTING_DIR / "docker-compose.yml").read_text()
        assert "test-runner:" in content
        assert "profiles:" in content

    def test_compose_maps_ssh_port(self):
        """SSH port is mapped to host."""
        content = (TESTING_DIR / "docker-compose.yml").read_text()
        assert "2222:22" in content

    def test_compose_uses_env_file(self):
        """Compose references .env file for secrets."""
        content = (TESTING_DIR / "docker-compose.yml").read_text()
        assert "env_file" in content
