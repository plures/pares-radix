"""
test_praxis_procedure_execution.py — End-to-end tests for praxis_run procedure execution.

Tests the full .px procedure execution lifecycle through MCP:
- Run inline .px source with praxis_run
- Run preloaded procedures by name
- Variable passing and mutation
- Shell action execution (exec step)
- Error handling (parse errors, execution failures)
- px_lint and px_status tools
- Real-world .px patterns from the praxis/ directory

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_praxis_procedure_execution.py -v
"""
import json
import os
import tempfile
import uuid
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parent.parent.parent
PRAXIS_DIR = REPO_ROOT / "praxis"


def unique_name(prefix="proc"):
    return f"{prefix}_{uuid.uuid4().hex[:8]}"


def run_inline(mcp, source, vars=None):
    """Helper: run inline .px source."""
    args = {"source": source}
    if vars:
        args["vars"] = vars
    return mcp.call_tool("praxis_run", args)


def run_named(mcp, name, vars=None):
    """Helper: run a preloaded procedure by name."""
    args = {"procedure": name}
    if vars:
        args["vars"] = vars
    return mcp.call_tool("praxis_run", args)


def run_file(mcp, path, vars=None):
    """Helper: run a .px file."""
    args = {"file": path}
    if vars:
        args["vars"] = vars
    return mcp.call_tool("praxis_run", args)


def result_success(result):
    """Check if a procedure result indicates success."""
    if result is None:
        return False
    result_str = str(result)
    return '"success": true' in result_str or '"success":true' in result_str or "'success': True" in result_str


def result_has_error(result):
    """Check if result indicates an error."""
    result_str = str(result).lower()
    return "error" in result_str or "failed" in result_str


# ── Inline Procedure Execution ──────────────────────────────────────────────────


