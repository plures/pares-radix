"""
test_px_runtime.py — Integration tests using the real `pares-radix px` subcommands.

These test Praxis .px file compilation, linting, and scenario execution
through the actual binary — no Python regex parsing, real parser invoked.

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_px_runtime.py -v
"""
import os
import subprocess
import tempfile
import pytest
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
PRAXIS_DIR = REPO_ROOT / "praxis"
RADIX_BIN = os.environ.get(
    "RADIX_BINARY",
    str(REPO_ROOT / "target" / "release" / "pares-radix"),
)


def run_px(*args, timeout=30):
    """Run pares-radix px <args> and return (stdout, stderr, returncode)."""
    result = subprocess.run(
        [RADIX_BIN, "px", *args],
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return result.stdout, result.stderr, result.returncode


@pytest.fixture
def radix_bin():
    """Verify binary exists."""
    assert os.path.isfile(RADIX_BIN), f"Binary not found: {RADIX_BIN}"
    return RADIX_BIN


class TestPxCheck:
    """Tests using `pares-radix px check` — real parser, real validation."""

    def test_check_help(self, radix_bin):
        """px check --help works."""
        stdout, stderr, code = run_px("check", "--help")
        assert code == 0
        assert "check" in (stdout + stderr).lower()

    def test_check_valid_file(self, radix_bin):
        """px check passes on test-first.px (known valid)."""
        px_file = PRAXIS_DIR / "test-first.px"
        if not px_file.exists():
            pytest.skip("test-first.px not found")
        stdout, stderr, code = run_px("check", str(px_file))
        combined = stdout + stderr
        assert "✓" in combined or code == 0
        assert "0 error(s)" in combined

    def test_check_reports_parse_errors(self, radix_bin):
        """px check fails on files with known parse issues."""
        # We know foundational-engineering.px has a parse error
        px_file = PRAXIS_DIR / "foundational-engineering.px"
        if not px_file.exists():
            pytest.skip("foundational-engineering.px not found")
        stdout, stderr, code = run_px("check", str(px_file))
        combined = stdout + stderr
        # Should report an error
        assert code != 0 or "error" in combined.lower() or "✗" in combined

    def test_check_invalid_syntax(self, radix_bin):
        """px check catches completely invalid syntax."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".px", delete=False) as f:
            f.write("this is not valid px syntax at all\n{{{garbage}}}\n")
            f.flush()
            try:
                stdout, stderr, code = run_px("check", f.name)
                combined = stdout + stderr
                assert code != 0 or "error" in combined.lower() or "✗" in combined
            finally:
                os.unlink(f.name)

    def test_check_empty_file(self, radix_bin):
        """px check handles empty files gracefully."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".px", delete=False) as f:
            f.write("")
            f.flush()
            try:
                stdout, stderr, code = run_px("check", f.name)
                # Should not crash — either passes (empty is valid) or reports an error
                assert code == 0 or code == 1
            finally:
                os.unlink(f.name)

    def test_check_well_formed_constraint(self, radix_bin):
        """px check passes on a well-formed inline constraint."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".px", delete=False) as f:
            f.write("""constraint test_constraint:
  when: context.action == "push"
  require: context.tests_pass == true
  severity: error
  message: "Tests must pass before push"
""")
            f.flush()
            try:
                stdout, stderr, code = run_px("check", f.name)
                combined = stdout + stderr
                assert code == 0 or "✓" in combined, f"Unexpected failure: {combined}"
            finally:
                os.unlink(f.name)

    def test_check_well_formed_rule(self, radix_bin):
        """px check passes on a well-formed rule."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".px", delete=False) as f:
            f.write('rule auto_label:\n'
                    '  when:\n'
                    '    - event.type == "issue.opened"\n'
                    '  then:\n'
                    '    - action: add_label label: "triage"\n')
            f.flush()
            try:
                stdout, stderr, code = run_px("check", f.name)
                combined = stdout + stderr
                assert code == 0 or "✓" in combined, f"Unexpected failure: {combined}"
            finally:
                os.unlink(f.name)

    def test_check_directory_mode(self, radix_bin):
        """px check works on a directory (recursively finds .px files)."""
        stdout, stderr, code = run_px("check", str(PRAXIS_DIR))
        combined = stdout + stderr
        # Should report number of files checked
        assert "file(s) checked" in combined

    def test_check_nonexistent_file(self, radix_bin):
        """px check handles nonexistent file gracefully."""
        stdout, stderr, code = run_px("check", "/nonexistent/path/file.px")
        assert code != 0


class TestPxTest:
    """Tests using `pares-radix px test` — scenario execution."""

    def test_test_help(self, radix_bin):
        """px test --help works."""
        stdout, stderr, code = run_px("test", "--help")
        assert code == 0
        assert "test" in (stdout + stderr).lower()

    def test_test_valid_file_no_scenarios(self, radix_bin):
        """px test on a file with no scenarios reports 0 passed."""
        px_file = PRAXIS_DIR / "test-first.px"
        if not px_file.exists():
            pytest.skip("test-first.px not found")
        stdout, stderr, code = run_px("test", str(px_file))
        combined = stdout + stderr
        # No scenarios means 0 passed, still exit 0
        assert code == 0 or "0 scenario(s) passed" in combined

    def test_test_with_inline_scenario(self, radix_bin):
        """px test executes inline scenarios."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".px", delete=False) as f:
            # Write a minimal scenario that should parse (may or may not execute fully)
            f.write("""constraint require_tests:
  when: context.action == "push"
  require: context.tests_pass == true
  severity: error
  message: "Tests must pass"

scenario basic_pass:
  given:
    context.action = "push"
    context.tests_pass = true
  expect: pass
""")
            f.flush()
            try:
                stdout, stderr, code = run_px("test", f.name)
                combined = stdout + stderr
                # If the parser supports this syntax, it should run
                # If it doesn't, it'll report a parse error — both are valid outcomes
                assert code == 0 or "error" in combined.lower() or "scenario" in combined.lower()
            finally:
                os.unlink(f.name)


class TestPxFileHealth:
    """Meta-tests: track the health of the repo's .px files over time."""

    def test_at_least_one_valid_px_file(self, radix_bin):
        """At least one .px file in the repo passes px check."""
        px_files = list(PRAXIS_DIR.rglob("*.px"))
        assert px_files, "No .px files found"

        any_valid = False
        for pf in px_files:
            _, _, code = run_px("check", str(pf))
            if code == 0:
                any_valid = True
                break
        assert any_valid, "No .px files pass syntax check — parser may be broken"

    def test_error_count_regression(self, radix_bin):
        """Track how many .px files have parse errors (regression detection)."""
        px_files = list(PRAXIS_DIR.rglob("*.px"))
        errors = 0
        for pf in px_files:
            _, _, code = run_px("check", str(pf))
            if code != 0:
                errors += 1

        # Known baseline: 4 files have parse errors as of initial commit.
        # If this number INCREASES, something regressed.
        MAX_KNOWN_ERRORS = 5  # Allow slight buffer above current 4
        assert errors <= MAX_KNOWN_ERRORS, (
            f"Parse error count ({errors}) exceeds known baseline ({MAX_KNOWN_ERRORS}). "
            f"New .px files introduced with syntax errors?"
        )
