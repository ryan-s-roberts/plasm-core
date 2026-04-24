//! Materialized entity graph for execute sessions: [`GraphCache`], [`CachedEntity`], [`CacheStore`].
//!
//! # Cache invariants (semi-formal)
//!
//! These statements describe **design intent** and **observable consistency** for maintainers and
//! for future session-scoped / concurrent cache wrappers. They are **not** a formal verification
//! contract unless backed by tests in this crate.
//!
//! ## Structural
//!
//! - **I1 (key–value identity).** For every entry `(k, v)` in the entity map, `v.reference == k`
//!   (the map key is always the entity’s [`Ref`]).
//! - **I2 (type index coverage).** If `r` is a key in `entities`, then `r` appears **at least once**
//!   in `type_index[r.entity_type]` after a successful insert of that ref. Operations that remove an
//!   entity remove `r` from `type_index` for that type ([`GraphCache::remove`], [`GraphCache::clear_type`]).
//!   *Implementation note:* [`GraphCache::insert`] appends to the per-type list even when merging into an
//!   existing row, so **duplicate `Ref` entries in that vector are possible**; treat `type_index` as a
//!   hint list and resolve through `entities` (e.g. [`GraphCache::get_entities_by_type`] deduplicates via lookup).
//!
//! ## Temporal / versioning
//!
//! - **I3 (monotonic clock).** Each internal `current_timestamp()` call strictly increases
//!   `version_counter` and supplies `last_updated` on insert paths.
//! - **I4 (entity version).** [`CachedEntity::version`] and `last_updated` are updated on merges per
//!   [`CachedEntity::merge`].
//!
//! ## Concurrency (external obligation)
//!
//! - **I5 (single writer).** [`GraphCache`] does **not** synchronize concurrent access. Safe use requires
//!   **at most one** `&mut GraphCache` at a time (one task/thread holding exclusivity). Parallel tasks
//!   must use separate cache instances, external locks, or a session facade that enforces ordering.
//! - **I6 (clone).** [`GraphCache::clone`] is a deep copy of in-memory state; independent forks must be
//!   merged back with an explicit policy when combining results into one session view.
//!
//! ## Merge semantics
//!
//! - **I7 (insert).** [`GraphCache::insert`] inserts a new row or merges into an existing row via
//!   [`CachedEntity::merge`]; relation keys with [`DecodedRelation::Unspecified`] are dropped on decode
//!   ([`CachedEntity::from_decoded`]).

use crate::RuntimeError;
use indexmap::IndexMap;
use plasm_compile::DecodedRelation;
use plasm_core::{EntityName, Ref, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Whether cached fields came from a list/query response or from a single-resource GET.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EntityCompleteness {
    /// List or partial payload; a GET may add fields.
    Summary,
    /// Single-resource response (or merged upgrade from Summary).
    #[default]
    Complete,
}

/// A cached entity instance with stable identity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedEntity {
    pub reference: Ref,
    pub fields: IndexMap<String, Value>,
    /// Only relations that were **specified** on the wire (see [`DecodedRelation::Specified`]); omitted edges have no key here.
    pub relations: IndexMap<String, Vec<Ref>>,
    /// Timestamp when this entity was last updated
    pub last_updated: u64,
    /// Version counter for optimistic concurrency
    pub version: u64,
    /// Provenance of field coverage (summary list rows vs detail GET).
    #[serde(default)]
    pub completeness: EntityCompleteness,
}

/// Graph cache with stable identity and merge semantics.
///
/// **Invariants:** See [Cache invariants (semi-formal)](crate::cache#cache-invariants-semi-formal) (*I1–I7*).
/// In short: map keys match row identity; the type index may list duplicate refs after repeated merges;
/// callers must provide external synchronization for concurrent use (*I5*).
#[derive(Debug, Clone)]
pub struct GraphCache {
    /// Entity storage: Ref -> CachedEntity
    entities: HashMap<Ref, CachedEntity>,
    /// Global version counter
    version_counter: u64,
    /// Entity type index for efficient queries
    type_index: HashMap<EntityName, Vec<Ref>>,
}

/// Options for cache operations
#[derive(Debug, Clone, Default)]
pub struct CacheOptions {
    /// Whether to perform deep validation on merge
    pub validate_on_merge: bool,
    /// Maximum number of entities to keep in memory
    pub max_entities: Option<usize>,
    /// Whether to track access times for LRU eviction
    pub track_access: bool,
}

