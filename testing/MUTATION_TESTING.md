# Mutation Testing with cargo-mutants

## Overview

We use [cargo-mutants](https://mutants.rs/) to find gaps in our test suite by verifying that mutations to the source code are detected by existing tests.

## Quick Start

```bash
# Run mutation testing on the praxis crate (focused)
cargo mutants --package pares-radix-praxis -f "crates/praxis/src/rule.rs" --timeout 60 -j 2

# Run on a specific file
cargo mutants -f "crates/praxis/src/factory.rs" --timeout 60

# List all possible mutants (dry run)
cargo mutants --package pares-radix-praxis --list

# Full workspace (very slow — ~2000+ mutants)
cargo mutants --timeout 120 -j 4
```

## Interpretation

- **Caught**: Mutant was killed by tests (good — tests detect the change)
- **Missed**: Mutant survived (bad — tests didn't detect the change)
- **Timeout**: Mutant caused tests to hang (usually means infinite loops — acceptable)
- **Unviable**: Mutant didn't compile (neutral)

## Focus Areas

Priority modules for mutation testing (high business logic density):
1. `crates/praxis/src/rule.rs` — constraint evaluation logic
2. `crates/praxis/src/factory.rs` — rule factory and evaluation composition
3. `crates/core/src/` — PluresDB operations, condition evaluation
4. `crates/mcp-server/src/` — MCP tool dispatch and response formatting

## CI Integration

Mutation testing is too slow for every PR. Run it:
- Weekly on `main` (scheduled workflow)
- On-demand when touching core logic
- Before major releases

## Results Location

Results are written to `mutants.out/` (gitignored).
