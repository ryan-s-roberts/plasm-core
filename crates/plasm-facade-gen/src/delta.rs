//! JSON-serializable `facade_delta` for `_meta.plasm` (code mode tools).

use serde::Serialize;
use std::collections::BTreeSet;

/// Top-level object embedded at `_meta.plasm.facade_delta` for `add_code_capabilities`.
#[derive(Debug, Clone, Serialize)]
pub struct FacadeDeltaV1 {
    pub version: u32,
    /// Stable registry catalog ids (entry_id) touched in this wave.
    pub catalog_entry_ids: Vec<String>,
    /// `entry_id` → display alias (e.g. `github`, `github2`).
    pub catalog_aliases: Vec<CatalogAliasRecord>,
    /// Exposed surface per entity, qualified by `entry_id`.
    pub qualified_entities: Vec<QualifiedEntitySurface>,
    /// If non-empty, entity names with namespace collisions in this session.
    pub collision_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogAliasRecord {
    pub entry_id: String,
    /// Safe JS/TS path segment, e.g. `github2`.
    pub alias: String,
    /// `PascalCase` namespace used in `declare namespace GitHub` fragments.
    pub namespace: String,
}

/// One CGS entity exposed in the current wave (or in this refresh).
#[derive(Debug, Clone, Serialize)]
pub struct QualifiedEntitySurface {
    pub entry_id: String,
    pub catalog_alias: String,
    pub entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub e_index: Option<usize>,
    /// Declared field typing for prompts (subset of full CGS).
    pub fields: Vec<FacadeField>,
    /// Reverse edges for relation-aware codegen.
    pub relations: Vec<FacadeRelation>,
    /// Capabilities that drive builders / effect typing.
    pub capabilities: Vec<FacadeCapability>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FacadeField {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub r#type: FieldTypeName,
    pub required: bool,
    /// Whether this field is listed in a disjoint `provides` (projection narrows) — best-effort.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_format: Option<String>,
    /// Select / multi-select literal options when the CGS enumerates them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select_values: Option<Vec<String>>,
    /// `EntityRef` target resource name, when the CGS specifies it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_ref_target: Option<String>,
}

/// Human-readable field class for JSON / LLM; mirrors CGS but stays JSON-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldTypeName {
    String,
    Integer,
    Number,
    Boolean,
    Uuid,
    Blob,
    Date,
    Json,
    Select,
    MultiSelect,
    Array,
    EntityRef,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct FacadeRelation {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Target entity name in the same or another catalog.
    pub target: String,
    /// `one` or `many`.
    pub cardinality: String,
    /// Relation materialization policy when set in CGS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialize: Option<String>,
}

/// One object-field parameter (invoke / rich query) as exposed in the facade delta.
#[derive(Debug, Clone, Serialize)]
pub struct FacadeInputParameter {
    pub name: String,
    pub r#type: FieldTypeName,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Enum-like select literals when the CGS lists them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub array_item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Summarize [`plasm_core::OutputType`] for prompts / code-gen (JSON-safe tag).
#[derive(Debug, Clone, Serialize)]
pub struct FacadeOutputSurface {
    /// `entity` | `collection` | `side_effect` | `status` | `custom` | `none`
    pub type_tag: String,
    /// Entity name when `type_tag` is `entity` or `collection`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

/// Declarative preflight: hydrate a parent row before invoke (CGS `invoke_preflight`).
#[derive(Debug, Clone, Serialize)]
pub struct FacadeInvokePreflight {
    pub hydrate_capability: String,
    pub env_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FacadeCapability {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: String,
    pub effect_class: String,
    /// Result type hint for TS (`list`, `single`, …) — not HTTP wire shape.
    pub result_shape: String,
    /// Declared `provides` (may be empty when defaulting).
    pub provides: Vec<String>,
    /// `true` when the CGS action only acknowledges side effect.
    #[serde(default)]
    pub is_side_effect_ack: bool,
    /// Object-field inputs when the CGS provides `input_type: object` (or empty for none/value-only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_parameters: Vec<FacadeInputParameter>,
    /// Structured output (projection / list vs side-effect) from the CGS output schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<FacadeOutputSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke_preflight: Option<FacadeInvokePreflight>,
}

/// Typed fragment attached to the MCP `add_code_capabilities` response.
#[derive(Debug, Clone, Serialize)]
pub struct TypeScriptCodeArtifacts {
    /// `declare` prelude with shared `Plasm` helper types; empty when unchanged for this call.
    pub agent_prelude: String,
    /// `declare namespace …` and `interface` additions for the wave; may be empty.
    pub agent_namespace_body: String,
    /// `declare interface LoadedApis` augmentation lines (may be empty when nothing new).
    pub agent_loaded_apis: String,
    /// Reference identifying the host-injected runtime; the runtime source is never prompt-facing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_bootstrap_ref: Option<String>,
    /// True when the server did not add new declarations and prior fragments remain valid.
    pub declarations_unchanged: bool,
    /// `entry_id` keys newly covered by `loaded_apis` in this wave.
    pub added_catalog_aliases: Vec<String>,
}

/// Set of (entry_id, entity) pairs; used to detect incremental work across waves.
pub type ExposedSet = BTreeSet<(String, String)>;
