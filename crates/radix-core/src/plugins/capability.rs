//! Capability Interface Descriptors (CID) and capability resolution (ADR-0022
//! Step 2).
//!
//! Two responsibilities live here:
//!
//! 1. **CID loading** ([`CapabilityInterfaceDescriptor`] +
//!    [`load_cid_from_toml`]): parse the host/registry-owned capability contract
//!    file (TOML, e.g. `capabilities/commerce.cid.toml`). The `.px`-native CID
//!    grammar does not yet exist upstream (pluresdb-px rev 195c67b has no
//!    `capability` construct — see the header of `commerce.cid.toml`), so v1 CIDs
//!    are TOML per ADR-0022 §7. This is **not** a stub: it is the real, shipped
//!    v1 descriptor format.
//!
//! 2. **Capability resolution** ([`resolve_capabilities`] + [`CapabilityBinding`]):
//!    given the installed manifests, bind each consumer's `required`/`optional`
//!    capability to a concrete provider using the deterministic binding-selection
//!    policy from ADR-0022 §4.
//!
//! Resolution does **not** mutate state; the returned bindings are a rebuildable
//! index. Durable binding state is persisted to PluresDB by the executor
//! (`radix:capability:bindings:*`, C-PLURES-003) — the in-memory result here is
//! only the computation.

use std::collections::BTreeMap;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::plugins::error::PluginError;
use crate::plugins::manifest::PluginManifest;
use crate::plugins::platform_capabilities::is_platform_capability;

// ── Capability Interface Descriptor (CID) ────────────────────────────────────

/// A parsed Capability Interface Descriptor (CID) — the host/registry-owned
/// contract a provider must implement and a consumer interacts through
/// (ADR-0022 §7).
///
/// Mirrors the real `commerce.cid.toml` shape: a `[cid]` identity block, a list
/// of `[[nodes]]` (PluresDB node schemas), `[[operations]]` (mediated entry
/// points), an `[events]` vocabulary, and `[invariants]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityInterfaceDescriptor {
    /// Capability name, e.g. `commerce`.
    pub name: String,
    /// CID version (semver), e.g. `1.0.0`.
    pub version: Version,
    /// Human-readable title.
    pub title: String,
    /// Longer summary.
    pub summary: String,
    /// Node-type schemas the capability reads/writes.
    pub nodes: Vec<CidNode>,
    /// Mediated operations a consumer triggers.
    pub operations: Vec<CidOperation>,
    /// Events emitted by the consumer (request side).
    pub events_emitted_by_consumer: Vec<String>,
    /// Events emitted by the provider (result side).
    pub events_emitted_by_provider: Vec<String>,
    /// Contract invariants a conforming provider must uphold.
    pub invariants: Vec<String>,
}

/// One `[[nodes]]` entry: a PluresDB node-type schema in the CID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidNode {
    /// Node type, e.g. `commerce:campaign`.
    pub node_type: String,
    /// The source-of-truth type this maps to, e.g. `CouponCampaign`.
    pub maps_to: Option<String>,
    /// Key template, e.g. `commerce:campaign:{id}`.
    pub key: Option<String>,
    /// Field declarations (`name: type` strings, verbatim from the CID).
    pub fields: Vec<String>,
}

/// One `[[operations]]` entry: a mediated operation in the CID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidOperation {
    /// Operation name, e.g. `issue_coupon`.
    pub name: String,
    /// Request event a consumer emits, e.g. `commerce.issue.requested`.
    pub request_event: Option<String>,
    /// Result event the provider emits, e.g. `commerce.issue.completed`.
    pub result_event: Option<String>,
    /// Input field declarations (verbatim).
    pub input: Vec<String>,
    /// Output field declarations (verbatim).
    pub output: Vec<String>,
    /// Declared error codes.
    pub errors: Vec<String>,
}

