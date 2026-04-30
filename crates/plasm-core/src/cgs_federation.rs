//! Federated execute sessions: **no** merged [`crate::schema::CGS`]. Each catalog row keeps its own
//! graph; [`crate::CgsContext`] (with [`crate::Prefix::Entry`]) is stored per `entry_id`. Prompt and
//! execution dispatch resolve the owning context by catalog + entity.

use crate::cgs_context::CgsContext;
use crate::schema::CGS;
use crate::symbol_tuning::DomainExposureSession;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;

/// Stable key for which catalog + CGS entity an `e#` row refers to (session-scoped).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct QualifiedEntityKey {
    pub catalog_entry_id: String,
    pub entity: String,
}

impl QualifiedEntityKey {
    pub fn new(catalog_entry_id: impl Into<String>, entity: impl Into<String>) -> Self {
        Self {
            catalog_entry_id: catalog_entry_id.into(),
            entity: entity.into(),
        }
    }
}

/// Maps exposed entity names to their owning registry row and [`CgsContext`] (HTTP backend, auth).
#[derive(Clone, Debug)]
pub struct FederationDispatch {
    pub by_entry: IndexMap<String, Arc<CgsContext>>,
    entity_to_entry: HashMap<String, String>,
}

impl FederationDispatch {
    /// Build from loaded contexts and a [`DomainExposureSession`] (parallel `entities` /
    /// `entity_catalog_entry_ids`).
    pub fn from_contexts_and_exposure(
        by_entry: IndexMap<String, Arc<CgsContext>>,
        exposure: &DomainExposureSession,
    ) -> Self {
        let mut entity_to_entry: HashMap<String, String> = HashMap::new();
        for (i, ent) in exposure.entities.iter().enumerate() {
            if let Some(eid) = exposure.entity_catalog_entry_ids.get(i) {
                entity_to_entry.insert(ent.clone(), eid.clone());
            }
        }
        Self {
            by_entry,
            entity_to_entry,
        }
    }

    pub fn cgs_for_entity(&self, entity: &str) -> Option<&CGS> {
        let eid = self.entity_to_entry.get(entity)?;
        self.by_entry.get(eid).map(|ctx| ctx.cgs.as_ref())
    }

    /// Prefer per-catalog backend; used when selecting HTTP origin for an operation.
    pub fn http_backend_for_entity(&self, entity: &str) -> Option<&str> {
        let eid = self.entity_to_entry.get(entity)?;
        self.by_entry.get(eid).map(|c| c.cgs.http_backend.as_str())
    }

    /// Resolve CGS for `entity`, else `fallback` (primary session graph).
    pub fn resolve_cgs<'a>(&'a self, entity: &str, fallback: &'a CGS) -> &'a CGS {
        self.cgs_for_entity(entity).unwrap_or(fallback)
    }

    /// Owning loaded context for an exposed entity name (HTTP backend + auth on the inner [`CGS`]).
    pub fn context_for_entity(&self, entity: &str) -> Option<&Arc<CgsContext>> {
        let eid = self.entity_to_entry.get(entity)?;
        self.by_entry.get(eid)
    }

    /// Owning `(catalog entry id, CGS entity name)` for an exposed federation entity name.
    ///
    /// Prefer this over [`Self::catalog_entry_id_for_entity`] at planner boundaries.
    pub fn qualified_entity_for_exposed_entity(&self, entity: &str) -> Option<QualifiedEntityKey> {
        self.entity_to_entry
            .get(entity)
            .map(|eid| QualifiedEntityKey::new(eid.clone(), entity.to_string()))
    }

    /// Owning registry `entry_id` for an exposed entity name (trace / dispatch labeling).
    #[deprecated(
        note = "use qualified_entity_for_exposed_entity — catalog ownership is (entry_id, entity)"
    )]
    pub fn catalog_entry_id_for_entity(&self, entity: &str) -> Option<&str> {
        self.entity_to_entry.get(entity).map(|s| s.as_str())
    }
}
