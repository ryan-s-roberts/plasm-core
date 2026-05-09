//! Typed-phase sketch for composed view execution (agent semantics vs executor mechanics).
//!
//! The runtime implements this model procedurally in [`crate::view_execution`]; these types document
//! invariants: only materialized rows cross into [`crate::GraphCache`], never raw node identifiers.

#![allow(dead_code)]

use indexmap::IndexMap;
use plasm_core::schema::{EntityDef, ViewDefinition};
use plasm_core::{CapabilitySchema, Ref, Value};

/// A view selected from CGS, before caller input is bound.
pub struct ViewPlan<'cgs> {
    pub name: &'cgs str,
    pub def: &'cgs ViewDefinition,
    pub entity: &'cgs EntityDef,
    pub capability: &'cgs CapabilitySchema,
}

/// Scope derived from [`plasm_core::QueryExpr`] predicates or [`plasm_core::GetExpr`] identity.
pub struct ScopedView<'cgs> {
    pub plan: ViewPlan<'cgs>,
    pub scope: IndexMap<String, Value>,
}

/// Inner nodes have executed; results keyed by DAG node id (executor-only).
pub struct ExecutedView<'cgs> {
    pub scoped: ScopedView<'cgs>,
    pub nodes: IndexMap<String, crate::execution::ExecutionResult>,
}

/// Agent-facing [`crate::cache::CachedEntity`] plus decoded relation refs (no `node_*` keys).
pub struct MaterializedView {
    pub reference: Ref,
    pub relation_refs: IndexMap<String, Vec<Ref>>,
}
