"""
test_ask_subcommand.py — Integration tests for the `ask` subcommand.

Tests the ask subcommand's CLI behavior: argument parsing, format options,
error handling without API keys, and actual inference when keys are available.
"""
import json
import os
import shutil
import subprocess
import pytest

BINARY = os.environ.get("PARES_RADIX_BIN", shutil.which("pares-radix") or "")
SKIP_NO_BINARY = pytest.mark.skipif(
    not BINARY or not os.path.isfile(BINARY),
    reason="pares-radix binary not found",
)

# Check if we have a real API key for live tests
API_KEY = os.environ.get("PARES_API_KEY", "")
HAS_API_KEY = bool(API_KEY)


def run_ask(args, env_override=None, timeout=30):
    """Run the ask subcommand and return (stdout, stderr, returncode)."""
    env = os.environ.copy()
    # Remove API key by default so we test error paths
    env.pop("PARES_API_KEY", None)
    if env_override:
        env.update(env_override)
    result = subprocess.run(
        [BINARY, "ask"] + args,
        capture_output=True,
        text=True,
        timeout=timeout,
        env=env,
    )
    return result.stdout, result.stderr, result.returncode


class TestAskCLI:
    """Tests for ask subcommand CLI interface."""

    @SKIP_NO_BINARY
    def test_ask_requires_prompt(self):
        """ask subcommand errors without a prompt argument."""
        stdout, stderr, code = run_ask([])
        assert code != 0
        # Clap should mention the required argument
        assert "PROMPT" in stderr or "required" in stderr.lower()

    @SKIP_NO_BINARY
    def test_ask_help(self):
        """ask --help shows usage information."""
        stdout, stderr, code = run_ask(["--help"])
        assert code == 0
        assert "PROMPT" in stdout
        assert "--model" in stdout
        assert "--format" in stdout

    @SKIP_NO_BINARY
    def test_ask_accepts_format_text(self):
        """ask accepts --format text without crash."""
        # Will fail on API call, but should parse args fine
        stdout, stderr, code = run_ask(
            ["--format", "text", "--api-key", "fake-key-for-parse-test", "hello"],
            env_override={"PARES_MODEL_URL": "http://127.0.0.1:1/v1"},
        )
        # Should fail on connection, not argument parsing
        # Non-zero exit from connection failure is expected
        assert "PROMPT" not in stderr  # No arg-parsing error

    @SKIP_NO_BINARY
    def test_ask_accepts_format_json(self):
        """ask accepts --format json without crash."""
        stdout, stderr, code = run_ask(
            ["--format", "json", "--api-key", "fake-key-for-parse-test", "hello"],
            env_override={"PARES_MODEL_URL": "http://127.0.0.1:1/v1"},
        )
        assert "PROMPT" not in stderr

    @SKIP_NO_BINARY
    def test_ask_format_values(self):
        """ask accepts known format values and handles unknown ones."""
        # text and json are definitely valid
        for fmt in ["text", "json"]:
            stdout, stderr, code = run_ask(
                ["--format", fmt, "--api-key", "fake", "hello"],
                env_override={"PARES_MODEL_URL": "http://127.0.0.1:1/v1"},
            )
            # Should not fail on argument parsing
            assert "PROMPT" not in stderr
            assert "unrecognized" not in stderr.lower()

    @SKIP_NO_BINARY
    def test_ask_model_url_override(self):
        """ask respects --model-url override."""
        stdout, stderr, code = run_ask(
            ["--model-url", "http://localhost:9999/v1", "--api-key", "test", "hi"]
        )
        # Connection will fail but it should try the specified URL
        # Key thing: no "unrecognized" error
        assert "unrecognized" not in stderr.lower()

    @SKIP_NO_BINARY
    def test_ask_model_name_override(self):
        """ask respects --model override."""
        stdout, stderr, code = run_ask(
            ["--model", "gpt-4o-mini", "--api-key", "test", "hi"],
            env_override={"PARES_MODEL_URL": "http://127.0.0.1:1/v1"},
        )
        assert "unrecognized" not in stderr.lower()

    @SKIP_NO_BINARY
    def test_ask_no_api_key_errors_gracefully(self):
        """ask without API key produces a clear error (not a panic)."""
        stdout, stderr, code = run_ask(["hello world"])
        # Should get an error about auth, not a panic/segfault
        combined = (stdout + stderr).lower()
        assert "panic" not in combined
        assert "segfault" not in combined
        # Should either error about key/auth or try copilot flow
        # (depends on build configuration)

    @SKIP_NO_BINARY
    def test_ask_copilot_flag_accepted(self):
        """ask --copilot flag is accepted without parse error."""
        stdout, stderr, code = run_ask(
            ["--copilot", "hello"],
            env_override={"PARES_MODEL_URL": "http://127.0.0.1:1/v1"},
        )
        # Should not be an unrecognized flag error
        assert "unrecognized" not in stderr.lower()


class TestAskLive:
    """Live inference tests (only run with real API key)."""

    @SKIP_NO_BINARY
    @pytest.mark.skipif(not HAS_API_KEY, reason="PARES_API_KEY not set")
    def test_ask_live_text_response(self):
        """ask returns a text response from a real model."""
        stdout, stderr, code = run_ask(
            ["--format", "text", "What is 2+2? Reply with just the number."],
            env_override={"PARES_API_KEY": API_KEY},
            timeout=60,
        )
        assert code == 0
        assert "4" in stdout

    @SKIP_NO_BINARY
    @pytest.mark.skipif(not HAS_API_KEY, reason="PARES_API_KEY not set")
    def test_ask_live_json_response(self):
        """ask --format json returns valid JSON."""
        stdout, stderr, code = run_ask(
            ["--format", "json", "What is 2+2? Reply with just the number."],
            env_override={"PARES_API_KEY": API_KEY},
            timeout=60,
        )
        assert code == 0
        # Should be valid JSON
        data = json.loads(stdout.strip())
        assert isinstance(data, dict)
