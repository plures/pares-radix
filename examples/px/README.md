# .px Examples

Real-world procedure files demonstrating the Praxis Intent Language.

For full language reference, see [docs/PX-LANGUAGE-GUIDE.md](../../docs/PX-LANGUAGE-GUIDE.md).

## Examples

| File | Demonstrates |
|------|-------------|
| `ci-pipeline.px` | Parallel branches, retry with backoff, try/catch, emit events |
| `incident-response.px` | Match patterns, loops, conditional escalation, event-driven triggers |
| `data-sync.px` | Loop over collections, error recovery, variable threading |
| `pr-review-bot.px` | Rules, constraints, facts, and procedures working together |
| `memory-maintenance.px` | Cron-triggered procedure, sequential steps, conditional logic |

## Running Examples

Examples can be loaded and executed by the Praxis engine:

```rust
use praxis::px::{parse_px_file, compile_procedure};

let ast = parse_px_file(include_str!("ci-pipeline.px"))?;
let compiled = compile_procedure(&ast.procedures[0])?;
```

Or validated via the CLI:

```sh
cargo run --bin radix -- praxis validate examples/px/ci-pipeline.px
```
