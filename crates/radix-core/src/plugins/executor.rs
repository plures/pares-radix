//! CRUD executor — performs plugin entity operations against PluresDB.

use std::sync::Arc;

use chrono::Utc;
use pluresdb::CrdtStore;
use serde_json::{json, Value};
use tracing::warn;
use uuid::Uuid;

use super::capability::CapabilityBinding;
use super::error::PluginError;
use crate::chronos::{ChronosAction, ChronosTimeline};
use crate::praxis::write_gate::PraxisWriteGate;

/// The PluresDB actor ID used for all plugin write operations.
const ACTOR: &str = "pares-radix-plugin";

/// Prefix for plugin entity nodes.
const ENTITY_PREFIX: &str = "plugin:";

/// Prefix for entity registry nodes (tracks entity IDs per type).
const REGISTRY_PREFIX: &str = "plugin:_registry:";

/// Prefix for installed plugin manifest nodes.
const INSTALLED_PREFIX: &str = "plugin:_installed:";

/// Prefix for persisted resolved capability bindings (ADR-0022 §4, C-PLURES-003).
///
/// PluresDB nodes under this prefix are the **source of truth** for which
/// provider satisfies each consumer's capability. The in-memory resolution
/// result (`Vec<CapabilityBinding>`) is a rebuildable index of these nodes.
const BINDINGS_PREFIX: &str = "radix:capability:bindings:";

/// Executes CRUD operations for plugin entities against PluresDB.
pub struct PluginCrudExecutor {
    store: Arc<CrdtStore>,
    write_gate: Option<Arc<PraxisWriteGate>>,
    chronos: Option<Arc<ChronosTimeline>>,
}

impl PluginCrudExecutor {
    /// Create a new executor backed by the given CrdtStore.
    pub fn new(store: Arc<CrdtStore>) -> Self {
        Self {
            store,
            write_gate: None,
            chronos: None,
        }
    }

    /// Create a new executor with a write gate.
    pub fn with_write_gate(store: Arc<CrdtStore>, gate: Arc<PraxisWriteGate>) -> Self {
        Self {
            store,
            write_gate: Some(gate),
            chronos: None,
        }
    }

    /// Create a new executor with a write gate and Chronos timeline.
    pub fn with_chronos(
        store: Arc<CrdtStore>,
        gate: Arc<PraxisWriteGate>,
        chronos: Arc<ChronosTimeline>,
    ) -> Self {
        Self {
            store,
            write_gate: Some(gate),
            chronos: Some(chronos),
        }
    }

    /// Validate data through the write gate before persisting.
    fn gate_put(&self, key: &str, actor: &str, data: Value) -> Result<(), PluginError> {
        let warnings = if let Some(gate) = &self.write_gate {
            match gate.evaluate(key, &data, actor) {
                Ok(warnings) => {
                    for w in &warnings {
                        warn!("praxis write-gate warning on '{key}': {w}");
                    }
                    warnings
                }
                Err(rejection) => {
                    return Err(PluginError::Storage(rejection.to_string()));
                }
            }
        } else {
            vec![]
        };
        self.store.put(key.to_string(), actor, data.clone());
        // Determine action type — Create if node didn't exist, Update otherwise.
        // For simplicity we always record; the caller can refine if needed.
        self.record_chronos(key, actor, ChronosAction::Update, &data, &warnings);
        Ok(())
    }

    /// Record a Chronos entry after a successful write.
    fn record_chronos(
        &self,
        key: &str,
        actor: &str,
        action: ChronosAction,
        data: &Value,
        warnings: &[String],
    ) {
        if let Some(chronos) = &self.chronos {
            let constraint_results: Vec<String> = warnings.to_vec();
            let entry = chronos.build_entry(key, actor, action, data, constraint_results, None);
            chronos.record(&entry);
        }
    }