impl CachedEntity {
    /// Create a new cached entity
    pub fn new(reference: Ref, timestamp: u64) -> Self {
        Self {
            reference,
            fields: IndexMap::new(),
            relations: IndexMap::new(),
            last_updated: timestamp,
            version: 1,
            completeness: EntityCompleteness::Complete,
        }
    }

    /// Create from decoded entity data ([`DecodedRelation::Unspecified`] drops that key — sparse relation map).
    pub fn from_decoded(
        reference: Ref,
        fields: IndexMap<String, Value>,
        relations: IndexMap<String, DecodedRelation>,
        timestamp: u64,
        completeness: EntityCompleteness,
    ) -> Self {
        let relations: IndexMap<String, Vec<Ref>> = relations
            .into_iter()
            .filter_map(|(k, dr)| match dr {
                DecodedRelation::Unspecified => None,
                DecodedRelation::Specified(refs) => Some((k, refs)),
            })
            .collect();
        Self {
            reference,
            fields,
            relations,
            last_updated: timestamp,
            version: 1,
            completeness,
        }
    }

    /// Get a field value
    pub fn get_field(&self, field: &str) -> Option<&Value> {
        self.fields.get(field)
    }

    /// Get related entity references
    pub fn get_relations(&self, relation: &str) -> Option<&Vec<Ref>> {
        self.relations.get(relation)
    }

    /// Update a field
    pub fn update_field(&mut self, field: String, value: Value, timestamp: u64) {
        self.fields.insert(field, value);
        self.last_updated = timestamp;
        self.version += 1;
    }

    /// Update relations
    pub fn update_relations(&mut self, relation: String, refs: Vec<Ref>, timestamp: u64) {
        self.relations.insert(relation, refs);
        self.last_updated = timestamp;
        self.version += 1;
    }

    /// Check if this entity has been updated more recently than the given timestamp
    pub fn is_newer_than(&self, timestamp: u64) -> bool {
        self.last_updated > timestamp
    }

    /// Serialize fields and relations to JSON (no `_ref` / `_version` cache metadata).
    pub fn payload_to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        for (k, v) in &self.fields {
            obj.insert(
                k.clone(),
                serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            );
        }
        for (k, refs) in &self.relations {
            obj.insert(
                k.clone(),
                serde_json::Value::Array(
                    refs.iter()
                        .map(|r| serde_json::Value::String(r.to_string()))
                        .collect(),
                ),
            );
        }
        serde_json::Value::Object(obj)
    }

    /// Merge another entity into this one (keeping the most recent data)
    pub fn merge(&mut self, other: &CachedEntity) -> Result<bool, RuntimeError> {
        if self.reference != other.reference {
            return Err(RuntimeError::CacheError {
                message: format!(
                    "Cannot merge entities with different references: {} vs {}",
                    self.reference, other.reference
                ),
            });
        }

        // Never let a list-shaped row replace a fully hydrated entity.
        if other.completeness == EntityCompleteness::Summary
            && self.completeness == EntityCompleteness::Complete
        {
            return Ok(false);
        }

        let upgrade_to_complete = self.completeness == EntityCompleteness::Summary
            && other.completeness == EntityCompleteness::Complete;

        if upgrade_to_complete {
            self.fields = other.fields.clone();
            for (k, v) in &other.relations {
                self.relations.insert(k.clone(), v.clone());
            }
            self.completeness = EntityCompleteness::Complete;
            self.last_updated = self.last_updated.max(other.last_updated);
            self.version = self.version.max(other.version) + 1;
            return Ok(true);
        }

        let mut changed = false;

        // Complete + Complete: additive (sparse) merge.
        //
        // When both entities are Complete, they may be disjoint projections of the same
        // resource returned by different API endpoints (e.g. a metadata endpoint and a
        // content endpoint that share the same ID but return entirely different fields).
        // In this case replacing all fields would silently erase the other projection.
        //
        // Instead, only update fields that are explicitly present and non-null in `other`.
        // Fields already in `self` that are absent from `other` are preserved.
        //
        // Summary → Complete: fields replace from `other`; relations overlay so sparse
        // decoded rows do not clear unspecified edges.
        let both_complete = self.completeness == EntityCompleteness::Complete
            && other.completeness == EntityCompleteness::Complete;

        if both_complete {
            // Additive merge: update with non-null fields from `other`, keep everything else.
            for (field, value) in &other.fields {
                if matches!(value, plasm_core::Value::Null) {
                    continue; // skip absent/null — don't overwrite existing value
                }
                if self.fields.get(field) != Some(value) {
                    self.fields.insert(field.clone(), value.clone());
                    changed = true;
                }
            }
            for (relation, refs) in &other.relations {
                if self.relations.get(relation) != Some(refs) {
                    self.relations.insert(relation.clone(), refs.clone());
                    changed = true;
                }
            }
            if changed {
                self.last_updated = other.last_updated.max(self.last_updated);
                self.version += 1;
            }
            return Ok(changed);
        }

        // Use the most recent version for all other cases (Summary + Summary,
        // Complete replacing Summary when upgrade_to_complete was false, etc.)
        if other.last_updated > self.last_updated {
            self.fields = other.fields.clone();
            for (k, v) in &other.relations {
                self.relations.insert(k.clone(), v.clone());
            }
            self.last_updated = other.last_updated;
            self.completeness = other.completeness;
            self.version = other.version.max(self.version) + 1;
            changed = true;
        } else if other.last_updated == self.last_updated {
            // Same timestamp, merge field by field
            for (field, value) in &other.fields {
                if self.fields.get(field) != Some(value) {
                    self.fields.insert(field.clone(), value.clone());
                    changed = true;
                }
            }

            for (relation, refs) in &other.relations {
                if self.relations.get(relation) != Some(refs) {
                    self.relations.insert(relation.clone(), refs.clone());
                    changed = true;
                }
            }

            if changed {
                self.version += 1;
            }
            if other.completeness == EntityCompleteness::Complete {
                self.completeness = EntityCompleteness::Complete;
            }
        }

        Ok(changed)
    }
}

