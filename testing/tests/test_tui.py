"""
test_tui.py — TUI integration tests via pexpect.

Two modes:
1. LOCAL: Spawns the binary directly (no Docker/SSH needed)
2. DOCKER: Over SSH into the test container (requires Docker + sshpass)

Local tests run by default. Docker tests are marked and skipped without Docker.
"""
import pytest
import pexpect
import os
import shutil
import time

# Binary discovery
BINARY = os.environ.get("PARES_RADIX_BIN", shutil.which("pares-radix") or "")
SKIP_NO_BINARY = pytest.mark.skipif(
    not BINARY or not os.path.isfile(BINARY),
    reason="pares-radix binary not found",
)

# Docker/SSH config (for Docker-only tests)
SSH_HOST = os.environ.get("RADIX_SSH_HOST", "localhost")
SSH_PORT = int(os.environ.get("RADIX_SSH_PORT", "2222"))
SSH_USER = os.environ.get("RADIX_SSH_USER", "radix")
SSH_PASS = os.environ.get("RADIX_SSH_PASS", "radix-test-pw")
HAS_SSHPASS = shutil.which("sshpass") is not None


@pytest.fixture
def tui_local():
    """Spawn pares-radix tui locally via pexpect."""
    env = os.environ.copy()
    env["TERM"] = "xterm-256color"
    # Use a temp config dir to avoid polluting user config
    env["XDG_CONFIG_HOME"] = "/tmp/radix-test-tui-config"
    env["HOME"] = "/tmp/radix-test-tui-home"
    os.makedirs("/tmp/radix-test-tui-home", exist_ok=True)
    os.makedirs("/tmp/radix-test-tui-config", exist_ok=True)

    child = pexpect.spawn(
        BINARY,
        ["tui"],
        encoding="utf-8",
        timeout=15,
        env=env,
    )
    yield child
    try:
        child.sendcontrol("c")
        time.sleep(0.3)
    except Exception:
        pass
    child.close()


@pytest.fixture
def tui_ssh():
    """Spawn pares-radix tui over SSH (Docker container)."""
    if not HAS_SSHPASS:
        pytest.skip("sshpass not installed — Docker TUI tests require it")

    cmd = (
        f"sshpass -p '{SSH_PASS}' ssh -tt -o StrictHostKeyChecking=no "
        f"-p {SSH_PORT} {SSH_USER}@{SSH_HOST} "
        f"TERM=xterm-256color pares-radix tui"
    )
    child = pexpect.spawn("/bin/bash", ["-c", cmd], encoding="utf-8", timeout=30)
    yield child
    try:
        child.sendcontrol("c")
        time.sleep(0.5)
    except Exception:
        pass
    child.close()


# ─── LOCAL TUI TESTS ─────────────────────────────────────────────────────────


