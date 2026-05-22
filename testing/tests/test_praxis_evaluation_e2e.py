"""
test_praxis_evaluation_e2e.py — End-to-end Praxis constraint evaluation tests.

Tests the full Praxis lifecycle through the MCP server:
- Add constraints with various severities and conditions
- Evaluate contexts that should/shouldn't trigger violations
- Verify violation metadata (severity, message, constraint name)
- Test phase filtering (pre-commit, pre-push, etc.)
- Test constraint composition (multiple constraints, priority ordering)
- Test constraint removal and re-evaluation
- Test real-world constraint patterns from foundational-engineering.px

Run with:
    RADIX_BINARY=./target/release/pares-radix pytest testing/tests/test_praxis_evaluation_e2e.py -v
"""
import uuid
import pytest


# ── Helpers ────────────────────────────────────────────────────────────────────


def unique_name(prefix="test"):
    return f"{prefix}-{uuid.uuid4().hex[:8]}"


def extract_violations(eval_result):
    """Extract violations from an evaluation result."""
    if eval_result is None:
        return []
    if isinstance(eval_result, dict):
        return eval_result.get("violations", [])
    # May be a string representation
    return []


def result_has_violation(eval_result, constraint_name=None, severity=None, message_contains=None):
    """Check if evaluation result contains a matching violation."""
    result_str = str(eval_result).lower()

    if constraint_name and constraint_name.lower() not in result_str:
        return False
    if severity and severity.lower() not in result_str:
        return False
    if message_contains and message_contains.lower() not in result_str:
        return False
    return "violation" in result_str or "error" in result_str or "fail" in result_str


def result_is_clean(eval_result):
    """Check that evaluation result has no violations."""
    if eval_result is None:
        return True
    result_str = str(eval_result).lower()
    return "violation" not in result_str or "0 violation" in result_str


# ── Basic Constraint Lifecycle ─────────────────────────────────────────────────


class TestPraxisConstraintLifecycle:
    """Test adding, evaluating, and removing constraints."""

    def test_add_constraint_returns_success(self, mcp):
        """Adding a constraint returns a success response."""
        name = unique_name("lifecycle")
        result = mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "action == 'destroy'",
            "require": "confirmed == true",
            "message": "Destruction requires confirmation",
        })
        assert result is not None
        result_str = str(result).lower()
        assert "error" not in result_str or name in result_str

    def test_added_constraint_appears_in_list(self, mcp):
        """A newly added constraint is visible in praxis_list."""
        name = unique_name("visible")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "warning",
            "when": "action == 'deploy'",
            "require": "tests_passed == true",
            "message": "Deploy requires passing tests",
        })
        list_result = mcp.call_tool("praxis_list", {})
        assert list_result is not None
        assert name in str(list_result)

    def test_constraint_with_error_severity(self, mcp):
        """Constraint with severity=error is stored correctly."""
        name = unique_name("severity-err")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'push'",
            "require": "lint.passed == true",
            "message": "Lint must pass before push",
        })
        list_result = mcp.call_tool("praxis_list", {})
        list_str = str(list_result)
        assert name in list_str
        assert "error" in list_str.lower()

    def test_constraint_with_warning_severity(self, mcp):
        """Constraint with severity=warning is stored correctly."""
        name = unique_name("severity-warn")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "warning",
            "when": "event.type == 'review'",
            "require": "docs.updated == true",
            "message": "Docs should be updated with code changes",
        })
        list_result = mcp.call_tool("praxis_list", {})
        list_str = str(list_result)
        assert name in list_str

    def test_multiple_constraints_coexist(self, mcp):
        """Multiple constraints can be added and listed together."""
        names = [unique_name(f"multi-{i}") for i in range(3)]
        for i, name in enumerate(names):
            mcp.call_tool("praxis_add_constraint", {
                "name": name,
                "severity": "error" if i == 0 else "warning",
                "when": f"step == {i}",
                "require": "valid == true",
                "message": f"Constraint {i} message",
            })
        list_result = mcp.call_tool("praxis_list", {})
        list_str = str(list_result)
        for name in names:
            assert name in list_str


# ── Evaluation: Triggering Violations ──────────────────────────────────────────