    /// Create a new entity node in PluresDB.
    ///
    /// Returns the entity ID.
    pub fn create(
        &self,
        entity_type: &str,
        plugin_name: &str,
        fields: Value,
    ) -> Result<String, PluginError> {
        let id = Uuid::new_v4().to_string();
        let key = format!("{ENTITY_PREFIX}{plugin_name}:{entity_type}:{id}");
        let now = Utc::now().to_rfc3339();

        let mut data = match fields {
            Value::Object(map) => map,
            _ => {
                return Err(PluginError::InvalidManifest(
                    "fields must be an object".into(),
                ))
            }
        };
        data.insert("_type".into(), json!("plugin_entity"));
        data.insert("_plugin".into(), json!(plugin_name));
        data.insert("_entity_type".into(), json!(entity_type));
        data.insert("_id".into(), json!(id));
        data.insert("_created_at".into(), json!(now));
        data.insert("_updated_at".into(), json!(now));

        self.gate_put(&key, ACTOR, Value::Object(data))?;

        // Update registry
        self.registry_add(plugin_name, entity_type, &id)?;

        Ok(id)
    }

    /// List entities with optional field filters.
    pub fn list(
        &self,
        entity_type: &str,
        plugin_name: &str,
        filters: Option<&Value>,
        limit: usize,
    ) -> Result<Vec<Value>, PluginError> {
        let ids = self.registry_get(plugin_name, entity_type);
        let prefix = format!("{ENTITY_PREFIX}{plugin_name}:{entity_type}:");
        let mut results = Vec::new();

        for id in ids {
            if results.len() >= limit {
                break;
            }
            let key = format!("{prefix}{id}");
            if let Some(record) = self.store.get(&key) {
                let data = record.data;
                // Check for soft-deleted
                if data
                    .get("_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    continue;
                }
                // Apply filters
                if let Some(Value::Object(filter_map)) = filters {
                    let matches = filter_map.iter().all(|(k, v)| data.get(k) == Some(v));
                    if !matches {
                        continue;
                    }
                }
                results.push(data);
            }
        }

        Ok(results)
    }

    /// Update fields on an existing entity.
    pub fn update(&self, entity_id: &str, fields: Value) -> Result<(), PluginError> {
        let key = self.find_key_by_id(entity_id)?;
        let record = self
            .store
            .get(&key)
            .ok_or_else(|| PluginError::NotFound(entity_id.to_string()))?;

        let mut data = match record.data {
            Value::Object(map) => map,
            _ => return Err(PluginError::Storage("corrupt entity data".into())),
        };

        if let Value::Object(update_map) = fields {
            for (k, v) in update_map {
                if !k.starts_with('_') {
                    data.insert(k, v);
                }
            }
        }
        data.insert("_updated_at".into(), json!(Utc::now().to_rfc3339()));

        self.gate_put(&key, ACTOR, Value::Object(data))?;
        Ok(())
    }

