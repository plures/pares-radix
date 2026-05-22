"""
Tests for match: step grammar termination fix.

The match: step previously used a greedy arm list `(match_arm ~ NEWLINE)+` that couldn't
terminate before the next step in a procedure. This caused parse failures when match: was
followed by any other step (call, emit, when, loop, parallel, try, return, abort).

Fix: Added `end` terminator to match: (consistent with when/loop/try/parallel blocks)
and negative lookahead `!("end")` in match_arm_list.
"""
import subprocess
import tempfile
import textwrap
from pathlib import Path

import pytest

BINARY = Path(__file__).parent.parent.parent / "target" / "release" / "pares-radix"
if not BINARY.exists():
    BINARY = Path(__file__).parent.parent.parent / "target" / "debug" / "pares-radix"


def px_check(source: str) -> subprocess.CompletedProcess:
    """Write source to temp file and run px check."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".px", delete=False) as f:
        f.write(textwrap.dedent(source))
        f.flush()
        return subprocess.run(
            [str(BINARY), "px", "check", f.name],
            capture_output=True,
            text=True,
            timeout=10,
        )


class TestMatchTermination:
    """Match: step can be followed by other steps when terminated with 'end'."""

    def test_match_then_call(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then call"
              match:
                $x == "a" -> handle_a
                $x == "b" -> handle_b
              end
              do_something {arg: "value"} -> $result
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_emit(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then emit"
              match:
                $x == "ok" -> handler
              end
              emit {status: "done"}
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_when(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then when"
              match:
                $s == "ok" -> handler
              end
              when $debug == "true":
                log {msg: "debug"}
              end
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_loop(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then loop"
              match:
                $mode == "batch" -> batch
              end
              loop times 3 as i:
                process {}
              end
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_parallel(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then parallel"
              match:
                $s == "fan" -> fan
              end
              parallel:
                branch a:
                  step_a {}
                end
                branch b:
                  step_b {}
                end
              end
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_try(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then try"
              match:
                $risk == "high" -> careful
              end
              try:
                risky_op {}
              catch:
                handle_error {}
              end
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_return(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then return"
              match:
                $x == "done" -> finished
              end
              return "completed"
        """)
        assert result.returncode == 0, result.stderr

    def test_match_then_abort(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match then abort"
              match:
                $x == "fatal" -> die
              end
              abort "failed"
        """)
        assert result.returncode == 0, result.stderr

    def test_match_between_steps(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match sandwiched"
              get_status {} -> $s
              match:
                $s == "ok" -> ok
                $s == "err" -> err
              end
              notify {done: "true"}
        """)
        assert result.returncode == 0, result.stderr

    def test_multiple_consecutive_matches(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "two match blocks"
              match:
                $phase == "1" -> one
                $phase == "2" -> two
              end
              match:
                $result == "ok" -> ok_handler
                $result == "fail" -> fail_handler
              end
              emit {done: "true"}
        """)
        assert result.returncode == 0, result.stderr

    def test_match_as_last_step(self):
        """Match as last step still works (backward compat)."""
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "match at end"
              match:
                $x == "1" -> one
                $x == "2" -> two
              end
        """)
        assert result.returncode == 0, result.stderr

    def test_match_single_arm(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "single arm match"
              match:
                $x == "only" -> handler
              end
              emit {result: "done"}
        """)
        assert result.returncode == 0, result.stderr

    def test_match_many_arms(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "many arms"
              match:
                $level == "trace" -> trace_handler
                $level == "debug" -> debug_handler
                $level == "info" -> info_handler
                $level == "warn" -> warn_handler
                $level == "error" -> error_handler
                $level == "fatal" -> fatal_handler
              end
              log_result {msg: "dispatched"}
        """)
        assert result.returncode == 0, result.stderr

    def test_match_with_complex_conditions(self):
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "complex conditions in arms"
              match:
                $x == "a" && $y == "b" -> both
                $x != "a" -> not_a
              end
              emit {matched: "true"}
        """)
        assert result.returncode == 0, result.stderr


class TestMatchWithoutEnd:
    """Match without 'end' should fail to parse when followed by steps."""

    def test_match_no_end_fails_with_next_step(self):
        """Without end terminator, next step causes parse error."""
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "no end"
              match:
                $x == "a" -> handle_a
              emit {bad: "true"}
        """)
        assert result.returncode != 0, "Should fail without 'end'"

    def test_match_no_end_at_eof_fails(self):
        """Match without end at end of file now fails (breaking change, requires end)."""
        result = px_check("""\
            procedure test:
              trigger: manual
              given: "no end at eof"
              match:
                $x == "a" -> handle_a
        """)
        assert result.returncode != 0, "Should fail without 'end' even at EOF"


class TestMatchLintIntegration:
    """Lint still works correctly with the new end-terminated match syntax."""

    def test_lint_triggers_file_parses(self):
        """The updated lint_triggers.px fixture parses correctly."""
        fixture = Path(__file__).parent.parent.parent / "crates" / "praxis" / "tests" / "fixtures" / "lint_triggers.px"
        if not fixture.exists():
            pytest.skip("Fixture not found")
        result = subprocess.run(
            [str(BINARY), "px", "check", str(fixture)],
            capture_output=True,
            text=True,
            timeout=10,
        )
        assert result.returncode == 0, result.stderr

    def test_incident_response_parses(self):
        """The updated incident-response.px example parses correctly."""
        example = Path(__file__).parent.parent.parent / "examples" / "px" / "incident-response.px"
        if not example.exists():
            pytest.skip("Example not found")
        result = subprocess.run(
            [str(BINARY), "px", "check", str(example)],
            capture_output=True,
            text=True,
            timeout=10,
        )
        assert result.returncode == 0, result.stderr