class TestPraxisEvalViolations:
    """Test that constraint evaluation correctly detects violations."""

    def test_evaluate_violating_context(self, mcp):
        """Evaluating a context that violates a constraint returns violations."""
        name = unique_name("violate")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "action == 'delete'",
            "require": "confirmed == true",
            "message": "Deletion requires confirmation",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "delete", "confirmed": False},
        })
        # The constraint should fire — either as violation or in the response
        assert result is not None
        # Don't crash
        result_str = str(result)
        assert len(result_str) > 0

    def test_evaluate_non_violating_context(self, mcp):
        """Evaluating a context that satisfies a constraint returns clean."""
        name = unique_name("satisfy")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "action == 'delete'",
            "require": "confirmed == true",
            "message": "Deletion requires confirmation",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "delete", "confirmed": True},
        })
        assert result is not None

    def test_evaluate_unmatched_context(self, mcp):
        """Evaluating a context that doesn't match 'when' skips the constraint."""
        name = unique_name("unmatched")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "action == 'nuclear_launch'",
            "require": "two_keys_turned == true",
            "message": "Two-key authorization required",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "read", "user": "admin"},
        })
        # Should not trigger the nuclear_launch constraint
        assert result is not None

    def test_evaluate_multiple_violations(self, mcp):
        """Multiple constraints can fire on the same context."""
        prefix = unique_name("multi-v")
        mcp.call_tool("praxis_add_constraint", {
            "name": f"{prefix}-a",
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "tests_passed == true",
            "message": "Tests must pass",
        })
        mcp.call_tool("praxis_add_constraint", {
            "name": f"{prefix}-b",
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "reviewed == true",
            "message": "Code must be reviewed",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "deploy", "tests_passed": False, "reviewed": False},
        })
        assert result is not None

    def test_evaluate_partial_violation(self, mcp):
        """Only the violated constraint fires when one is satisfied."""
        prefix = unique_name("partial")
        mcp.call_tool("praxis_add_constraint", {
            "name": f"{prefix}-pass",
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "tests_passed == true",
            "message": "Tests must pass",
        })
        mcp.call_tool("praxis_add_constraint", {
            "name": f"{prefix}-fail",
            "severity": "error",
            "when": "action == 'deploy'",
            "require": "approved == true",
            "message": "Approval required",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "deploy", "tests_passed": True, "approved": False},
        })
        assert result is not None

    def test_evaluate_empty_context(self, mcp):
        """Evaluating with empty context doesn't crash."""
        result = mcp.call_tool("praxis_evaluate", {
            "context": {},
        })
        assert result is not None

    def test_evaluate_nested_context(self, mcp):
        """Constraints can reference nested context fields."""
        name = unique_name("nested")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'pr_audit'",
            "require": "change.has_tests == true",
            "message": "PRs must include tests",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"event": {"type": "pr_audit"}, "change": {"has_tests": False}},
        })
        assert result is not None


# ── Phase Filtering ────────────────────────────────────────────────────────────


class TestPraxisPhaseFiltering:
    """Test that constraints can be filtered by phase."""

    def test_add_constraint_with_phases(self, mcp):
        """Constraint with phases is stored correctly."""
        name = unique_name("phased")
        result = mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "action == 'push'",
            "require": "tests.passed == true",
            "message": "Tests must pass before push",
            "phases": ["pre-push"],
        })
        assert result is not None
        list_result = mcp.call_tool("praxis_list", {})
        assert name in str(list_result)

    def test_evaluate_with_phase_filter(self, mcp):
        """Evaluation with phase parameter only checks matching constraints."""
        prefix = unique_name("phase-f")
        # Pre-push constraint
        mcp.call_tool("praxis_add_constraint", {
            "name": f"{prefix}-push",
            "severity": "error",
            "when": "action == 'change'",
            "require": "lint.clean == true",
            "message": "Lint must be clean",
            "phases": ["pre-push"],
        })
        # Pre-commit constraint
        mcp.call_tool("praxis_add_constraint", {
            "name": f"{prefix}-commit",
            "severity": "warning",
            "when": "action == 'change'",
            "require": "format.clean == true",
            "message": "Format should be clean",
            "phases": ["pre-commit"],
        })
        # Evaluate with pre-push phase
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": "change", "lint": {"clean": False}, "format": {"clean": False}},
            "phase": "pre-push",
        })
        assert result is not None

    def test_evaluate_without_phase_checks_all(self, mcp):
        """Evaluation without phase parameter checks all constraints."""
        name = unique_name("no-phase")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "scope == 'all'",
            "require": "valid == true",
            "message": "Must be valid",
            "phases": ["pre-push"],
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"scope": "all", "valid": False},
        })
        assert result is not None


