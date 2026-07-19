//! Plugin manifest types — the declaration of what a plugin provides.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A plugin manifest — the full declaration of a plugin's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin name (kebab-case).
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,

    /// PluresDB schema definitions.
    #[serde(default)]
    pub schema: PluginSchema,

    /// Praxis rules and constraints.
    #[serde(default)]
    pub logic: PluginLogic,

    /// Custom tools the plugin provides to the agent.
    #[serde(default)]
    pub tools: Vec<PluginTool>,

    /// UI component paths (Svelte, for design-dojo).
    #[serde(default)]
    pub ui: Option<PluginUI>,

    /// Permissions the plugin requires.
    #[serde(default)]
    pub permissions: PluginPermissions,

    /// Lifecycle hooks the plugin registers.
    #[serde(default)]
    pub hooks: Vec<crate::plugins::hooks::HookDeclaration>,

    /// Plugin dependencies (other plugin names that must be installed first).
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Capability contracts this plugin requires, optionally requires, and/or
    /// provides (ADR-0022). Distinct from [`PluginPermissions`]: permissions are
    /// the allow/deny I/O axis (ADR-0011); capabilities are versioned interface
    /// contracts resolved against providers by the loader (Step 2).
    #[serde(default)]
    pub capabilities: PluginCapabilities,
}

// ── Schema ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSchema {
    /// Entity types this plugin manages.
    #[serde(default)]
    pub entities: Vec<EntityDefinition>,
    /// Relationships between entities.
    #[serde(default)]
    pub relationships: Vec<RelationshipDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDefinition {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    String,
    Number,
    Date,
    Boolean,
    /// Reference to another entity type by name.
    Reference(std::string::String),
    /// Allowed values.
    Enum(Vec<std::string::String>),
    Currency,
    /// URL or file path.
    Image,
    /// Lat/lng or address string.
    Location,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipDefinition {
    pub name: String,
    pub from_entity: String,
    pub to_entity: String,
    /// `"many_to_one"`, `"one_to_many"`, `"many_to_many"`.
    pub cardinality: String,
}

// ── Logic ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginLogic {
    #[serde(default)]
    pub rules: Vec<PluginRule>,
    #[serde(default)]
    pub constraints: Vec<PluginConstraint>,
    #[serde(default)]
    pub procedures: Vec<PluginProcedure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRule {
    pub name: String,
    pub description: String,
    /// DSL or natural language condition.
    pub condition: String,
    /// What happens when condition is met.
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConstraint {
    pub name: String,
    pub description: String,
    /// Validation expression.
    pub check: String,
    pub error_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginProcedure {
    pub name: String,
    pub description: String,
    /// Event that triggers this procedure.
    pub trigger: String,
    /// Steps to execute.
    pub steps: Vec<serde_json::Value>,
}

// ── Tools ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ToolParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

// ── UI ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUI {
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub routes: Vec<UIRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIRoute {
    pub path: String,
    pub component: String,
    pub label: String,
}

// ── Permissions ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPermissions {
    #[serde(default)]
    pub pluresdb_scopes: Vec<String>,
    #[serde(default)]
    pub tool_access: Vec<String>,
    #[serde(default)]
    pub network: bool,
}

// ── Capabilities (ADR-0022) ──────────────────────────────────────────────────

/// Capability contracts declared by a plugin (ADR-0022 §1, §6).
///
/// Three orthogonal maps:
/// - `required` / `optional`: capabilities this plugin **consumes**. The map
///   value is a **semver range** (e.g. `commerce = "^1.0"`) matched against a
///   provider's concrete version at resolution time (Step 2).
/// - `provided`: capabilities this plugin **implements**. The map value is the
///   **concrete version** of the Capability Interface Descriptor (CID) it
///   satisfies (e.g. `commerce = "1.2.0"`).
///
/// `interface` carries the optional `[capabilities.interface.<name>]` blocks
/// pointing a provided capability at the CID it implements.
///
/// `BTreeMap` is used (not `HashMap`) for deterministic ordering / stable
/// serialization, consistent with the C-PLURES-003 "no ad-hoc maps for state"
/// spirit.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCapabilities {
    /// Capabilities required to activate. Value = semver range. Unsatisfied
    /// required capabilities block activation (enforced by the Step 2 resolver).
    #[serde(default)]
    pub required: BTreeMap<String, String>,

    /// Capabilities used if a provider is present, feature-detected if absent.
    /// Value = semver range. Unsatisfied optional capabilities do NOT block.
    #[serde(default)]
    pub optional: BTreeMap<String, String>,

    /// Capabilities this plugin implements. Value = concrete CID version.
    #[serde(default)]
    pub provided: BTreeMap<String, String>,

    /// Optional `[capabilities.interface.<name>]` references binding a provided
    /// (or required) capability name to the CID it targets.
    #[serde(default)]
    pub interface: BTreeMap<String, CapabilityInterfaceRef>,
}

/// A reference to a Capability Interface Descriptor (CID) from a
/// `[capabilities.interface.<name>]` block (ADR-0022 §6, §7).
///
/// Example TOML:
/// ```toml
/// [capabilities.interface.commerce]
/// cid = "commerce@1.x"
/// spec = "capabilities/commerce.cid.toml"
/// # Provider-only: the mediated surface this plugin implements (ADR-0022 §7).
/// provides_operations = ["issue_coupon", "authorize_redemption", "check_nullifier", "decide_tier"]
/// provides_events = [
///   "commerce.issue.completed", "commerce.redeem.completed",
///   "commerce.nullifier.check.completed", "commerce.tier.decide.completed",
/// ]
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityInterfaceRef {
    /// CID identity, e.g. `commerce@1.x` (`name@semver-range`).
    pub cid: String,
    /// Optional path to the CID descriptor file (TOML-declared in v1).
    #[serde(default)]
    pub spec: Option<String>,

    /// **Provider-declared** mediated operations this plugin services
    /// (ADR-0022 §7). A consumer never sets this; a provider lists the CID
    /// `[[operations]]` names it implements so the loader can validate the
    /// declared surface against the CID at install. Empty on a pure consumer.
    ///
    /// This is the honest place a provider states *what it actually services*:
    /// the `[capabilities.provided] <name> = "<version>"` map only carries the
    /// CID version, not the surface, so without this field there is nothing to
    /// validate the operation/event coverage against (ADR-0022 §7 gap).
    #[serde(default)]
    pub provides_operations: Vec<String>,

    /// **Provider-declared** result/notification events this plugin emits
    /// (ADR-0022 §7). Must cover every CID `result_event` and every
    /// `events.emitted_by_provider`. Empty on a pure consumer.
    #[serde(default)]
    pub provides_events: Vec<String>,
}

/// A capability version validation failure (ADR-0022): a declared range or
/// concrete version was not valid semver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityVersionError {
    /// The capability name whose version/range was malformed.
    pub capability: String,
    /// The offending version/range string.
    pub value: String,
    /// Whether this came from a `required`/`optional` range or a `provided`
    /// concrete version.
    pub kind: CapabilityVersionKind,
    /// The underlying semver parse error message.
    pub reason: String,
}

