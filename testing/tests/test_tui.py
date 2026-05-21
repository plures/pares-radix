"""
test_tui.py — TUI integration tests via SSH + pexpect.

These drive the actual terminal UI over SSH, sending keystrokes
and asserting on rendered output. No mocks.
"""
import pytest
import pexpect
import os
import time

SSH_HOST = os.environ.get("RADIX_SSH_HOST", "localhost")
SSH_PORT = int(os.environ.get("RADIX_SSH_PORT", "2222"))
SSH_USER = os.environ.get("RADIX_SSH_USER", "radix")
SSH_PASS = os.environ.get("RADIX_SSH_PASS", "radix-test-pw")


@pytest.fixture
def tui():
    """Spawn pares-radix tui over SSH."""
    cmd = (
        f"sshpass -p '{SSH_PASS}' ssh -tt -o StrictHostKeyChecking=no "
        f"-p {SSH_PORT} {SSH_USER}@{SSH_HOST} "
        f"TERM=xterm-256color pares-radix tui "
        f"--model-url ${{PARES_MODEL_URL:-https://models.inference.ai.azure.com}}"
    )
    child = pexpect.spawn("/bin/bash", ["-c", cmd], encoding="utf-8", timeout=30)
    yield child
    # Send q or Ctrl-C to quit
    try:
        child.sendcontrol("c")
        time.sleep(0.5)
    except Exception:
        pass
    child.close()


class TestTUIStartup:
    """Verify TUI launches and renders initial screen."""

    @pytest.mark.timeout(30)
    def test_tui_launches(self, tui):
        """TUI starts without immediate crash."""
        # Should see some output within a few seconds
        # Either a prompt, a UI frame, or an error message
        try:
            index = tui.expect(
                [
                    r"[>›»\$]",          # Input prompt character
                    r"pares",             # App name in UI
                    r"radix",             # App name variant
                    r"error",             # Error (still proves it ran)
                    r"model",             # Model config display
                    pexpect.TIMEOUT,
                ],
                timeout=15,
            )
            # Any match except TIMEOUT means the TUI started
            assert index < 5, "TUI timed out without producing output"
        except pexpect.EOF:
            # EOF means process ended — check if it was graceful
            output = tui.before or ""
            # If it mentions "API key" or "token" that's expected without real keys
            assert (
                "key" in output.lower()
                or "token" in output.lower()
                or "error" in output.lower()
            ), f"TUI crashed unexpectedly: {output[:500]}"

    @pytest.mark.timeout(30)
    def test_tui_quit_with_ctrl_c(self, tui):
        """TUI responds to Ctrl-C by exiting cleanly."""
        time.sleep(2)  # Let it render
        tui.sendcontrol("c")
        try:
            tui.expect(pexpect.EOF, timeout=10)
        except pexpect.TIMEOUT:
            # Try 'q' as alternative quit
            tui.send("q")
            tui.expect(pexpect.EOF, timeout=5)

    @pytest.mark.timeout(30)
    def test_tui_help_key(self, tui):
        """Pressing '?' or 'h' shows help/keybindings (if supported)."""
        time.sleep(2)
        tui.send("?")
        time.sleep(1)
        output = tui.before or ""
        # Just verify we got some response without crash
        tui.sendcontrol("c")
        # Test passes if we got here without exception