/// Cache abstraction for execution ([`GraphCache`] is the default in-memory store).
pub trait CacheStore {
    fn get(&self, reference: &Ref) -> Option<&CachedEntity>;
    fn insert(&mut self, entity: CachedEntity) -> Result<bool, RuntimeError>;
    fn merge(&mut self, entities: Vec<CachedEntity>) -> Result<usize, RuntimeError>;
    fn remove(&mut self, reference: &Ref) -> Option<CachedEntity>;
}

impl CacheStore for GraphCache {
    fn get(&self, reference: &Ref) -> Option<&CachedEntity> {
        GraphCache::get(self, reference)
    }

    fn insert(&mut self, entity: CachedEntity) -> Result<bool, RuntimeError> {
        GraphCache::insert(self, entity)
    }

    fn merge(&mut self, entities: Vec<CachedEntity>) -> Result<usize, RuntimeError> {
        GraphCache::merge(self, entities)
    }

    fn remove(&mut self, reference: &Ref) -> Option<CachedEntity> {
        GraphCache::remove(self, reference)
    }
}

impl GraphCache {
    /// Create a new empty graph cache
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            version_counter: 0,
            type_index: HashMap::new(),
        }
    }

    /// Create with options
    pub fn with_options(_options: CacheOptions) -> Self {
        // For now, ignore options but could be used for configuration
        Self::new()
    }

    /// Get the current timestamp
    fn current_timestamp(&mut self) -> u64 {
        self.version_counter += 1;
        self.version_counter
    }

    /// Get an entity by reference
    pub fn get(&self, reference: &Ref) -> Option<&CachedEntity> {
        self.entities.get(reference)
    }

    /// Get a mutable reference to an entity
    pub fn get_mut(&mut self, reference: &Ref) -> Option<&mut CachedEntity> {
        self.entities.get_mut(reference)
    }

    /// Insert or update an entity
    pub fn insert(&mut self, entity: CachedEntity) -> Result<bool, RuntimeError> {
        let timestamp = self.current_timestamp();
        let reference = entity.reference.clone();

        // Update type index
        let entity_type = reference.entity_type.clone();
        self.type_index
            .entry(entity_type)
            .or_default()
            .push(reference.clone());

        // Insert or merge
        if let Some(existing) = self.entities.get_mut(&reference) {
            let mut updated_entity = entity;
            updated_entity.last_updated = timestamp;
            existing.merge(&updated_entity)
        } else {
            let mut new_entity = entity;
            new_entity.last_updated = timestamp;
            self.entities.insert(reference, new_entity);
            Ok(true)
        }
    }

    /// Merge multiple entities into the cache
    pub fn merge(&mut self, entities: Vec<CachedEntity>) -> Result<usize, RuntimeError> {
        let mut changed_count = 0;

        for entity in entities {
            if self.insert(entity)? {
                changed_count += 1;
            }
        }

        Ok(changed_count)
    }

    /// Merge every entity from `other` into `self` ([`CachedEntity::merge`] / [`GraphCache::insert`] semantics).
    ///
    /// Sort order is deterministic for stable tests. When applying multiple forked caches (e.g. a
    /// parallel-safe query stage), merge **in invocation order** so later lines win on conflicting refs.
    pub fn merge_from_graph(&mut self, other: &GraphCache) -> Result<usize, RuntimeError> {
        let mut entities: Vec<CachedEntity> = other.entities.values().cloned().collect();
        entities.sort_by(|a, b| {
            a.reference
                .entity_type
                .to_string()
                .cmp(&b.reference.entity_type.to_string())
                .then_with(|| {
                    format!("{:?}", a.reference.key).cmp(&format!("{:?}", b.reference.key))
                })
        });
        self.merge(entities)
    }

    /// Get all entities of a specific type
    pub fn get_entities_by_type(&self, entity_type: &str) -> Vec<&CachedEntity> {
        if let Some(refs) = self.type_index.get(entity_type) {
            refs.iter().filter_map(|r| self.entities.get(r)).collect()
        } else {
            Vec::new()
        }
    }

    /// Remove an entity from the cache
    pub fn remove(&mut self, reference: &Ref) -> Option<CachedEntity> {
        // Remove from type index
        if let Some(refs) = self.type_index.get_mut(&reference.entity_type) {
            refs.retain(|r| r != reference);
        }

        self.entities.remove(reference)
    }

    /// Clear all entities of a specific type
    pub fn clear_type(&mut self, entity_type: &str) {
        if let Some(refs) = self.type_index.remove(entity_type) {
            for reference in refs {
                self.entities.remove(&reference);
            }
        }
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.entities.clear();
        self.type_index.clear();
        self.version_counter = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            total_entities: self.entities.len(),
            entity_types: self.type_index.len(),
            version: self.version_counter,
        }
    }

    /// Check if cache contains an entity
    pub fn contains(&self, reference: &Ref) -> bool {
        self.entities.contains_key(reference)
    }

    /// Get all entity references
    pub fn all_references(&self) -> Vec<&Ref> {
        self.entities.keys().collect()
    }

    /// Invalidate entities matching a predicate
    pub fn invalidate_matching<F>(&mut self, predicate: F) -> usize
    where
        F: Fn(&CachedEntity) -> bool,
    {
        let to_remove: Vec<Ref> = self
            .entities
            .iter()
            .filter(|(_, entity)| predicate(entity))
            .map(|(reference, _)| reference.clone())
            .collect();

        let count = to_remove.len();
        for reference in to_remove {
            self.remove(&reference);
        }

        count
    }

    /// Convert entity to JSON for external use
    pub fn entity_to_json(&self, reference: &Ref) -> Result<serde_json::Value, RuntimeError> {
        let entity = self
            .get(reference)
            .ok_or_else(|| RuntimeError::CacheError {
                message: format!("Entity not found: {}", reference),
            })?;

        let mut json = serde_json::json!({
            "id": reference.primary_slot_str(),
            "_ref": reference.to_string(),
            "_version": entity.version,
            "_last_updated": entity.last_updated
        });

        if let serde_json::Value::Object(payload) = entity.payload_to_json() {
            for (k, v) in payload {
                json[k] = v;
            }
        }

        Ok(json)
    }
}