    /// Soft-delete an entity by ID.
    pub fn delete(&self, entity_id: &str) -> Result<(), PluginError> {
        let key = self.find_key_by_id(entity_id)?;
        let record = self
            .store
            .get(&key)
            .ok_or_else(|| PluginError::NotFound(entity_id.to_string()))?;

        let mut data = match record.data {
            Value::Object(map) => map,
            _ => return Err(PluginError::Storage("corrupt entity data".into())),
        };

        // Extract plugin/entity info for registry removal
        let plugin_name = data
            .get("_plugin")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let entity_type = data
            .get("_entity_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        data.insert("_deleted".into(), json!(true));
        data.insert("_updated_at".into(), json!(Utc::now().to_rfc3339()));
        self.gate_put(&key, ACTOR, Value::Object(data))?;

        // Remove from registry
        if !plugin_name.is_empty() && !entity_type.is_empty() {
            self.registry_remove(&plugin_name, &entity_type, entity_id);
        }

        Ok(())
    }

    /// Move an entity to a new parent by updating its parent reference.
    pub fn move_entity(
        &self,
        entity_id: &str,
        new_parent_id: &str,
        relationship: &str,
    ) -> Result<(), PluginError> {
        let key = self.find_key_by_id(entity_id)?;
        let record = self
            .store
            .get(&key)
            .ok_or_else(|| PluginError::NotFound(entity_id.to_string()))?;

        let mut data = match record.data {
            Value::Object(map) => map,
            _ => return Err(PluginError::Storage("corrupt entity data".into())),
        };

        data.insert(format!("_parent_{relationship}"), json!(new_parent_id));
        data.insert("_updated_at".into(), json!(Utc::now().to_rfc3339()));
        self.gate_put(&key, ACTOR, Value::Object(data))?;
        Ok(())
    }

    /// Search entities by matching query against all string fields.
    pub fn search(
        &self,
        query: &str,
        plugin_name: &str,
        entity_types: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<Value>, PluginError> {
        let query_lower = query.to_lowercase();
        let prefix = format!("{ENTITY_PREFIX}{plugin_name}:");
        let mut results = Vec::new();

        for record in self.store.list() {
            if results.len() >= limit {
                break;
            }
            if !record.id.starts_with(&prefix) {
                continue;
            }
            // Skip registry/installed nodes
            if record.id.starts_with(REGISTRY_PREFIX) || record.id.starts_with(INSTALLED_PREFIX) {
                continue;
            }
            let data = &record.data;
            if data
                .get("_deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                continue;
            }
            // Filter by entity types if specified
            if let Some(types) = entity_types {
                let et = data
                    .get("_entity_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !types.iter().any(|t| t == et) {
                    continue;
                }
            }
            // Match query against string fields
            if let Value::Object(map) = data {
                let matches = map.values().any(|v| {
                    if let Some(s) = v.as_str() {
                        s.to_lowercase().contains(&query_lower)
                    } else {
                        false
                    }
                });
                if matches {
                    results.push(data.clone());
                }
            }
        }

        Ok(results)
    }

    /// Store a plugin manifest in PluresDB for persistence across restarts.
    pub fn persist_manifest(&self, name: &str, manifest_json: &Value) -> Result<(), PluginError> {
        let key = format!("{INSTALLED_PREFIX}{name}");
        self.gate_put(&key, ACTOR, manifest_json.clone())?;
        Ok(())
    }

    /// Remove a persisted plugin manifest.
    pub fn remove_manifest(&self, name: &str) -> Result<(), PluginError> {
        let key = format!("{INSTALLED_PREFIX}{name}");
        self.store
            .delete(&key)
            .map_err(|_| PluginError::NotFound(name.to_string()))?;
        Ok(())
    }

    /// Load all persisted plugin manifests from PluresDB.
    pub fn load_persisted_manifests(&self) -> Vec<Value> {
        self.store
            .list()
            .into_iter()
            .filter(|r| r.id.starts_with(INSTALLED_PREFIX))
            .map(|r| r.data)
            .collect()
    }

    /// Persist a resolved capability binding to PluresDB (ADR-0022 §4,
    /// C-PLURES-003). Mirrors [`Self::persist_manifest`] exactly: same
    /// [`Self::gate_put`] write path (write-gate evaluation + Chronos record).
    ///
    /// The node key is `radix:capability:bindings:{consumer}:{capability}` so a
    /// consumer's binding for a given capability is a single addressable node
    /// (re-resolving overwrites it deterministically rather than accumulating).
    pub fn persist_binding(&self, binding: &CapabilityBinding) -> Result<(), PluginError> {
        let key = format!("{BINDINGS_PREFIX}{}:{}", binding.consumer, binding.capability);
        let data = serde_json::to_value(binding)
            .map_err(|e| PluginError::Storage(format!("serialize binding: {e}")))?;
        self.gate_put(&key, ACTOR, data)?;
        Ok(())
    }

    /// Load all persisted capability bindings from PluresDB. Mirrors
    /// [`Self::load_persisted_manifests`]: filters by the bindings prefix and
    /// deserializes each node back into a [`CapabilityBinding`].
    ///
    /// A node under the prefix that fails to deserialize is a real data-corruption
    /// signal, surfaced as [`PluginError::Storage`] rather than silently dropped.
    pub fn load_bindings(&self) -> Result<Vec<CapabilityBinding>, PluginError> {
        self.store
            .list()
            .into_iter()
            .filter(|r| r.id.starts_with(BINDINGS_PREFIX))
            .map(|r| {
                serde_json::from_value::<CapabilityBinding>(r.data).map_err(|e| {
                    PluginError::Storage(format!("corrupt capability binding '{}': {e}", r.id))
                })
            })
            .collect()
    }

    /// Count entities for a given plugin and entity type.
    pub fn count(&self, plugin_name: &str, entity_type: &str) -> usize {
        self.registry_get(plugin_name, entity_type).len()
    }

    // ── Registry helpers ─────────────────────────────────────────────────

    fn registry_key(plugin_name: &str, entity_type: &str) -> String {
        format!("{REGISTRY_PREFIX}{plugin_name}:{entity_type}")
    }

    fn registry_get(&self, plugin_name: &str, entity_type: &str) -> Vec<String> {
        let key = Self::registry_key(plugin_name, entity_type);
        self.store
            .get(&key)
            .and_then(|r| {
                r.data.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    fn registry_add(
        &self,
        plugin_name: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), PluginError> {
        let key = Self::registry_key(plugin_name, entity_type);
        let mut ids = self.registry_get(plugin_name, entity_type);
        if !ids.contains(&entity_id.to_string()) {
            ids.push(entity_id.to_string());
        }
        self.store.put(&key, ACTOR, json!(ids));
        Ok(())
    }

    fn registry_remove(&self, plugin_name: &str, entity_type: &str, entity_id: &str) {
        let key = Self::registry_key(plugin_name, entity_type);
        let mut ids = self.registry_get(plugin_name, entity_type);
        ids.retain(|id| id != entity_id);
        self.store.put(&key, ACTOR, json!(ids));
    }

    /// Find the full PluresDB key for an entity given its short UUID.
    fn find_key_by_id(&self, entity_id: &str) -> Result<String, PluginError> {
        let suffix = format!(":{entity_id}");
        for record in self.store.list() {
            if record.id.starts_with(ENTITY_PREFIX)
                && record.id.ends_with(&suffix)
                && !record.id.starts_with(REGISTRY_PREFIX)
                && !record.id.starts_with(INSTALLED_PREFIX)
            {
                return Ok(record.id);
            }
        }
        Err(PluginError::NotFound(entity_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluresdb::{CrdtStore, MemoryStorage, StorageEngine};

    fn test_store() -> Arc<CrdtStore> {
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        Arc::new(CrdtStore::default().with_persistence(storage))
    }

    #[test]
    fn create_and_list_entities() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        let id1 = executor
            .create(
                "item",
                "inventory",
                json!({"name": "Laptop", "value": 1500}),
            )
            .unwrap();
        let id2 = executor
            .create("item", "inventory", json!({"name": "Phone", "value": 800}))
            .unwrap();

        assert_ne!(id1, id2);

        let items = executor.list("item", "inventory", None, 50).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.get("name").unwrap() == "Laptop"));
        assert!(items.iter().any(|i| i.get("name").unwrap() == "Phone"));
    }

    #[test]
    fn update_entity() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        let id = executor
            .create(
                "item",
                "inventory",
                json!({"name": "Laptop", "value": 1500}),
            )
            .unwrap();
        executor.update(&id, json!({"value": 2000})).unwrap();

        let items = executor.list("item", "inventory", None, 50).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].get("value").unwrap(), 2000);
    }

    #[test]
    fn delete_entity() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        let id = executor
            .create("item", "inventory", json!({"name": "Laptop"}))
            .unwrap();
        executor.delete(&id).unwrap();

        let items = executor.list("item", "inventory", None, 50).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn move_entity() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        let room_id = executor
            .create("room", "inventory", json!({"name": "Office"}))
            .unwrap();
        let item_id = executor
            .create("item", "inventory", json!({"name": "Laptop"}))
            .unwrap();

        executor.move_entity(&item_id, &room_id, "room").unwrap();

        let items = executor.list("item", "inventory", None, 50).unwrap();
        assert_eq!(
            items[0]
                .get("_parent_room".to_string())
                .unwrap()
                .as_str()
                .unwrap(),
            room_id
        );
    }

    #[test]
    fn search_entities() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        executor
            .create(
                "item",
                "inventory",
                json!({"name": "MacBook Pro", "category": "electronics"}),
            )
            .unwrap();
        executor
            .create(
                "item",
                "inventory",
                json!({"name": "Coffee Table", "category": "furniture"}),
            )
            .unwrap();

        let results = executor.search("macbook", "inventory", None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("name").unwrap(), "MacBook Pro");
    }