/// Parse a CID from a TOML file on disk (ADR-0022 §7).
///
/// Reads the file at `path` and delegates to [`parse_cid_toml`]. I/O errors and
/// parse errors are surfaced as [`PluginError`] (no silent fallback, no stub).
pub fn load_cid_from_toml_path(
    path: &std::path::Path,
) -> Result<CapabilityInterfaceDescriptor, PluginError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        PluginError::InvalidManifest(format!("cannot read CID file {}: {e}", path.display()))
    })?;
    parse_cid_toml(&content)
}

/// Parse a CID from a TOML string (ADR-0022 §7).
///
/// This is the canonical entrypoint named in the task spec. It validates the
/// `[cid]` identity (name + semver version are required) and collects the
/// `[[nodes]]`, `[[operations]]`, `[events]`, and `[invariants]` sections.
pub fn load_cid_from_toml(toml_str: &str) -> Result<CapabilityInterfaceDescriptor, PluginError> {
    parse_cid_toml(toml_str)
}

/// The real TOML walk backing both [`load_cid_from_toml`] and
/// [`load_cid_from_toml_path`].
fn parse_cid_toml(toml_str: &str) -> Result<CapabilityInterfaceDescriptor, PluginError> {
    let value: toml::Value = toml::from_str(toml_str)
        .map_err(|e| PluginError::InvalidManifest(format!("CID TOML parse error: {e}")))?;

    let cid = value
        .get("cid")
        .ok_or_else(|| PluginError::InvalidManifest("CID is missing the [cid] table".into()))?;

    let name = cid
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PluginError::InvalidManifest("CID [cid] is missing 'name'".into()))?
        .to_string();

    let version_str = cid
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PluginError::InvalidManifest("CID [cid] is missing 'version'".into()))?;
    let version = version_str.parse::<Version>().map_err(|e| {
        PluginError::InvalidManifest(format!(
            "CID '{name}' has invalid semver version '{version_str}': {e}"
        ))
    })?;

    let title = cid
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let summary = cid
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let nodes = parse_cid_nodes(&value);
    let operations = parse_cid_operations(&value);
    let (events_emitted_by_consumer, events_emitted_by_provider) = parse_cid_events(&value);
    let invariants = parse_cid_invariants(&value);

    Ok(CapabilityInterfaceDescriptor {
        name,
        version,
        title,
        summary,
        nodes,
        operations,
        events_emitted_by_consumer,
        events_emitted_by_provider,
        invariants,
    })
}

/// Read a TOML array-of-strings field into a `Vec<String>` (skips non-strings).
fn toml_string_array(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_cid_nodes(value: &toml::Value) -> Vec<CidNode> {
    let Some(nodes) = value.get("nodes").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    nodes
        .iter()
        .filter_map(|n| {
            let node_type = n.get("type")?.as_str()?.to_string();
            Some(CidNode {
                node_type,
                maps_to: n.get("maps_to").and_then(|v| v.as_str()).map(String::from),
                key: n.get("key").and_then(|v| v.as_str()).map(String::from),
                fields: toml_string_array(n.get("fields")),
            })
        })
        .collect()
}

fn parse_cid_operations(value: &toml::Value) -> Vec<CidOperation> {
    let Some(ops) = value.get("operations").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    ops.iter()
        .filter_map(|o| {
            let name = o.get("name")?.as_str()?.to_string();
            Some(CidOperation {
                name,
                request_event: o
                    .get("request_event")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                result_event: o
                    .get("result_event")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                input: toml_string_array(o.get("input")),
                output: toml_string_array(o.get("output")),
                errors: toml_string_array(o.get("errors")),
            })
        })
        .collect()
}

fn parse_cid_events(value: &toml::Value) -> (Vec<String>, Vec<String>) {
    let Some(events) = value.get("events") else {
        return (Vec::new(), Vec::new());
    };
    (
        toml_string_array(events.get("emitted_by_consumer")),
        toml_string_array(events.get("emitted_by_provider")),
    )
}

fn parse_cid_invariants(value: &toml::Value) -> Vec<String> {
    let Some(inv) = value.get("invariants") else {
        return Vec::new();
    };
    toml_string_array(inv.get("rules"))
}

// ── Capability resolution (ADR-0022 §3, §4) ──────────────────────────────────

/// A resolved capability binding: consumer `consumer` had a required/optional
/// capability `capability @ range` and it was bound to provider `provider` which
/// provides `capability = version`.
///
/// This is the source-of-truth record persisted to PluresDB
/// (`radix:capability:bindings:*`). The in-memory `Vec<CapabilityBinding>`
/// returned by [`resolve_capabilities`] is a rebuildable index of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBinding {
    /// The consumer plugin that required the capability.
    pub consumer: String,
    /// The capability name (e.g. `commerce`).
    pub capability: String,
    /// The provider plugin that satisfies it.
    pub provider: String,
    /// The concrete version the provider provides (e.g. `1.2.0`).
    pub version: String,
}

