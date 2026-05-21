"""
test_praxis_constraints.py — Verify .px constraint files are valid and parseable.

Tests that the Praxis constraint files in the repo are well-formed:
- Valid syntax (no missing fields)
- Constraints have required fields (when, require, severity, message)
- No duplicate constraint names
- Procedures have valid structure

This runs locally, parsing the .px files directly. No Docker needed.
"""
import os
import re
import pytest
from pathlib import Path


REPO_ROOT = Path(__file__).parent.parent.parent
PRAXIS_DIR = REPO_ROOT / "praxis"
CONFIG_DIR = REPO_ROOT / "config"


def find_px_files():
    """Find all .px files in praxis/ and config/ directories."""
    px_files = []
    for search_dir in [PRAXIS_DIR, CONFIG_DIR]:
        if search_dir.exists():
            for f in search_dir.rglob("*.px"):
                px_files.append(f)
    return px_files


def parse_constraints(content: str) -> list[dict]:
    """Parse constraint blocks from .px file content."""
    constraints = []
    # Pattern: constraint <name>:
    #   field: value
    constraint_pattern = re.compile(
        r"^constraint\s+(\w+):\s*$", re.MULTILINE
    )

    for match in constraint_pattern.finditer(content):
        name = match.group(1)
        start = match.end()
        # Find the block (indented lines following the constraint declaration)
        block_lines = []
        for line in content[start:].split("\n"):
            if line.strip() == "":
                continue
            if line.startswith("  ") or line.startswith("\t"):
                block_lines.append(line.strip())
            else:
                break

        fields = {}
        for bline in block_lines:
            if ":" in bline:
                key, _, val = bline.partition(":")
                fields[key.strip()] = val.strip()

        constraints.append({"name": name, "fields": fields, "raw_block": block_lines})
    return constraints


def parse_procedures(content: str) -> list[dict]:
    """Parse procedure blocks from .px file content."""
    procedures = []
    proc_pattern = re.compile(r"^procedure\s+(\w+):\s*$", re.MULTILINE)
    for match in proc_pattern.finditer(content):
        name = match.group(1)
        procedures.append({"name": name})
    return procedures


class TestPxFileDiscovery:
    """Verify .px files exist in the repo."""

    def test_praxis_dir_exists(self):
        """praxis/ directory exists."""
        assert PRAXIS_DIR.exists(), f"Expected praxis/ at {PRAXIS_DIR}"

    def test_px_files_present(self):
        """At least one .px file exists."""
        px_files = find_px_files()
        assert len(px_files) > 0, "No .px files found in praxis/ or config/"

    def test_foundational_engineering_exists(self):
        """Core foundational-engineering.px exists."""
        f = PRAXIS_DIR / "foundational-engineering.px"
        assert f.exists(), f"Missing {f}"

    def test_test_first_exists(self):
        """test-first.px exists."""
        f = PRAXIS_DIR / "test-first.px"
        assert f.exists(), f"Missing {f}"


class TestConstraintSyntax:
    """Validate constraint syntax in .px files."""

    @pytest.fixture
    def all_constraints(self):
        """Parse all constraints from all .px files."""
        all_c = []
        for px_file in find_px_files():
            content = px_file.read_text()
            constraints = parse_constraints(content)
            for c in constraints:
                c["file"] = str(px_file.relative_to(REPO_ROOT))
            all_c.extend(constraints)
        return all_c

    def test_constraints_have_severity(self, all_constraints):
        """Every constraint declares a severity."""
        for c in all_constraints:
            assert "severity" in c["fields"], (
                f"Constraint '{c['name']}' in {c['file']} missing 'severity' field"
            )

    def test_constraints_have_message(self, all_constraints):
        """Every constraint declares a message."""
        for c in all_constraints:
            assert "message" in c["fields"], (
                f"Constraint '{c['name']}' in {c['file']} missing 'message' field"
            )

    def test_constraints_have_when_or_require(self, all_constraints):
        """Hand-written constraints have at least 'when' or 'require'.
        
        Auto-generated constraints (plures-dev-guide.px) may be advisory-only
        with just severity + message, which is valid in the .px grammar.
        """
        # Skip auto-generated files that are advisory-only constraints
        generated_files = {"plures-dev-guide.px"}
        for c in all_constraints:
            if any(g in c["file"] for g in generated_files):
                continue
            has_when = "when" in c["fields"]
            has_require = "require" in c["fields"]
            assert has_when or has_require, (
                f"Constraint '{c['name']}' in {c['file']} has neither 'when' nor 'require'"
            )

    def test_no_duplicate_names_per_file(self):
        """No duplicate constraint names within a single file."""
        for px_file in find_px_files():
            content = px_file.read_text()
            constraints = parse_constraints(content)
            names = [c["name"] for c in constraints]
            dupes = [n for n in names if names.count(n) > 1]
            assert not dupes, (
                f"Duplicate constraint names in {px_file.name}: {set(dupes)}"
            )

    def test_severity_values_valid(self, all_constraints):
        """Severity is one of: error, warning, info."""
        valid = {"error", "warning", "info"}
        for c in all_constraints:
            sev = c["fields"].get("severity", "").strip('"').strip("'")
            if sev:
                assert sev in valid, (
                    f"Constraint '{c['name']}' has invalid severity '{sev}' "
                    f"(expected one of {valid})"
                )


class TestPxFileIntegrity:
    """Validate overall .px file structure."""

    @pytest.mark.parametrize("px_file", find_px_files(), ids=lambda f: f.name)
    def test_file_not_empty(self, px_file):
        """Each .px file has meaningful content."""
        content = px_file.read_text()
        # Strip comments and blank lines
        meaningful = [
            line
            for line in content.split("\n")
            if line.strip() and not line.strip().startswith("#")
        ]
        assert len(meaningful) > 0, f"{px_file.name} is empty (only comments)"

    @pytest.mark.parametrize("px_file", find_px_files(), ids=lambda f: f.name)
    def test_file_utf8(self, px_file):
        """Each .px file is valid UTF-8."""
        try:
            px_file.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            pytest.fail(f"{px_file.name} is not valid UTF-8")

    @pytest.mark.parametrize("px_file", find_px_files(), ids=lambda f: f.name)
    def test_no_trailing_whitespace_on_declarations(self, px_file):
        """Constraint/procedure declarations don't have extraneous whitespace issues."""
        content = px_file.read_text()
        for i, line in enumerate(content.split("\n"), 1):
            if line.startswith("constraint ") or line.startswith("procedure "):
                # Should end with ':'
                stripped = line.rstrip()
                assert stripped.endswith(":"), (
                    f"{px_file.name}:{i} — declaration doesn't end with ':': {line!r}"
                )