/// Which capability map a [`CapabilityVersionError`] originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityVersionKind {
    /// A `required`/`optional` semver **range** (`VersionReq`).
    Range,
    /// A `provided` **concrete version** (`Version`).
    Version,
}

impl std::fmt::Display for CapabilityVersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            CapabilityVersionKind::Range => "range",
            CapabilityVersionKind::Version => "version",
        };
        write!(
            f,
            "capability '{}' has invalid semver {kind} '{}': {}",
            self.capability, self.value, self.reason
        )
    }
}

impl std::error::Error for CapabilityVersionError {}

impl PluginCapabilities {
    /// Validate every declared capability version using the `semver` crate
    /// (ADR-0022 step 1 item E — no hand-rolled version parsing):
    ///
    /// - `required` / `optional` values must parse as a [`semver::VersionReq`]
    ///   (a range like `^1.0`).
    /// - `provided` values must parse as a concrete [`semver::Version`]
    ///   (e.g. `1.2.0`).
    ///
    /// Returns all offending entries (deterministic order, since the maps are
    /// `BTreeMap`). An empty `Vec` means every declared version is well-formed.
    /// This does not perform provider *resolution* (that is the Step 2 loader);
    /// it only checks that the manifest's declared versions are syntactically
    /// valid semver.
    pub fn validate_versions(&self) -> Vec<CapabilityVersionError> {
        let mut errors = Vec::new();
        for (map, kind) in [
            (&self.required, CapabilityVersionKind::Range),
            (&self.optional, CapabilityVersionKind::Range),
        ] {
            for (cap, range) in map {
                if let Err(e) = range.parse::<semver::VersionReq>() {
                    errors.push(CapabilityVersionError {
                        capability: cap.clone(),
                        value: range.clone(),
                        kind,
                        reason: e.to_string(),
                    });
                }
            }
        }
        for (cap, version) in &self.provided {
            if let Err(e) = version.parse::<semver::Version>() {
                errors.push(CapabilityVersionError {
                    capability: cap.clone(),
                    value: version.clone(),
                    kind: CapabilityVersionKind::Version,
                    reason: e.to_string(),
                });
            }
        }
        errors
    }
}

