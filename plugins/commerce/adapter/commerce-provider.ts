// commerce-provider — adapter location pointer (ADR-0022 step 4)
//
// The mediated provider adapter (the IO actor the `.px` `on_<op>_requested`
// procedures invoke) is OASIS ZK-commerce logic and therefore lives in the
// OASIS package that owns the logic it adapts:
//
//   oasis/packages/crypto-verification/src/commerce-provider/index.ts
//   (exported from @oasis/crypto-verification)
//
// It is NOT duplicated here: a second compilable copy in the pares-radix tree
// could drift from the real OASIS surface it wraps, which would violate the
// no-stub/no-reimplementation gate (C-NOSTUB-001). The Radix plugin DECLARATION
// (manifest + .px + schema) is what belongs in this directory; the runtime IO
// actor it references is the single OASIS implementation, tested in OASIS via
//   src/__tests__/commerce-provider-mediated.test.ts
//
// The mediated boundary: a consumer only ever puts a `commerce:request:<op>:<id>`
// node and reads a `commerce:result:<op>:<id>` node through PluresDB. No direct
// function reference crosses the plugin boundary (ADR-0011). `CommerceProvider`
// (in OASIS) is the reactive body; `putRequest` / `readResult` are the consumer
// side.
//
// See ./README.md for the full wiring description.

export {};
