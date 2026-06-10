# ADR-0021: Grammar as Generated Artifact from px-schema

## Status: Accepted

## Date: 2026-06-10

## Context

We have experienced three rounds of grammar divergence across the plures org:

1. **Round 1**: pluresdb and praxis each grew independent grammars. Neither was complete.
2. **Round 2**: Extended pluresdb grammar with 14 commits to cover .px v3 syntax.
3. **Round 3** (today): Manually merged both grammars into a "unified v4" and copied between repos.

Each round was a tactical fix that passed tests but didn't address the root cause: **two independent grammar files with no automated synchronization** (violates C-DRIFT-001).

The existing `px-schema` crate (in the praxis repo) already has:
- `PxSchemaDocument` — describes constructs, fields, types as metadata
- `#[derive(PxSchema)]` proc macro — auto-generates schema from Rust struct definitions
- `validator` — validates parsed .px documents against schema

However, `px-schema` currently operates AFTER parsing (validation layer). It does not drive the parser itself.

## Decision

**The grammar (pest file) is a GENERATED ARTIFACT derived from canonical Rust AST types.**

The derivation pipeline:

```
Rust AST types (with #[derive(PxSchema, PxGrammar)])
    ↓ build.rs codegen (compile-time)
grammar.pest (GENERATED — never manually edited)
    ↓ pest_derive (compile-time)
Parser (pest-generated, type-safe)
    ↓ runtime
Parsed .px AST (matches the Rust types exactly)
```

### Source of truth hierarchy:

1. **Canonical**: `praxis/crates/px-ast/src/` — Rust type definitions for every .px construct
2. **Generated**: `pluresdb/crates/pluresdb-px/src/px/grammar.pest` — derived from (1) at build time
3. **Consumers**: pares-radix, praxis-native — depend on pluresdb-px for parsing

### Key principles:

- Manual grammar edits are **rejected by CI** (hash check against generated output)
- Adding a new .px construct means adding a Rust type → grammar regenerates automatically
- Expression grammar borrows operator precedence from Rust reference grammar
- Declaration grammar borrows structure from YAML (key: value, indentation-optional)
- Procedure bodies use Rust-style code blocks `{ let x = f(); ... }`

### What changes:

| Component | Before | After |
|-----------|--------|-------|
| Grammar source | Hand-written pest | Generated from Rust types |
| Grammar location | Two copies (pluresdb + praxis) | One generated (pluresdb), depended on by others |
| Adding syntax | Edit grammar → fix parser → fix builder | Add Rust type → regenerate → done |
| Validation | Post-parse (px-schema validator) | Parse IS validation (grammar enforces structure) |
| CI enforcement | `all_px_files_parse_cleanly` test | Hash check + parse test + schema conformance |

### What does NOT change:

- `.px` file syntax (end-user experience is identical)
- Queue-driven dataflow procedures (v3 semantics preserved)
- The unified grammar content pushed today (it becomes the reference implementation for codegen to match)

## Implementation Plan

### Phase 1: Canonical AST crate (`px-ast`)

Create `praxis/crates/px-ast/` with Rust types for every .px construct:

```rust
#[derive(PxSchema, PxGrammar)]
#[px(keyword = "entity")]
pub struct EntityDecl {
    pub name: Ident,
    #[px(optional)]
    pub prefix: Option<StringLiteral>,
    pub fields: Vec<FieldDecl>,
}

#[derive(PxSchema, PxGrammar)]
#[px(keyword = "procedure", syntax = "dataflow")]
pub struct DataflowProcedureDecl {
    pub name: Ident,
    pub params: Vec<DataflowParam>,
    #[px(optional)]
    pub return_type: Option<DataflowReturnType>,
    #[px(optional)]
    pub given: Option<StringLiteral>,
    pub body: ProcedureBody,  // enum: StepList | CodeBlock
}
```

### Phase 2: Grammar codegen (`px-grammar-gen`)

A build-time tool that:
1. Reads the `px-ast` types via reflection (or proc macro output)
2. Emits valid pest grammar rules
3. Expression grammar is hand-curated (operator precedence is too complex for pure codegen) but pinned as a CONSTANT FRAGMENT included verbatim
4. Declaration grammar is fully generated from struct shapes

### Phase 3: CI enforcement

- `grammar.pest` has a header comment with generation hash
- CI step: regenerate → compare → fail if different
- Pre-push hook: same check locally

### Phase 4: Deprecate praxis-native grammar copy

- praxis-native depends on pluresdb-px for parsing (no local grammar)
- Builder code in praxis-native uses shared AST types from px-ast

## Consequences

### Positive
- Single source of truth eliminates grammar drift permanently
- Adding new syntax is additive (add type → get grammar for free)
- Parser and AST are guaranteed to match (same types drive both)
- CI catches grammar/schema mismatches before merge

### Negative
- More complex build pipeline (codegen step)
- Expression grammar partially hand-curated (acceptable — it's pinned, not copied)
- Transition period: existing grammar works, codegen replaces it incrementally

### Risks
- pest grammar generation might not cover all pest features (mitigated: expression fragment is hand-curated)
- Build time increase from codegen (mitigated: runs once, output cached)

## Evidence

| Observation | Tested? | Source |
|-------------|---------|--------|
| Grammar diverged 3 times in 4 weeks | Yes | This ADR, memory/2026-06-10.md |
| Manual merge took 45+ minutes each time | Yes | Session 11 timing |
| px-schema proc macro works | Yes | praxis/crates/px-schema-derive tests pass |
| Unified grammar v4 passes all tests | Yes | 503 + 559 + 24 tests across 3 repos |
| C-DRIFT-001 violated by copy-between-repos | Yes | MEMORY.md constraint |

## References

- C-DRIFT-001: "If artifact B depends on source A, the derivation MUST be automated in the build"
- Rust reference grammar (expression precedence): https://doc.rust-lang.org/reference/expressions.html
- pest grammar docs: https://pest.rs/book/grammars/syntax.html
- Existing px-schema: `C:\Projects\praxis\crates\px-schema\` and `px-schema-derive\`\n