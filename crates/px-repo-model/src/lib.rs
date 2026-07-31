//! px-repo-model: repository graph schema types and canonical
//! identity/hashing (ADR-0040, graph-schema.md, px-bundle-format.md — M0
//! deliverables of epic `pares-radix:procedure-graph-repository-substrate`).

pub mod canonical;
pub mod exact_artifact;
pub mod identity;
pub mod merkle;
pub mod schema;

#[cfg(test)]
mod conformance_test;
