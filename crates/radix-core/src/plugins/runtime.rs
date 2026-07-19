//! Plugin runtime — loads, manages, and exposes plugins to the agent.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::model::ToolDefinition;
use crate::plugins::capability::resolve_capabilities;
use crate::plugins::crud;
use crate::plugins::error::PluginError;
use crate::plugins::manifest::*;

/// A loaded, running plugin.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub installed_at: u64,
}

/// The plugin runtime manages all installed plugins.
pub struct PluginRuntime {
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
}

impl PluginRuntime {
    /// Create an empty plugin runtime.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Install a plugin from a manifest.
    /// Validates dependencies are satisfied before installing.
    pub async fn install(&self, manifest: PluginManifest) -> Result<(), PluginError> {
        let name = manifest.name.clone();
        let mut plugins = self.plugins.write().await;
        if plugins.contains_key(&name) {
            return Err(PluginError::AlreadyInstalled(name));
        }
        // Validate: check that all declared dependencies are installed
        for dep in &manifest.dependencies {
            if !plugins.contains_key(dep) {
                return Err(PluginError::MissingDependency {
                    plugin: name,
                    dependency: dep.clone(),
                });
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        plugins.insert(
            name,
            LoadedPlugin {
                manifest,
                installed_at: now,
            },
        );
        Ok(())
    }

    /// Install multiple plugins in dependency order (topological sort).
    ///
    /// The ordering now includes capability-provider edges (ADR-0022 §3): a
    /// consumer is installed after every provider that satisfies one of its
    /// required capabilities. A required capability with no satisfying provider
    /// (or an ambiguous one) is surfaced as a real
    /// [`PluginError::UnsatisfiedCapability`] / [`PluginError::AmbiguousCapability`]
    /// from [`topological_sort`] — installation does not proceed.
    pub async fn install_batch(
        &self,
        manifests: Vec<PluginManifest>,
    ) -> Result<Vec<String>, PluginError> {
        let (installed, _bindings) = self.install_batch_resolving(manifests).await?;
        Ok(installed)
    }

    /// Like [`Self::install_batch`], but also returns the resolved capability
    /// bindings (ADR-0022 §4) computed from the same manifest set used for
    /// ordering. The bindings are an in-memory, rebuildable index; persist them
    /// to PluresDB (the source of truth, C-PLURES-003) via
    /// [`Self::install_batch_persisting`].
    pub async fn install_batch_resolving(
        &self,
        manifests: Vec<PluginManifest>,
    ) -> Result<(Vec<String>, Vec<crate::plugins::capability::CapabilityBinding>), PluginError> {
        let ordered = topological_sort(&manifests)?;
        // Resolve bindings from the full batch (same policy the topo-sort used).
        let bindings = resolve_capabilities(&manifests)?;
        let mut installed = Vec::new();
        for manifest in ordered {
            let name = manifest.name.clone();
            self.install(manifest).await?;
            installed.push(name);
        }
        Ok((installed, bindings))
    }

    /// Install a batch (in capability order) and persist every resolved binding
    /// to PluresDB via the given executor (ADR-0022 §4, C-PLURES-003).
    ///
    /// This is the real resolve+persist path requirement D wires. It is kept
    /// separate from [`Self::install_batch`] because [`PluginRuntime`] is a pure
    /// in-memory registry with no store handle of its own; the caller owns the
    /// [`PluginCrudExecutor`] (and therefore the PluresDB store) and threads it
    /// in. The production boot call site that constructs both is a separate
    /// step (not wired here).
    pub async fn install_batch_persisting(
        &self,
        manifests: Vec<PluginManifest>,
        executor: &crate::plugins::executor::PluginCrudExecutor,
    ) -> Result<Vec<String>, PluginError> {
        let (installed, bindings) = self.install_batch_resolving(manifests).await?;
        for binding in &bindings {
            executor.persist_binding(binding)?;
        }
        Ok(installed)
    }

    /// Install from a TOML string. The TOML uses `[plugin]` as the top-level
    /// table with `schema`, `logic`, `permissions` as siblings.
    pub async fn install_from_toml(&self, toml_str: &str) -> Result<String, PluginError> {
        let manifest = parse_toml_manifest(toml_str)?;
        let name = manifest.name.clone();
        self.install(manifest).await?;
        Ok(name)
    }

    /// List installed plugins.
    pub async fn list(&self) -> Vec<PluginManifest> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| p.manifest.clone())
            .collect()
    }

