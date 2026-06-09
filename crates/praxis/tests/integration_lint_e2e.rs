//! End-to-end lint integration tests.
//!
//! Parses .px fixture files through the full pipeline (parse → lint) and verifies
//! that expected diagnostics are produced for all 12 lint rules.

use pares_radix_praxis::px::lint::{lint, LintSeverity};
use pares_radix_praxis::px::parse;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Parse the lint_triggers.px fixture and run lints — produces diagnostics for all rules.
#[test]
fn lint_triggers_fixture_parses_successfully() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).expect("lint_triggers.px should parse without errors");

    // We expect multiple procedures to have been parsed
    assert!(
        doc.procedures.len() >= 10,
        "Expected at least 10 procedures, got {}",
        doc.procedures.len()
    );
}

#[test]
fn lint_triggers_fixture_produces_parseable_codes() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).expect("parse failed");
    let diags = lint(&doc);

    // Collect unique codes
    let codes: std::collections::HashSet<&str> = diags.iter().map(|d| d.code).collect();

    // L001 and L007 cannot be triggered from parsed .px (grammar requires non-empty
    // procedure bodies and catch blocks). They're validated in unit tests instead.
    let expected = [
        "PX-L002", "PX-L003", "PX-L004", "PX-L005", "PX-L006", "PX-L008", "PX-L009", "PX-L010",
        "PX-L011", "PX-L012",
    ];

    for code in &expected {
        assert!(
            codes.contains(code),
            "Expected lint code {} to fire but it didn't. Got codes: {:?}",
            code,
            codes
        );
    }

    // Verify that at least 10 of 12 rules fire from parsed text
    assert!(
        codes.len() >= 10,
        "Expected at least 10 unique codes, got {}",
        codes.len()
    );
}

#[test]
fn l001_empty_proc_not_parseable() {
    // L001 cannot be triggered from parsed .px text because the grammar requires
    // at least one step. Verified here that no L001 fires from our fixture.
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L001")
        .collect();
    assert_eq!(diags.len(), 0, "L001 should not fire from parsed text");
}

#[test]
fn l002_non_exhaustive_match_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L002")
        .collect();

    // no_wildcard procedure should trigger L002, and dup_arms (no wildcard either)
    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("no_wildcard")),
        "no_wildcard procedure should trigger PX-L002"
    );
}

#[test]
fn l003_unreachable_arm_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L003")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("unreachable_arm")),
        "unreachable_arm procedure should trigger PX-L003"
    );
}

#[test]
fn l004_duplicate_arms_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L004")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("dup_arms")),
        "dup_arms procedure should trigger PX-L004"
    );
}

#[test]
fn l005_unused_output_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L005")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("unused_output")),
        "unused_output procedure should trigger PX-L005"
    );
}

#[test]
fn l006_unused_loop_var_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L006")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("unused_loop_var")),
        "unused_loop_var procedure should trigger PX-L006"
    );
}

#[test]
fn l007_empty_catch_not_parseable() {
    // L007 cannot be triggered from parsed .px text because the grammar requires
    // at least one step in catch blocks. Verified here that no L007 fires.
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L007")
        .collect();
    assert_eq!(diags.len(), 0, "L007 should not fire from parsed text");
}

#[test]
fn l008_shadowed_output_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L008")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("shadowed_var")),
        "shadowed_var procedure should trigger PX-L008"
    );
}

#[test]
fn l009_unreachable_after_return_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L009")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("dead_code")),
        "dead_code procedure should trigger PX-L009"
    );
}

#[test]
fn l010_unused_param_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L010")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("unused_param")),
        "unused_param procedure should trigger PX-L010"
    );
}

#[test]
fn l011_undefined_call_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L011")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("calls_undefined")),
        "calls_undefined procedure should trigger PX-L011"
    );
}

#[test]
fn l012_arity_mismatch_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags: Vec<_> = lint(&doc)
        .into_iter()
        .filter(|d| d.code == "PX-L012")
        .collect();

    assert!(
        diags
            .iter()
            .any(|d| d.procedure.as_deref() == Some("caller")),
        "caller procedure should trigger PX-L012 (arity mismatch)"
    );
}

/// Verify the aggregate diagnostic summary matches expectations.
#[test]
fn lint_triggers_diagnostic_summary() {
    let source = fs::read_to_string(fixtures_dir().join("lint_triggers.px")).unwrap();
    let doc = parse(&source).unwrap();
    let diags = lint(&doc);

    let warnings = diags
        .iter()
        .filter(|d| d.severity == LintSeverity::Warning)
        .count();
    let errors = diags
        .iter()
        .filter(|d| d.severity == LintSeverity::Error)
        .count();

    // All our lint rules produce warnings (no errors in this fixture)
    assert!(warnings > 0, "Should have at least some warnings");

    // Total diagnostics: at least 12 (one per rule, many rules fire more)
    assert!(
        diags.len() >= 12,
        "Expected at least 12 diagnostics, got {}",
        diags.len()
    );

    eprintln!("=== Lint Summary ===");
    eprintln!(
        "Total: {} diagnostics ({} warnings, {} errors)",
        diags.len(),
        warnings,
        errors
    );
    for d in &diags {
        eprintln!("  {}", d);
    }
}