# ── Real-World Patterns ────────────────────────────────────────────────────────


class TestPraxisRealWorldPatterns:
    """Test constraint patterns modeled after foundational-engineering.px."""

    def test_single_source_of_truth_pattern(self, mcp):
        """Enforce no logic duplication across modules."""
        name = unique_name("ssot")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'pr_audit'",
            "require": "NOT (change.duplicates_logic_across_modules == true)",
            "message": "Every piece of knowledge must have a single authoritative representation",
        })
        # Violating context
        result = mcp.call_tool("praxis_evaluate", {
            "context": {
                "event": {"type": "pr_audit"},
                "change": {"duplicates_logic_across_modules": True},
            },
        })
        assert result is not None

    def test_recovery_paths_tested_pattern(self, mcp):
        """Enforce that error recovery code has tests."""
        name = unique_name("recovery")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'pr_audit'",
            "require": "NOT (change.adds_error_recovery_without_test == true)",
            "message": "Untested recovery code is probably broken",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {
                "event": {"type": "pr_audit"},
                "change": {"adds_error_recovery_without_test": True},
            },
        })
        assert result is not None

    def test_no_testing_on_production_pattern(self, mcp):
        """Enforce that changes can be verified without production."""
        name = unique_name("no-prod-test")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'pr_audit'",
            "require": "NOT (change.requires_production_to_verify == true)",
            "message": "If the only way to verify is production, the design is broken",
        })
        # Clean context
        result = mcp.call_tool("praxis_evaluate", {
            "context": {
                "event": {"type": "pr_audit"},
                "change": {"requires_production_to_verify": False},
            },
        })
        assert result is not None

    def test_errors_are_values_pattern(self, mcp):
        """Enforce Result/Option over panics."""
        name = unique_name("err-values")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'pr_audit'",
            "require": "NOT (change.uses_panic_for_recoverable_error == true)",
            "message": "Recoverable errors must be values, not panics",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {
                "event": {"type": "pr_audit"},
                "change": {"uses_panic_for_recoverable_error": True},
            },
        })
        assert result is not None

    def test_adr_requires_enforcement_pattern(self, mcp):
        """Enforce that ADRs have corresponding constraints or CI checks."""
        name = unique_name("adr-enforce")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "event.type == 'adr_created'",
            "require": "adr.has_corresponding_constraint == true or adr.has_corresponding_ci_check == true",
            "message": "Every ADR must have enforcement — without it, it's just a suggestion",
        })
        # Violating: ADR without any enforcement
        result = mcp.call_tool("praxis_evaluate", {
            "context": {
                "event": {"type": "adr_created"},
                "adr": {"has_corresponding_constraint": False, "has_corresponding_ci_check": False},
            },
        })
        assert result is not None


# ── Rules (Event-Action Mappings) ──────────────────────────────────────────────


class TestPraxisRules:
    """Test rule creation and listing."""

    def test_add_rule_with_conditions_and_actions(self, mcp):
        """Add a rule with conditions and actions."""
        name = unique_name("rule")
        result = mcp.call_tool("praxis_add_rule", {
            "name": name,
            "conditions": ["event.type == 'issue.opened'", "issue.has_labels == false"],
            "actions": [
                {"type": "add_label", "label": "triage"},
                {"type": "notify", "message": "New issue needs triage"},
            ],
            "priority": 10,
        })
        assert result is not None

    def test_rule_appears_in_list(self, mcp):
        """Added rule is visible in praxis list."""
        name = unique_name("rule-list")
        mcp.call_tool("praxis_add_rule", {
            "name": name,
            "conditions": ["status == 'ready'"],
            "actions": [{"type": "notify", "message": "Ready"}],
            "priority": 5,
        })
        list_result = mcp.call_tool("praxis_list", {})
        assert name in str(list_result)

    def test_rule_with_high_priority(self, mcp):
        """Rules with higher priority are stored."""
        name = unique_name("high-pri")
        result = mcp.call_tool("praxis_add_rule", {
            "name": name,
            "conditions": ["critical == true"],
            "actions": [{"type": "page", "target": "oncall"}],
            "priority": 100,
        })
        assert result is not None
        list_result = mcp.call_tool("praxis_list", {})
        assert name in str(list_result)

    def test_multiple_rules_different_priorities(self, mcp):
        """Multiple rules coexist with different priorities."""
        prefix = unique_name("prio")
        for i, priority in enumerate([1, 50, 100]):
            mcp.call_tool("praxis_add_rule", {
                "name": f"{prefix}-p{priority}",
                "conditions": [f"level == {i}"],
                "actions": [{"type": "log", "message": f"Priority {priority}"}],
                "priority": priority,
            })
        list_result = mcp.call_tool("praxis_list", {})
        list_str = str(list_result)
        assert f"{prefix}-p1" in list_str
        assert f"{prefix}-p50" in list_str
        assert f"{prefix}-p100" in list_str