// ── Unified manifest parsing ─────────────────────────────────────────────────

/// Error type for manifest parsing.
#[derive(Debug)]
pub enum ManifestError {
    ParseFailed(String),
    JsonError(serde_json::Error),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::ParseFailed(msg) => write!(f, "manifest parse failed: {msg}"),
            ManifestError::JsonError(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<serde_json::Error> for ManifestError {
    fn from(e: serde_json::Error) -> Self {
        ManifestError::JsonError(e)
    }
}

/// Parse a plugin manifest from either TOML or JSON.
///
/// Tries TOML first (native format), then JSON (legacy pares-modulus format).
pub fn parse_manifest(content: &str) -> Result<PluginManifest, ManifestError> {
    // Try TOML first — look for `[plugin]` header as a quick heuristic
    if let Ok(manifest) = parse_toml_manifest_unified(content) {
        return Ok(manifest);
    }
    // Try JSON (legacy modulus format)
    if let Ok(manifest) = parse_json_manifest(content) {
        return Ok(manifest);
    }
    Err(ManifestError::ParseFailed(
        "Could not parse as TOML or JSON".into(),
    ))
}

/// Parse a native TOML manifest into `PluginManifest`.
fn parse_toml_manifest_unified(toml_str: &str) -> Result<PluginManifest, ManifestError> {
    // Re-use the TOML parsing from runtime (via serde)
    // We deserialize through our own intermediate structure here.
    let value: toml::Value =
        toml::from_str(toml_str).map_err(|e| ManifestError::ParseFailed(e.to_string()))?;

    let plugin = value
        .get("plugin")
        .ok_or_else(|| ManifestError::ParseFailed("missing [plugin] table".into()))?;

    let name = plugin
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let version = plugin
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let description = plugin
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let author = plugin
        .get("author")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Schema
    let entities = parse_toml_entities(&value);
    let relationships = parse_toml_relationships(&value);

    // Logic
    let rules = parse_toml_rules(&value);
    let constraints = parse_toml_constraints(&value);

    // Permissions
    let permissions = parse_toml_permissions(&value);

    // Dependencies ([dependencies].plugins) — previously dropped (C-DRIFT-001).
    let dependencies = parse_toml_dependencies(&value);

    // Capabilities ([capabilities.required/optional/provided] + interface.*).
    let capabilities = parse_toml_capabilities(&value);

    Ok(PluginManifest {
        name,
        version,
        description,
        author,
        schema: PluginSchema {
            entities,
            relationships,
        },
        logic: PluginLogic {
            rules,
            constraints,
            procedures: Vec::new(),
        },
        tools: Vec::new(),
        ui: None,
        permissions,
        hooks: Vec::new(),
        dependencies,
        capabilities,
    })
}

/// Parse a legacy pares-modulus JSON manifest into our unified format.
fn parse_json_manifest(json: &str) -> Result<PluginManifest, ManifestError> {
    let v: serde_json::Value = serde_json::from_str(json)?;

    // Map legacy fields:
    // "id" → name, "name" → description (legacy "name" is display name)
    let name = v["id"]
        .as_str()
        .unwrap_or(v["name"].as_str().unwrap_or("unknown"))
        .to_string();
    let version = v["version"].as_str().unwrap_or("0.0.0").to_string();
    let description = v["description"]
        .as_str()
        .unwrap_or(v["name"].as_str().unwrap_or(""))
        .to_string();
    let author = v["author"].as_str().map(String::from);

    // Map inference rules → PluginLogic rules
    let rules = map_inference_rules(&v["inferenceRules"]);

    // Map routes → PluginUI
    let ui = map_routes(&v["routes"]);

    Ok(PluginManifest {
        name,
        version,
        description,
        author,
        schema: PluginSchema {
            entities: vec![],
            relationships: vec![],
        },
        logic: PluginLogic {
            rules,
            constraints: vec![],
            procedures: vec![],
        },
        tools: vec![],
        ui,
        permissions: PluginPermissions {
            pluresdb_scopes: vec!["read".into(), "write".into()],
            tool_access: vec![],
            network: false,
        },
        hooks: vec![],
        dependencies: vec![],
        // Legacy JSON modulus manifests predate the capability model (ADR-0022).
        capabilities: PluginCapabilities::default(),
    })
}

// ── JSON helper mappers ──────────────────────────────────────────────────────

fn map_inference_rules(v: &serde_json::Value) -> Vec<PluginRule> {
    let Some(arr) = v.as_array() else {
        return vec![];
    };
    arr.iter()
        .filter_map(|rule| {
            let name = rule["name"].as_str()?.to_string();
            let description = rule["description"].as_str().unwrap_or("").to_string();
            let condition = rule["condition"].as_str().unwrap_or("").to_string();
            let action = rule["action"].as_str().unwrap_or("").to_string();
            Some(PluginRule {
                name,
                description,
                condition,
                action,
            })
        })
        .collect()
}

fn map_routes(v: &serde_json::Value) -> Option<PluginUI> {
    let arr = v.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let routes = arr
        .iter()
        .filter_map(|route| {
            let path = route["path"].as_str()?.to_string();
            let component = route["component"].as_str().unwrap_or("").to_string();
            let label = route["label"]
                .as_str()
                .unwrap_or(route["name"].as_str().unwrap_or(""))
                .to_string();
            Some(UIRoute {
                path,
                component,
                label,
            })
        })
        .collect();
    Some(PluginUI {
        components: vec![],
        routes,
    })
}

// ── TOML helper parsers ──────────────────────────────────────────────────────

fn parse_toml_entities(value: &toml::Value) -> Vec<EntityDefinition> {
    let Some(schema) = value.get("schema") else {
        return vec![];
    };
    let Some(entities) = schema.get("entities").and_then(|e| e.as_array()) else {
        return vec![];
    };
    entities
        .iter()
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?.to_string();
            let display_name = e.get("display_name")?.as_str()?.to_string();
            let icon = e.get("icon").and_then(|v| v.as_str()).map(String::from);
            let fields = e
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            serde_json::to_string(f)
                                .ok()
                                .and_then(|s| serde_json::from_str(&s).ok())
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(EntityDefinition {
                name,
                display_name,
                icon,
                fields,
            })
        })
        .collect()
}

