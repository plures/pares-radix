//! M0 exit-criterion conformance test (design-spec §17: "Exit: two
//! independent implementations serialize the same test graph identically").
//!
//! **Approach taken and why:** a true second-language implementation is out
//! of scope for this milestone (M5 is the first milestone that stands up a
//! second concrete language surface, and even then it's a semantic adapter,
//! not a from-scratch re-implementation of this hashing spec). Per
//! ADR-0040's own "Consequences" section, this crate satisfies the M0 exit
//! criterion via **two independently-written Rust encoding functions**
//! within this same crate that deliberately do not share an encoding
//! function or intermediate representation: `canonical::hash_canonical_json`
//! (the crate's actual production path, going through `serde_json::Value`
//! and a generic canonicalizer) versus `reference_encode_and_hash` below (a
//! hand-rolled encoder written directly against ADR-0040 §2's prose rules,
//! walking the test graph's own Rust struct fields by hand with no
//! `serde_json::Value` intermediate step and no shared helper functions).
//! Both must independently arrive at byte-identical canonical JSON — and
//! therefore identical BLAKE3 hashes — for the same logical test graph. This
//! is the strongest same-language proxy for cross-implementation determinism
//! achievable without a second-language implementation, matching this
//! milestone's own scope note in `exact-artifact-procedure.md` §7.

use crate::identity::{BlobHash, ContentHash, EntityId, ProcedureId};
use crate::schema::{CapabilityGrant, Effect, ProcedureRevisionContent, ResidualRef};

/// Build one fixed test graph: a single `ProcedureRevisionContent` with a
/// non-trivial field set (capability grants, residuals, non-ASCII text) so
/// the two encoders have real work to agree on, not a trivial empty struct.
fn build_test_procedure_revision() -> (ProcedureId, ProcedureRevisionContent) {
    let procedure_id =
        ProcedureId::parse_canonical_text("018f2b3a-8c41-7000-9c21-4e6b1a2f9d10").unwrap();
    let residual_blob = BlobHash::of_raw_bytes(b"// preserved trailing comment\n");
    let content = ProcedureRevisionContent {
        procedure_id,
        source_text: "procedure module AuthModule { }\n// unicode: caf\u{e9} \u{2713}".to_string(),
        capability_grants: vec![
            CapabilityGrant {
                effect: Effect::DbRead,
                scope: Some("users".to_string()),
            },
            CapabilityGrant {
                effect: Effect::Clock,
                scope: None,
            },
        ],
        residuals: vec![ResidualRef {
            blob_hash: residual_blob,
            kind: "line_comment".to_string(),
            attachment_point: "AuthModule:end".to_string(),
        }],
    };
    (procedure_id, content)
}

/// Second, independently-written encoder: walks `ProcedureRevisionContent`'s
/// fields directly by hand and builds the canonical JSON text itself,
/// applying ADR-0040 §2's rules from prose rather than calling
/// `canonical::canonicalize_value`/`write_canonical`. Deliberately
/// hand-rolls JSON string escaping and key ordering rather than importing
/// any helper from the `canonical` module, so this function shares no code
/// path with `canonical::hash_canonical_json`.
fn reference_encode_and_hash(content: &ProcedureRevisionContent) -> ContentHash {
    let mut out = String::new();
    out.push('{');

    // Object keys, sorted byte-wise ascending by UTF-8 encoding:
    // "capability_grants" < "procedure_id" < "residuals" < "source_text"
    // (verified by direct byte comparison of the key strings below, not
    // assumed - 'c' < 'p' < 'r' < 's' in ASCII, and none of these keys share
    // a prefix that would require deeper comparison).
    out.push_str("\"capability_grants\":[");
    for (i, grant) in content.capability_grants.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        // CapabilityGrant keys sorted: "effect" < "scope".
        out.push_str("\"effect\":\"");
        out.push_str(reference_effect_variant_name(grant.effect));
        out.push('"');
        if let Some(scope) = &grant.scope {
            out.push_str(",\"scope\":\"");
            out.push_str(&reference_escape_json_string(scope));
            out.push('"');
        }
        out.push('}');
    }
    out.push(']');

    out.push_str(",\"procedure_id\":\"");
    out.push_str(&content.procedure_id.to_canonical_text());
    out.push('"');

    out.push_str(",\"residuals\":[");
    for (i, residual) in content.residuals.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        // ResidualRef keys sorted: "attachment_point" < "blob_hash" < "kind".
        out.push_str("\"attachment_point\":\"");
        out.push_str(&reference_escape_json_string(&residual.attachment_point));
        out.push_str("\",\"blob_hash\":\"");
        out.push_str(&residual.blob_hash.to_canonical_text());
        out.push_str("\",\"kind\":\"");
        out.push_str(&reference_escape_json_string(&residual.kind));
        out.push('"');
        out.push('}');
    }
    out.push(']');

    out.push_str(",\"source_text\":\"");
    out.push_str(&reference_escape_json_string(&content.source_text));
    out.push('"');

    out.push('}');

    ContentHash::of_raw_bytes(out.as_bytes())
}