    /// Uninstall a plugin by name.
    pub async fn uninstall(&self, name: &str, _delete_data: bool) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        if plugins.remove(name).is_none() {
            return Err(PluginError::NotFound(name.to_string()));
        }
        // TODO: if delete_data, remove PluresDB nodes tagged with this plugin
        Ok(())
    }

    /// Get a specific plugin's manifest.
    pub async fn get(&self, name: &str) -> Option<PluginManifest> {
        self.plugins
            .read()
            .await
            .get(name)
            .map(|p| p.manifest.clone())
    }

    /// Build tool definitions for all installed plugins.
    /// Returns the generic CRUD tools parameterized by the available entity types.
    pub async fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let plugins = self.plugins.read().await;
        let mut entity_types: Vec<String> = Vec::new();
        for plugin in plugins.values() {
            for entity in &plugin.manifest.schema.entities {
                entity_types.push(format!("{}/{}", plugin.manifest.name, entity.name));
            }
        }
        if entity_types.is_empty() {
            return Vec::new();
        }
        crud::tool_definitions(&entity_types)
    }

    /// Generate schema context for system prompt injection.
    pub async fn schema_context(&self) -> String {
        let plugins = self.plugins.read().await;
        if plugins.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n## Installed Plugins\n");
        for plugin in plugins.values() {
            let m = &plugin.manifest;
            out.push_str(&format!(
                "\n### {} (v{})\n{}\n",
                m.name, m.version, m.description
            ));
            if !m.schema.entities.is_empty() {
                out.push_str("Entities: ");
                let entities: Vec<String> = m
                    .schema
                    .entities
                    .iter()
                    .map(|e| {
                        let fields: Vec<&str> = e.fields.iter().map(|f| f.name.as_str()).collect();
                        format!("{} ({})", e.display_name, fields.join(", "))
                    })
                    .collect();
                out.push_str(&entities.join(", "));
                out.push('\n');
            }
            if !m.schema.relationships.is_empty() {
                out.push_str("Relationships: ");
                let rels: Vec<String> = m
                    .schema
                    .relationships
                    .iter()
                    .map(|r| format!("{} {} {}", r.from_entity, r.name, r.to_entity))
                    .collect();
                out.push_str(&rels.join(", "));
                out.push('\n');
            }
            out.push_str(
                "Tools: plugin_create, plugin_list, plugin_update, plugin_delete, plugin_search\n",
            );
        }
        out
    }
}

// ── Topological sort ─────────────────────────────────────────────────────────

