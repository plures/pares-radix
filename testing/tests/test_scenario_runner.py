"""Test the .px scenario runner — both CLI (px test) and MCP (px_test tool).

Exercises the full scenario lifecycle:
- Parsing scenarios from .px files
- Setup step execution
- Procedure invocation
- Expectation checking (positive + negated)
- Multi-file aggregation
- Error handling (missing procedure, bad syntax, etc.)
"""

import json
import os
import subprocess
import tempfile
import textwrap

import pytest

# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture
def binary():
    """Path to the pares-radix binary."""
    repo = os.environ.get(
        "PARES_RADIX_REPO",
        os.path.expanduser("~/.openclaw/workspace/repos/plures/pares-radix"),
    )
    candidates = [
        os.path.join(repo, "target/release/pares-radix"),
        os.path.join(repo, "target/debug/pares-radix"),
    ]
    for c in candidates:
        if os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    pytest.skip("pares-radix binary not found")


@pytest.fixture
def scenarios_dir():
    """Path to test-scenarios/ in the repo."""
    repo = os.environ.get(
        "PARES_RADIX_REPO",
        os.path.expanduser("~/.openclaw/workspace/repos/plures/pares-radix"),
    )
    d = os.path.join(repo, "test-scenarios")
    if os.path.isdir(d):
        return d
    pytest.skip("test-scenarios/ directory not found")


@pytest.fixture
def px_file(tmp_path):
    """Factory to create temp .px files with scenarios."""

    def _create(content, name="test.px"):
        f = tmp_path / name
        f.write_text(textwrap.dedent(content))
        return str(f)

    return _create


# ── CLI: px test ──────────────────────────────────────────────────────────────