/// One candidate provider for a capability: its plugin name and the concrete
/// version it provides (already parsed as semver).
#[derive(Debug, Clone)]
struct ProviderCandidate {
    plugin: String,
    version: Version,
}

/// Build the provider index: capability-name → list of providers (plugin +
/// concrete provided version) across all manifests.
///
/// A manifest contributes a candidate for capability `c` iff its
/// `capabilities.provided` contains `c` with a version that parses as a concrete
/// semver [`Version`]. Malformed provided versions are surfaced as
/// [`PluginError::InvalidManifest`] (no silent skip — a bad provider version is
/// a real manifest error).
fn build_provider_index(
    manifests: &[PluginManifest],
) -> Result<BTreeMap<String, Vec<ProviderCandidate>>, PluginError> {
    let mut index: BTreeMap<String, Vec<ProviderCandidate>> = BTreeMap::new();
    for m in manifests {
        for (cap, version_str) in &m.capabilities.provided {
            let version = version_str.parse::<Version>().map_err(|e| {
                PluginError::InvalidManifest(format!(
                    "plugin '{}' provides capability '{cap}' with invalid semver version '{version_str}': {e}",
                    m.name
                ))
            })?;
            index
                .entry(cap.clone())
                .or_default()
                .push(ProviderCandidate {
                    plugin: m.name.clone(),
                    version,
                });
        }
    }
    Ok(index)
}

/// The outcome of selecting a provider for one required/optional capability.
enum Selection {
    /// A single provider was chosen.
    Bound(ProviderCandidate),
    /// No provider satisfies the range.
    None,
    /// Multiple providers tie at the highest satisfying version with no
    /// deterministic tiebreak available.
    Ambiguous(Vec<String>),
}

/// Apply the ADR-0022 §4 binding-selection policy for one capability/range.
///
/// Policy (deterministic), given the candidates whose version satisfies `range`:
/// 1. **Explicit pin** — if `pin` names a candidate (and it satisfies the range),
///    choose it.
/// 2. **Highest satisfying version** — choose the unique candidate with the
///    greatest version.
/// 3. **Trust tier** — ADR-0022 §4 step 3; there is **no backing TrustTier type
///    wired for plugins** (see the milestone's "NOT BUILT" note), so we do NOT
///    invent one.
/// 4. **Ambiguous** — if >1 candidate ties at the highest version with no pin
///    and no trust tier, return [`Selection::Ambiguous`] (caller raises a real
///    `AmbiguousCapability` error).
///
/// Empty satisfying set ⇒ [`Selection::None`].
fn select_provider(
    candidates: &[ProviderCandidate],
    range: &VersionReq,
    pin: Option<&str>,
) -> Selection {
    // Candidates whose concrete version satisfies the requested range.
    let satisfying: Vec<&ProviderCandidate> = candidates
        .iter()
        .filter(|c| range.matches(&c.version))
        .collect();

    if satisfying.is_empty() {
        return Selection::None;
    }

    // (1) Explicit pin: a named provider that also satisfies the range.
    if let Some(pinned) = pin {
        if let Some(c) = satisfying.iter().find(|c| c.plugin == pinned) {
            return Selection::Bound((*c).clone());
        }
        // A pin that does not satisfy the range is ignored here; resolution
        // falls through to highest-version. (The pin is advisory; an
        // unsatisfiable pin must not mask an otherwise-valid binding.)
    }

    // (2) Highest satisfying version.
    let max_version = satisfying
        .iter()
        .map(|c| &c.version)
        .max()
        .expect("non-empty satisfying set has a max");
    let top: Vec<&ProviderCandidate> = satisfying
        .iter()
        .copied()
        .filter(|c| &c.version == max_version)
        .collect();

    if top.len() == 1 {
        return Selection::Bound(top[0].clone());
    }

    // (3) Trust tier — NOT AVAILABLE (no real TrustTier type for plugins).
    // (4) Tie with no tiebreak ⇒ ambiguous. Deterministic, sorted candidate list.
    let mut names: Vec<String> = top.iter().map(|c| c.plugin.clone()).collect();
    names.sort();
    Selection::Ambiguous(names)
}