impl Default for GraphCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub total_entities: usize,
    pub entity_types: usize,
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plasm_compile::DecodedRelation;
    use plasm_core::Value;

    fn create_test_entity(id: &str, entity_type: &str) -> CachedEntity {
        let reference = Ref::new(entity_type, id);
        let mut fields = IndexMap::new();
        fields.insert("name".to_string(), Value::String(format!("Entity {}", id)));
        fields.insert("value".to_string(), Value::Float(100.0));

        CachedEntity::from_decoded(
            reference,
            fields,
            IndexMap::new(),
            1,
            EntityCompleteness::Complete,
        )
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = GraphCache::new();
        let entity = create_test_entity("test-1", "TestEntity");
        let reference = entity.reference.clone();

        assert!(cache.insert(entity).unwrap());
        assert!(cache.contains(&reference));

        let retrieved = cache.get(&reference).unwrap();
        assert_eq!(retrieved.reference, reference);
        assert_eq!(
            retrieved.get_field("name"),
            Some(&Value::String("Entity test-1".to_string()))
        );
    }

    #[test]
    fn test_cache_merge() {
        let mut cache = GraphCache::new();
        let entity1 = create_test_entity("test-1", "TestEntity");
        let reference = entity1.reference.clone();

        // Insert initial entity
        cache.insert(entity1).unwrap();

        // Create updated version
        let mut entity2 = create_test_entity("test-1", "TestEntity");
        entity2.update_field(
            "name".to_string(),
            Value::String("Updated Entity".to_string()),
            2,
        );

        // Merge should update the cached entity
        cache.insert(entity2).unwrap();

        let retrieved = cache.get(&reference).unwrap();
        assert_eq!(
            retrieved.get_field("name"),
            Some(&Value::String("Updated Entity".to_string()))
        );
    }

    #[test]
    fn test_cache_type_index() {
        let mut cache = GraphCache::new();
        let entity1 = create_test_entity("test-1", "TestEntity");
        let entity2 = create_test_entity("test-2", "TestEntity");
        let entity3 = create_test_entity("test-3", "OtherEntity");

        cache.insert(entity1).unwrap();
        cache.insert(entity2).unwrap();
        cache.insert(entity3).unwrap();

        let test_entities = cache.get_entities_by_type("TestEntity");
        assert_eq!(test_entities.len(), 2);

        let other_entities = cache.get_entities_by_type("OtherEntity");
        assert_eq!(other_entities.len(), 1);
    }

    #[test]
    fn test_cache_relations() {
        let mut cache = GraphCache::new();

        // Create entities with relations
        let account_ref = Ref::new("Account", "acc-1");
        let contact1_ref = Ref::new("Contact", "c-1");
        let contact2_ref = Ref::new("Contact", "c-2");

        let mut account_fields = IndexMap::new();
        account_fields.insert("name".to_string(), Value::String("Acme Corp".to_string()));

        let mut account_relations = IndexMap::new();
        account_relations.insert(
            "contacts".to_string(),
            DecodedRelation::Specified(vec![contact1_ref.clone(), contact2_ref.clone()]),
        );

        let account = CachedEntity::from_decoded(
            account_ref.clone(),
            account_fields,
            account_relations,
            1,
            EntityCompleteness::Complete,
        );
        cache.insert(account).unwrap();

        let retrieved = cache.get(&account_ref).unwrap();
        let contacts = retrieved.get_relations("contacts").unwrap();
        assert_eq!(contacts.len(), 2);
        assert!(contacts.contains(&contact1_ref));
        assert!(contacts.contains(&contact2_ref));
    }

    #[test]
    fn test_entity_merge() {
        let mut entity1 = create_test_entity("test-1", "TestEntity");
        let mut entity2 = create_test_entity("test-1", "TestEntity");

        // Make entity2 newer
        entity2.last_updated = 2;
        entity2.fields.insert(
            "new_field".to_string(),
            Value::String("new_value".to_string()),
        );

        let changed = entity1.merge(&entity2).unwrap();
        assert!(changed);
        assert_eq!(entity1.last_updated, 2);
        assert_eq!(
            entity1.get_field("new_field"),
            Some(&Value::String("new_value".to_string()))
        );
    }

    #[test]
    fn test_merge_summary_does_not_downgrade_complete() {
        let reference = Ref::new("TestEntity", "a");
        let mut summary = CachedEntity::from_decoded(
            reference.clone(),
            IndexMap::from([("x".into(), Value::Float(1.0))]),
            IndexMap::new(),
            2,
            EntityCompleteness::Summary,
        );
        let complete = CachedEntity::from_decoded(
            reference,
            IndexMap::from([
                ("x".into(), Value::Float(2.0)),
                ("detail".into(), Value::String("full".into())),
            ]),
            IndexMap::new(),
            1,
            EntityCompleteness::Complete,
        );

        let mut merged = complete.clone();
        assert!(!merged.merge(&summary).unwrap());
        assert_eq!(
            merged.get_field("detail"),
            Some(&Value::String("full".into()))
        );

        assert!(summary.merge(&complete).unwrap());
        assert_eq!(summary.completeness, EntityCompleteness::Complete);
        assert_eq!(
            summary.get_field("detail"),
            Some(&Value::String("full".into()))
        );
    }

    #[test]
    fn merge_complete_preserves_relation_when_other_sparse() {
        let reference = Ref::new("Issue", "i1");
        let r1 = Ref::new("Issue", "c1");
        let mut full = CachedEntity::from_decoded(
            reference.clone(),
            IndexMap::from([("id".into(), Value::String("i1".into()))]),
            IndexMap::from([(
                "children".into(),
                DecodedRelation::Specified(vec![r1.clone()]),
            )]),
            1,
            EntityCompleteness::Complete,
        );
        let partial = CachedEntity::from_decoded(
            reference,
            IndexMap::from([
                ("id".into(), Value::String("i1".into())),
                ("title".into(), Value::String("updated".into())),
            ]),
            IndexMap::new(),
            2,
            EntityCompleteness::Complete,
        );
        assert!(full.merge(&partial).unwrap());
        let kids = full.get_relations("children").expect("children preserved");
        assert_eq!(kids, &vec![r1.clone()]);
    }

    #[test]
    fn test_cache_invalidation() {
        let mut cache = GraphCache::new();
        let entity1 = create_test_entity("test-1", "TestEntity");
        let entity2 = create_test_entity("test-2", "TestEntity");
        let entity3 = create_test_entity("test-3", "OtherEntity");

        cache.insert(entity1).unwrap();
        cache.insert(entity2).unwrap();
        cache.insert(entity3).unwrap();

        // Invalidate all TestEntity entities
        let invalidated =
            cache.invalidate_matching(|entity| entity.reference.entity_type == "TestEntity");
        assert_eq!(invalidated, 2);

        let remaining = cache.get_entities_by_type("TestEntity");
        assert_eq!(remaining.len(), 0);

        let other_remaining = cache.get_entities_by_type("OtherEntity");
        assert_eq!(other_remaining.len(), 1);
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = GraphCache::new();
        assert_eq!(cache.stats().total_entities, 0);

        let entity1 = create_test_entity("test-1", "TestEntity");
        let entity2 = create_test_entity("test-2", "OtherEntity");

        cache.insert(entity1).unwrap();
        cache.insert(entity2).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.total_entities, 2);
        assert_eq!(stats.entity_types, 2);
        assert!(stats.version > 0);
    }

    #[test]
    fn merge_from_graph_pulls_entities_from_other() {
        let mut a = GraphCache::new();
        let mut b = GraphCache::new();
        let e = create_test_entity("x1", "TestEntity");
        b.insert(e.clone()).unwrap();
        a.merge_from_graph(&b).unwrap();
        assert!(a.contains(&e.reference));
        assert_eq!(a.stats().total_entities, 1);
    }

    fn assert_i1_key_reference(cache: &GraphCache) {
        for r in cache.all_references() {
            let v = cache.get(r).expect("all_references keys must resolve");
            assert_eq!(r, &v.reference);
        }
    }

    mod property_tests {
        use super::super::GraphCache;
        use super::{assert_i1_key_reference, create_test_entity};
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn merge_from_empty_graph_noop(count in 0usize..16) {
                let mut a = GraphCache::new();
                for i in 0..count {
                    let e = create_test_entity(&format!("merge_empty_{i}"), "TestEntity");
                    a.insert(e).unwrap();
                }
                let before = a.clone();
                prop_assert_eq!(a.merge_from_graph(&GraphCache::new()).unwrap(), 0);
                prop_assert_eq!(a.stats().total_entities, before.stats().total_entities);
                assert_i1_key_reference(&a);
            }

            #[test]
            fn merge_from_graph_pulls_all_refs(k in 0usize..=32) {
                let mut a = GraphCache::new();
                let mut b = GraphCache::new();
                for i in 0..k {
                    let e = create_test_entity(&format!("pull_{i}"), "TestEntity");
                    b.insert(e).unwrap();
                }
                prop_assert_eq!(a.merge_from_graph(&b).unwrap(), k);
                for r in b.all_references() {
                    prop_assert!(a.contains(r));
                    prop_assert!(a.get(r).is_some());
                }
                assert_i1_key_reference(&a);
            }

            #[test]
            fn merge_from_graph_disjoint_union(i in 0usize..8, j in 0usize..8) {
                let mut a = GraphCache::new();
                let mut b = GraphCache::new();
                let ea = create_test_entity(&format!("left_{i}"), "TestEntity");
                let eb = create_test_entity(&format!("right_{j}"), "OtherEntity");
                a.insert(ea.clone()).unwrap();
                b.insert(eb.clone()).unwrap();
                prop_assert_eq!(a.merge_from_graph(&b).unwrap(), 1);
                prop_assert!(a.contains(&ea.reference));
                prop_assert!(a.contains(&eb.reference));
                prop_assert_eq!(a.stats().total_entities, 2);
                assert_i1_key_reference(&a);
            }
        }
    }
}
