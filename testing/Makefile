# testing/Makefile — Pares Radix testing convenience targets
#
# Usage:
#   make test          — Run all local tests (no Docker needed)
#   make test-docker   — Build image + run full E2E suite
#   make build         — Build the release binary
#   make clean         — Remove test artifacts

REPO_ROOT := $(shell cd .. && pwd)
BINARY := $(REPO_ROOT)/target/release/pares-radix
export RADIX_BINARY := $(BINARY)

.PHONY: test test-docker test-quick build clean docker-build docker-up docker-down

# ─── Local Testing (no Docker) ─────────────────────────────────────────────

build:
	cd $(REPO_ROOT) && cargo build --release -p pares-radix-cli
	@echo "Binary: $(BINARY)"
	@$(BINARY) --version

test: build
	pytest tests/ -v --tb=short -m "not docker" --timeout=30

test-cov: build
	pytest tests/ -v --tb=short -m "not docker" --timeout=30 --cov=tests --cov-report=term-missing --cov-report=html:htmlcov

test-quick:
	@test -f $(BINARY) || (echo "ERROR: Binary not found. Run 'make build' first."; exit 1)
	pytest tests/ -v --tb=short -m "not docker" --timeout=30 -x

# ─── Docker E2E Testing ───────────────────────────────────────────────────

docker-build:
	docker compose build pares-radix

docker-up: docker-build
	docker compose up -d pares-radix
	@echo "Waiting for service..."
	@for i in $$(seq 1 30); do \
		docker compose exec pares-radix pares-radix --version 2>/dev/null && break; \
		sleep 2; \
	done
	@echo "Service ready. SSH: ssh radix@localhost -p 2222 (password: radix-test)"

docker-down:
	docker compose --profile test down -v --remove-orphans

test-docker: docker-up
	docker compose --profile test build test-runner
	docker compose --profile test run --rm test-runner
	$(MAKE) docker-down

# ─── Utilities ─────────────────────────────────────────────────────────────

doctor:
	@bash scripts/doctor.sh

clean:
	rm -rf .pytest_cache tests/__pycache__
	docker compose --profile test down -v --remove-orphans 2>/dev/null || true

ssh:
	ssh -o StrictHostKeyChecking=no radix@localhost -p 2222