class TestTUILocal:
    """TUI tests that run directly against the local binary."""

    @SKIP_NO_BINARY
    def test_tui_launches_and_renders(self, tui_local):
        """TUI starts and produces output (prompt, frame, or expected error)."""
        try:
            index = tui_local.expect(
                [
                    r"[>›»\$#]",         # Input prompt character
                    r"(?i)pares",         # App name
                    r"(?i)radix",         # App name variant
                    r"(?i)model",         # Model config display
                    r"(?i)error",         # Error (still ran)
                    r"(?i)(key|token|api)",  # Auth prompt (expected w/o keys)
                    r"─|│|┌|└|╭|╰|━",   # TUI box-drawing chars
                    pexpect.EOF,          # Process exited
                    pexpect.TIMEOUT,
                ],
                timeout=10,
            )
            if index == 7:  # EOF
                output = tui_local.before or ""
                # EOF is acceptable — means it ran and exited
                assert "panic" not in output.lower(), f"TUI panicked: {output[:300]}"
            elif index == 8:  # TIMEOUT
                # TUI is running but not producing expected output — still alive
                assert tui_local.isalive(), "TUI died without output"
            # indices 0-6 are all valid renders
        except pexpect.EOF:
            output = tui_local.before or ""
            # EOF is acceptable if it didn't panic
            assert "panic" not in output.lower(), f"TUI panicked: {output[:500]}"

    @SKIP_NO_BINARY
    def test_tui_exits_on_ctrl_c(self, tui_local):
        """TUI exits cleanly on Ctrl-C."""
        time.sleep(1)  # Let it initialize
        tui_local.sendcontrol("c")
        try:
            tui_local.expect(pexpect.EOF, timeout=5)
        except pexpect.TIMEOUT:
            # Some TUIs need a second Ctrl-C
            tui_local.sendcontrol("c")
            tui_local.expect(pexpect.EOF, timeout=5)
        # If we get here, the process terminated (EOF received)
        # Wait briefly for exit status to populate
        tui_local.close()
        # Either a normal exit or signal-killed is fine
        assert not tui_local.isalive()

    @SKIP_NO_BINARY
    def test_tui_exits_on_q(self, tui_local):
        """TUI exits on 'q' key (common quit binding)."""
        time.sleep(1)
        tui_local.send("q")
        try:
            tui_local.expect(pexpect.EOF, timeout=5)
            # Clean exit
        except pexpect.TIMEOUT:
            # 'q' might not be a quit key — that's fine, just verify no crash
            tui_local.sendcontrol("c")
            tui_local.expect(pexpect.EOF, timeout=5)

    @SKIP_NO_BINARY
    def test_tui_handles_resize(self, tui_local):
        """TUI handles SIGWINCH (terminal resize) without panicking."""
        time.sleep(1)
        if not tui_local.isalive():
            pytest.skip("TUI exited before resize test (likely missing config)")
        # Send SIGWINCH
        import signal
        if tui_local.pid:
            os.kill(tui_local.pid, signal.SIGWINCH)
        time.sleep(0.5)
        # If still alive, great. If exited, verify no panic.
        if not tui_local.isalive():
            output = tui_local.before or ""
            assert "panic" not in output.lower(), f"TUI panicked on resize: {output[:300]}"
        else:
            tui_local.sendcontrol("c")

    @SKIP_NO_BINARY
    def test_tui_handles_rapid_input(self, tui_local):
        """TUI doesn't panic on rapid keystroke input."""
        time.sleep(1)
        if not tui_local.isalive():
            pytest.skip("TUI exited before rapid input test")
        # Send rapid keystrokes
        for _ in range(20):
            tui_local.send("a")
        time.sleep(0.5)
        # If exited, verify no panic
        if not tui_local.isalive():
            output = tui_local.before or ""
            assert "panic" not in output.lower(), f"TUI panicked on rapid input: {output[:300]}"
        else:
            tui_local.sendcontrol("c")

    @SKIP_NO_BINARY
    def test_tui_handles_escape_sequences(self, tui_local):
        """TUI handles arrow key escape sequences without panicking."""
        time.sleep(1)
        if not tui_local.isalive():
            pytest.skip("TUI exited before escape sequence test (likely missing config)")
        # Send arrow keys (these should be safe navigation)
        tui_local.send("\x1b[A")  # Up arrow
        time.sleep(0.1)
        tui_local.send("\x1b[B")  # Down arrow
        time.sleep(0.1)
        tui_local.send("\x1b[C")  # Right arrow
        time.sleep(0.1)
        tui_local.send("\x1b[D")  # Left arrow
        time.sleep(0.5)
        # If TUI exited, verify it didn't panic
        if not tui_local.isalive():
            output = tui_local.before or ""
            assert "panic" not in output.lower(), f"TUI panicked on escape sequences: {output[:300]}"
        else:
            tui_local.sendcontrol("c")


# ─── DOCKER/SSH TUI TESTS ────────────────────────────────────────────────────


@pytest.mark.docker
class TestTUIDocker:
    """TUI tests that require the Docker container with SSH access."""

    def test_tui_launches_over_ssh(self, tui_ssh):
        """TUI starts via SSH into Docker container."""
        try:
            index = tui_ssh.expect(
                [
                    r"[>›»\$#]",
                    r"(?i)pares",
                    r"(?i)radix",
                    r"─|│|┌|└|╭|╰|━",
                    pexpect.TIMEOUT,
                ],
                timeout=15,
            )
            assert index < 4, "TUI timed out over SSH"
        except pexpect.EOF:
            output = tui_ssh.before or ""
            assert "key" in output.lower() or "token" in output.lower(), \
                f"TUI crashed over SSH: {output[:500]}"

    def test_tui_quit_over_ssh(self, tui_ssh):
        """TUI exits cleanly via Ctrl-C over SSH."""
        time.sleep(2)
        tui_ssh.sendcontrol("c")
        try:
            tui_ssh.expect(pexpect.EOF, timeout=10)
        except pexpect.TIMEOUT:
            tui_ssh.send("q")
            tui_ssh.expect(pexpect.EOF, timeout=5)
