"""
test_smoke.py — Basic smoke tests for pares-radix in the container.

These verify the binary works, SSH is functional, and core commands respond.
No mocks. All tests run against the real running container.
Requires Docker (SSH into container).
"""
import pytest

pytestmark = pytest.mark.docker


class TestBinarySmoke:
    """Verify the pares-radix binary is operational."""

    def test_version(self, ssh_exec):
        """Binary reports its version without error."""
        stdout, stderr, code = ssh_exec("pares-radix --version")
        assert code == 0, f"--version failed: {stderr}"
        assert "pares-radix" in stdout.lower() or "pares" in stdout.lower()

    def test_help(self, ssh_exec):
        """Help text includes expected subcommands."""
        stdout, stderr, code = ssh_exec("pares-radix --help")
        assert code == 0, f"--help failed: {stderr}"
        # Should list known subcommands
        assert "serve" in stdout.lower() or "tui" in stdout.lower()

    def test_migrate_dry_run(self, ssh_exec):
        """Migrate --dry-run runs without crashing (no source needed)."""
        stdout, stderr, code = ssh_exec(
            "pares-radix migrate --dry-run --from /nonexistent --output /tmp/migrate-test"
        )
        # Should either succeed or fail gracefully (not segfault)
        assert code in (0, 1), f"Unexpected exit code {code}: {stderr}"


class TestEnvironment:
    """Verify the container environment is correct for TUI operation."""

    def test_locale_utf8(self, ssh_exec):
        """Locale is set to UTF-8 (required for TUI rendering)."""
        stdout, _, code = ssh_exec("locale | grep LANG")
        assert code == 0
        assert "UTF-8" in stdout or "utf8" in stdout.lower()

    def test_term_set(self, ssh_exec):
        """TERM is set to a color-capable value."""
        stdout, _, code = ssh_exec("echo $TERM")
        assert code == 0
        term = stdout.strip()
        assert term in ("xterm-256color", "xterm", "screen-256color", "tmux-256color")

    def test_binary_in_path(self, ssh_exec):
        """pares-radix is in PATH and executable."""
        stdout, _, code = ssh_exec("which pares-radix")
        assert code == 0
        assert "/pares-radix" in stdout

    def test_home_directory(self, ssh_exec):
        """User home exists with correct ownership."""
        stdout, _, code = ssh_exec("ls -la ~/ | head -5")
        assert code == 0
        assert "radix" in stdout


class TestSSHAccess:
    """Verify SSH server configuration for automation."""

    def test_password_auth(self, ssh_exec):
        """Password authentication works (already proven by fixture, but explicit)."""
        stdout, _, code = ssh_exec("whoami")
        assert code == 0
        assert stdout.strip() == "radix"

    def test_no_root_login(self, ssh_exec):
        """Root login is disabled in sshd config."""
        stdout, _, code = ssh_exec("grep -i 'PermitRootLogin' /etc/ssh/sshd_config")
        assert code == 0
        assert "no" in stdout.lower()
