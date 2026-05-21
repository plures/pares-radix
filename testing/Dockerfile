# testing/Dockerfile — Real testing infrastructure for pares-radix
# Multi-stage: build all binaries, then slim runtime with SSH + TUI support
#
# Usage:
#   docker build -f testing/Dockerfile -t pares-radix-test .
#   docker run -d -p 2222:22 --env-file testing/.env pares-radix-test

# ─── Stage 1: Build ───────────────────────────────────────────────────────────
# Ubuntu 24.04 (glibc 2.39) required — ort-sys prebuilt ONNX Runtime objects
# reference __isoc23_strtoll/strtoull (glibc 2.38+), bookworm only has 2.36.
FROM ubuntu:24.04 AS builder

# Install build toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl build-essential pkg-config libssl-dev ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust via rustup
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.87.0
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY src-tauri/ src-tauri/

# Build the CLI binary (includes TUI and MCP server as library deps)
# Exclude tauri-app (requires system libs we don't need for testing)
RUN cargo build --release -p pares-radix-cli 2>&1 && \
    echo "BUILD OK"

# ─── Stage 2: Runtime ─────────────────────────────────────────────────────────
# Must match builder glibc (2.39) — bookworm-slim only has 2.36
FROM ubuntu:24.04

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    openssh-server \
    locales \
    ncurses-term \
    curl \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Generate locale for TUI
RUN sed -i '/en_US.UTF-8/s/^# //' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 TERM=xterm-256color

# SSH setup
RUN mkdir -p /run/sshd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config && \
    echo "AllowUsers radix" >> /etc/ssh/sshd_config && \
    # SSH setup
RUN mkdir -p /run/sshd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config && \
    sed -i 's/#PermitUserEnvironment.*/PermitUserEnvironment yes/' /etc/ssh/sshd_config && \
    echo "AllowUsers radix" >> /etc/ssh/sshd_config && \
    echo 'TERM=xterm-256color' >> /etc/environment

# Create test user
RUN useradd -m -s /bin/bash radix && \
    echo "radix:radix-test-pw" | chpasswd && \
    mkdir -p /home/radix/.config/pares-radix /home/radix/.local/share/pares-radix && \
    mkdir -p /home/radix/.ssh && \
    echo 'TERM=xterm-256color' > /home/radix/.ssh/environment && \
    chown -R radix:radix /home/radix/.ssh

# Copy binary from builder (single CLI binary with subcommands for serve/tui/mcp)
COPY --from=builder /app/target/release/pares-radix /usr/local/bin/pares-radix

# Copy config/personality
COPY config/ /etc/pares-radix/config/
COPY praxis/ /etc/pares-radix/praxis/

# Setup radix home
RUN cp -r /etc/pares-radix/config /home/radix/.config/pares-radix/ && \
    cp -r /etc/pares-radix/praxis /home/radix/.config/pares-radix/ && \
    chown -R radix:radix /home/radix

# Entrypoint: starts SSH + MCP server
COPY testing/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Health check: verify SSH is up and radix binary responds
HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
    CMD ssh -o StrictHostKeyChecking=no -o ConnectTimeout=2 radix@localhost -p 22 "pares-radix --version" || exit 1

EXPOSE 22 3100

ENTRYPOINT ["/entrypoint.sh"]