    #[test]
    fn list_with_filters() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        executor
            .create(
                "item",
                "inventory",
                json!({"name": "Laptop", "category": "electronics"}),
            )
            .unwrap();
        executor
            .create(
                "item",
                "inventory",
                json!({"name": "Desk", "category": "furniture"}),
            )
            .unwrap();

        let filters = json!({"category": "electronics"});
        let results = executor
            .list("item", "inventory", Some(&filters), 50)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("name").unwrap(), "Laptop");
    }

    #[test]
    fn persist_and_load_manifests() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        let manifest = json!({"name": "inventory", "version": "1.0.0"});
        executor.persist_manifest("inventory", &manifest).unwrap();

        let loaded = executor.load_persisted_manifests();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].get("name").unwrap(), "inventory");
    }

    #[test]
    fn persist_and_load_capability_bindings_roundtrip() {
        use crate::plugins::capability::CapabilityBinding;
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        let binding = CapabilityBinding {
            consumer: "shop".into(),
            capability: "commerce".into(),
            provider: "oasis-commerce".into(),
            version: "1.2.0".into(),
        };
        executor.persist_binding(&binding).unwrap();

        let loaded = executor.load_bindings().unwrap();
        assert_eq!(loaded.len(), 1, "binding round-trips through PluresDB");
        assert_eq!(loaded[0], binding);

        // Re-persisting the same (consumer, capability) overwrites, not appends.
        let updated = CapabilityBinding {
            version: "1.5.0".into(),
            ..binding.clone()
        };
        executor.persist_binding(&updated).unwrap();
        let loaded = executor.load_bindings().unwrap();
        assert_eq!(loaded.len(), 1, "same key overwrites the binding node");
        assert_eq!(loaded[0].version, "1.5.0");
    }

    #[test]
    fn count_entities() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        executor
            .create("item", "inventory", json!({"name": "A"}))
            .unwrap();
        executor
            .create("item", "inventory", json!({"name": "B"}))
            .unwrap();
        executor
            .create("room", "inventory", json!({"name": "C"}))
            .unwrap();

        assert_eq!(executor.count("inventory", "item"), 2);
        assert_eq!(executor.count("inventory", "room"), 1);
    }

    #[test]
    fn delete_nonexistent_fails() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);
        assert!(executor.delete("nonexistent-uuid").is_err());
    }

    #[test]
    fn search_respects_entity_type_filter() {
        let store = test_store();
        let executor = PluginCrudExecutor::new(store);

        executor
            .create("item", "inventory", json!({"name": "Laptop"}))
            .unwrap();
        executor
            .create(
                "room",
                "inventory",
                json!({"name": "Living Room with Laptop"}),
            )
            .unwrap();

        let types = vec!["item".to_string()];
        let results = executor
            .search("laptop", "inventory", Some(&types), 10)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("_entity_type").unwrap(), "item");
    }
}