fn parse_toml_relationships(value: &toml::Value) -> Vec<RelationshipDefinition> {
    let Some(schema) = value.get("schema") else {
        return vec![];
    };
    let Some(rels) = schema.get("relationships").and_then(|r| r.as_array()) else {
        return vec![];
    };
    rels.iter()
        .filter_map(|r| {
            Some(RelationshipDefinition {
                name: r.get("name")?.as_str()?.to_string(),
                from_entity: r.get("from_entity")?.as_str()?.to_string(),
                to_entity: r.get("to_entity")?.as_str()?.to_string(),
                cardinality: r.get("cardinality")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn parse_toml_rules(value: &toml::Value) -> Vec<PluginRule> {
    let Some(logic) = value.get("logic") else {
        return vec![];
    };
    let Some(rules) = logic.get("rules").and_then(|r| r.as_array()) else {
        return vec![];
    };
    rules
        .iter()
        .filter_map(|r| {
            Some(PluginRule {
                name: r.get("name")?.as_str()?.to_string(),
                description: r
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                condition: r
                    .get("condition")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                action: r
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

fn parse_toml_constraints(value: &toml::Value) -> Vec<PluginConstraint> {
    let Some(logic) = value.get("logic") else {
        return vec![];
    };
    let Some(constraints) = logic.get("constraints").and_then(|c| c.as_array()) else {
        return vec![];
    };
    constraints
        .iter()
        .filter_map(|c| {
            Some(PluginConstraint {
                name: c.get("name")?.as_str()?.to_string(),
                description: c
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                check: c
                    .get("check")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                error_message: c
                    .get("error_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

fn parse_toml_permissions(value: &toml::Value) -> PluginPermissions {
    let Some(perms) = value.get("permissions") else {
        return PluginPermissions::default();
    };
    PluginPermissions {
        pluresdb_scopes: perms
            .get("pluresdb_scopes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        tool_access: perms
            .get("tool_access")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        network: perms
            .get("network")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// Parse `[dependencies].plugins` into the flat `Vec<String>` of plugin names
/// that `PluginManifest::dependencies` expects.
///
/// Previously this table was silently dropped on the TOML path (C-DRIFT-001),
/// so TOML plugins could not declare dependencies at all. The canonical TOML
/// shape is:
/// ```toml
/// [dependencies]
/// plugins = ["base-plugin", "another-plugin"]
/// ```
fn parse_toml_dependencies(value: &toml::Value) -> Vec<String> {
    let Some(deps) = value.get("dependencies") else {
        return vec![];
    };
    deps.get("plugins")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the `[capabilities.*]` tables (ADR-0022 §6) into [`PluginCapabilities`].
///
/// Handles three string→string maps (`required`, `optional`, `provided`) and the
/// nested `[capabilities.interface.<name>]` blocks. Unknown / missing tables
/// yield empty maps (the field is `#[serde(default)]`).
fn parse_toml_capabilities(value: &toml::Value) -> PluginCapabilities {
    let Some(caps) = value.get("capabilities") else {
        return PluginCapabilities::default();
    };
    PluginCapabilities {
        required: parse_toml_string_map(caps.get("required")),
        optional: parse_toml_string_map(caps.get("optional")),
        provided: parse_toml_string_map(caps.get("provided")),
        interface: parse_toml_interface_map(caps.get("interface")),
    }
}

/// Parse a TOML table of `key = "value"` string pairs into a `BTreeMap`.
/// Non-string values are skipped (the loader validates semver later).
fn parse_toml_string_map(table: Option<&toml::Value>) -> BTreeMap<String, String> {
    let Some(tbl) = table.and_then(|v| v.as_table()) else {
        return BTreeMap::new();
    };
    tbl.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

/// Parse the `[capabilities.interface.<name>]` sub-tables into a map of
/// capability name → [`CapabilityInterfaceRef`]. Entries missing the required
/// `cid` key are skipped.
fn parse_toml_interface_map(
    table: Option<&toml::Value>,
) -> BTreeMap<String, CapabilityInterfaceRef> {
    let Some(tbl) = table.and_then(|v| v.as_table()) else {
        return BTreeMap::new();
    };
    tbl.iter()
        .filter_map(|(name, block)| {
            let cid = block.get("cid")?.as_str()?.to_string();
            let spec = block.get("spec").and_then(|v| v.as_str()).map(String::from);
            let provides_operations = block
                .get("provides_operations")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let provides_events = block
                .get("provides_events")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Some((
                name.clone(),
                CapabilityInterfaceRef {
                    cid,
                    spec,
                    provides_operations,
                    provides_events,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_from_json() {
        let json = serde_json::json!({
            "name": "test-plugin",
            "version": "0.1.0",
            "description": "A test plugin",
            "schema": {
                "entities": [{
                    "name": "widget",
                    "display_name": "Widget",
                    "fields": [{
                        "name": "color",
                        "field_type": "String",
                        "required": true
                    }]
                }],
                "relationships": []
            },
            "logic": { "rules": [], "constraints": [], "procedures": [] },
            "tools": [],
            "permissions": { "pluresdb_scopes": ["read"], "tool_access": [], "network": false }
        });
        let manifest: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.schema.entities.len(), 1);
        assert_eq!(manifest.schema.entities[0].fields[0].name, "color");
    }

    #[test]
    fn parse_enum_field_type() {
        let json = serde_json::json!({
            "name": "status",
            "field_type": { "Enum": ["active", "archived"] },
            "required": false
        });
        let field: FieldDefinition = serde_json::from_value(json).unwrap();
        match &field.field_type {
            FieldType::Enum(vals) => assert_eq!(vals, &["active", "archived"]),
            _ => panic!("expected Enum"),
        }
    }

    // ── Unified manifest parsing tests ───────────────────────────────────

    #[test]
    fn parse_manifest_toml_format() {
        let toml = r#"
[plugin]
name = "home-inventory"
version = "0.1.0"
description = "Track household items"
author = "plures"

[[schema.entities]]
name = "item"
display_name = "Item"
icon = "📦"

[[schema.entities.fields]]
name = "name"
field_type = "String"
required = true

[[schema.entities.fields]]
name = "location"
field_type = "String"
required = false

[[schema.relationships]]
name = "item_in_room"
from_entity = "item"
to_entity = "room"
cardinality = "many_to_one"

[permissions]
pluresdb_scopes = ["read", "write"]
network = false
"#;
        let manifest = parse_manifest(toml).expect("should parse TOML");
        assert_eq!(manifest.name, "home-inventory");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.schema.entities.len(), 1);
        assert_eq!(manifest.schema.entities[0].name, "item");
        assert_eq!(manifest.schema.entities[0].fields.len(), 2);
        assert_eq!(manifest.schema.relationships.len(), 1);
        assert_eq!(manifest.permissions.pluresdb_scopes, vec!["read", "write"]);
        assert!(!manifest.permissions.network);
    }

    #[test]
    fn parse_manifest_json_legacy_format() {
        let json = r#"{
            "id": "financial-advisor",
            "name": "Financial Advisor",
            "version": "1.0.0",
            "description": "Personal finance management",
            "author": "plures",
            "routes": [
                { "path": "/dashboard", "component": "Dashboard.svelte", "label": "Dashboard" },
                { "path": "/accounts", "component": "Accounts.svelte", "label": "Accounts" }
            ],
            "inferenceRules": [
                {
                    "name": "vendor-match",
                    "description": "Match transactions to known vendors",
                    "condition": "transaction.vendor is unknown",
                    "action": "suggest vendor from alias map"
                }
            ]
        }"#;
        let manifest = parse_manifest(json).expect("should parse JSON");
        assert_eq!(manifest.name, "financial-advisor");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.description, "Personal finance management");
        assert_eq!(manifest.author, Some("plures".to_string()));
        // JSON plugins don't have schema entities
        assert!(manifest.schema.entities.is_empty());
        // But they do have logic rules mapped from inferenceRules
        assert_eq!(manifest.logic.rules.len(), 1);
        assert_eq!(manifest.logic.rules[0].name, "vendor-match");
        // And UI routes
        let ui = manifest.ui.as_ref().expect("should have UI");
        assert_eq!(ui.routes.len(), 2);
        assert_eq!(ui.routes[0].path, "/dashboard");
    }

    #[test]
    fn parse_manifest_unknown_format_fails() {
        let garbage = "this is neither TOML nor JSON !!@#$";
        assert!(parse_manifest(garbage).is_err());
    }

    #[test]
    fn parse_manifest_both_produce_valid_manifest() {
        // Verify both formats produce a manifest with the required fields
        let toml = r#"
[plugin]
name = "test-toml"
version = "0.1.0"
description = "TOML test"

[permissions]
pluresdb_scopes = ["read"]
network = false
"#;
        let json = r#"{"id": "test-json", "version": "0.1.0", "description": "JSON test"}"#;

        let toml_m = parse_manifest(toml).unwrap();
        let json_m = parse_manifest(json).unwrap();

        // Both have name and version
        assert_eq!(toml_m.name, "test-toml");
        assert_eq!(json_m.name, "test-json");
        assert_eq!(toml_m.version, "0.1.0");
        assert_eq!(json_m.version, "0.1.0");
    }

    // ── Capability + dependency parsing (ADR-0022 step 1) ──────────────────

    /// Full consumer manifest modeled on inner-space's real `plugin.toml`:
    /// `[dependencies]` + all three capability tables + an
    /// `[capabilities.interface.commerce]` block. Asserts dependencies are NO
    /// LONGER dropped (C-DRIFT-001) and that capabilities are fully populated.
    #[test]
    fn parse_manifest_toml_with_dependencies_and_capabilities() {
        let toml = r#"
[plugin]
name = "inner-space"
version = "0.1.0"
description = "Micro-scale combat and colony game in your real surroundings"
author = "plures"

[dependencies]
plugins = ["commerce-provider", "scene-provider"]

[capabilities.required]
scanning = "^1.0"
scene = "^1.0"
physics = "^1.0"
audio = "^1.0"
input = "^1.0"
location = "^1.0"
commerce = "^1.0"
network = "^1.0"

[capabilities.optional]
ar = "^1.0"
notify = "^1.0"
media = "^1.0"

[capabilities.interface.commerce]
cid = "commerce@1.x"
spec = "capabilities/commerce.cid.toml"
"#;
        let m = parse_manifest(toml).expect("should parse consumer TOML");

        // Dependencies are no longer silently dropped (C-DRIFT-001 fix).
        assert_eq!(
            m.dependencies,
            vec![
                "commerce-provider".to_string(),
                "scene-provider".to_string()
            ],
            "[dependencies].plugins must be parsed, not dropped"
        );

        // Required capabilities (semver ranges) — mirrors inner-space.
        assert_eq!(m.capabilities.required.len(), 8);
        assert_eq!(
            m.capabilities.required.get("commerce").map(String::as_str),
            Some("^1.0")
        );
        assert_eq!(
            m.capabilities.required.get("scanning").map(String::as_str),
            Some("^1.0")
        );
        assert_eq!(
            m.capabilities.required.get("network").map(String::as_str),
            Some("^1.0")
        );

        // Optional capabilities.
        assert_eq!(m.capabilities.optional.len(), 3);
        assert_eq!(
            m.capabilities.optional.get("ar").map(String::as_str),
            Some("^1.0")
        );

        // Consumer provides nothing.
        assert!(m.capabilities.provided.is_empty());

        // Interface reference resolved.
        let iface = m
            .capabilities
            .interface
            .get("commerce")
            .expect("commerce interface ref present");
        assert_eq!(iface.cid, "commerce@1.x");
        assert_eq!(iface.spec.as_deref(), Some("capabilities/commerce.cid.toml"));

        // Declared ranges are valid semver (uses the `semver` crate, no
        // hand-rolled parsing).
        assert!(
            m.capabilities.validate_versions().is_empty(),
            "all declared capability ranges should be valid semver"
        );
    }

    /// A provider manifest declaring `[capabilities.provided] commerce = "1.2.0"`
    /// must round-trip: parse from TOML, then serialize+deserialize unchanged.
    #[test]
    fn parse_manifest_provider_capability_roundtrips() {
        let toml = r#"
[plugin]
name = "oasis-commerce"
version = "1.2.0"
description = "ZK commerce capability provider (ported from OASIS)"
author = "plures"

[capabilities.provided]
commerce = "1.2.0"

[capabilities.interface.commerce]
cid = "commerce@1.x"
spec = "capabilities/commerce.cid.toml"
"#;
        let m = parse_manifest(toml).expect("should parse provider TOML");

        assert_eq!(
            m.capabilities.provided.get("commerce").map(String::as_str),
            Some("1.2.0")
        );
        assert!(m.capabilities.required.is_empty());
        assert!(m.capabilities.optional.is_empty());

        // Provided concrete version is valid semver.
        assert!(m.capabilities.validate_versions().is_empty());

        // JSON round-trip preserves the capability block exactly (serde).
        let json = serde_json::to_string(&m).expect("serialize");
        let back: PluginManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.capabilities, m.capabilities);
        assert_eq!(
            back.capabilities.provided.get("commerce").map(String::as_str),
            Some("1.2.0")
        );
    }

    /// A manifest with no `[capabilities]` table yields an empty (default)
    /// `PluginCapabilities` — absence is honest, not an error.
    #[test]
    fn parse_manifest_without_capabilities_is_empty_default() {
        let toml = r#"
[plugin]
name = "plain"
version = "0.1.0"
description = "no capabilities"
"#;
        let m = parse_manifest(toml).unwrap();
        assert_eq!(m.capabilities, PluginCapabilities::default());
        assert!(m.capabilities.required.is_empty());
        assert!(m.capabilities.validate_versions().is_empty());
    }

    /// `validate_versions` actually exercises the `semver` crate: a malformed
    /// range and a malformed concrete version are both reported.
    #[test]
    fn validate_versions_flags_bad_semver() {
        let toml = r#"
[plugin]
name = "bad-versions"
version = "0.1.0"
description = "intentionally malformed capability versions"

[capabilities.required]
commerce = "not-a-range"

[capabilities.provided]
scene = "also-bad"
"#;
        let m = parse_manifest(toml).unwrap();
        // Parsing still succeeds (schema layer accepts the strings); validation
        // is a separate, explicit step.
        let errors = m.capabilities.validate_versions();
        assert_eq!(errors.len(), 2, "both malformed versions should be flagged");

        let by_cap: std::collections::BTreeMap<_, _> =
            errors.iter().map(|e| (e.capability.as_str(), e.kind)).collect();
        assert_eq!(by_cap.get("commerce"), Some(&CapabilityVersionKind::Range));
        assert_eq!(by_cap.get("scene"), Some(&CapabilityVersionKind::Version));
    }
}
