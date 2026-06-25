# Mutation Testing with cargo-mutants

## Overview

We use [cargo-mutants](https://mutants.rs/) to find gaps in our test suite by verifying that mutations to the source code are detected by existing tests. A 100% kill rate on viable mutants means every logic branch is covered.

## Quick Start

```bash
# Run on all priority modules
./testing/scripts/run-mutations.sh all

# Run on a specific module
./testing/scripts/run-mutations.sh core-chronos
./testing/scripts/run-mutations.sh praxis-rule

# Available modules: praxis-rule, praxis-factory, core-chronos, core-procedure, core-memory

# Or run manually on any file
cargo mutants --package pares-agens-core -f "crates/core/src/chronos.rs" --timeout 120 -j 2

# List all possible mutants (dry run)
cargo mutants --package pares-radix-praxis --list

# Full workspace (very slow — ~2000+ mutants)
cargo mutants --timeout 120 -j 4
```

## CI Integration

### Scheduled (Weekly)
The `mutation-testing.yml` workflow runs every Sunday at 04:00 UTC against all priority modules in a matrix build. Results are uploaded as artifacts.

### On-Demand
Trigger manually via workflow_dispatch with optional `target_file` and `package` inputs for ad-hoc coverage checks.

### What Gets Tested
| Module | Package | File |
|--------|---------|------|
| praxis-rule | pares-radix-praxis | crates/praxis/src/rule.rs |
| praxis-factory | pares-radix-praxis | crates/praxis/src/factory.rs |
| core-chronos | pares-agens-core | crates/core/src/chronos.rs |
| core-procedure | pares-agens-core | crates/core/src/procedure.rs |
| core-memory | pares-agens-core | crates/core/src/memory/store.rs |
| privacy | pares-radix-privacy | crates/privacy/src/lib.rs |
| core-ledger | pares-agens-core | crates/core/src/praxis/ledger.rs |
| core-cerebellum | pares-agens-core | crates/core/src/cerebellum/pipeline.rs |
| core-forgetting | pares-agens-core | crates/core/src/memory/forgetting/engine.rs |
| modules-safety | pares-radix-praxis | crates/praxis/src/modules/safety.rs |
| px-compiler | pares-radix-praxis | crates/praxis/src/px/compiler.rs |
| px-lint | pares-radix-praxis | crates/praxis/src/px/lint.rs |
| px-scenario-runner | pares-radix-praxis | crates/praxis/src/px/scenario_runner.rs |
| px-resolver | pares-radix-praxis | crates/praxis/src/px/resolver.rs |
| px-compose | pares-radix-praxis | crates/praxis/src/px/compose.rs |
| tool-governance | pares-agens-core | crates/core/src/tool_governance.rs |

## Interpretation

- **Caught**: Mutant was killed by tests ✅ (good — tests detect the change)
- **Missed**: Mutant survived ❌ (bad — tests didn't detect the change)  
- **Timeout**: Mutant caused tests to hang ⏱️ (usually infinite loops — acceptable)
- **Unviable**: Mutant didn't compile 🚫 (neutral — type system caught it)

## Results

### Verified (as of 2026-05-31)
| File | Caught | Missed | Unviable | Kill Rate |
|------|--------|--------|----------|-----------|
| praxis/rule.rs | 20 | 0 | 1 | 100% |
| praxis/factory.rs | 14 | 0 | 6 | 100% |
| core/chronos.rs | 35 | 1* | 14 | 100%* |
| modules/safety.rs | 53 | 1** | 13 | 100%** |
| px/compiler.rs | 2 | 0 | 12 | 100% |
| px/lint.rs | 67 | 0 | 1 | 100% |
| px/scenario_runner.rs | 28 | 2*** | 2 | 100%*** |
| px/resolver.rs | 3 | 0 | 3 | 100% |
| px/compose.rs | 27 | 0 | 0 | 100% |
| tool_governance.rs | 6 | 0 | 4 | 100% |

\* The 1 missed mutant in chronos.rs is an equivalent mutant: deleting `1 => Self::Info` in `from_u8` falls through to `_ => Self::Info` — identical behavior, untestable by design.

\** The 1 missed mutant in safety.rs is an equivalent mutant: deleting `None => RuleResult::Pass` in `RiskScoreWithinBounds::evaluate` falls through to `_ => RuleResult::Pass` — identical behavior.

\*** The 2 missed mutants in scenario_runner.rs are equivalent: (1) deleting `"advance_time"` arm falls through to `_` which also returns `Ok(Value::Null)`, (2) deleting `"call"` arm falls through to `_` which performs identical logic (both extract name/params and call handler).

### Results Location
Results are written to `mutants.out/` (gitignored).

## When to Run

- **Weekly on main** — scheduled CI workflow catches regressions
- **On-demand** — when touching core logic modules
- **Before major releases** — full workspace sweep
- **After fixing a missed mutant** — verify the new test catches it

## Adding New Modules

1. Add entry to the `MODULES` associative array in `testing/scripts/run-mutations.sh`
2. Add matrix entry to `.github/workflows/mutation-testing.yml`
3. Run locally first to establish baseline
4. Update the results table above