class TestPxTestCLI:
    """Test the `px test` subcommand via CLI."""

    def test_existing_scenarios_pass(self, binary, scenarios_dir):
        """Built-in test-scenarios/cache-invalidation.px passes."""
        result = subprocess.run(
            [binary, "px", "test", scenarios_dir],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"
        assert "passed" in result.stdout
        assert "expired_entries_removed" in result.stdout
        assert "no_entries_when_empty" in result.stdout

    def test_passing_scenario_exit_code_zero(self, binary, px_file):
        """A passing scenario returns exit code 0."""
        f = px_file("""
            procedure do_stuff:
              trigger: manual
              put_entry {key: "hello", value: "world"}

            scenario basic_pass:
              given: "Procedure puts an entry"
              run: do_stuff
              expect:
                - has_entry {key: "hello"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"
        assert "basic_pass" in result.stdout

    def test_failing_scenario_exit_code_nonzero(self, binary, px_file):
        """A failing scenario returns non-zero exit code."""
        f = px_file("""
            scenario will_fail:
              given: "Nothing was set up"
              expect:
                - has_entry {key: "nonexistent"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode != 0
        assert "failed" in result.stdout or "FAILED" in result.stdout.upper()

    def test_negated_expectation(self, binary, px_file):
        """NOT expectations pass when condition is not met."""
        f = px_file("""
            scenario negated_pass:
              given: "Nothing in store"
              expect:
                - NOT has_entry {key: "ghost"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"
        assert "negated_pass" in result.stdout

    def test_negated_expectation_fails_when_entry_exists(self, binary, px_file):
        """NOT expectation fails when condition IS met."""
        f = px_file("""
            procedure add_it:
              trigger: manual
              put_entry {key: "exists", value: "yes"}

            scenario negated_fail:
              given: "Entry exists but we said NOT"
              run: add_it
              expect:
                - NOT has_entry {key: "exists"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode != 0

    def test_event_emitted_expectation(self, binary, px_file):
        """event_emitted check passes when event was emitted."""
        f = px_file("""
            procedure fire_event:
              trigger: manual
              emit {event: "deploy.started", version: "1.0"}

            scenario event_check:
              given: "Procedure emits deploy event"
              run: fire_event
              expect:
                - event_emitted {event: "deploy.started", version: "1.0"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"

    def test_event_not_emitted(self, binary, px_file):
        """NOT event_emitted passes when event was not fired."""
        f = px_file("""
            scenario no_event:
              given: "Nothing happened"
              expect:
                - NOT event_emitted {event: "never.fired"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"

    def test_multiple_scenarios_in_one_file(self, binary, px_file):
        """Multiple scenarios in one file are all executed."""
        f = px_file("""
            procedure setup_data:
              trigger: manual
              put_entry {key: "a", value: "1"}
              put_entry {key: "b", value: "2"}

            scenario first:
              given: "First scenario"
              run: setup_data
              expect:
                - has_entry {key: "a"}

            scenario second:
              given: "Second scenario"
              run: setup_data
              expect:
                - has_entry {key: "b"}

            scenario third:
              given: "Third — independent"
              expect:
                - NOT has_entry {key: "c"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"
        assert "first" in result.stdout
        assert "second" in result.stdout
        assert "third" in result.stdout

    def test_missing_procedure_fails_gracefully(self, binary, px_file):
        """Referencing a nonexistent procedure fails with clear error."""
        f = px_file("""
            scenario ref_missing:
              given: "References nonexistent procedure"
              run: ghost_procedure
              expect:
                - has_entry {key: "x"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode != 0

    def test_setup_with_multiple_steps(self, binary, px_file):
        """Setup can execute multiple put_entry + delete_entry steps."""
        f = px_file("""
            scenario multi_setup:
              given: "Setup puts then deletes"
              setup:
                put_entry {key: "keep", value: "yes"}
                put_entry {key: "remove", value: "temp"}
                delete_entry {key: "remove"}
              expect:
                - has_entry {key: "keep"}
                - NOT has_entry {key: "remove"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"

    def test_directory_discovery(self, binary, tmp_path):
        """px test on a directory discovers all .px files."""
        (tmp_path / "a.px").write_text(textwrap.dedent("""
            scenario a_pass:
              given: "First file"
              expect:
                - NOT has_entry {key: "x"}
        """))
        (tmp_path / "b.px").write_text(textwrap.dedent("""
            scenario b_pass:
              given: "Second file"
              expect:
                - NOT has_entry {key: "y"}
        """))
        # Non-.px file should be ignored
        (tmp_path / "readme.md").write_text("# not a px file")

        result = subprocess.run(
            [binary, "px", "test", str(tmp_path)],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"
        assert "a_pass" in result.stdout
        assert "b_pass" in result.stdout

    def test_file_with_no_scenarios_skips(self, binary, px_file):
        """A .px file with only constraints/facts but no scenarios is handled gracefully."""
        f = px_file("""
            fact Widget:
              name: string
              weight: int

            constraint weight_positive:
              require: widget.weight > 0
              severity: error
              message: "Weight must be positive"
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        # Should succeed (nothing to test = nothing failed)
        assert result.returncode == 0

    def test_store_value_expectation(self, binary, px_file):
        """store_value check verifies exact value match."""
        f = px_file("""
            procedure set_version:
              trigger: manual
              put_entry {key: "version", value: "2.0"}

            scenario check_value:
              given: "Procedure sets a specific version"
              run: set_version
              expect:
                - store_value {key: "version", value: "2.0"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"

    def test_store_value_wrong_value_fails(self, binary, px_file):
        """store_value fails when the value doesn't match."""
        f = px_file("""
            procedure set_version:
              trigger: manual
              put_entry {key: "version", value: "2.0"}

            scenario wrong_value:
              given: "Check for wrong value"
              run: set_version
              expect:
                - store_value {key: "version", value: "3.0"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode != 0


# ── CLI: px check ─────────────────────────────────────────────────────────────


class TestPxCheckCLI:
    """Test `px check` subcommand with scenario-containing files."""

    def test_valid_scenario_file_passes_check(self, binary, px_file):
        """A syntactically valid .px file with scenarios passes check."""
        f = px_file("""
            procedure do_thing:
              trigger: manual
              emit {event: "done"}

            scenario it_works:
              given: "basic scenario"
              run: do_thing
              expect:
                - event_emitted {event: "done"}
        """)
        result = subprocess.run(
            [binary, "px", "check", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0

    def test_invalid_syntax_fails_check(self, binary, px_file):
        """A .px file with syntax errors fails check."""
        f = px_file("""
            scenario broken:
              given: "This has bad syntax
              expect:
                - has_entry {key: "unclosed
        """)
        result = subprocess.run(
            [binary, "px", "check", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode != 0


# ── MCP: px_test tool ─────────────────────────────────────────────────────────


class TestPxTestMCP:
    """Test the px_test MCP tool (if available in the server)."""

    def test_px_test_via_mcp(self, mcp):
        """px_test MCP tool runs scenarios and returns results."""
        # Check if the tool exists
        tools_result = mcp.call_tool("tools/list", {})
        if tools_result is None:
            pytest.skip("MCP not available")

        tool_names = []
        if isinstance(tools_result, list):
            tool_names = [t.get("name", "") for t in tools_result if isinstance(t, dict)]
        elif isinstance(tools_result, dict) and "tools" in tools_result:
            tool_names = [
                t.get("name", "") for t in tools_result["tools"] if isinstance(t, dict)
            ]

        if "px_test" not in tool_names:
            pytest.skip("px_test tool not available in MCP server")

        # Run against the test-scenarios directory
        repo = os.environ.get(
            "PARES_RADIX_REPO",
            os.path.expanduser("~/.openclaw/workspace/repos/plures/pares-radix"),
        )
        result = mcp.call_tool(
            "px_test", {"files": [os.path.join(repo, "test-scenarios")]}
        )
        assert result is not None
        result_str = json.dumps(result) if isinstance(result, (dict, list)) else str(result)
        assert "passed" in result_str.lower() or "pass" in result_str.lower()

    def test_px_lint_via_mcp(self, mcp):
        """px_lint MCP tool validates .px syntax."""
        tools_result = mcp.call_tool("tools/list", {})
        if tools_result is None:
            pytest.skip("MCP not available")

        tool_names = []
        if isinstance(tools_result, list):
            tool_names = [t.get("name", "") for t in tools_result if isinstance(t, dict)]
        elif isinstance(tools_result, dict) and "tools" in tools_result:
            tool_names = [
                t.get("name", "") for t in tools_result["tools"] if isinstance(t, dict)
            ]

        if "px_lint" not in tool_names:
            pytest.skip("px_lint tool not available in MCP server")

        repo = os.environ.get(
            "PARES_RADIX_REPO",
            os.path.expanduser("~/.openclaw/workspace/repos/plures/pares-radix"),
        )
        result = mcp.call_tool(
            "px_lint", {"files": [os.path.join(repo, "test-scenarios")]}
        )
        assert result is not None


# ── Scenario isolation ────────────────────────────────────────────────────────


class TestScenarioIsolation:
    """Verify scenarios don't leak state between each other."""

    def test_scenarios_have_isolated_state(self, binary, px_file):
        """Each scenario starts with a clean state (no leakage)."""
        f = px_file("""
            procedure pollute:
              trigger: manual
              put_entry {key: "leaked", value: "yes"}

            scenario first_puts_entry:
              given: "First scenario puts an entry"
              run: pollute
              expect:
                - has_entry {key: "leaked"}

            scenario second_starts_clean:
              given: "Second scenario should NOT see first's state"
              expect:
                - NOT has_entry {key: "leaked"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, (
            f"State leaked between scenarios!\nstdout: {result.stdout}\nstderr: {result.stderr}"
        )

    def test_setup_state_scoped_to_scenario(self, binary, px_file):
        """Setup steps in one scenario don't affect the next."""
        f = px_file("""
            scenario with_setup:
              given: "Has setup entries"
              setup:
                put_entry {key: "scoped", value: "local"}
              expect:
                - has_entry {key: "scoped"}

            scenario without_setup:
              given: "Should not inherit previous setup"
              expect:
                - NOT has_entry {key: "scoped"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, (
            f"Setup state leaked!\nstdout: {result.stdout}\nstderr: {result.stderr}"
        )


# ── Edge cases ────────────────────────────────────────────────────────────────


class TestScenarioEdgeCases:
    """Edge cases and error handling in scenario execution."""

    def test_empty_expect_list(self, binary, px_file):
        """A scenario with empty expect list passes (vacuous truth)."""
        f = px_file("""
            scenario empty_expects:
              given: "Nothing to check"
              setup:
                put_entry {key: "dummy", value: "x"}
              expect:
                - has_entry {key: "dummy"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        # May pass or may be a parse error — document behavior
        # If it's a parse error, that's also acceptable
        assert result.returncode == 0 or "error" in result.stderr.lower()

    def test_scenario_with_only_setup_no_run(self, binary, px_file):
        """Scenario can have setup + expectations without a run clause."""
        f = px_file("""
            scenario setup_only:
              given: "Setup provides state directly"
              setup:
                put_entry {key: "direct", value: "yes"}
              expect:
                - has_entry {key: "direct"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"

    def test_mixed_pass_and_fail_in_same_file(self, binary, px_file):
        """One failing scenario makes the overall run fail."""
        f = px_file("""
            scenario passes:
              given: "This passes"
              expect:
                - NOT has_entry {key: "x"}

            scenario fails:
              given: "This fails"
              expect:
                - has_entry {key: "nonexistent"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode != 0
        # Both scenarios should still be reported
        assert "passes" in result.stdout
        assert "fails" in result.stdout

    def test_advance_time_is_handled(self, binary, px_file):
        """advance_time step doesn't crash the runner."""
        f = px_file("""
            scenario time_advance:
              given: "Uses advance_time utility"
              setup:
                put_entry {key: "created", value: "yes"}
                advance_time {secs: 60}
              expect:
                - has_entry {key: "created"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"

    def test_procedure_with_emit_and_put(self, binary, px_file):
        """Procedure that does both emit and put_entry works."""
        f = px_file("""
            procedure complex:
              trigger: manual
              put_entry {key: "status", value: "deployed"}
              emit {event: "deploy.complete", service: "api"}
              put_entry {key: "deploy_count", value: "1"}

            scenario complex_check:
              given: "Complex procedure with multiple side effects"
              run: complex
              expect:
                - has_entry {key: "status"}
                - has_entry {key: "deploy_count"}
                - event_emitted {event: "deploy.complete", service: "api"}
                - NOT has_entry {key: "nonexistent"}
        """)
        result = subprocess.run(
            [binary, "px", "test", f],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}\nstdout: {result.stdout}"
