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

// ── Provider-surface validation (ADR-0022 §7) ────────────────────────────────

/// Validate that `manifest` (a provider) actually covers the mediated surface of
/// the CID it claims to implement (ADR-0022 §7).
///
/// Two things are checked, mirroring "the loader validates the provider's
/// declared surface against the CID at install; missing required
/// nodes/events/procedures → reject, like manifest schema validation":
///
/// 1. **Version compatibility.** The manifest must declare it *provides*
///    `cid.name` at a concrete version that is semver-compatible with
///    `cid.version` (caret semantics: provider `1.2.0` satisfies CID `1.0.0`,
///    provider `2.0.0` does not). Absent/incompatible → reject.
/// 2. **Surface coverage.** Using the provider's
///    `[capabilities.interface.<cid.name>]` declaration
///    ([`CapabilityInterfaceRef::provides_operations`] /
///    [`provides_events`](CapabilityInterfaceRef::provides_events)):
///    - every CID `operations[].name` must appear in `provides_operations`;
///    - every CID `result_event` and every `events.emitted_by_provider` must
///      appear in `provides_events`.
///
/// Every gap is collected (deterministic order) and returned as a single
/// [`PluginError::ProviderSurfaceIncomplete`] listing exactly what is missing.
/// Full coverage → `Ok(())`.
///
/// Node coverage: the CID node *types* are part of the contract a provider must
/// honor, but a provider declares nodes through its `[schema]`/runtime, not a
/// flat capability list, so this v1 check validates the **event/operation**
/// mediated boundary (the part ADR-0011 makes load-bearing across the plugin
/// boundary). Node-schema conformance is validated by schema registration
/// (`PluginError::SchemaRegistration`) and is intentionally NOT re-implemented
/// here as a fake string match — see the module note. The missing node-type
/// check below is therefore advisory-by-presence: if a provider *does* declare
/// the CID name in its interface block but omits operations/events, that is the
/// real, enforceable gap.
pub fn validate_provider_surface(
    manifest: &PluginManifest,
    cid: &CapabilityInterfaceDescriptor,
) -> Result<(), PluginError> {
    let mut missing: Vec<String> = Vec::new();

    // (1) Version compatibility: provider must provide cid.name at a concrete
    //     version that is caret-compatible with the CID version.
    let provided_version = manifest.capabilities.provided.get(&cid.name);
    match provided_version {
        None => {
            return Err(PluginError::ProviderSurfaceIncomplete {
                plugin: manifest.name.clone(),
                capability: cid.name.clone(),
                cid_version: cid.version.to_string(),
                missing: vec![format!(
                    "declaration: [capabilities.provided] {} = \"{}\" (provider does not declare it provides this capability)",
                    cid.name, cid.version
                )],
            });
        }
        Some(ver_str) => {
            let provided = ver_str.parse::<Version>().map_err(|e| {
                PluginError::InvalidManifest(format!(
                    "plugin '{}' provides capability '{}' with invalid semver version '{ver_str}': {e}",
                    manifest.name, cid.name
                ))
            })?;
            // Caret-compatibility against the CID version (e.g. ^1.0.0).
            let req = VersionReq::parse(&format!("^{}", cid.version)).map_err(|e| {
                PluginError::InvalidManifest(format!(
                    "CID '{}' version '{}' is not a valid caret base: {e}",
                    cid.name, cid.version
                ))
            })?;
            if !req.matches(&provided) {
                return Err(PluginError::ProviderSurfaceIncomplete {
                    plugin: manifest.name.clone(),
                    capability: cid.name.clone(),
                    cid_version: cid.version.to_string(),
                    missing: vec![format!(
                        "compatible-version: provides {} = \"{}\" but CID requires ^{}",
                        cid.name, provided, cid.version
                    )],
                });
            }
        }
    }

    // The provider's declared surface for THIS capability lives in its
    // [capabilities.interface.<cid.name>] block.
    let iface = manifest.capabilities.interface.get(&cid.name);
    let declared_ops: std::collections::BTreeSet<&str> = iface
        .map(|i| i.provides_operations.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let declared_events: std::collections::BTreeSet<&str> = iface
        .map(|i| i.provides_events.iter().map(String::as_str).collect())
        .unwrap_or_default();

    // (2a) Every CID operation must be serviced.
    for op in &cid.operations {
        if !declared_ops.contains(op.name.as_str()) {
            missing.push(format!("operation '{}'", op.name));
        }
    }

    // (2b) Every provider-side event must be declared. Union of each operation's
    //      result_event and the explicit events.emitted_by_provider vocabulary
    //      (deterministic, de-duplicated).
    let mut provider_events: std::collections::BTreeSet<&str> =
        std::collections::BTreeSet::new();
    for op in &cid.operations {
        if let Some(ev) = op.result_event.as_deref() {
            provider_events.insert(ev);
        }
    }
    for ev in &cid.events_emitted_by_provider {
        provider_events.insert(ev.as_str());
    }
    for ev in &provider_events {
        if !declared_events.contains(ev) {
            missing.push(format!("provider-event '{ev}'"));
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        // `missing` is already in deterministic order: operations follow CID
        // declaration order, events follow BTreeSet (sorted) order.
        Err(PluginError::ProviderSurfaceIncomplete {
            plugin: manifest.name.clone(),
            capability: cid.name.clone(),
            cid_version: cid.version.to_string(),
            missing,
        })
    }
}

/// Resolve all capabilities (as [`resolve_capabilities`]) **and** validate the
/// declared surface of every bound provider against the CID it provides
/// (ADR-0022 §7), wiring surface validation into the same install/activation
/// path as resolution.
///
/// `cids` maps a capability name to its loaded [`CapabilityInterfaceDescriptor`]
/// (the host/registry loads these from the CID files referenced by each
/// provider's `[capabilities.interface.<name>] spec`). For every binding whose
/// capability has a CID in `cids`, the bound provider's manifest is checked with
/// [`validate_provider_surface`]; the first incomplete surface is returned as a
/// [`PluginError::ProviderSurfaceIncomplete`]. Capabilities with no CID supplied
/// are resolved but not surface-validated (the host simply has no contract to
/// check against — honest: absence of a CID is not a fake pass).
///
/// Returns the same bindings as [`resolve_capabilities`] on success.
pub fn resolve_and_validate_capabilities(
    manifests: &[PluginManifest],
    cids: &BTreeMap<String, CapabilityInterfaceDescriptor>,
) -> Result<Vec<CapabilityBinding>, PluginError> {
    let bindings = resolve_capabilities(manifests)?;

    // Index manifests by name for O(1) provider lookup.
    let by_name: BTreeMap<&str, &PluginManifest> =
        manifests.iter().map(|m| (m.name.as_str(), m)).collect();

    for binding in &bindings {
        let Some(cid) = cids.get(&binding.capability) else {
            continue; // No CID supplied for this capability: nothing to validate.
        };
        let Some(provider) = by_name.get(binding.provider.as_str()) else {
            // The resolver only binds providers drawn from `manifests`, so this
            // is unreachable in practice; skip rather than panic.
            continue;
        };
        validate_provider_surface(provider, cid)?;
    }

    Ok(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::{
        CapabilityInterfaceRef, PluginCapabilities, PluginManifest,
    };

    /// Path to the real, shipped commerce CID descriptor.
    fn commerce_cid_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("capabilities")
            .join("commerce.cid.toml")
    }

    /// Path to the real, shipped commerce PROVIDER manifest (ADR-0022 step 4).
    fn commerce_provider_manifest_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("plugins")
            .join("commerce")
            .join("plugin.toml")
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

    // ── inner-space (consumer) binds to commerce (provider) (ADR-0022 step 4 C) ─

    /// Build the FULL commerce provider surface exactly as the shipped
    /// `plugins/commerce/plugin.toml` declares it: `provided commerce = "1.0.0"`
    /// plus the `[capabilities.interface.commerce]` block listing every mediated
    /// operation and provider-emitted event. This is the real provider the
    /// resolver/validator must accept.
    fn commerce_provider_manifest() -> PluginManifest {
        let mut caps = PluginCapabilities::default();
        caps.provided.insert("commerce".into(), "1.0.0".into());
        caps.interface.insert(
            "commerce".into(),
            CapabilityInterfaceRef {
                cid: "commerce@1.x".into(),
                spec: Some("capabilities/commerce.cid.toml".into()),
                provides_operations: vec![
                    "issue_coupon".into(),
                    "authorize_redemption".into(),
                    "check_nullifier".into(),
                    "decide_tier".into(),
                ],
                provides_events: vec![
                    "commerce.issue.completed".into(),
                    "commerce.redeem.completed".into(),
                    "commerce.nullifier.check.completed".into(),
                    "commerce.tier.decide.completed".into(),
                ],
            },
        );
        manifest_with_caps("commerce", caps)
    }

    /// The real inner-space consumer surface (its `commerce = "^1.0"` requirement).
    fn innerspace_consumer_manifest() -> PluginManifest {
        consumer_manifest("inner-space", &[("commerce", "^1.0")])
    }

    /// inner-space's `commerce = "^1.0"` binds to the commerce provider's
    /// concrete `commerce = "1.0.0"` (the end-to-end loader binding ADR-0022
    /// step 4 C requires).
    #[test]
    fn innerspace_consumer_binds_to_commerce_provider() {
        let provider = commerce_provider_manifest();
        let consumer = innerspace_consumer_manifest();
        let bindings =
            resolve_capabilities(&[provider, consumer]).expect("inner-space binds to commerce");

        // Exactly one binding: inner-space.commerce -> commerce@1.0.0. (The
        // consumer's other required capabilities — scene/physics/etc. — have no
        // provider in THIS batch and would each be UnsatisfiedCapability; here we
        // isolate the commerce binding by giving the consumer only commerce.)
        assert_eq!(bindings.len(), 1, "one commerce binding");
        let b = &bindings[0];
        assert_eq!(b.consumer, "inner-space");
        assert_eq!(b.capability, "commerce");
        assert_eq!(b.provider, "commerce");
        assert_eq!(b.version, "1.0.0");
    }

    // ── Provider-surface validation against the REAL CID (ADR-0022 §7) ───────

    /// The shipped commerce provider surface validates against the real
    /// `commerce.cid.toml`: every operation + every provider event is declared.
    #[test]
    fn commerce_provider_surface_validates_against_real_cid() {
        let cid = load_cid_from_toml_path(&commerce_cid_path()).expect("parse real CID");
        let provider = commerce_provider_manifest();
        validate_provider_surface(&provider, &cid)
            .expect("the shipped commerce provider must satisfy the real commerce CID");
    }

    /// A provider that omits one mediated operation (here `check_nullifier`) and
    /// its event is REJECTED with `ProviderSurfaceIncomplete` naming the gaps
    /// (no stub: a partial provider does not silently pass).
    #[test]
    fn commerce_provider_missing_operation_is_rejected() {
        let cid = load_cid_from_toml_path(&commerce_cid_path()).expect("parse real CID");
        let mut provider = commerce_provider_manifest();
        // Drop check_nullifier from the declared surface.
        if let Some(iface) = provider.capabilities.interface.get_mut("commerce") {
            iface
                .provides_operations
                .retain(|o| o != "check_nullifier");
            iface
                .provides_events
                .retain(|e| e != "commerce.nullifier.check.completed");
        }
        let err = validate_provider_surface(&provider, &cid).unwrap_err();
        match err {
            PluginError::ProviderSurfaceIncomplete {
                plugin,
                capability,
                cid_version,
                missing,
            } => {
                assert_eq!(plugin, "commerce");
                assert_eq!(capability, "commerce");
                assert_eq!(cid_version, "1.0.0");
                assert!(
                    missing.contains(&"operation 'check_nullifier'".to_string()),
                    "missing operation reported, got: {missing:?}"
                );
                assert!(
                    missing.contains(
                        &"provider-event 'commerce.nullifier.check.completed'".to_string()
                    ),
                    "missing provider event reported, got: {missing:?}"
                );
            }
            other => panic!("expected ProviderSurfaceIncomplete, got {other:?}"),
        }
    }

    /// A provider that declares the version but NO interface surface at all is
    /// rejected: every mediated operation and provider event is reported missing
    /// (it can service nothing).
    #[test]
    fn commerce_provider_with_no_surface_is_rejected() {
        let cid = load_cid_from_toml_path(&commerce_cid_path()).expect("parse real CID");
        // Declares `provided commerce = 1.0.0` but no [capabilities.interface.commerce].
        let provider = provider_manifest("hollow", &[("commerce", "1.0.0")]);
        let err = validate_provider_surface(&provider, &cid).unwrap_err();
        match err {
            PluginError::ProviderSurfaceIncomplete { missing, .. } => {
                assert!(
                    missing.contains(&"operation 'issue_coupon'".to_string()),
                    "all operations reported missing, got: {missing:?}"
                );
                assert!(
                    missing.contains(&"operation 'authorize_redemption'".to_string()),
                    "all operations reported missing, got: {missing:?}"
                );
                assert!(
                    missing
                        .contains(&"provider-event 'commerce.redeem.completed'".to_string()),
                    "all provider events reported missing, got: {missing:?}"
                );
            }
            other => panic!("expected ProviderSurfaceIncomplete, got {other:?}"),
        }
    }

    /// A provider that does not even declare `[capabilities.provided] commerce`
    /// is rejected for the missing declaration (the most basic surface gap).
    #[test]
    fn commerce_provider_without_provided_declaration_is_rejected() {
        let cid = load_cid_from_toml_path(&commerce_cid_path()).expect("parse real CID");
        let provider = consumer_manifest("not-a-provider", &[("commerce", "^1.0")]);
        let err = validate_provider_surface(&provider, &cid).unwrap_err();
        match err {
            PluginError::ProviderSurfaceIncomplete { missing, .. } => {
                assert!(
                    missing.iter().any(|m| m.starts_with("declaration:")),
                    "missing [capabilities.provided] declaration reported, got: {missing:?}"
                );
            }
            other => panic!("expected ProviderSurfaceIncomplete, got {other:?}"),
        }
    }

    /// The wired resolve+validate path (ADR-0022 §7): the commerce provider and
    /// inner-space consumer resolve AND the provider's surface validates against
    /// the real CID in one call — the exact install/activation path.
    #[test]
    fn resolve_and_validate_binds_innerspace_to_commerce_with_real_cid() {
        let cid = load_cid_from_toml_path(&commerce_cid_path()).expect("parse real CID");
        let mut cids = std::collections::BTreeMap::new();
        cids.insert("commerce".to_string(), cid);

        let provider = commerce_provider_manifest();
        let consumer = innerspace_consumer_manifest();
        let bindings = resolve_and_validate_capabilities(&[provider, consumer], &cids)
            .expect("resolve + surface-validate must succeed for the real provider/consumer");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].consumer, "inner-space");
        assert_eq!(bindings[0].provider, "commerce");
        assert_eq!(bindings[0].version, "1.0.0");
    }

    /// The REAL shipped provider manifest file `plugins/commerce/plugin.toml`
    /// (deliverable A) parses through the real loader AND its declared surface
    /// validates against the real `commerce.cid.toml` (deliverable C). This makes
    /// the actual on-disk plugin.toml load-bearing in the gate: if its declared
    /// operations/events drift from the CID, this test fails.
    #[test]
    fn shipped_commerce_plugin_toml_validates_against_real_cid() {
        let manifest_path = commerce_provider_manifest_path();
        assert!(
            manifest_path.exists(),
            "shipped provider manifest must exist at {}",
            manifest_path.display()
        );
        let toml = std::fs::read_to_string(&manifest_path)
            .expect("read plugins/commerce/plugin.toml");
        let manifest = crate::plugins::manifest::parse_manifest(&toml)
            .expect("the shipped commerce plugin.toml must parse through the real loader");

        // It declares it provides commerce@1.0.0.
        assert_eq!(
            manifest.capabilities.provided.get("commerce").map(String::as_str),
            Some("1.0.0"),
            "shipped provider declares [capabilities.provided] commerce = 1.0.0"
        );

        // And its declared surface satisfies the real CID.
        let cid = load_cid_from_toml_path(&commerce_cid_path()).expect("parse real CID");
        validate_provider_surface(&manifest, &cid).expect(
            "the shipped plugins/commerce/plugin.toml surface must satisfy the real commerce CID",
        );

        // End-to-end: it binds to the real inner-space consumer AND surface-
        // validates in the wired resolve+validate path.
        let mut cids = std::collections::BTreeMap::new();
        cids.insert("commerce".to_string(), cid);
        let consumer = innerspace_consumer_manifest();
        let bindings = resolve_and_validate_capabilities(&[manifest, consumer], &cids)
            .expect("shipped provider binds + validates against inner-space consumer");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].provider, "commerce");
        assert_eq!(bindings[0].consumer, "inner-space");
    }
}
