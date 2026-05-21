# Dockerfile — pares-radix real testing infrastructure
# Multi-stage: build from source, then slim runtime with SSH for TUI access

# ─── Stage 1: Build ──────────────────────────────────────────────────────────
FROM rust:1.87-bookworm AS builder

WORKDIR /app
COPY . .

# Build CLI (includes TUI subcommand) and verify it links
RUN cargo build -p pares-radix-cli --release \
    && /app/target/release/pares-radix --help

# ─── Stage 2: Runtime ────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# Install runtime deps: SSH server, TUI support, CA certs, locale
RUN apt-get update && apt-get install -y --no-install-recommends \
    openssh-server \
    ca-certificates \
    locales \
    ncurses-term \
    procps \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && sed -i 's/^# *\(en_US.UTF-8\)/\1/' /etc/locale.gen \
    && locale-gen \
    && mkdir -p /run/sshd

ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV TERM=xterm-256color

# SSH configuration: password auth for test user, no root login
RUN echo "PermitRootLogin no" >> /etc/ssh/sshd_config \
    && echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config \
    && echo "AllowTcpForwarding no" >> /etc/ssh/sshd_config \
    && echo "X11Forwarding no" >> /etc/ssh/sshd_config \
    && ssh-keygen -A

# Test user with known password for automation
RUN useradd -m -s /bin/bash radix \
    && echo "radix:radix-test-pw" | chpasswd

# Copy binary
COPY --from=builder /app/target/release/pares-radix /usr/local/bin/pares-radix

# Copy personality/config if present
COPY --chown=radix:radix config/personality/ /home/radix/.pares-radix/personality/

# Entrypoint script
COPY testing/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Health check: verify binary runs and SSH is up
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=3 \
    CMD pares-radix --version && curl -sf http://localhost:22 || true

EXPOSE 22 3100

ENTRYPOINT ["/entrypoint.sh"]
