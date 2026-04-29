# Dockerfile — Pares Agens multi-stage build
FROM rust:1.87-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build -p pares-agens-cli --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/pares-agens-cli /usr/local/bin/pares-agens
COPY config/personality/ /etc/pares-agens/personality/
ENV HOME=/home/test
RUN useradd -m test
USER test
RUN mkdir -p /home/test/.pares-agens && cp /etc/pares-agens/personality/*.md /home/test/.pares-agens/
ENTRYPOINT ["pares-agens"]
CMD ["--help"]