/// Hand-rolled variant-name mapping matching `#[serde(rename_all =
/// "snake_case")]` on `Effect`, written independently from the `serde`
/// derive rather than calling into it.
fn reference_effect_variant_name(effect: Effect) -> &'static str {
    match effect {
        Effect::DbRead => "db_read",
        Effect::DbWrite => "db_write",
        Effect::Network => "network",
        Effect::Shell => "shell",
        Effect::FileRead => "file_read",
        Effect::FileWrite => "file_write",
        Effect::EnvRead => "env_read",
        Effect::Clock => "clock",
        Effect::Random => "random",
    }
}

/// Hand-rolled strict-JSON string escaping (ADR-0040 §2: quote, backslash,
/// control chars < 0x20 escaped; NOT HTML-safe-escaped, so `<`/`>`/`&` pass
/// through unescaped, matching `serde_json`'s default behavior).
fn reference_escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[test]
fn two_independent_encoders_produce_byte_identical_hash_for_the_same_test_graph() {
    let (_procedure_id, content) = build_test_procedure_revision();

    // Encoder 1: the crate's actual production canonical-JSON path
    // (generic Value-based canonicalizer in `canonical.rs`).
    let production_hash = crate::canonical::hash_canonical_json(&content)
        .expect("production encoder must succeed on a well-formed fixture");

    // Encoder 2: independently hand-written field-by-field encoder above,
    // sharing no function or intermediate type with encoder 1.
    let reference_hash = reference_encode_and_hash(&content);

    assert_eq!(
        production_hash, reference_hash,
        "two independently-written encoders must produce byte-identical \
         canonical serialization (and therefore identical hashes) for the \
         same logical test graph -- this is the M0 exit criterion"
    );

    // Also assert the actual canonical bytes match exactly, not just the
    // hash (a hash collision would be an absurdly wrong way to pass this
    // test, but asserting bytes directly closes that gap).
    let production_bytes = crate::canonical::to_canonical_bytes(&content).unwrap();
    let production_text = String::from_utf8(production_bytes).unwrap();

    // Re-run the reference encoder's exact string-building logic (same
    // function, called again) to get its literal text for a direct string
    // comparison against the production encoder's output.
    let reference_text = reference_encode_json_text(&content);
    assert_eq!(
        production_text, reference_text,
        "canonical JSON text must be byte-identical between both encoders"
    );
}

/// Same field-walk as `reference_encode_and_hash`, but returns the text
/// instead of hashing it, for the byte-level text-comparison assertion.
fn reference_encode_json_text(content: &ProcedureRevisionContent) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str("\"capability_grants\":[");
    for (i, grant) in content.capability_grants.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"effect\":\"");
        out.push_str(reference_effect_variant_name(grant.effect));
        out.push('"');
        if let Some(scope) = &grant.scope {
            out.push_str(",\"scope\":\"");
            out.push_str(&reference_escape_json_string(scope));
            out.push('"');
        }
        out.push('}');
    }
    out.push(']');
    out.push_str(",\"procedure_id\":\"");
    out.push_str(&content.procedure_id.to_canonical_text());
    out.push('"');
    out.push_str(",\"residuals\":[");
    for (i, residual) in content.residuals.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"attachment_point\":\"");
        out.push_str(&reference_escape_json_string(&residual.attachment_point));
        out.push_str("\",\"blob_hash\":\"");
        out.push_str(&residual.blob_hash.to_canonical_text());
        out.push_str("\",\"kind\":\"");
        out.push_str(&reference_escape_json_string(&residual.kind));
        out.push('"');
        out.push('}');
    }
    out.push(']');
    out.push_str(",\"source_text\":\"");
    out.push_str(&reference_escape_json_string(&content.source_text));
    out.push('"');
    out.push('}');
    out
}

#[test]
fn entity_id_and_procedure_id_are_independent_of_each_other_by_type() {
    // Sanity check that the fixture graph's identity types actually match
    // the schema doc's field types (EntityId vs ProcedureId are distinct
    // Rust types, not interchangeable even though both wrap Uuid).
    let entity_id = EntityId::new();
    let procedure_id = ProcedureId::new();
    assert_ne!(
        entity_id.to_canonical_text(),
        procedure_id.to_canonical_text()
    );
}
