//! Guard for the workspace `default-members` list.
//!
//! `src-tauri` (pares-radix-app) links the system GTK/WebKit stack through
//! glib-sys, so a bare `cargo test` / `cargo build` fails on any machine without
//! the GTK -dev packages — including the org release pipeline, which runs a bare
//! `cargo test` with no apt step. Keeping it out of `default-members` (while it
//! stays a full workspace member for `cargo tauri build` and `-p`) is what makes
//! those bare cargo invocations work.
//!
//! Every other member must stay in `default-members`, otherwise new crates would
//! silently drop out of the default test/build set.

use std::fs;
use std::path::PathBuf;

const EXCLUDED_FROM_DEFAULT: &[&str] = &["src-tauri"];

fn workspace_manifest() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Cargo.toml");
    fs::read_to_string(&root).unwrap_or_else(|e| panic!("failed to read {}: {e}", root.display()))
}

/// Extract the string entries of a top-level `key = [ ... ]` array from the
/// `[workspace]` table.
fn array_entries(manifest: &str, key: &str) -> Vec<String> {
    let needle = format!("\n{key} = [");
    let start = manifest
        .find(&needle)
        .unwrap_or_else(|| panic!("`{key}` not found in workspace Cargo.toml"))
        + needle.len();
    let end = start
        + manifest[start..]
            .find(']')
            .unwrap_or_else(|| panic!("unterminated `{key}` array in workspace Cargo.toml"));
    manifest[start..end]
        .split(',')
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

#[test]
fn tauri_app_is_excluded_from_default_members() {
    let manifest = workspace_manifest();
    let default_members = array_entries(&manifest, "default-members");

    for excluded in EXCLUDED_FROM_DEFAULT {
        assert!(
            !default_members.iter().any(|m| m == excluded),
            "`{excluded}` must stay out of workspace `default-members`: it needs the system \
             GTK/WebKit -dev packages, which bare `cargo test` environments do not install"
        );
    }
}

#[test]
fn every_other_member_is_a_default_member() {
    let manifest = workspace_manifest();
    let members = array_entries(&manifest, "members");
    let default_members = array_entries(&manifest, "default-members");

    for member in members {
        if EXCLUDED_FROM_DEFAULT.contains(&member.as_str()) {
            continue;
        }
        assert!(
            default_members.contains(&member),
            "workspace member `{member}` is missing from `default-members`, so bare cargo \
             commands would silently skip it"
        );
    }
}
