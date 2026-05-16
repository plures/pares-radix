//! Plugin manifest types — the declaration of what a plugin provides.

use serde::{Deserialize, Serialize};

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
        dependencies: Vec::new(),
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
}