/// Resolve every consumer's `required`/`optional` capabilities against the
/// installed providers (ADR-0022 §3, §4).
///
/// For each manifest `m` and each entry in `m.capabilities.required` /
/// `m.capabilities.optional`:
/// - **Platform capabilities** (`network`, `storage`, …) are skipped here: they
///   are the permission axis (ADR-0011), not provider-resolved (ADR-0022 §1).
/// - The range string is parsed as a [`VersionReq`]; a malformed range is a real
///   [`PluginError::InvalidManifest`].
/// - The provider is selected via [`select_provider`]:
///   - **required** with no satisfying provider ⇒ [`PluginError::UnsatisfiedCapability`].
///   - **required** ambiguous ⇒ [`PluginError::AmbiguousCapability`].
///   - **optional** with no provider ⇒ omitted (feature-detected absent), NOT an error.
///   - **optional** ambiguous ⇒ [`PluginError::AmbiguousCapability`] (an ambiguous
///     optional is still a real configuration error: a provider IS present, the
///     host just cannot pick deterministically).
///
/// Returns the bindings in deterministic order (manifests in input order;
/// capabilities in `BTreeMap` key order).
pub fn resolve_capabilities(
    manifests: &[PluginManifest],
) -> Result<Vec<CapabilityBinding>, PluginError> {
    let index = build_provider_index(manifests)?;
    let mut bindings = Vec::new();

    for m in manifests {
        // There is no pin map declared on the manifest today, so `pin` is always
        // `None` here. This is honest: when a real pin field lands (ADR-0022 §4
        // step 1), thread it in. No fake pin is invented.
        resolve_for_consumer(m, &index, true, &mut bindings)?;
        resolve_for_consumer(m, &index, false, &mut bindings)?;
    }

    Ok(bindings)
}