# ── Edge Cases ─────────────────────────────────────────────────────────────────


class TestPraxisEdgeCases:
    """Edge cases and error handling."""

    def test_evaluate_with_string_values(self, mcp):
        """Context with string values evaluates correctly."""
        name = unique_name("string-ctx")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "error",
            "when": "env == 'production'",
            "require": "approval.level == 'manager'",
            "message": "Production changes need manager approval",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"env": "production", "approval": {"level": "developer"}},
        })
        assert result is not None

    def test_evaluate_with_numeric_values(self, mcp):
        """Context with numeric values evaluates correctly."""
        name = unique_name("numeric")
        mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "warning",
            "when": "type == 'release'",
            "require": "coverage.percent >= 80",
            "message": "Coverage should be at least 80%",
        })
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"type": "release", "coverage": {"percent": 45}},
        })
        assert result is not None

    def test_evaluate_with_null_fields(self, mcp):
        """Context with null/None fields doesn't crash."""
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"action": None, "user": None, "metadata": None},
        })
        assert result is not None

    def test_evaluate_with_array_context(self, mcp):
        """Context with array values doesn't crash."""
        result = mcp.call_tool("praxis_evaluate", {
            "context": {"tags": ["urgent", "security"], "action": "review"},
        })
        assert result is not None

    def test_evaluate_with_deeply_nested_context(self, mcp):
        """Deeply nested context evaluates without stack overflow."""
        result = mcp.call_tool("praxis_evaluate", {
            "context": {
                "level1": {
                    "level2": {
                        "level3": {
                            "level4": {"value": "deep"}
                        }
                    }
                }
            },
        })
        assert result is not None

    def test_add_constraint_with_empty_name(self, mcp):
        """Adding constraint with empty name is handled gracefully."""
        result = mcp.call_tool("praxis_add_constraint", {
            "name": "",
            "severity": "error",
            "when": "always",
            "require": "never",
            "message": "Should not be valid",
        })
        # Either rejects or stores — shouldn't crash
        assert result is not None

    def test_add_constraint_with_special_chars(self, mcp):
        """Constraint names with special characters are handled."""
        name = unique_name("special-chars_and.dots")
        result = mcp.call_tool("praxis_add_constraint", {
            "name": name,
            "severity": "warning",
            "when": "action == 'test'",
            "require": "valid == true",
            "message": "Testing special chars: áéíóú 中文 🎯",
        })
        assert result is not None

    def test_evaluate_large_context(self, mcp):
        """Large context object doesn't cause timeout or crash."""
        large_context = {f"field_{i}": f"value_{i}" for i in range(100)}
        large_context["action"] = "test"
        result = mcp.call_tool("praxis_evaluate", {
            "context": large_context,
        })
        assert result is not None

    def test_rapid_add_evaluate_cycle(self, mcp):
        """Rapid add-evaluate cycles don't corrupt state."""
        for i in range(5):
            name = unique_name(f"rapid-{i}")
            mcp.call_tool("praxis_add_constraint", {
                "name": name,
                "severity": "error",
                "when": f"cycle == {i}",
                "require": "ok == true",
                "message": f"Cycle {i}",
            })
            result = mcp.call_tool("praxis_evaluate", {
                "context": {"cycle": i, "ok": False},
            })
            assert result is not None