/// Sort plugins in dependency order. Returns error on circular dependencies.
fn topological_sort(manifests: &[PluginManifest]) -> Result<Vec<PluginManifest>, PluginError> {
    use std::collections::{HashMap, VecDeque};

    let name_map: HashMap<&str, &PluginManifest> =
        manifests.iter().map(|m| (m.name.as_str(), m)).collect();

    // Kahn's algorithm
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for m in manifests {
        in_degree.entry(m.name.as_str()).or_insert(0);
        for dep in &m.dependencies {
            if name_map.contains_key(dep.as_str()) {
                *in_degree.entry(m.name.as_str()).or_insert(0) += 1;
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(m.name.as_str());
            }
            // External deps (not in batch) are assumed satisfied
        }
    }

    // Capability-provider edges (ADR-0022 §3): a consumer's REQUIRED capability
    // must activate AFTER its resolved provider, so add a provider → consumer
    // edge for every resolved binding. `resolve_capabilities` applies the §4
    // binding-selection policy and returns a real `UnsatisfiedCapability` /
    // `AmbiguousCapability` error if a required capability cannot be bound
    // deterministically — propagated here, never silently skipped.
    //
    // We only add an edge when BOTH provider and consumer are in this batch
    // (an out-of-batch provider is assumed already installed, exactly like an
    // out-of-batch dependency above), and we de-duplicate against any identical
    // dependency edge already recorded, so the in-degree is not double-counted
    // when a plugin is both a declared dependency AND a capability provider.
    let bindings = resolve_capabilities(manifests)?;
    for binding in &bindings {
        let provider = binding.provider.as_str();
        let consumer = binding.consumer.as_str();
        if provider == consumer {
            continue;
        }
        // Map the binding's owned names back to the `&str` keys that borrow from
        // `manifests` (these outlive the local `bindings`). If the provider is
        // not in this batch, it is assumed already installed (same treatment as
        // an out-of-batch dependency above).
        let (Some(&provider_name), Some(&consumer_name)) = (
            name_map.get_key_value(provider).map(|(k, _v)| k),
            name_map.get_key_value(consumer).map(|(k, _v)| k),
        ) else {
            continue;
        };
        let edge_exists = dependents
            .get(provider_name)
            .is_some_and(|deps| deps.contains(&consumer_name));
        if edge_exists {
            continue;
        }
        *in_degree.entry(consumer_name).or_insert(0) += 1;
        dependents
            .entry(provider_name)
            .or_default()
            .push(consumer_name);
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut result = Vec::new();
    while let Some(name) = queue.pop_front() {
        if let Some(m) = name_map.get(name) {
            result.push((*m).clone());
        }
        if let Some(deps) = dependents.get(name) {
            for &dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    if result.len() != manifests.len() {
        let stuck: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&name, _)| name)
            .collect();
        return Err(PluginError::CircularDependency(stuck.join(", ")));
    }

    Ok(result)
}

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ── TOML parsing ─────────────────────────────────────────────────────────────

/// Parse a native TOML plugin manifest.
///
/// **C-DRIFT-001 fix (ADR-0022 step 1):** this previously held a *second*,
/// independent serde-based TOML deserializer that disagreed with
/// [`manifest::parse_manifest`] — most notably both silently dropped
/// `[dependencies]`, and a divergent second parser would also have dropped the
/// new `[capabilities.*]` tables. The two parsers are now collapsed into a
/// single source of truth: this delegates to [`parse_manifest`] (the unified
/// TOML-first, JSON-legacy-fallback parser). There is exactly one place that
/// turns manifest source text into a [`PluginManifest`].
fn parse_toml_manifest(toml_str: &str) -> Result<PluginManifest, PluginError> {
    parse_manifest(toml_str).map_err(|e| PluginError::TomlParse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn install_and_list() {
        let rt = PluginRuntime::new();
        let manifest = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "Test plugin".into(),
            author: None,
            schema: PluginSchema::default(),
            logic: PluginLogic::default(),
            tools: Vec::new(),
            ui: None,
            permissions: PluginPermissions::default(),
            hooks: Vec::new(),
            dependencies: Vec::new(),
            capabilities: PluginCapabilities::default(),
        };
        rt.install(manifest).await.unwrap();
        assert_eq!(rt.list().await.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_install_fails() {
        let rt = PluginRuntime::new();
        let manifest = PluginManifest {
            name: "dupe".into(),
            version: "1.0.0".into(),
            description: "".into(),
            author: None,
            schema: PluginSchema::default(),
            logic: PluginLogic::default(),
            tools: Vec::new(),
            ui: None,
            permissions: PluginPermissions::default(),
            hooks: Vec::new(),
            dependencies: Vec::new(),
            capabilities: PluginCapabilities::default(),
        };
        rt.install(manifest.clone()).await.unwrap();
        assert!(rt.install(manifest).await.is_err());
    }

    #[tokio::test]
    async fn uninstall_works() {
        let rt = PluginRuntime::new();
        let manifest = PluginManifest {
            name: "removable".into(),
            version: "1.0.0".into(),
            description: "".into(),
            author: None,
            schema: PluginSchema::default(),
            logic: PluginLogic::default(),
            tools: Vec::new(),
            ui: None,
            permissions: PluginPermissions::default(),
            hooks: Vec::new(),
            dependencies: Vec::new(),
            capabilities: PluginCapabilities::default(),
        };
        rt.install(manifest).await.unwrap();
        rt.uninstall("removable", false).await.unwrap();
        assert!(rt.list().await.is_empty());
    }

    #[tokio::test]
    async fn schema_context_generation() {
        let rt = PluginRuntime::new();
        let manifest = PluginManifest {
            name: "inventory".into(),
            version: "1.0.0".into(),
            description: "Track stuff".into(),
            author: None,
            schema: PluginSchema {
                entities: vec![super::super::EntityDefinition {
                    name: "item".into(),
                    display_name: "Item".into(),
                    fields: vec![super::super::FieldDefinition {
                        name: "name".into(),
                        field_type: super::super::FieldType::String,
                        required: true,
                        default: None,
                        description: None,
                    }],
                    icon: Some("📦".into()),
                }],
                relationships: Vec::new(),
            },
            logic: PluginLogic::default(),
            tools: Vec::new(),
            ui: None,
            permissions: PluginPermissions::default(),
            hooks: Vec::new(),
            dependencies: Vec::new(),
            capabilities: PluginCapabilities::default(),
        };
        rt.install(manifest).await.unwrap();
        let ctx = rt.schema_context().await;
        assert!(ctx.contains("inventory"));
        assert!(ctx.contains("Item (name)"));
        assert!(ctx.contains("plugin_create"));
    }

    #[tokio::test]
    async fn install_from_toml_works() {
        let toml = r#"
[plugin]
name = "from-toml"
version = "0.1.0"
description = "Parsed from TOML"

[[schema.entities]]
name = "thing"
display_name = "Thing"

[[schema.entities.fields]]
name = "label"
field_type = "String"
required = true

[permissions]
pluresdb_scopes = ["read"]
network = false
"#;
        let rt = PluginRuntime::new();
        let name = rt.install_from_toml(toml).await.unwrap();
        assert_eq!(name, "from-toml");
        let plugins = rt.list().await;
        assert_eq!(plugins[0].schema.entities[0].name, "thing");
    }

    /// C-DRIFT-001 regression: the `install_from_toml` path (formerly a second,
    /// divergent TOML parser) now delegates to `manifest::parse_manifest`, so it
    /// must carry BOTH `[dependencies].plugins` AND `[capabilities.*]` through to
    /// the installed manifest — neither was parsed on this path before.
    #[tokio::test]
    async fn install_from_toml_carries_dependencies_and_capabilities() {
        let provider_toml = r#"
[plugin]
name = "commerce-provider"
version = "1.2.0"
description = "provides commerce"

[capabilities.provided]
commerce = "1.2.0"
"#;
        let consumer_toml = r#"
[plugin]
name = "shop-app"
version = "0.1.0"
description = "consumes commerce"

[dependencies]
plugins = ["commerce-provider"]

[capabilities.required]
commerce = "^1.0"

[capabilities.optional]
notify = "^1.0"
"#;
        let rt = PluginRuntime::new();
        // Provider must exist first (install() validates declared deps).
        rt.install_from_toml(provider_toml).await.unwrap();
        rt.install_from_toml(consumer_toml).await.unwrap();

        let provider = rt.get("commerce-provider").await.unwrap();
        assert_eq!(
            provider
                .capabilities
                .provided
                .get("commerce")
                .map(String::as_str),
            Some("1.2.0"),
            "provided capability must survive the install_from_toml path"
        );

        let consumer = rt.get("shop-app").await.unwrap();
        assert_eq!(
            consumer.dependencies,
            vec!["commerce-provider".to_string()],
            "dependencies must survive the install_from_toml path (C-DRIFT-001)"
        );
        assert_eq!(
            consumer
                .capabilities
                .required
                .get("commerce")
                .map(String::as_str),
            Some("^1.0")
        );
        assert_eq!(
            consumer
                .capabilities
                .optional
                .get("notify")
                .map(String::as_str),
            Some("^1.0")
        );
    }

    fn make_manifest(name: &str, deps: &[&str]) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            description: "".into(),
            author: None,
            schema: PluginSchema::default(),
            logic: PluginLogic::default(),
            tools: Vec::new(),
            ui: None,
            permissions: PluginPermissions::default(),
            hooks: Vec::new(),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            capabilities: PluginCapabilities::default(),
        }
    }

    #[tokio::test]
    async fn install_with_missing_dep_fails() {
        let rt = PluginRuntime::new();
        let manifest = make_manifest("child", &["parent"]);
        let err = rt.install(manifest).await.unwrap_err();
        assert!(err.to_string().contains("parent"));
    }

    #[tokio::test]
    async fn install_with_satisfied_dep_works() {
        let rt = PluginRuntime::new();
        rt.install(make_manifest("parent", &[])).await.unwrap();
        rt.install(make_manifest("child", &["parent"]))
            .await
            .unwrap();
        assert_eq!(rt.list().await.len(), 2);
    }

    #[tokio::test]
    async fn batch_install_resolves_order() {
        let rt = PluginRuntime::new();
        let plugins = vec![
            make_manifest("c", &["b"]),
            make_manifest("a", &[]),
            make_manifest("b", &["a"]),
        ];
        let order = rt.install_batch(plugins).await.unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn batch_install_circular_dep_fails() {
        let rt = PluginRuntime::new();
        let plugins = vec![make_manifest("x", &["y"]), make_manifest("y", &["x"])];
        let err = rt.install_batch(plugins).await.unwrap_err();
        assert!(err.to_string().contains("circular"));
    }

    // ── ADR-0022 Step 2: capability-provider ordering + binding persistence ──

    /// A manifest that PROVIDES a capability at a concrete version.
    fn make_provider(name: &str, capability: &str, version: &str) -> PluginManifest {
        let mut m = make_manifest(name, &[]);
        m.capabilities
            .provided
            .insert(capability.to_string(), version.to_string());
        m
    }

    /// A manifest that REQUIRES a capability at a semver range.
    fn make_consumer(name: &str, capability: &str, range: &str) -> PluginManifest {
        let mut m = make_manifest(name, &[]);
        m.capabilities
            .required
            .insert(capability.to_string(), range.to_string());
        m
    }

    fn test_executor() -> crate::plugins::executor::PluginCrudExecutor {
        use pluresdb::{CrdtStore, MemoryStorage, StorageEngine};
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        let store = Arc::new(CrdtStore::default().with_persistence(storage));
        crate::plugins::executor::PluginCrudExecutor::new(store)
    }

    /// Provider must be ordered BEFORE the consumer that requires its capability,
    /// even though there is no explicit `[dependencies]` edge between them.
    #[tokio::test]
    async fn capability_provider_orders_before_consumer() {
        let rt = PluginRuntime::new();
        // Input order deliberately puts the consumer first.
        let plugins = vec![
            make_consumer("shop", "commerce", "^1.0"),
            make_provider("oasis-commerce", "commerce", "1.2.0"),
        ];
        let order = rt.install_batch(plugins).await.unwrap();
        let provider_idx = order.iter().position(|n| n == "oasis-commerce").unwrap();
        let consumer_idx = order.iter().position(|n| n == "shop").unwrap();
        assert!(
            provider_idx < consumer_idx,
            "provider must activate before consumer, got order {order:?}"
        );
    }

    /// A required capability with no provider in the batch blocks installation
    /// with a real UnsatisfiedCapability error.
    #[tokio::test]
    async fn install_batch_unsatisfied_required_capability_errors() {
        let rt = PluginRuntime::new();
        let plugins = vec![make_consumer("shop", "commerce", "^1.0")];
        let err = rt.install_batch(plugins).await.unwrap_err();
        match err {
            PluginError::UnsatisfiedCapability {
                plugin,
                capability,
                range,
            } => {
                assert_eq!(plugin, "shop");
                assert_eq!(capability, "commerce");
                assert_eq!(range, "^1.0");
            }
            other => panic!("expected UnsatisfiedCapability, got {other:?}"),
        }
        // Nothing should have been installed.
        assert_eq!(rt.list().await.len(), 0);
    }

    /// install_batch_persisting resolves the binding AND writes it to PluresDB;
    /// load_bindings returns it (round-trips through the store).
    #[tokio::test]
    async fn install_batch_persists_binding_to_pluresdb() {
        let rt = PluginRuntime::new();
        let executor = test_executor();
        let plugins = vec![
            make_consumer("shop", "commerce", "^1.0"),
            make_provider("oasis-commerce", "commerce", "1.2.0"),
        ];
        let order = rt
            .install_batch_persisting(plugins, &executor)
            .await
            .unwrap();
        assert_eq!(order.len(), 2);

        let bindings = executor.load_bindings().unwrap();
        assert_eq!(bindings.len(), 1, "exactly one capability binding persisted");
        let b = &bindings[0];
        assert_eq!(b.consumer, "shop");
        assert_eq!(b.capability, "commerce");
        assert_eq!(b.provider, "oasis-commerce");
        assert_eq!(b.version, "1.2.0");
    }
}