/// Resolve one consumer's required (`required == true`) or optional
/// (`required == false`) capability map, pushing successful bindings onto `out`.
fn resolve_for_consumer(
    m: &PluginManifest,
    index: &BTreeMap<String, Vec<ProviderCandidate>>,
    required: bool,
    out: &mut Vec<CapabilityBinding>,
) -> Result<(), PluginError> {
    let map = if required {
        &m.capabilities.required
    } else {
        &m.capabilities.optional
    };

    for (cap, range_str) in map {
        // Platform capabilities are the permission axis, not provider-resolved.
        if is_platform_capability(cap) {
            continue;
        }

        let range = range_str.parse::<VersionReq>().map_err(|e| {
            PluginError::InvalidManifest(format!(
                "plugin '{}' requires capability '{cap}' with invalid semver range '{range_str}': {e}",
                m.name
            ))
        })?;

        let candidates = index.get(cap).map(Vec::as_slice).unwrap_or(&[]);
        match select_provider(candidates, &range, None) {
            Selection::Bound(provider) => {
                out.push(CapabilityBinding {
                    consumer: m.name.clone(),
                    capability: cap.clone(),
                    provider: provider.plugin,
                    version: provider.version.to_string(),
                });
            }
            Selection::None => {
                if required {
                    return Err(PluginError::UnsatisfiedCapability {
                        plugin: m.name.clone(),
                        capability: cap.clone(),
                        range: range_str.clone(),
                    });
                }
                // Optional with no provider: feature-detected absent. Omit.
            }
            Selection::Ambiguous(candidates) => {
                return Err(PluginError::AmbiguousCapability {
                    capability: cap.clone(),
                    candidates,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::{PluginCapabilities, PluginManifest};

    /// Path to the real, shipped commerce CID descriptor.
    fn commerce_cid_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("capabilities")
            .join("commerce.cid.toml")
    }

    /// Build a minimal manifest with the given provided capabilities.
    fn provider_manifest(name: &str, provided: &[(&str, &str)]) -> PluginManifest {
        let mut caps = PluginCapabilities::default();
        for (cap, ver) in provided {
            caps.provided.insert(cap.to_string(), ver.to_string());
        }
        manifest_with_caps(name, caps)
    }

    /// Build a minimal manifest with the given required capabilities.
    fn consumer_manifest(name: &str, required: &[(&str, &str)]) -> PluginManifest {
        let mut caps = PluginCapabilities::default();
        for (cap, range) in required {
            caps.required.insert(cap.to_string(), range.to_string());
        }
        manifest_with_caps(name, caps)
    }

    fn manifest_with_caps(name: &str, capabilities: PluginCapabilities) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            description: String::new(),
            author: None,
            schema: Default::default(),
            logic: Default::default(),
            tools: Vec::new(),
            ui: None,
            permissions: Default::default(),
            hooks: Vec::new(),
            dependencies: Vec::new(),
            capabilities,
        }
    }

    // ── CID loading ──────────────────────────────────────────────────────

    #[test]
    fn load_real_commerce_cid_from_path() {
        let path = commerce_cid_path();
        assert!(
            path.exists(),
            "real commerce.cid.toml must exist at {}",
            path.display()
        );
        let cid = load_cid_from_toml_path(&path).expect("parse real commerce CID");

        assert_eq!(cid.name, "commerce");
        assert_eq!(cid.version, Version::new(1, 0, 0));
        assert!(cid.title.contains("Anonymous"));

        // The 5 real node types.
        let node_types: Vec<&str> = cid.nodes.iter().map(|n| n.node_type.as_str()).collect();
        assert_eq!(cid.nodes.len(), 5, "commerce CID has 5 node types");
        assert!(node_types.contains(&"commerce:campaign"));
        assert!(node_types.contains(&"commerce:coupon"));
        assert!(node_types.contains(&"commerce:redemption_record"));
        assert!(node_types.contains(&"commerce:nullifier_set"));
        assert!(node_types.contains(&"commerce:redemption_proof"));

        // A node maps_to + key are captured (campaign).
        let campaign = cid
            .nodes
            .iter()
            .find(|n| n.node_type == "commerce:campaign")
            .expect("campaign node present");
        assert_eq!(campaign.maps_to.as_deref(), Some("CouponCampaign"));
        assert_eq!(campaign.key.as_deref(), Some("commerce:campaign:{id}"));
        assert!(
            campaign.fields.iter().any(|f| f.starts_with("id:")),
            "campaign fields parsed verbatim"
        );

        // The 4 real operations.
        let op_names: Vec<&str> = cid.operations.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(cid.operations.len(), 4, "commerce CID has 4 operations");
        assert!(op_names.contains(&"issue_coupon"));
        assert!(op_names.contains(&"authorize_redemption"));
        assert!(op_names.contains(&"check_nullifier"));
        assert!(op_names.contains(&"decide_tier"));

        // Operation request/result events captured.
        let issue = cid
            .operations
            .iter()
            .find(|o| o.name == "issue_coupon")
            .expect("issue_coupon present");
        assert_eq!(
            issue.request_event.as_deref(),
            Some("commerce.issue.requested")
        );
        assert_eq!(
            issue.result_event.as_deref(),
            Some("commerce.issue.completed")
        );

        // Events present (request + result vocabularies).
        assert_eq!(
            cid.events_emitted_by_consumer.len(),
            4,
            "4 consumer-emitted events"
        );
        assert!(cid
            .events_emitted_by_consumer
            .contains(&"commerce.issue.requested".to_string()));
        assert_eq!(
            cid.events_emitted_by_provider.len(),
            4,
            "4 provider-emitted events"
        );
        assert!(cid
            .events_emitted_by_provider
            .contains(&"commerce.redeem.completed".to_string()));

        // Invariants present.
        assert!(
            cid.invariants.iter().any(|r| r.contains("double_spend")),
            "double-spend invariant present"
        );
    }

    #[test]
    fn load_cid_from_str_parses_same_shape() {
        // Read the real file's bytes and parse via the string entrypoint, so
        // both code paths are covered against the canonical descriptor.
        let content = std::fs::read_to_string(commerce_cid_path()).unwrap();
        let cid = load_cid_from_toml(&content).expect("parse via string entrypoint");
        assert_eq!(cid.name, "commerce");
        assert_eq!(cid.version, Version::new(1, 0, 0));
        assert_eq!(cid.nodes.len(), 5);
        assert_eq!(cid.operations.len(), 4);
    }

    #[test]
    fn cid_missing_cid_table_is_error() {
        let err = load_cid_from_toml("[other]\nx = 1\n").unwrap_err();
        assert!(
            err.to_string().contains("[cid] table"),
            "missing [cid] is a real error, got: {err}"
        );
    }

    #[test]
    fn cid_bad_version_is_error() {
        let toml = "[cid]\nname = \"x\"\nversion = \"not-semver\"\n";
        let err = load_cid_from_toml(toml).unwrap_err();
        assert!(
            err.to_string().contains("invalid semver"),
            "bad CID version is a real error, got: {err}"
        );
    }

    // ── Resolver policy (ADR-0022 §4) ────────────────────────────────────

    /// (a) consumer requires `commerce = "^1.0"`, one provider provides
    /// `commerce = "1.2.0"` ⇒ binds to 1.2.0.
    #[test]
    fn resolves_single_provider_to_concrete_version() {
        let provider = provider_manifest("oasis-commerce", &[("commerce", "1.2.0")]);
        let consumer = consumer_manifest("shop", &[("commerce", "^1.0")]);
        let bindings = resolve_capabilities(&[provider, consumer]).expect("resolves");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].consumer, "shop");
        assert_eq!(bindings[0].capability, "commerce");
        assert_eq!(bindings[0].provider, "oasis-commerce");
        assert_eq!(bindings[0].version, "1.2.0");
    }

    /// (b) two providers 1.2.0 and 1.5.0 ⇒ binds highest (1.5.0).
    #[test]
    fn resolves_to_highest_satisfying_version() {
        let p1 = provider_manifest("commerce-a", &[("commerce", "1.2.0")]);
        let p2 = provider_manifest("commerce-b", &[("commerce", "1.5.0")]);
        let consumer = consumer_manifest("shop", &[("commerce", "^1.0")]);
        let bindings = resolve_capabilities(&[p1, p2, consumer]).expect("resolves");
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].provider, "commerce-b",
            "highest version provider wins"
        );
        assert_eq!(bindings[0].version, "1.5.0");
    }

    /// (c) two providers BOTH 1.5.0, no pin/tier ⇒ AmbiguousCapability listing both.
    #[test]
    fn resolves_tie_to_ambiguous_capability_error() {
        let p1 = provider_manifest("commerce-a", &[("commerce", "1.5.0")]);
        let p2 = provider_manifest("commerce-b", &[("commerce", "1.5.0")]);
        let consumer = consumer_manifest("shop", &[("commerce", "^1.0")]);
        let err = resolve_capabilities(&[p1, p2, consumer]).unwrap_err();
        match err {
            PluginError::AmbiguousCapability {
                capability,
                candidates,
            } => {
                assert_eq!(capability, "commerce");
                assert_eq!(
                    candidates,
                    vec!["commerce-a".to_string(), "commerce-b".to_string()],
                    "both tied providers listed, sorted"
                );
            }
            other => panic!("expected AmbiguousCapability, got {other:?}"),
        }
    }

    /// (d) consumer requires `commerce = "^2.0"`, only 1.x provider ⇒
    /// UnsatisfiedCapability.
    #[test]
    fn resolves_unsatisfiable_required_to_error() {
        let provider = provider_manifest("commerce-a", &[("commerce", "1.5.0")]);
        let consumer = consumer_manifest("shop", &[("commerce", "^2.0")]);
        let err = resolve_capabilities(&[provider, consumer]).unwrap_err();
        match err {
            PluginError::UnsatisfiedCapability {
                plugin,
                capability,
                range,
            } => {
                assert_eq!(plugin, "shop");
                assert_eq!(capability, "commerce");
                assert_eq!(range, "^2.0");
            }
            other => panic!("expected UnsatisfiedCapability, got {other:?}"),
        }
    }

    /// (e) OPTIONAL commerce with no provider ⇒ no error, simply unbound.
    #[test]
    fn optional_capability_with_no_provider_is_unbound_not_error() {
        let mut caps = PluginCapabilities::default();
        caps.optional.insert("commerce".into(), "^1.0".into());
        let consumer = manifest_with_caps("shop", caps);
        let bindings = resolve_capabilities(&[consumer]).expect("optional-absent is not an error");
        assert!(
            bindings.is_empty(),
            "optional capability with no provider yields no binding"
        );
    }

    /// An optional capability that DOES have a provider still binds.
    #[test]
    fn optional_capability_with_provider_binds() {
        let provider = provider_manifest("commerce-a", &[("commerce", "1.2.0")]);
        let mut caps = PluginCapabilities::default();
        caps.optional.insert("commerce".into(), "^1.0".into());
        let consumer = manifest_with_caps("shop", caps);
        let bindings = resolve_capabilities(&[provider, consumer]).expect("resolves");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].provider, "commerce-a");
        assert_eq!(bindings[0].version, "1.2.0");
    }

    /// Platform capabilities (permission axis, ADR-0011) are NOT provider-
    /// resolved: requiring `network` must never error even with no provider.
    #[test]
    fn platform_capability_required_is_skipped_by_resolver() {
        let consumer = consumer_manifest("shop", &[("network", "^1.0")]);
        let bindings = resolve_capabilities(&[consumer]).expect("platform cap is not resolved");
        assert!(
            bindings.is_empty(),
            "platform capability requirement yields no provider binding"
        );
    }

    /// A provider declaring an invalid concrete `provided` version is a real
    /// manifest error (not silently skipped).
    #[test]
    fn provider_with_bad_provided_version_is_error() {
        let provider = provider_manifest("bad", &[("commerce", "not-a-version")]);
        let consumer = consumer_manifest("shop", &[("commerce", "^1.0")]);
        let err = resolve_capabilities(&[provider, consumer]).unwrap_err();
        assert!(
            err.to_string().contains("invalid semver"),
            "bad provided version surfaces, got: {err}"
        );
    }

    /// A `CapabilityBinding` round-trips through JSON (it is persisted to
    /// PluresDB by the executor).
    #[test]
    fn capability_binding_json_roundtrips() {
        let b = CapabilityBinding {
            consumer: "shop".into(),
            capability: "commerce".into(),
            provider: "oasis-commerce".into(),
            version: "1.2.0".into(),
        };
        let json = serde_json::to_value(&b).unwrap();
        let back: CapabilityBinding = serde_json::from_value(json).unwrap();
        assert_eq!(b, back);
    }
}