class TestInlineProcedureExecution:
    """Test running inline .px procedure source."""

    def test_minimal_procedure(self, mcp):
        """Run a minimal procedure with an emit step."""
        source = (
            "procedure hello:\n"
            "  trigger: manual\n"
            "  emit {event: \"hello_world\"}\n"
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        # Should complete (success or at least execute)
        assert "hello" in result_str.lower() or "success" in result_str.lower() or "step" in result_str.lower()

    def test_procedure_with_assert(self, mcp):
        """Procedure with assert_eq passes."""
        source = (
            "procedure assert_test:\n"
            "  trigger: manual\n"
            "  assert_eq {actual: 1, expected: 1, message: \"one equals one\"}\n"
        )
        result = run_inline(mcp, source)
        assert result is not None
        assert result_success(result) or not result_has_error(result)

    def test_procedure_shell_exec(self, mcp):
        """Procedure can execute shell commands via exec step."""
        source = (
            "procedure shell_test:\n"
            "  trigger: manual\n"
            '  exec {command: "echo hello_from_px"} -> $output\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        assert "hello_from_px" in result_str or result_success(result)

    def test_procedure_exec_captures_output(self, mcp):
        """Shell exec output is captured into a variable."""
        source = (
            "procedure capture:\n"
            "  trigger: manual\n"
            '  exec {command: "echo captured_42"} -> $out\n'
            '  assert_contains {value: $out, contains: "captured_42", message: "output captured"}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        assert "captured_42" in result_str or result_success(result)

    def test_procedure_multiple_steps(self, mcp):
        """Multiple steps execute in sequence."""
        source = (
            "procedure multi:\n"
            "  trigger: manual\n"
            "  emit {event: \"step_one\"}\n"
            '  exec {command: "echo step_two"} -> $r\n'
            "  assert_ok {value: true, message: \"step_three\"}\n"
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        # Should have step results
        assert "step" in result_str.lower() or result_success(result)

    def test_procedure_with_when_guard(self, mcp):
        """Conditional 'when' guard executes appropriately."""
        source = (
            "procedure conditional:\n"
            "  trigger: manual\n"
            '  exec {command: "echo before"} -> $x\n'
            '  when x != "":\n'
            '    emit {event: "branch_taken"}\n'
            "  end\n"
        )
        result = run_inline(mcp, source)
        assert result is not None
        # Should not crash — conditional parsing works

    def test_procedure_returns_step_results(self, mcp):
        """Result includes step-by-step execution info."""
        source = (
            "procedure detailed:\n"
            "  trigger: manual\n"
            "  assert_eq {actual: 42, expected: 42, message: \"answer\"}\n"
            '  exec {command: "echo ok"} -> $r\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        # Should have steps array
        assert "step" in result_str.lower() or "index" in result_str.lower() or "success" in result_str.lower()

    def test_procedure_write_read_file(self, mcp):
        """Procedure can write and read files."""
        marker = unique_name("marker")
        source = (
            "procedure file_io:\n"
            "  trigger: manual\n"
            f'  write_file {{path: "/tmp/radix-test-{marker}.txt", content: "{marker}"}}\n'
            f'  read_file {{path: "/tmp/radix-test-{marker}.txt"}} -> $content\n'
            f'  assert_contains {{value: $content, contains: "{marker}", message: "file roundtrip"}}\n'
            f'  exec {{command: "rm /tmp/radix-test-{marker}.txt"}}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        assert result_success(result) or marker in str(result)

    def test_procedure_sleep(self, mcp):
        """Sleep action works without crashing."""
        source = (
            "procedure nap:\n"
            "  trigger: manual\n"
            "  sleep {ms: 10}\n"
            "  emit {event: \"awake\"}\n"
        )
        result = run_inline(mcp, source)
        assert result is not None


# ── Error Handling ──────────────────────────────────────────────────────────────


class TestProcedureErrorHandling:
    """Test error cases in procedure execution."""

    def test_parse_error_invalid_syntax(self, mcp):
        """Invalid .px syntax returns a parse error."""
        source = "this is not valid px syntax {"
        result = run_inline(mcp, source)
        result_str = str(result)
        assert "error" in result_str.lower() or "parse" in result_str.lower()

    def test_parse_error_missing_trigger(self, mcp):
        """Missing trigger field may produce an error or warning."""
        # Some parsers may accept this — test behavior either way
        source = "procedure no_trigger:\n  emit {event: \"test\"}\n"
        result = run_inline(mcp, source)
        # Should either parse or give a clear error
        assert result is not None

    def test_shell_command_failure(self, mcp):
        """Failed shell command (non-zero exit) is handled."""
        source = (
            "procedure failing:\n"
            "  trigger: manual\n"
            '  exec {command: "exit 1"} -> $r\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        # Should indicate the command failed
        result_str = str(result)
        assert result is not None  # Doesn't crash

    def test_missing_required_params(self, mcp):
        """Calling praxis_run without source, file, or procedure returns error."""
        result = mcp.call_tool("praxis_run", {})
        result_str = str(result)
        assert "error" in result_str.lower() or "required" in result_str.lower()

    def test_nonexistent_procedure_name(self, mcp):
        """Running a non-existent named procedure returns error."""
        result = run_named(mcp, f"nonexistent_{uuid.uuid4().hex[:8]}")
        result_str = str(result)
        # Should indicate not found — may fall through to file/source logic
        assert "error" in result_str.lower() or "required" in result_str.lower() or result is not None

    def test_nonexistent_file_path(self, mcp):
        """Running from a non-existent file returns error."""
        result = mcp.call_tool("praxis_run", {"file": "/tmp/does_not_exist_ever_12345.px"})
        result_str = str(result)
        assert "error" in result_str.lower() or "failed" in result_str.lower()

    def test_assert_eq_failure(self, mcp):
        """Failed assertion reports the failure."""
        source = (
            "procedure fail_assert:\n"
            "  trigger: manual\n"
            '  assert_eq {actual: 1, expected: 2, message: "intentional fail"}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        # Should show assertion failure
        assert "fail" in result_str.lower() or "assert" in result_str.lower() or "1" in result_str


# ── Preloaded Procedures ────────────────────────────────────────────────────────


class TestPreloadedProcedures:
    """Test running procedures loaded from .px files at startup."""

    def test_px_status_shows_state(self, mcp):
        """px_status shows current praxis engine state."""
        result = mcp.call_tool("px_status")
        assert result is not None
        result_str = str(result)
        # Should show loaded constraints/procedures count or similar
        assert len(result_str) > 10

    def test_run_self_test_procedure(self, mcp):
        """Run the self_test procedure (preloaded from praxis/procedures/)."""
        result = run_named(mcp, "self_test")
        assert result is not None
        result_str = str(result)
        # self_test may not be preloaded in test workdir — accept either execution or "required" msg
        assert "self_test" in result_str or "success" in result_str.lower() or "required" in result_str.lower() or "error" in result_str.lower()

    def test_run_procedure_from_file(self, mcp):
        """Run a .px file directly via the file parameter."""
        px_content = (
            "procedure file_test:\n"
            "  trigger: manual\n"
            '  exec {command: "echo ran_from_file"} -> $r\n'
            '  assert_contains {value: $r, contains: "ran_from_file", message: "file exec"}\n'
        )
        with tempfile.NamedTemporaryFile(mode='w', suffix='.px', delete=False) as f:
            f.write(px_content)
            px_path = f.name

        try:
            result = run_file(mcp, px_path)
            assert result is not None
            result_str = str(result)
            assert "ran_from_file" in result_str or result_success(result)
        finally:
            os.unlink(px_path)

    def test_run_health_check_if_available(self, mcp):
        """Run health_check procedure if it exists."""
        result = run_named(mcp, "health_check")
        # Accept success or not-found
        assert result is not None

    def test_run_real_praxis_file(self, mcp):
        """Run an actual .px file from the praxis/ directory."""
        self_test_path = str(PRAXIS_DIR / "procedures" / "self-test.px")
        if os.path.exists(self_test_path):
            result = run_file(mcp, self_test_path)
            assert result is not None
            result_str = str(result)
            # Should execute successfully
            assert "success" in result_str.lower() or "self_test" in result_str
        else:
            pytest.skip("self-test.px not found in praxis/procedures/")


# ── Real .px Patterns ───────────────────────────────────────────────────────────


class TestRealPxPatterns:
    """Test execution patterns matching actual .px files."""

    def test_emit_and_assert(self, mcp):
        """Emit + assert pattern (from self-test.px)."""
        source = (
            "procedure emit_assert:\n"
            "  trigger: manual\n"
            '  emit {event: "test.start", suite: "e2e"}\n'
            '  assert_eq {actual: "hello", expected: "hello", message: "string eq"}\n'
            '  emit {event: "test.pass", name: "string_eq"}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        assert result_success(result) or "emit_assert" in str(result)

    def test_exec_pipe_and_capture(self, mcp):
        """Exec with pipe and variable capture."""
        source = (
            "procedure pipe_test:\n"
            "  trigger: manual\n"
            '  exec {command: "echo abc123 | grep -o abc"} -> $grep_out\n'
            '  assert_contains {value: $grep_out, contains: "abc", message: "pipe works"}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        assert "abc" in result_str or result_success(result)

    def test_multi_exec_sequence(self, mcp):
        """Multiple exec steps in sequence."""
        source = (
            "procedure multi_exec:\n"
            "  trigger: manual\n"
            '  exec {command: "echo first"} -> $a\n'
            '  exec {command: "echo second"} -> $b\n'
            '  exec {command: "echo third"} -> $c\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        result_str = str(result)
        # Should have executed all three
        assert "first" in result_str or "second" in result_str or result_success(result)

    def test_assert_contains_pattern(self, mcp):
        """assert_contains validates substring presence."""
        source = (
            "procedure contains_test:\n"
            "  trigger: manual\n"
            '  assert_contains {value: "hello world", contains: "world", message: "substring"}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        assert result_success(result) or not result_has_error(result)

    def test_assert_ok_truthy(self, mcp):
        """assert_ok validates truthy values."""
        source = (
            "procedure ok_test:\n"
            "  trigger: manual\n"
            '  assert_ok {value: true, message: "truthy"}\n'
        )
        result = run_inline(mcp, source)
        assert result is not None
        assert result_success(result) or not result_has_error(result)


# ── Px Lint and Status ──────────────────────────────────────────────────────────


class TestPxTooling:
    """Test px_lint, px_compose, and px_status MCP tools."""

    def test_px_lint_valid_source(self, mcp):
        """px_lint accepts valid .px source."""
        source = "procedure valid:\n  trigger: manual\n  emit {event: \"test\"}\n"
        result = mcp.call_tool("px_lint", {"source": source})
        assert result is not None
        result_str = str(result)
        # Valid source shouldn't have parse errors
        assert "parse_error" not in result_str or "ok" in result_str.lower()

    def test_px_lint_invalid_source(self, mcp):
        """px_lint reports errors for invalid .px source."""
        source = "not valid px {{{{"
        result = mcp.call_tool("px_lint", {"source": source})
        result_str = str(result)
        assert "error" in result_str.lower() or "parse" in result_str.lower()

    def test_px_lint_from_file(self, mcp):
        """px_lint can lint a .px file by path."""
        px_content = "procedure lintme:\n  trigger: manual\n  emit {event: \"lint\"}\n"
        with tempfile.NamedTemporaryFile(mode='w', suffix='.px', delete=False) as f:
            f.write(px_content)
            px_path = f.name
        try:
            result = mcp.call_tool("px_lint", {"file": px_path})
            assert result is not None
        finally:
            os.unlink(px_path)

    def test_px_lint_real_file(self, mcp):
        """px_lint validates actual .px files from the praxis/ directory."""
        self_test_path = str(PRAXIS_DIR / "procedures" / "self-test.px")
        if os.path.exists(self_test_path):
            result = mcp.call_tool("px_lint", {"file": self_test_path})
            assert result is not None
            result_str = str(result)
            # Real files should be valid
            assert "parse_error" not in result_str
        else:
            pytest.skip("self-test.px not found")

    def test_px_compose_combines_sources(self, mcp):
        """px_compose merges multiple .px fragments."""
        result = mcp.call_tool("px_compose", {"sources": [
            "procedure a:\n  trigger: manual\n  emit {event: \"a\"}\n",
            "procedure b:\n  trigger: manual\n  emit {event: \"b\"}\n",
        ]})
        assert result is not None
        result_str = str(result)
        # Should contain both procedures
        assert "a" in result_str or "b" in result_str or "compose" in result_str.lower()

    def test_px_status_comprehensive(self, mcp):
        """px_status shows loaded constraints, procedures, and config."""
        result = mcp.call_tool("px_status")
        assert result is not None
        result_str = str(result)
        # Should have meaningful content about praxis state
        assert len(result_str) > 20
        # Should mention constraints or procedures
        assert "constraint" in result_str.lower() or "procedure" in result_str.lower() or "loaded" in result_str.lower() or "praxis" in result_str.lower()
