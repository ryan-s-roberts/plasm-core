use crate::identity::{
    CapabilityName, CapabilityParamName, EntityFieldName, EntityName, PathMethodSegment,
    RelationName,
};
use crate::{FieldType, SchemaError, ValueWireFormat};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

/// Opaque CML mapping payload (HTTP or EVM); validated at load via `plasm_compile::parse_capability_template`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityTemplateJson(pub serde_json::Value);

impl Deref for CapabilityTemplateJson {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<serde_json::Value> for CapabilityTemplateJson {
    fn from(value: serde_json::Value) -> Self {
        Self(value)
    }
}

/// A complete schema definition for a resource/entity type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceSchema {
    pub name: EntityName,
    /// What this entity represents in the domain.
    #[serde(default)]
    pub description: String,
    pub id_field: EntityFieldName,
    /// Format convention for the id_field value when its FieldType is String.
    ///
    /// `None` means unknown/not specified — grammar emits bare `Entity(id_field)` in the get rule.
    ///
    /// This is **not** enforced at runtime — it is a documentation hint that lets the
    /// prompt renderer and LLM tooling generate correctly-shaped ID values.
    ///
    /// # Authoring note
    ///
    /// This is best populated during CGS extraction.  When an LLM extracts a domain
    /// model from an OpenAPI spec it can observe patterns in example values (e.g.
    /// `"pikachu"`, `"master-ball"` → `slug`; `"550e8400-…"` → `uuid`) and record
    /// the convention here, making future grammar generation and parser normalisation
    /// deterministic without requiring hard-coded per-API knowledge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_format: Option<IdFormat>,
    /// When list/detail JSON rows have no top-level `id`, extract the stable id from this
    /// path (object keys only), e.g. `["location_area","url"]` or author shorthand `"location_area.url"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_from: Option<Vec<String>>,
    pub fields: Vec<FieldSchema>,
    #[serde(default)]
    pub relations: Vec<RelationSchema>,
    /// Alternate spellings accepted by the path parser for this entity (e.g. `Workspace` → `Team`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expression_aliases: Vec<String>,
    /// GET responses keyed only by path (no row id in JSON) — decode uses the request id as `id_field`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub implicit_request_identity: bool,
    /// Ordered logical key parts for compound path identity (GitHub `owner`/`repo`/`number`, etc.).
    /// Empty means a single scalar key via [`Self::id_field`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_vars: Vec<EntityFieldName>,
    /// When true, this entity is only reached via relations / nested JSON — no top-level capabilities
    /// or DOMAIN block. YAML key: `abstract`.
    #[serde(
        default,
        rename = "abstract",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub abstract_entity: bool,
    /// When false, DOMAIN omits the `[field,…]` projection list on this entity’s **heading** line (default: true).
    #[serde(
        default = "default_domain_projection_examples",
        skip_serializing_if = "domain_projection_examples_is_default"
    )]
    pub domain_projection_examples: bool,
    /// Optional override: capability **id** of a **Get** on this entity that defines ordered `provides` /
    /// default field order for DOMAIN heading projection teaching. If the id is missing, not a Get, or targets
    /// another entity, [`CGS::resolved_primary_get_for_projection`] falls back to [`CGS::primary_get_capability`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_read: Option<String>,
}

fn default_domain_projection_examples() -> bool {
    true
}

fn domain_projection_examples_is_default(v: &bool) -> bool {
    *v
}

/// Convention for string-typed id_field values.
///
/// Populated during CGS authoring (ideally by the extraction LLM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdFormat {
    /// Lowercase kebab-case slug, e.g. `"master-ball"`, `"ancient-black-dragon"`.
    Slug,
    /// UUID / GUID string, e.g. `"550e8400-e29b-41d4-a716-446655440000"`.
    Uuid,
    /// Plain integer stored as a string, e.g. `"42"`.
    Integer,
    /// Email address.
    Email,
    /// Any other / unknown format.
    Other,
}

/// Declared on-rails meaning of a `string` field for authoring and agent output policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StringSemantics {
    Short,
    Markdown,
    Document,
    Html,
    #[serde(rename = "json_text")]
    JsonText,
    Blob,
}

/// Optional media classification for [`FieldType::Blob`] fields (prompt/tool hints; wire shape unchanged).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentMediaKind {
    Generic,
    Image,
    Audio,
    Video,
    Document,
}

impl StringSemantics {
    /// Keyword used in DOMAIN `p#` gloss for `string` fields/parameters when semantics are set.
    /// [`StringSemantics::Short`] maps to the generic `str` label via [`None`].
    pub fn gloss_type_keyword(self) -> Option<&'static str> {
        match self {
            StringSemantics::Short => None,
            StringSemantics::Markdown => Some("markdown"),
            StringSemantics::Document => Some("document"),
            StringSemantics::Html => Some("html"),
            StringSemantics::JsonText => Some("json_text"),
            StringSemantics::Blob => Some("blob"),
        }
    }

    /// True for semantics beyond plain short strings: markdown, HTML, documents, JSON text, blobs, etc.
    /// Drives prompts and diagnostics when multiline or structured payloads are expected.
    #[inline]
    pub fn is_structured_or_multiline(self) -> bool {
        !matches!(self, StringSemantics::Short)
    }
}

/// How agents should surface a string field in summaries (table/compact); JSON bodies stay full-fidelity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPresentation {
    Default,
    ReferenceOnly,
    Lossy,
}

/// Element type for [`FieldType::Array`] (domain `items:` / CGS interchange).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrayItemsSchema {
    #[serde(with = "serde_yaml::with::singleton_map")]
    pub field_type: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_format: Option<ValueWireFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,
}

/// Definition of a single field within a resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: EntityFieldName,
    /// What this field represents.
    #[serde(default)]
    pub description: String,
    #[serde(with = "serde_yaml::with::singleton_map")]
    pub field_type: FieldType,
    /// Required when `field_type` is [`FieldType::Date`]: on-wire shape for **predicate and input
    /// expression** values (path parser coercion). Does **not** govern how responses are shown.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_format: Option<ValueWireFormat>,
    pub allowed_values: Option<Vec<String>>,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array_items: Option<ArrayItemsSchema>,
    /// When `field_type` is [`FieldType::String`], classifies payload shape for prompts and summaries.
    /// For [`FieldType::Blob`], omit this key (opaque binary / attachment payloads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_semantics: Option<StringSemantics>,
    /// Override for how table/compact formatters show this string (`None` → derived from semantics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_presentation: Option<AgentPresentation>,
    /// Optional MIME type for **tabular summaries** when the wire value is reference-only or will
    /// be modeled as an attachment: MCP/HTTP table and TSV cells include this next to the ref
    /// placeholder (see `plasm-agent` output formatters). Per-row MIME should instead live on the
    /// decoded value (reserved `__plasm_attachment` object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type_hint: Option<String>,
    /// When `field_type` is [`FieldType::Blob`], optional hint for images/audio/video vs generic binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_media: Option<AttachmentMediaKind>,
    /// JSON object key path for decoding this field from API responses (e.g. `owner.login`).
    /// When unset, the field is read from the top-level object key matching [`Self::name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_path: Option<Vec<String>>,
    /// Derive this field’s value from the JSON at [`Self::wire_path`] (or top-level [`Self::name`])
    /// using a transport-agnostic rule (URL path segment, name/value array lookup, object key, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<FieldDeriveRule>,
}

fn default_name_value_match_key_field() -> String {
    "name".to_string()
}

fn default_name_value_value_field() -> String {
    "value".to_string()
}

/// Wire decode derivation for a field (runs after JSON extraction, before compound-ref assembly).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldDeriveRule {
    /// Strip a prefix, split the remainder on `/`, take the `part_index` segment (0-based).
    /// Empty segments from repeated slashes are skipped.
    SegmentsAfterPrefix {
        prefix: String,
        /// Additional prefixes to try if `prefix` does not match (e.g. `http` vs `https`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        alternate_prefixes: Vec<String>,
        #[serde(default)]
        part_index: usize,
    },
    /// Input must be a JSON **array** of objects (e.g. `[{ "name": "From", "value": "…" }, …]`).
    /// Returns the `value_field` from the **first** object whose `match_key_field` equals `equals`
    /// (optionally case-insensitive). If nothing matches, decodes to JSON null.
    ///
    /// Covers Gmail `payload.headers`, AWS-style `[{ "Key": "…", "Value": "…" }]` tags when
    /// `match_key_field` / `value_field` are set to `Key` / `Value`.
    NameValueArrayLookup {
        equals: String,
        #[serde(default = "default_name_value_match_key_field")]
        match_key_field: String,
        #[serde(default = "default_name_value_value_field")]
        value_field: String,
        #[serde(default)]
        case_insensitive: bool,
    },
    /// Input must be a JSON **object**. Returns `obj[key]` (JSON null if absent).
    /// With `case_insensitive`, finds the first object key that matches `key` ignoring ASCII case.
    ObjectKeyLookup {
        key: String,
        #[serde(default)]
        case_insensitive: bool,
    },
}

impl FieldSchema {
    pub fn effective_string_semantics(&self) -> StringSemantics {
        self.string_semantics.unwrap_or(StringSemantics::Short)
    }

    /// When unset: [`StringSemantics::Short`] → [`AgentPresentation::Default`]; any other semantics → [`AgentPresentation::ReferenceOnly`].
    /// [`FieldType::Blob`] defaults to [`AgentPresentation::ReferenceOnly`] (same as non-`short` strings).
    pub fn effective_agent_presentation(&self) -> AgentPresentation {
        if let Some(p) = self.agent_presentation {
            return p;
        }
        match &self.field_type {
            FieldType::Blob => AgentPresentation::ReferenceOnly,
            FieldType::String => match self.effective_string_semantics() {
                StringSemantics::Short => AgentPresentation::Default,
                _ => AgentPresentation::ReferenceOnly,
            },
            _ => AgentPresentation::Default,
        }
    }
}

/// JSON path segment relative to a parent GET response root (`key` / `wildcard`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonPathSegment {
    Key { key: String },
    Wildcard { wildcard: bool },
}

/// How a relation’s targets are resolved at runtime (scoped query vs embedded GET payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelationMaterialization {
    /// Declared many-edge without chain support yet (`materialize` omitted in YAML = this at validate).
    Unavailable,
    /// Extract related entity refs from nested JSON on the parent entity’s GET body.
    FromParentGet { path: Vec<JsonPathSegment> },
    /// Single scope parameter on a target query/search capability; value from parent id (same as legacy `via_param`).
    ///
    /// `capability` must name a `query` / `search` capability on [`RelationSchema::target_resource`]
    /// whose object input declares `param` — never inferred from parameter name alone.
    QueryScoped {
        capability: CapabilityName,
        param: CapabilityParamName,
    },
    /// Multiple scope parameters; map keys are capability param names, values are parent entity field names.
    ///
    /// `capability` must name a `query` / `search` capability on the target entity whose object input
    /// declares every binding key.
    QueryScopedBindings {
        capability: CapabilityName,
        bindings: IndexMap<CapabilityParamName, EntityFieldName>,
    },
}

/// Definition of a relation (graph edge) between resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationSchema {
    pub name: RelationName,
    /// Why this relation exists.
    #[serde(default)]
    pub description: String,
    pub target_resource: EntityName,
    pub cardinality: Cardinality,
    /// When set, defines how chain traversal materializes targets (`cardinality: many` required).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialize: Option<RelationMaterialization>,
}

/// Cardinality of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    One,
    Many,
}

/// What to do with a compound [`FieldType::EntityRef`] **scope** parameter after runtime splat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeAggregateKeyPolicy {
    /// Keep the aggregate key (e.g. `repository`) in the CML env alongside splatted keys.
    #[default]
    Retain,
    /// Remove the aggregate key once every target-entity `key_vars` slot is satisfied in the env.
    OmitWhenRedundant,
}

/// A capability defines how to interact with a resource (query, get, invoke).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySchema {
    pub name: CapabilityName,
    /// What this capability does.
    #[serde(default)]
    pub description: String,
    pub kind: CapabilityKind,
    pub domain: EntityName, // Entity this capability operates on
    pub mapping: CapabilityMapping,
    /// Input schema for invoke capabilities (optional for query/get)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<InputSchema>,
    /// Output schema specification (for validation and projection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<OutputSchema>,
    /// Entity fields this capability populates in its response (field-level provenance).
    ///
    /// Used by the runtime to auto-invoke the right capability when a projected field
    /// is absent from the cache — the "field provider" mechanism.
    ///
    /// **Defaults when absent** (backward-compatible):
    /// - `get` → all entity fields (single-resource fetch, expected to provide everything)
    /// - `query` / `search` → all entity fields (optimistic; hydration fixes gaps if wrong)
    /// - `create` / `update` / `delete` / `action` → empty (may only return `id`)
    ///
    /// Declare explicitly only when the response is a **disjoint projection** of the
    /// entity — i.e. when two or more capabilities for the same entity return different
    /// non-overlapping field subsets (same ID, different fields).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<String>,
    /// Policy for the aggregate scope param after compound `entity_ref` scope splat runs.
    #[serde(default)]
    pub scope_aggregate_key_policy: ScopeAggregateKeyPolicy,
    /// Before compiling the invoke template, run another capability (typically `kind: get`)
    /// on the invoke target and merge decoded fields into the CML env under `env_prefix_*`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke_preflight: Option<InvokePreflight>,
}

/// Declarative preflight for [`CapabilitySchema`] (e.g. hydrate parent row before a write).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvokePreflight {
    /// CGS capability name to run (must be [`CapabilityKind::Get`] on [`CapabilitySchema::domain`]).
    pub hydrate_capability: String,
    /// Each decoded field name `foo` is merged as `{env_prefix}_foo` (e.g. `parent_threadId`).
    pub env_prefix: String,
}

/// The type of operation this capability performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    /// Filter/list a collection by field predicates.
    Query,
    /// Full-text relevance search returning ranked results.
    /// Primary input is a free-text `q`/`query`/`search` parameter, not field predicates.
    /// CLI verb: `entity search`, distinct from `entity query`.
    Search,
    Get,    // Fetch resource by ID
    Create, // Create a new resource (no target ID)
    Update, // Modify an existing resource (target ID required)
    Delete, // Remove a resource (target ID required)
    Action, // Any other entity-scoped operation
}

/// Semantic role of a capability parameter.
///
/// All roles produce the same HTTP transport (a query param or path segment),
/// but carry different meaning for agents and LLM tooling:
/// - [`Filter`]: equality/range predicate on entity field values
/// - [`Search`]: free-text relevance query (`q`, `query`, `search`)
/// - [`Sort`]: selects a sort field (`order_by`)
/// - [`SortDirection`]: ascending/descending companion to Sort
/// - [`ResponseControl`]: modifies payload shape (`embed`, `fields`, `inc`)
/// - [`Scope`]: parent-entity pivot wired into the URL path (entity_ref, required)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParameterRole {
    /// Default. Equality or range predicate on entity field values.
    #[default]
    Filter,
    /// Free-text relevance query (`q`, `query`, `search`).
    Search,
    /// Sort field selector (`order_by`, `sort_by`).
    Sort,
    /// Sort direction (`sort`, `asc`/`desc`) — companion to [`Sort`].
    SortDirection,
    /// Payload shape control (`embed`, `fields`, `inc`, `exc`).
    ResponseControl,
    /// Parent-entity FK pivot wired into the URL path segment.
    Scope,
}

/// Mapping configuration for how this capability translates to backend calls.
/// This is a JSON object that will be interpreted by the CML compiler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityMapping {
    pub template: CapabilityTemplateJson,
}

/// HTTP path segment variable names from CML `path` (in order), `type: var` only.
///
/// GraphQL capabilities often have **no** `path` vars (POST `/graphql` is all literals); subject id
/// may live under `body` — see [`template_domain_exemplar_requires_entity_anchor`] and
/// [`template_invoke_requires_explicit_anchor_id`].
pub fn path_var_names_from_mapping_json(template: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(path) = template.get("path").and_then(|p| p.as_array()) else {
        return out;
    };
    for seg in path {
        if seg.get("type").and_then(|t| t.as_str()) == Some("var") {
            if let Some(name) = seg.get("name").and_then(|n| n.as_str()) {
                out.push(name.to_string());
            }
        }
    }
    out
}

fn collect_template_var_refs(template: &serde_json::Value, out: &mut Vec<String>) {
    match template {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|t| t.as_str()) == Some("var") {
                if let Some(name) = map.get("name").and_then(|n| n.as_str()) {
                    out.push(name.to_string());
                }
            }
            for v in map.values() {
                collect_template_var_refs(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for e in arr {
                collect_template_var_refs(e, out);
            }
        }
        _ => {}
    }
}

/// Every CML `type: var` / `name` in the mapping template JSON (including nested bodies).
pub fn capability_template_all_var_names(template: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_template_var_refs(template, &mut out);
    out.sort();
    out.dedup();
    out
}

/// `type: var` / `name` entries under GraphQL `body` → `variables` (operation variables only).
///
/// Used with [`path_var_names_from_mapping_json`] for zero-arity `Issue(id).get()`: the HTTP `path` is
/// only `/graphql`, but `variables.id` still needs the anchor id — without this, the parser wrongly
/// defaulted the target id to `"0"`.
///
/// We intentionally **do not** scan the whole template (login bodies, pagination, etc.).
pub fn graphql_operation_variable_names(template: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if template.get("transport").and_then(|t| t.as_str()) != Some("graphql") {
        return out;
    }
    let Some(body) = template.get("body") else {
        return out;
    };
    graphql_find_variables_block(body, &mut out);
    out.sort();
    out.dedup();
    out
}

/// True when DOMAIN exemplars must use `Entity($)` / an anchored receiver (`Entity($).m()`), not a
/// bare pathless `Entity.get()`-style line.
///
/// Transport-neutral predicate: combines HTTP path `var` segments with GraphQL operation `variables`
/// that bind the primary subject (`id`). Prompt synthesis calls this via
/// [`CapabilitySchema::domain_exemplar_requires_entity_anchor`]; it does **not** import GraphQL by name.
pub fn template_domain_exemplar_requires_entity_anchor(template: &serde_json::Value) -> bool {
    if !path_var_names_from_mapping_json(template).is_empty() {
        return true;
    }
    if template.get("transport").and_then(|t| t.as_str()) != Some("graphql") {
        return false;
    }
    graphql_operation_variable_names(template)
        .iter()
        .any(|n| n == "id")
}

/// True when parse of dotted-call alias `Entity($).method()` cannot default the subject id to `"0"`: any path
/// template variable or any GraphQL operation variable (pagination vars count for queries).
pub fn template_invoke_requires_explicit_anchor_id(template: &serde_json::Value) -> bool {
    !path_var_names_from_mapping_json(template).is_empty()
        || !graphql_operation_variable_names(template).is_empty()
}

fn graphql_find_variables_block(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(fields) = map.get("fields").and_then(|f| f.as_array()) {
                for item in fields {
                    if let Some(pair) = item.as_array() {
                        if pair.len() >= 2 {
                            let key = pair[0].as_str();
                            let val = &pair[1];
                            if key == Some("variables") {
                                collect_template_var_refs(val, out);
                                return;
                            }
                        }
                    }
                    graphql_find_variables_block(item, out);
                }
            }
            for val in map.values() {
                graphql_find_variables_block(val, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for e in arr {
                graphql_find_variables_block(e, out);
            }
        }
        _ => {}
    }
}

/// Path method segment for prompts and parser matching (`team_seats` → `seats` after domain strip).
///
/// This is the **surface label** after `Entity(id).` that selects a capability — not generic casing,
/// and distinct from the schema’s [`CapabilityName`](crate::identity::CapabilityName) string.
pub fn capability_path_method_segment(cap: &CapabilitySchema) -> PathMethodSegment {
    PathMethodSegment::new(capability_method_label_kebab_inner(cap))
}

fn capability_method_label_kebab_inner(cap: &CapabilitySchema) -> String {
    let ent = cap.domain.as_str();
    let prefix = format!("{}_", ent.to_lowercase());
    cap.name
        .strip_prefix(&prefix)
        .unwrap_or(cap.name.as_str())
        .replace('_', "-")
}

/// String form of [`capability_path_method_segment`] (HTTP / legacy call sites).
#[inline]
pub fn capability_method_label_kebab(cap: &CapabilitySchema) -> String {
    capability_path_method_segment(cap).into_inner()
}

/// True when this capability has no required invoke inputs — valid for `Entity(id).method()` / `method()`
/// when combined with an invoke-on-ref kind (`Action`, `Update`, `Delete`), regardless of HTTP verb.
pub fn capability_is_zero_arity_invoke(cap: &CapabilitySchema) -> bool {
    match &cap.input_schema {
        None => true,
        Some(is) => match &is.input_type {
            InputType::Object { fields, .. } => !fields.iter().any(|f| f.required),
            InputType::None => true,
            _ => false,
        },
    }
}

/// Deprecated alias for [`capability_is_zero_arity_invoke`].
#[inline]
pub fn capability_is_zero_arity_action(cap: &CapabilitySchema) -> bool {
    capability_is_zero_arity_invoke(cap)
}

/// Input schema for invoke capabilities - defines expected input structure and validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputSchema {
    /// The expected input type
    pub input_type: InputType,
    /// Validation rules for the input
    #[serde(default)]
    pub validation: InputValidation,
    /// Human-readable description of the input
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Examples of valid input
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<serde_json::Value>,
}

/// Types of input that capabilities can accept
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InputType {
    /// No input required
    #[serde(rename = "none")]
    None,

    /// Single value input
    #[serde(rename = "value")]
    Value {
        #[serde(with = "serde_yaml::with::singleton_map")]
        field_type: FieldType,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_values: Option<Vec<String>>,
    },

    /// Object input with typed fields
    #[serde(rename = "object")]
    Object {
        fields: Vec<InputFieldSchema>,
        /// Whether additional fields beyond those defined are allowed
        #[serde(default)]
        additional_fields: bool,
    },

    /// Array input
    #[serde(rename = "array")]
    Array {
        element_type: Box<InputType>,
        #[serde(skip_serializing_if = "Option::is_none")]
        min_length: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_length: Option<usize>,
    },

    /// Union of multiple possible types
    #[serde(rename = "union")]
    Union { variants: Vec<InputType> },
}

/// Field schema for object inputs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputFieldSchema {
    pub name: String,
    #[serde(with = "serde_yaml::with::singleton_map")]
    pub field_type: FieldType,
    /// Required when `field_type` is [`FieldType::Date`]: wire shape for **input** (query params /
    /// body fields built from expressions), not display formatting.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_format: Option<ValueWireFormat>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array_items: Option<ArrayItemsSchema>,
    /// When `field_type` is [`FieldType::String`], classifies payload shape for prompts and summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_semantics: Option<StringSemantics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default value if not provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<crate::Value>,
    /// Semantic role of this parameter. Defaults to `filter`.
    /// Agents and LLM tooling use this to understand how the param affects results.
    #[serde(default, skip_serializing_if = "is_default_role")]
    pub role: Option<ParameterRole>,
}

fn is_default_role(r: &Option<ParameterRole>) -> bool {
    matches!(r, None | Some(ParameterRole::Filter))
}

impl InputFieldSchema {
    pub fn effective_string_semantics(&self) -> StringSemantics {
        self.string_semantics.unwrap_or(StringSemantics::Short)
    }
}

/// Validation constraints for inputs
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct InputValidation {
    /// Custom validation predicates that must be satisfied
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predicates: Vec<ValidationPredicate>,
    /// Whether null/undefined inputs are allowed
    #[serde(default)]
    pub allow_null: bool,
    /// Cross-field validation rules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_field_rules: Vec<CrossFieldRule>,
}

/// A validation predicate for input values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationPredicate {
    /// Field path this predicate applies to (dot notation: "user.email")
    pub field_path: String,
    /// The validation operator
    pub operator: ValidationOp,
    /// The value to validate against
    pub value: crate::Value,
    /// Error message if validation fails
    pub error_message: String,
}

/// Validation operators for input constraints
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOp {
    /// Minimum length for strings/arrays
    MinLength,
    /// Maximum length for strings/arrays
    MaxLength,
    /// Minimum value for numbers
    MinValue,
    /// Maximum value for numbers
    MaxValue,
    /// Regular expression pattern for strings
    Pattern,
    /// Custom validation function reference
    CustomFunction,
    /// Dependency on another field
    DependsOn,
}

/// Cross-field validation rules
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossFieldRule {
    /// Type of cross-field validation
    pub rule_type: CrossFieldRuleType,
    /// Fields involved in this rule
    pub fields: Vec<String>,
    /// Error message if rule fails
    pub error_message: String,
}

/// Types of cross-field validation rules
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossFieldRuleType {
    /// At least one of the fields must be present
    AtLeastOne,
    /// Exactly one of the fields must be present
    ExactlyOne,
    /// All fields must be present together or none
    AllOrNone,
    /// If field A is present, field B must also be present
    Implies,
    /// Fields are mutually exclusive
    MutuallyExclusive,
}

fn default_empty_json_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Output schema specification for capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSchema {
    /// Expected output structure (`type: side_effect` | `entity` | … at the same level as `decoder`).
    #[serde(flatten)]
    pub output_type: OutputType,
    /// Decoder specification for parsing responses (JSON for now, typed later)
    #[serde(default = "default_empty_json_object")]
    pub decoder: serde_json::Value,
    /// Whether the output is expected to be idempotent
    #[serde(default)]
    pub idempotent: bool,
}

/// Types of output that capabilities can produce
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputType {
    /// Effectful operation with no entity projection (empty or ignored response body).
    /// `description` must state **what** changes in the domain; required and non-empty when trimmed.
    #[serde(rename = "side_effect")]
    SideEffect { description: String },

    /// Single entity
    #[serde(rename = "entity")]
    Entity { entity_type: String },

    /// Collection of entities
    #[serde(rename = "collection")]
    Collection {
        entity_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_count: Option<usize>,
    },

    /// Status/acknowledgment response
    #[serde(rename = "status")]
    Status { success_indicators: Vec<String> },

    /// Custom structured response
    #[serde(rename = "custom")]
    Custom { schema: serde_json::Value },
}

/// Authentication scheme declared in domain.yaml under the top-level `auth:` key.
///
/// Use [`AuthScheme::None`] to mark **public** catalogs (no outbound credentials). Omitting `auth`
/// entirely is still accepted for backward compatibility but is ambiguous for tooling.
///
/// For each secret-bearing slot on other variants, specify **exactly one** of:
/// - `env` — environment variable name (local dev / operator-managed)
/// - `hosted_kv` — auth-framework `kv_store` key (Plasm-hosted secrets; must start with `plasm:outbound:`)
///
/// The runtime resolves values via [`plasm_runtime::auth::SecretProvider`] (`get_secret` vs `get_hosted_secret`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scheme", rename_all = "snake_case")]
pub enum AuthScheme {
    /// No outbound HTTP credentials (public / open API). YAML: `auth: { scheme: none }`.
    #[serde(rename = "none")]
    None,
    /// Static API key sent as a request header.
    /// e.g. `X-Api-Key: <value>` or `Authorization: <value>`
    ApiKeyHeader {
        /// Header name to use (e.g. `X-Api-Key`, `Authorization`)
        header: String,
        /// Name of the environment variable holding the key value
        #[serde(default)]
        env: Option<String>,
        /// auth-framework KV key for the stored secret
        #[serde(default)]
        hosted_kv: Option<String>,
    },
    /// Static API key appended as a URL query parameter.
    /// e.g. `?apikey=<value>`
    ApiKeyQuery {
        /// Query parameter name (e.g. `apikey`, `api_key`, `key`)
        param: String,
        /// Name of the environment variable holding the key value
        #[serde(default)]
        env: Option<String>,
        /// auth-framework KV key for the stored secret
        #[serde(default)]
        hosted_kv: Option<String>,
    },
    /// Bearer token sent as `Authorization: Bearer <token>`.
    /// Semantically distinct from `ApiKeyHeader` for agent tooling.
    BearerToken {
        /// Name of the environment variable holding the bearer token
        #[serde(default)]
        env: Option<String>,
        /// auth-framework KV key for the stored token
        #[serde(default)]
        hosted_kv: Option<String>,
    },
    /// OAuth 2.0 Client Credentials flow.
    /// The runtime exchanges `client_id` + `client_secret` for an access token,
    /// caches it, and refreshes on expiry or 401.
    Oauth2ClientCredentials {
        /// Token endpoint URL
        token_url: String,
        /// Env var holding the OAuth2 client ID
        #[serde(default)]
        client_id_env: Option<String>,
        /// auth-framework KV key for the client ID
        #[serde(default)]
        client_id_hosted_kv: Option<String>,
        /// Env var holding the OAuth2 client secret
        #[serde(default)]
        client_secret_env: Option<String>,
        /// auth-framework KV key for the client secret
        #[serde(default)]
        client_secret_hosted_kv: Option<String>,
        /// Optional list of scopes to request
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        scopes: Vec<String>,
    },
}

impl AuthScheme {
    pub(crate) fn validate(&self) -> Result<(), crate::error::SchemaError> {
        use crate::error::SchemaError;

        fn one_of_env_hosted(
            env: Option<&str>,
            hosted: Option<&str>,
            ctx: &'static str,
        ) -> Result<(), SchemaError> {
            let e = env.map(str::trim).filter(|s| !s.is_empty());
            let h = hosted.map(str::trim).filter(|s| !s.is_empty());
            match (e, h) {
                (Some(_), Some(_)) => Err(SchemaError::AuthCredentialSourceInvalid {
                    context: ctx.into(),
                }),
                (None, None) => Err(SchemaError::AuthCredentialSourceInvalid {
                    context: ctx.into(),
                }),
                (None, Some(k)) => {
                    if !k.starts_with("plasm:outbound:") {
                        return Err(SchemaError::AuthHostedKvKeyPrefix { field: ctx.into() });
                    }
                    Ok(())
                }
                (Some(_), None) => Ok(()),
            }
        }

        match self {
            AuthScheme::None => Ok(()),
            AuthScheme::ApiKeyHeader { env, hosted_kv, .. } => {
                one_of_env_hosted(env.as_deref(), hosted_kv.as_deref(), "api_key_header")
            }
            AuthScheme::ApiKeyQuery { env, hosted_kv, .. } => {
                one_of_env_hosted(env.as_deref(), hosted_kv.as_deref(), "api_key_query")
            }
            AuthScheme::BearerToken { env, hosted_kv } => {
                one_of_env_hosted(env.as_deref(), hosted_kv.as_deref(), "bearer_token")
            }
            AuthScheme::Oauth2ClientCredentials {
                token_url,
                client_id_env,
                client_id_hosted_kv,
                client_secret_env,
                client_secret_hosted_kv,
                ..
            } => {
                if token_url.trim().is_empty() {
                    return Err(SchemaError::AuthOauth2TokenUrlEmpty);
                }
                one_of_env_hosted(
                    client_id_env.as_deref(),
                    client_id_hosted_kv.as_deref(),
                    "oauth2_client_credentials.client_id",
                )?;
                one_of_env_hosted(
                    client_secret_env.as_deref(),
                    client_secret_hosted_kv.as_deref(),
                    "oauth2_client_credentials.client_secret",
                )
            }
        }
    }
}

/// One entry in the [`OauthExtension::scopes`] catalog (documentation and validation anchor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OauthScopeEntry {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
}

/// Named bundle of scopes for documentation (e.g. control-plane defaults); not an auth config object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OauthDefaultScopeSet {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// OAuth scope requirement: either **any** of `any_of`, or **all** of nested `all_of` clauses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScopeRequirement {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all_of: Vec<ScopeRequirement>,
}

impl ScopeRequirement {
    /// True if `granted` contains at least one scope from `any_of`, or every nested `all_of` clause holds.
    pub fn satisfied_by(&self, granted: &std::collections::HashSet<String>) -> bool {
        if !self.any_of.is_empty() {
            return self.any_of.iter().any(|s| granted.contains(s.as_str()));
        }
        self.all_of.iter().all(|r| r.satisfied_by(granted))
    }
}

/// Per-capability and per-relation OAuth scope implications (orthogonal to `auth:` transport).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OauthRequirements {
    #[serde(default)]
    pub capabilities: IndexMap<String, ScopeRequirement>,
    /// Keys are `Entity.relation` (e.g. `Issue.comments`).
    #[serde(default)]
    pub relations: IndexMap<String, ScopeRequirement>,
}

/// Declarative OAuth scope model for excluding capabilities when granted scopes are insufficient.
///
/// Runtime scope grants come from the control plane; this block only describes implications for CGS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OauthExtension {
    pub provider: String,
    #[serde(default)]
    pub scopes: IndexMap<String, OauthScopeEntry>,
    #[serde(default)]
    pub default_scope_sets: IndexMap<String, OauthDefaultScopeSet>,
    #[serde(default)]
    pub requirements: OauthRequirements,
}

/// Pre-classified capability surface for one entity. Built once by [`CGS::capability_manifest`]
/// and consumed by both CLI generation (`plasm-agent`) and prompt rendering (`prompt_render`).
#[derive(Debug)]
pub struct CapabilityManifest<'a> {
    /// Unscoped query (the "list all" verb), if any.
    pub primary_query: Option<&'a CapabilitySchema>,
    /// Unscoped search, if any.
    pub primary_search: Option<&'a CapabilitySchema>,
    /// Scoped queries and non-primary search caps (get named subcommands in CLI).
    pub named_queries: Vec<&'a CapabilitySchema>,
    /// The primary Get capability (non-singleton), if any.
    pub get: Option<&'a CapabilitySchema>,
    /// Pathless, parameterless Gets (e.g. `user_get_me` → `User.get-me()`).
    pub singleton_gets: Vec<&'a CapabilitySchema>,
    /// Action / Update / Delete caps with no required inputs.
    pub zero_arity_methods: Vec<&'a CapabilitySchema>,
    /// Action / Update / Delete caps with required inputs.
    pub multi_arity_methods: Vec<&'a CapabilitySchema>,
    /// Create caps whose domain is this entity (may or may not bind from an anchor).
    pub standalone_creates: Vec<&'a CapabilitySchema>,
}

/// Precomputed (entity, kind) → capability name list in [`CGS::capabilities`] iteration order.
/// Built lazily on first [`CGS::find_capabilities`] to keep prompt/runtime lookups O(k) per entity
/// instead of O(total_capabilities) per call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgsCapabilityIndex {
    by_domain_kind: IndexMap<(EntityName, CapabilityKind), Vec<CapabilityName>>,
}

impl CgsCapabilityIndex {
    pub fn build(cgs: &CGS) -> Self {
        let mut by_domain_kind: IndexMap<(EntityName, CapabilityKind), Vec<CapabilityName>> =
            IndexMap::new();
        for (name, cap) in cgs.capabilities.iter() {
            let key = (cap.domain.clone(), cap.kind);
            by_domain_kind.entry(key).or_default().push(name.clone());
        }
        Self { by_domain_kind }
    }

    #[inline]
    pub fn names_for_domain_kind(&self, entity: &str, kind: CapabilityKind) -> &[CapabilityName] {
        let key = (EntityName::from(entity), kind);
        self.by_domain_kind
            .get(&key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Default HTTP origin when constructing an empty [`CGS`] in tests or programmatically.
pub const DEFAULT_HTTP_BACKEND: &str = "http://localhost:1080";

/// Capability Graph Schema (CGS) - the root schema container.
#[derive(Debug, Serialize, Deserialize)]
pub struct CGS {
    pub entities: IndexMap<EntityName, EntityDef>,
    pub capabilities: IndexMap<CapabilityName, CapabilitySchema>,
    /// Default HTTP(S) origin for CML execution against this catalog (required in interchange).
    pub http_backend: String,
    /// Optional authentication scheme for all requests made against this schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthScheme>,
    /// Optional declarative OAuth scope implications (control plane supplies granted scopes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OauthExtension>,
    /// When this CGS is distributed as a self-describing plugin, stable catalog id (optional for file-backed schemas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    /// Monotonic distribution version for this catalog entry (`0` when omitted in plain file schemas).
    #[serde(default)]
    pub version: u64,
    /// Lazily built index for [`Self::find_capabilities`] / [`Self::find_capability`].
    #[serde(skip, default = "new_capability_index_lock")]
    capability_index: OnceLock<Arc<CgsCapabilityIndex>>,
    /// Capability names grouped by [`CapabilitySchema::domain`] (lazy; reset on [`CGS`] clone).
    #[serde(skip, default = "new_capability_names_by_domain_lock")]
    capability_names_by_domain: OnceLock<Arc<IndexMap<String, Vec<CapabilityName>>>>,
}

fn new_capability_index_lock() -> OnceLock<Arc<CgsCapabilityIndex>> {
    OnceLock::new()
}

fn new_capability_names_by_domain_lock() -> OnceLock<Arc<IndexMap<String, Vec<CapabilityName>>>> {
    OnceLock::new()
}

impl Clone for CGS {
    fn clone(&self) -> Self {
        Self {
            entities: self.entities.clone(),
            capabilities: self.capabilities.clone(),
            http_backend: self.http_backend.clone(),
            auth: self.auth.clone(),
            oauth: self.oauth.clone(),
            entry_id: self.entry_id.clone(),
            version: self.version,
            capability_index: OnceLock::new(),
            capability_names_by_domain: OnceLock::new(),
        }
    }
}

impl PartialEq for CGS {
    fn eq(&self, other: &Self) -> bool {
        self.entities == other.entities
            && self.capabilities == other.capabilities
            && self.http_backend == other.http_backend
            && self.auth == other.auth
            && self.oauth == other.oauth
            && self.entry_id == other.entry_id
            && self.version == other.version
    }
}

impl Eq for CGS {}

/// Internal representation of an entity definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityDef {
    pub name: EntityName,
    /// What this entity represents in the domain.
    #[serde(default)]
    pub description: String,
    pub id_field: EntityFieldName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_format: Option<IdFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_from: Option<Vec<String>>,
    pub fields: IndexMap<EntityFieldName, FieldSchema>,
    pub relations: IndexMap<RelationName, RelationSchema>,
    /// Alternate spellings accepted by the path parser for this entity (must be unique across CGS).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expression_aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub implicit_request_identity: bool,
    /// When non-empty with more than one name, [`plasm_core::Ref`] must use [`EntityKey::Compound`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_vars: Vec<EntityFieldName>,
    /// Embed-only / relation target only — excluded from expression witness and DOMAIN full list.
    #[serde(
        default,
        rename = "abstract",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub abstract_entity: bool,
    /// When false, DOMAIN omits the `[field,…]` projection list on this entity’s **heading** line (default: true).
    #[serde(
        default = "default_domain_projection_examples",
        skip_serializing_if = "domain_projection_examples_is_default"
    )]
    pub domain_projection_examples: bool,
    /// Optional: capability **id** of a **Get** on this entity for DOMAIN heading projection order (see [`CGS::resolved_primary_get_for_projection`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_read: Option<String>,
}

impl ResourceSchema {
    /// Convert this resource schema to an internal EntityDef.
    pub fn to_entity_def(&self) -> Result<EntityDef, SchemaError> {
        let mut fields = IndexMap::new();
        let mut relations = IndexMap::new();

        // Check for duplicate fields
        for field in &self.fields {
            if fields.contains_key(&field.name) {
                return Err(SchemaError::DuplicateField {
                    entity: self.name.to_string(),
                    field: field.name.to_string(),
                });
            }
            fields.insert(field.name.clone(), field.clone());
        }

        // Check for duplicate relations
        for relation in &self.relations {
            if relations.contains_key(&relation.name) {
                return Err(SchemaError::DuplicateRelation {
                    entity: self.name.to_string(),
                    relation: relation.name.to_string(),
                });
            }
            relations.insert(relation.name.clone(), relation.clone());
        }

        // Verify ID field exists as a declared field, unless id_from supplies identity from JSON.
        let id_from_path = self
            .id_from
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !fields.contains_key(&self.id_field) && !id_from_path && !self.implicit_request_identity
        {
            return Err(SchemaError::MissingIdField {
                entity: self.name.to_string(),
                id_field: self.id_field.to_string(),
            });
        }

        if self.key_vars.len() > 1 {
            for kv in &self.key_vars {
                if !fields.contains_key(kv) {
                    return Err(SchemaError::UnknownKeyVarField {
                        entity: self.name.to_string(),
                        field: kv.to_string(),
                    });
                }
            }
        }

        Ok(EntityDef {
            name: self.name.clone(),
            description: self.description.clone(),
            id_field: self.id_field.clone(),
            id_format: self.id_format,
            id_from: self.id_from.clone(),
            fields,
            relations,
            expression_aliases: self.expression_aliases.clone(),
            implicit_request_identity: self.implicit_request_identity,
            key_vars: self.key_vars.clone(),
            abstract_entity: self.abstract_entity,
            domain_projection_examples: self.domain_projection_examples,
            primary_read: self.primary_read.clone(),
        })
    }
}

impl CGS {
    /// Create a new empty CGS.
    pub fn new() -> Self {
        Self {
            entities: IndexMap::new(),
            capabilities: IndexMap::new(),
            http_backend: DEFAULT_HTTP_BACKEND.to_string(),
            auth: None,
            oauth: None,
            entry_id: None,
            version: 0,
            capability_index: new_capability_index_lock(),
            capability_names_by_domain: new_capability_names_by_domain_lock(),
        }
    }

    /// Capability names registered on each domain entity, in schema declaration order.
    pub(crate) fn capability_names_by_domain(&self) -> &IndexMap<String, Vec<CapabilityName>> {
        self.capability_names_by_domain
            .get_or_init(|| {
                let mut m = IndexMap::new();
                for (name, cap) in self.capabilities.iter() {
                    m.entry(cap.domain.to_string())
                        .or_insert_with(Vec::new)
                        .push(name.clone());
                }
                Arc::new(m)
            })
            .as_ref()
    }

    /// Stable hex digest (SHA-256) of canonical JSON for this CGS, used for session pinning and plugin integrity.
    pub fn catalog_cgs_hash_hex(&self) -> String {
        let v = serde_json::to_vec(self).expect("CGS JSON serialization for catalog hash");
        let digest = Sha256::digest(&v);
        hex::encode(digest)
    }

    /// When [`Self::oauth`] is set and lists a requirement for `capability`, returns whether
    /// `granted_scopes` satisfies it. If there is no oauth block or no entry for the capability,
    /// returns `None` (caller treats as not OAuth-gated by this schema).
    pub fn oauth_capability_satisfied(
        &self,
        capability: &str,
        granted_scopes: &std::collections::HashSet<String>,
    ) -> Option<bool> {
        let oauth = self.oauth.as_ref()?;
        let req = oauth.requirements.capabilities.get(capability)?;
        Some(req.satisfied_by(granted_scopes))
    }

    /// Same as [`Self::oauth_capability_satisfied`] for relation traversal keys `Entity.relation`.
    pub fn oauth_relation_satisfied(
        &self,
        entity_relation: &str,
        granted_scopes: &std::collections::HashSet<String>,
    ) -> Option<bool> {
        let oauth = self.oauth.as_ref()?;
        let req = oauth.requirements.relations.get(entity_relation)?;
        Some(req.satisfied_by(granted_scopes))
    }

    fn capability_index_arc(&self) -> &Arc<CgsCapabilityIndex> {
        self.capability_index
            .get_or_init(|| Arc::new(CgsCapabilityIndex::build(self)))
    }

    /// Add a resource schema to this CGS.
    pub fn add_resource(&mut self, resource: ResourceSchema) -> Result<(), SchemaError> {
        if self.entities.contains_key(&resource.name) {
            return Err(SchemaError::DuplicateEntity {
                name: resource.name.to_string(),
            });
        }

        let entity_def = resource.to_entity_def()?;
        self.entities.insert(resource.name.clone(), entity_def);
        Ok(())
    }

    /// Add a capability to this CGS.
    pub fn add_capability(&mut self, capability: CapabilitySchema) -> Result<(), SchemaError> {
        // Verify the domain entity exists
        if !self.entities.contains_key(&capability.domain) {
            return Err(SchemaError::UnknownTargetEntity {
                entity: "capability".to_string(),
                relation: capability.name.to_string(),
                target: capability.domain.to_string(),
            });
        }

        self.capabilities
            .insert(capability.name.clone(), capability);
        Ok(())
    }

    /// Validate all cross-references in this schema.
    pub fn validate(&self) -> Result<(), SchemaError> {
        for (entity_name, entity) in &self.entities {
            if let Some(ref cap_id) = entity.primary_read {
                let Some(cap) = self.capabilities.get(cap_id.as_str()) else {
                    return Err(SchemaError::UnknownPrimaryReadCapability {
                        entity: entity_name.to_string(),
                        capability: cap_id.clone(),
                    });
                };
                if cap.domain != *entity_name {
                    return Err(SchemaError::PrimaryReadWrongDomain {
                        entity: entity_name.to_string(),
                        capability: cap_id.clone(),
                        domain: cap.domain.to_string(),
                    });
                }
                if cap.kind != CapabilityKind::Get {
                    return Err(SchemaError::PrimaryReadNotGet {
                        entity: entity_name.to_string(),
                        capability: cap_id.clone(),
                        kind: format!("{:?}", cap.kind),
                    });
                }
            }
        }

        // Check that all relation targets exist
        for (entity_name, entity) in &self.entities {
            for (relation_name, relation) in &entity.relations {
                if !self.entities.contains_key(&relation.target_resource) {
                    return Err(SchemaError::UnknownTargetEntity {
                        entity: entity_name.to_string(),
                        relation: relation_name.to_string(),
                        target: relation.target_resource.to_string(),
                    });
                }
            }
        }

        // Relation materialization (many requires `materialize:`; one must omit it)
        for (entity_name, entity) in &self.entities {
            for (relation_name, relation) in &entity.relations {
                match relation.cardinality {
                    Cardinality::Many => {
                        let mat = relation
                            .materialize
                            .as_ref()
                            .unwrap_or(&RelationMaterialization::Unavailable);
                        match mat {
                            RelationMaterialization::Unavailable => {}
                            RelationMaterialization::FromParentGet { path } => {
                                if path.is_empty() {
                                    return Err(SchemaError::RelationFromParentGetEmptyPath {
                                        entity: entity_name.to_string(),
                                        relation: relation_name.to_string(),
                                    });
                                }
                                for seg in path {
                                    if let JsonPathSegment::Wildcard { wildcard } = seg {
                                        if !wildcard {
                                            return Err(
                                                SchemaError::RelationFromParentGetInvalidWildcard {
                                                    entity: entity_name.to_string(),
                                                    relation: relation_name.to_string(),
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                            RelationMaterialization::QueryScoped { capability, param } => {
                                self.validate_chain_materialize_capability(
                                    entity_name.as_str(),
                                    relation_name.as_str(),
                                    relation.target_resource.as_str(),
                                    capability,
                                    &[param.as_str()],
                                )?;
                            }
                            RelationMaterialization::QueryScopedBindings {
                                capability,
                                bindings,
                            } => {
                                if bindings.is_empty() {
                                    return Err(SchemaError::RelationMaterializeEmptyBindings {
                                        entity: entity_name.to_string(),
                                        relation: relation_name.to_string(),
                                    });
                                }
                                let keys: Vec<&str> = bindings.keys().map(|k| k.as_str()).collect();
                                self.validate_chain_materialize_capability(
                                    entity_name.as_str(),
                                    relation_name.as_str(),
                                    relation.target_resource.as_str(),
                                    capability,
                                    &keys,
                                )?;
                                for parent_field in bindings.values() {
                                    let ok = parent_field.as_str() == entity.id_field.as_str()
                                        || entity.fields.contains_key(parent_field.as_str())
                                        || entity
                                            .key_vars
                                            .iter()
                                            .any(|k| k.as_str() == parent_field.as_str());
                                    if !ok {
                                        return Err(
                                            SchemaError::RelationMaterializeUnknownParentField {
                                                entity: entity_name.to_string(),
                                                relation: relation_name.to_string(),
                                                field: parent_field.as_str().to_string(),
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Cardinality::One => match &relation.materialize {
                        None => {}
                        Some(RelationMaterialization::FromParentGet { .. }) => {}
                        Some(_) => {
                            return Err(SchemaError::RelationOneWithDisallowedMaterialize {
                                entity: entity_name.to_string(),
                                relation: relation_name.to_string(),
                            });
                        }
                    },
                }
            }
        }

        // EntityRef targets on entity fields; typed arrays / multi_select
        for (entity_name, entity) in &self.entities {
            for (field_name, field) in &entity.fields {
                if matches!(field.field_type, FieldType::Blob) && field.string_semantics.is_some() {
                    return Err(SchemaError::StringSemanticsOnNonString {
                        entity: entity_name.to_string(),
                        field: field_name.to_string(),
                    });
                } else if !matches!(field.field_type, FieldType::String | FieldType::Blob) {
                    if field.string_semantics.is_some() {
                        return Err(SchemaError::StringSemanticsOnNonString {
                            entity: entity_name.to_string(),
                            field: field_name.to_string(),
                        });
                    }
                    if field.agent_presentation.is_some() {
                        return Err(SchemaError::AgentPresentationOnNonString {
                            entity: entity_name.to_string(),
                            field: field_name.to_string(),
                        });
                    }
                    if field.attachment_media.is_some() {
                        return Err(SchemaError::AttachmentMediaOnNonBlob {
                            entity: entity_name.to_string(),
                            field: field_name.to_string(),
                        });
                    }
                }
                if let FieldType::EntityRef { target } = &field.field_type {
                    if !self.entities.contains_key(target) {
                        return Err(SchemaError::EntityRefUnknownTarget {
                            target: target.to_string(),
                            context: format!("entity '{}', field '{}'", entity_name, field_name),
                        });
                    }
                }
                if matches!(field.field_type, FieldType::Array) && field.array_items.is_none() {
                    return Err(SchemaError::ArrayFieldMissingItems {
                        entity: entity_name.to_string(),
                        field: field_name.to_string(),
                    });
                }
                if matches!(field.field_type, FieldType::MultiSelect)
                    && field.allowed_values.as_ref().is_none_or(|v| v.is_empty())
                {
                    return Err(SchemaError::MultiSelectFieldMissingAllowedValues {
                        entity: entity_name.to_string(),
                        field: field_name.to_string(),
                    });
                }
                if let Some(ai) = &field.array_items {
                    if let FieldType::EntityRef { target } = &ai.field_type {
                        if !self.entities.contains_key(target) {
                            return Err(SchemaError::EntityRefUnknownTarget {
                                target: target.to_string(),
                                context: format!(
                                    "entity '{}', field '{}', items",
                                    entity_name, field_name
                                ),
                            });
                        }
                    }
                }
            }
        }

        // EntityRef on capability parameters, name-alignment for query capabilities
        for (cap_name, cap) in &self.capabilities {
            let Some(fields) = cap.object_params() else {
                continue;
            };

            let domain_entity = self.entities.get(&cap.domain);

            for param in fields {
                if !matches!(param.field_type, FieldType::String)
                    && param.string_semantics.is_some()
                {
                    return Err(SchemaError::StringSemanticsOnNonStringParam {
                        capability: cap_name.to_string(),
                        param: param.name.clone(),
                    });
                }
                if let FieldType::EntityRef { target } = &param.field_type {
                    if !self.entities.contains_key(target) {
                        return Err(SchemaError::EntityRefUnknownTarget {
                            target: target.to_string(),
                            context: format!(
                                "capability '{}', parameter '{}'",
                                cap_name, param.name
                            ),
                        });
                    }
                }
                if matches!(param.field_type, FieldType::Array) && param.array_items.is_none() {
                    return Err(SchemaError::ArrayParamMissingItems {
                        capability: cap_name.to_string(),
                        param: param.name.clone(),
                    });
                }
                if matches!(param.field_type, FieldType::MultiSelect)
                    && param.allowed_values.as_ref().is_none_or(|v| v.is_empty())
                {
                    return Err(SchemaError::MultiSelectParamMissingAllowedValues {
                        capability: cap_name.to_string(),
                        param: param.name.clone(),
                    });
                }
                if let Some(ai) = &param.array_items {
                    if let FieldType::EntityRef { target } = &ai.field_type {
                        if !self.entities.contains_key(target) {
                            return Err(SchemaError::EntityRefUnknownTarget {
                                target: target.to_string(),
                                context: format!(
                                    "capability '{}', parameter '{}', items",
                                    cap_name, param.name
                                ),
                            });
                        }
                    }
                }

                if cap.kind == CapabilityKind::Query {
                    if let FieldType::EntityRef {
                        target: param_target,
                    } = &param.field_type
                    {
                        if let Some(ent) = domain_entity {
                            if let Some(entity_field) = ent.fields.get(param.name.as_str()) {
                                if let FieldType::EntityRef {
                                    target: field_target,
                                } = &entity_field.field_type
                                {
                                    if param_target != field_target {
                                        return Err(SchemaError::EntityRefNameMismatch {
                                            capability: cap_name.to_string(),
                                            parameter: param.name.clone(),
                                            param_target: param_target.to_string(),
                                            field_target: field_target.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        for (cap_name, cap) in &self.capabilities {
            if cap.kind != CapabilityKind::Action {
                continue;
            }
            if cap.output_schema.is_none() && cap.provides.is_empty() {
                return Err(SchemaError::ActionUntypedResponse {
                    capability: cap_name.to_string(),
                    entity: cap.domain.to_string(),
                });
            }
        }

        for (cap_name, cap) in &self.capabilities {
            if let Some(ref os) = cap.output_schema {
                if let OutputType::SideEffect { description } = &os.output_type {
                    if description.trim().is_empty() {
                        return Err(SchemaError::SideEffectMissingDescription {
                            capability: cap_name.to_string(),
                        });
                    }
                }
            }
        }

        self.validate_expression_aliases()?;
        self.validate_temporal_value_formats()?;
        self.validate_pipeline_segment_disjointness()?;

        // At most one parameterless (no required params at all) query/search per entity.
        // Multiple query caps with required params are fine — the first unscoped one
        // becomes the `query` verb; others get named subcommands.
        // Only flag an error if there are multiple capabilities with zero required params
        // (ambiguous which is the "list all" endpoint).
        //
        // Query and Search resolution are structurally disjoint: `resolve_query_capability`
        // only considers Query caps; Search is resolved at parse time (`Entity~"text"` stamps
        // `capability_name`) or by CLI dispatch (`"search"` verb). No cross-kind fallback.
        for entity_name in self.entities.keys() {
            for kind in [CapabilityKind::Query, CapabilityKind::Search] {
                let parameterless: Vec<_> = self
                    .find_capabilities(entity_name, kind)
                    .into_iter()
                    .filter(|cap| !cap.has_required_scope_param() && !cap.has_any_required_param())
                    .collect();
                if parameterless.len() > 1 {
                    let names: Vec<_> = parameterless.iter().map(|c| c.name.as_str()).collect();
                    return Err(SchemaError::DuplicateCapability {
                        entity: entity_name.to_string(),
                        kind: format!("{:?}", kind),
                        capabilities: names.iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
        }

        crate::cgs_expression_validate::validate_cgs_expression_surface(self)?;

        if matches!(self.auth.as_ref(), Some(AuthScheme::None)) && self.oauth.is_some() {
            return Err(SchemaError::AuthNoneIncompatibleWithOauthExtension);
        }

        if let Some(ref auth) = self.auth {
            auth.validate()?;
        }

        self.validate_oauth_extension()?;

        Ok(())
    }

    fn validate_oauth_extension(&self) -> Result<(), SchemaError> {
        let Some(ref oauth) = self.oauth else {
            return Ok(());
        };
        if oauth.provider.trim().is_empty() {
            return Err(SchemaError::OauthProviderEmpty);
        }

        for (cap_name, req) in &oauth.requirements.capabilities {
            if !self.capabilities.contains_key(cap_name.as_str()) {
                return Err(SchemaError::OauthUnknownCapability {
                    capability: cap_name.clone(),
                });
            }
            Self::validate_scope_requirement(
                req,
                &oauth.scopes,
                &format!("requirements.capabilities.{cap_name}"),
            )?;
        }

        for (rel_key, req) in &oauth.requirements.relations {
            let Some((entity, relation)) = rel_key.split_once('.') else {
                return Err(SchemaError::OauthUnknownRelation {
                    key: rel_key.clone(),
                    entity: String::new(),
                    relation: rel_key.clone(),
                });
            };
            let Some(ent) = self.entities.get(entity) else {
                return Err(SchemaError::OauthUnknownRelation {
                    key: rel_key.clone(),
                    entity: entity.to_string(),
                    relation: relation.to_string(),
                });
            };
            if !ent.relations.contains_key(relation) {
                return Err(SchemaError::OauthUnknownRelation {
                    key: rel_key.clone(),
                    entity: entity.to_string(),
                    relation: relation.to_string(),
                });
            }
            Self::validate_scope_requirement(
                req,
                &oauth.scopes,
                &format!("requirements.relations.{rel_key}"),
            )?;
        }

        for (set_name, bundle) in &oauth.default_scope_sets {
            for s in &bundle.scopes {
                if !oauth.scopes.contains_key(s) {
                    return Err(SchemaError::OauthUnknownScope {
                        context: format!("default_scope_sets.{set_name}"),
                        scope: s.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    fn validate_scope_requirement(
        req: &ScopeRequirement,
        catalog: &IndexMap<String, OauthScopeEntry>,
        ctx: &str,
    ) -> Result<(), SchemaError> {
        let has_any = !req.any_of.is_empty();
        let has_all = !req.all_of.is_empty();
        if !has_any && !has_all {
            return Err(SchemaError::OauthRequirementEmpty {
                context: ctx.to_string(),
            });
        }
        if has_any && has_all {
            return Err(SchemaError::OauthRequirementMixed {
                context: ctx.to_string(),
            });
        }
        if has_any {
            for s in &req.any_of {
                if !catalog.contains_key(s) {
                    return Err(SchemaError::OauthUnknownScope {
                        context: ctx.to_string(),
                        scope: s.clone(),
                    });
                }
            }
        } else {
            for (i, child) in req.all_of.iter().enumerate() {
                Self::validate_scope_requirement(child, catalog, &format!("{ctx}.all_of[{i}]"))?;
            }
        }
        Ok(())
    }

    /// Violations when a `string` entity field or capability parameter omits `string_semantics` (required at load).
    pub fn string_semantics_violations(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (entity_name, entity) in &self.entities {
            for (field_name, field) in &entity.fields {
                if matches!(field.field_type, FieldType::String) && field.string_semantics.is_none()
                {
                    out.push(format!(
                        "entity '{}', field '{}': string field must declare string_semantics (short, markdown, document, html, json_text, or blob); use field_type: blob for opaque binary instead of string_semantics: blob",
                        entity_name, field_name
                    ));
                }
            }
        }
        for (cap_name, cap) in &self.capabilities {
            let Some(fields) = cap.object_params() else {
                continue;
            };
            for param in fields {
                if matches!(param.field_type, FieldType::String) && param.string_semantics.is_none()
                {
                    out.push(format!(
                        "capability '{}', parameter '{}': string parameter must declare string_semantics (short, markdown, document, html, json_text, or blob)",
                        cap_name, param.name
                    ));
                }
            }
        }
        out
    }

    /// Resolve a path token to the canonical entity name (`Team` or an `expression_aliases` hit).
    pub fn canonical_entity_name(&self, token: &str) -> Option<String> {
        if self.entities.contains_key(token) {
            return Some(token.to_string());
        }
        for (name, ent) in &self.entities {
            if ent.expression_aliases.iter().any(|a| a == token) {
                return Some(name.to_string());
            }
        }
        None
    }

    fn validate_expression_aliases(&self) -> Result<(), SchemaError> {
        let mut owner: IndexMap<String, String> = IndexMap::new();
        for (name, ent) in &self.entities {
            for a in &ent.expression_aliases {
                if a.is_empty() {
                    continue;
                }
                if self.entities.contains_key(a.as_str()) && a.as_str() != name.as_str() {
                    return Err(SchemaError::ExpressionAliasShadowsEntity {
                        entity: name.to_string(),
                        alias: a.clone(),
                    });
                }
                match owner.get(a) {
                    None => {
                        owner.insert(a.clone(), name.to_string());
                    }
                    Some(prev) if prev != name.as_str() => {
                        return Err(SchemaError::DuplicateExpressionAlias {
                            alias: a.clone(),
                            owner: prev.clone(),
                            other: name.to_string(),
                        });
                    }
                    Some(_) => {}
                }
            }
        }
        Ok(())
    }

    fn validate_temporal_value_formats(&self) -> Result<(), SchemaError> {
        for (entity_name, ent) in &self.entities {
            for (field_name, field) in &ent.fields {
                match &field.field_type {
                    FieldType::Date => match &field.value_format {
                        Some(ValueWireFormat::Temporal(_)) => {}
                        None => {
                            return Err(SchemaError::DateFieldMissingValueFormat {
                                entity: entity_name.to_string(),
                                field: field_name.to_string(),
                            });
                        }
                    },
                    FieldType::Array => {
                        if field.value_format.is_some() {
                            return Err(SchemaError::ValueFormatOnNonDateField {
                                entity: entity_name.to_string(),
                                field: field_name.to_string(),
                            });
                        }
                        if let Some(ai) = &field.array_items {
                            match &ai.field_type {
                                FieldType::Date => match &ai.value_format {
                                    Some(ValueWireFormat::Temporal(_)) => {}
                                    None => {
                                        return Err(SchemaError::DateFieldMissingValueFormat {
                                            entity: entity_name.to_string(),
                                            field: format!("{field_name}.items"),
                                        });
                                    }
                                },
                                _ => {
                                    if ai.value_format.is_some() {
                                        return Err(SchemaError::ValueFormatOnNonDateField {
                                            entity: entity_name.to_string(),
                                            field: format!("{field_name}.items"),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        if field.value_format.is_some() {
                            return Err(SchemaError::ValueFormatOnNonDateField {
                                entity: entity_name.to_string(),
                                field: field_name.to_string(),
                            });
                        }
                    }
                }
            }
        }

        for (cap_name, cap) in &self.capabilities {
            let Some(input) = cap.input_schema.as_ref() else {
                continue;
            };
            if let InputType::Object { fields, .. } = &input.input_type {
                for param in fields {
                    match &param.field_type {
                        FieldType::Date => match &param.value_format {
                            Some(ValueWireFormat::Temporal(_)) => {}
                            None => {
                                return Err(SchemaError::DateParamMissingValueFormat {
                                    capability: cap_name.to_string(),
                                    param: param.name.clone(),
                                });
                            }
                        },
                        FieldType::Array => {
                            if param.value_format.is_some() {
                                return Err(SchemaError::ValueFormatOnNonDateParam {
                                    capability: cap_name.to_string(),
                                    param: param.name.clone(),
                                });
                            }
                            if let Some(ai) = &param.array_items {
                                match &ai.field_type {
                                    FieldType::Date => match &ai.value_format {
                                        Some(ValueWireFormat::Temporal(_)) => {}
                                        None => {
                                            return Err(SchemaError::DateParamMissingValueFormat {
                                                capability: cap_name.to_string(),
                                                param: format!("{}.items", param.name),
                                            });
                                        }
                                    },
                                    _ => {
                                        if ai.value_format.is_some() {
                                            return Err(SchemaError::ValueFormatOnNonDateParam {
                                                capability: cap_name.to_string(),
                                                param: format!("{}.items", param.name),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            if param.value_format.is_some() {
                                return Err(SchemaError::ValueFormatOnNonDateParam {
                                    capability: cap_name.to_string(),
                                    param: param.name.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Zero-arity pipeline method labels (kebab) must not collide with relation names or any field name
    /// on the same entity, so `.segment` without `()` parses unambiguously.
    fn validate_pipeline_segment_disjointness(&self) -> Result<(), SchemaError> {
        for (entity_name, ent) in &self.entities {
            let mut by_label: IndexMap<PathMethodSegment, String> = IndexMap::new();

            for kind in [
                CapabilityKind::Action,
                CapabilityKind::Update,
                CapabilityKind::Delete,
            ] {
                for cap in self.find_capabilities(entity_name, kind) {
                    if !capability_is_zero_arity_invoke(cap) {
                        continue;
                    }
                    let label = capability_path_method_segment(cap);
                    let cap_name = cap.name.to_string();
                    if let Some(prev) = by_label.insert(label.clone(), cap_name.clone()) {
                        return Err(SchemaError::PipelineSegmentConflict {
                            entity: entity_name.to_string(),
                            segment: label.to_string(),
                            message: format!(
                                "duplicate zero-arity pipeline method label on capabilities '{prev}' and '{cap_name}'"
                            ),
                        });
                    }
                }
            }

            for (label, cap_name) in &by_label {
                if ent.relations.contains_key(label.as_str()) {
                    return Err(SchemaError::PipelineSegmentConflict {
                        entity: entity_name.to_string(),
                        segment: label.to_string(),
                        message: format!(
                            "zero-arity pipeline method '{cap_name}' label '{label}' collides with relation '{label}'"
                        ),
                    });
                }
                if let Some(f) = ent.fields.get(label.as_str()) {
                    return Err(SchemaError::PipelineSegmentConflict {
                        entity: entity_name.to_string(),
                        segment: label.to_string(),
                        message: format!(
                            "zero-arity pipeline method '{cap_name}' label '{label}' collides with field '{}'",
                            f.name
                        ),
                    });
                }
            }

            for rel_name in ent.relations.keys() {
                if let Some(f) = ent.fields.get(rel_name.as_str()) {
                    if matches!(f.field_type, FieldType::EntityRef { .. }) {
                        return Err(SchemaError::PipelineSegmentConflict {
                            entity: entity_name.to_string(),
                            segment: rel_name.to_string(),
                            message: format!(
                                "relation '{rel_name}' has the same name as EntityRef field '{}'",
                                f.name
                            ),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Query capabilities whose parameters include `EntityRef(source_entity)`.
    pub fn find_reverse_traversal_caps<'a>(
        &'a self,
        source_entity: &str,
    ) -> Vec<(&'a CapabilitySchema, &'a str)> {
        let mut out = Vec::new();
        for cap in self.capabilities.values() {
            if cap.kind != CapabilityKind::Query {
                continue;
            }
            let Some(fields) = cap.object_params() else {
                continue;
            };
            for p in fields {
                if let FieldType::EntityRef { target } = &p.field_type {
                    if target.as_str() == source_entity {
                        out.push((cap, p.name.as_str()));
                    }
                }
            }
        }
        out
    }

    /// All `EntityRef` fields on `entity`, with their target entity names.
    pub fn entity_ref_fields<'a>(&'a self, entity: &str) -> Vec<(&'a FieldSchema, &'a str)> {
        let Some(ent) = self.entities.get(entity) else {
            return Vec::new();
        };
        ent.fields
            .values()
            .filter_map(|f| {
                if let FieldType::EntityRef { target } = &f.field_type {
                    Some((f, target.as_str()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get an entity definition by name.
    pub fn get_entity(&self, name: &str) -> Option<&EntityDef> {
        self.entities.get(name)
    }

    /// Get a capability by name.
    pub fn get_capability(&self, name: &str) -> Option<&CapabilitySchema> {
        self.capabilities.get(name)
    }

    /// Find the first capability for a given entity and kind (legacy single-match).
    pub fn find_capability(&self, entity: &str, kind: CapabilityKind) -> Option<&CapabilitySchema> {
        self.capability_index_arc()
            .names_for_domain_kind(entity, kind)
            .first()
            .and_then(|n| self.capabilities.get(n.as_str()))
    }

    /// Find **all** capabilities for a given entity and kind.
    pub fn find_capabilities(&self, entity: &str, kind: CapabilityKind) -> Vec<&CapabilitySchema> {
        self.capability_index_arc()
            .names_for_domain_kind(entity, kind)
            .iter()
            .filter_map(|n| self.capabilities.get(n.as_str()))
            .collect()
    }

    /// # Response-field ordering (three roles)
    ///
    /// Several helpers derive ordered field names; they are **not** interchangeable:
    ///
    /// 1. **DOMAIN / prompt teaching** — [`Self::effective_ordered_response_fields`],
    ///    [`Self::domain_projection_heading_fields`] / [`Self::projection_prompt_field_prefixes`]: use explicit capability `provides` when present;
    ///    otherwise [`Self::default_ordered_entity_field_names`] on the capability’s domain entity
    ///    (`id_field` first, then remaining fields lexicographically).
    /// 2. **Runtime decode, cache, and [`Self::field_providers`]** — [`Self::effective_provides`]:
    ///    same `provides` vs default rule as (1) so empty-`provides` defaults stay aligned with DOMAIN.
    /// 3. **Short error / CLI hints** — internal `error_render` projection scalars (scalar-only,
    ///    sorted, `prioritize_projection_scalars`): intentionally **not** the full DOMAIN projection field list.
    ///
    /// Primary **Get** for an entity — same selection as DOMAIN / CLI use for the main fetch pattern.
    ///
    /// Picks the first Get (by capability name) that is not a trivial zero-arity pathless invoke,
    /// or falls back to the first Get when all are trivial.
    pub fn primary_get_capability(&self, entity: &str) -> Option<&CapabilitySchema> {
        let mut get_caps = self.find_capabilities(entity, CapabilityKind::Get);
        if get_caps.is_empty() {
            return None;
        }
        get_caps.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        if let Some(c) = get_caps.iter().find(|c| {
            !capability_is_zero_arity_invoke(c) || c.domain_exemplar_requires_entity_anchor()
        }) {
            return Some(*c);
        }
        Some(get_caps[0])
    }

    /// Ordered field names for DOMAIN heading projection teaching: explicit `provides`, or default entity order when empty.
    pub fn effective_ordered_response_fields(&self, cap: &CapabilitySchema) -> Vec<String> {
        if !cap.provides.is_empty() {
            return cap.provides.clone();
        }
        match cap.kind {
            CapabilityKind::Get | CapabilityKind::Query | CapabilityKind::Search => self
                .get_entity(cap.domain.as_str())
                .map(Self::default_ordered_entity_field_names)
                .unwrap_or_default(),
            _ => vec![],
        }
    }

    /// [`id_field`] first (when declared), then remaining fields lexicographically — for stable projection teaching.
    pub fn default_ordered_entity_field_names(ent: &EntityDef) -> Vec<String> {
        let mut names: Vec<String> = ent.fields.keys().map(|k| k.as_str().to_string()).collect();
        names.sort_unstable();
        if ent.fields.contains_key(&ent.id_field) {
            let id_s = ent.id_field.as_str();
            names.retain(|n| n != id_s);
            let mut out = vec![id_s.to_string()];
            out.extend(names);
            out
        } else {
            names
        }
    }

    /// Resolve which **Get** supplies ordered `provides` / default field order for DOMAIN heading projection.
    ///
    /// When `primary_read` is set it must name a [`CapabilityKind::Get`] whose [`CapabilitySchema::domain`]
    /// is this entity; otherwise falls back to [`Self::primary_get_capability`] (same anchor as the DOMAIN
    /// get exemplar when multiple Gets exist).
    pub fn resolved_primary_get_for_projection<'a>(
        &'a self,
        entity_name: &str,
        ent: &'a EntityDef,
    ) -> Option<&'a CapabilitySchema> {
        if let Some(pid) = ent.primary_read.as_deref() {
            if let Some(c) = self.capabilities.get(pid) {
                if c.kind == CapabilityKind::Get && c.domain.as_str() == entity_name {
                    return Some(c);
                }
            }
        }
        self.primary_get_capability(entity_name)
    }

    /// Ordered **wire** field names for DOMAIN heading projection teaching: primary Get’s
    /// [`Self::effective_ordered_response_fields`] when [`EntityDef::domain_projection_examples`] is on.
    ///
    /// Independent of how DOMAIN renders the fetch exemplar (e.g. `Entity($)` vs zero-arity `Entity.m#()`);
    /// use this (or [`Self::projection_prompt_field_prefixes`]) as the single source for the allowed
    /// scalar set **`F`**.
    pub fn domain_projection_heading_fields(
        &self,
        entity_name: &str,
        ent: &EntityDef,
    ) -> Option<Vec<String>> {
        if !ent.domain_projection_examples {
            return None;
        }
        let cap = self.resolved_primary_get_for_projection(entity_name, ent)?;
        let f = self.effective_ordered_response_fields(cap);
        if f.is_empty() {
            None
        } else {
            Some(f)
        }
    }

    /// One vector per DOMAIN projection teaching: the **full** ordered field list **`F`** for the
    /// entity heading’s `[f1,…,fN]` (canonical order). The renderer places that bracket on the **entity
    /// heading** line after `;;` (not as a separate indented expression line). The Valid expressions
    /// preamble teaches that any non-empty subset of **`F`** is valid; we do not emit every prefix.
    pub fn projection_prompt_field_prefixes(
        &self,
        entity_name: &str,
        ent: &EntityDef,
    ) -> Vec<Vec<String>> {
        match self.domain_projection_heading_fields(entity_name, ent) {
            Some(f) => vec![f],
            None => vec![],
        }
    }

    /// Build a [`CapabilityManifest`] for an entity — the single source of truth for
    /// which capabilities are available, classified by role. Both CLI generation and
    /// prompt rendering should consume this instead of independent `find_capabilities` loops.
    pub fn capability_manifest(&self, entity: &str) -> CapabilityManifest<'_> {
        let primary_query = self.primary_query_capability(entity);
        let primary_search = self.primary_search_capability(entity);
        let named_queries = self.named_query_capabilities(entity);

        let get_caps = self.find_capabilities(entity, CapabilityKind::Get);
        let get = self.primary_get_capability(entity);
        let singleton_gets: Vec<_> = get_caps
            .iter()
            .filter(|c| {
                !c.domain_exemplar_requires_entity_anchor() && capability_is_zero_arity_invoke(c)
            })
            .copied()
            .collect();

        let mut zero_arity_methods = Vec::new();
        let mut multi_arity_methods = Vec::new();
        for kind in [
            CapabilityKind::Action,
            CapabilityKind::Update,
            CapabilityKind::Delete,
        ] {
            for cap in self.find_capabilities(entity, kind) {
                if capability_is_zero_arity_invoke(cap) {
                    zero_arity_methods.push(cap);
                } else {
                    multi_arity_methods.push(cap);
                }
            }
        }

        let mut standalone_creates = Vec::new();
        for cap in self.find_capabilities(entity, CapabilityKind::Create) {
            standalone_creates.push(cap);
        }

        CapabilityManifest {
            primary_query,
            primary_search,
            named_queries,
            get,
            singleton_gets,
            zero_arity_methods,
            multi_arity_methods,
            standalone_creates,
        }
    }

    /// Find the **primary** query capability for an entity.
    ///
    /// Priority order:
    /// 1. The unscoped query with no required params (the "list all" endpoint)
    /// 2. The first unscoped query (has required filter params but no scope)
    /// 3. None (entity only has scoped sub-resource queries)
    ///
    /// The primary gets the `entity query` CLI verb. All others get named subcommands.
    pub fn primary_query_capability(&self, entity: &str) -> Option<&CapabilitySchema> {
        let caps = self.find_capabilities(entity, CapabilityKind::Query);
        let unscoped: Vec<_> = caps
            .iter()
            .filter(|c| !c.has_required_scope_param())
            .collect();
        // Prefer the parameterless one
        if let Some(c) = unscoped.iter().find(|c| !c.has_any_required_param()) {
            return Some(*c);
        }
        // Fallback: first unscoped with required params
        unscoped.first().map(|c| **c)
    }

    /// Find the **primary** search capability for an entity (same rules as query).
    pub fn primary_search_capability(&self, entity: &str) -> Option<&CapabilitySchema> {
        let caps = self.find_capabilities(entity, CapabilityKind::Search);
        let unscoped: Vec<_> = caps
            .iter()
            .filter(|c| !c.has_required_scope_param())
            .collect();
        if let Some(c) = unscoped.iter().find(|c| !c.has_any_required_param()) {
            return Some(*c);
        }
        unscoped.first().map(|c| **c)
    }

    /// All non-primary query/search capabilities for an entity.
    /// These get named subcommands in the CLI instead of the generic `query`/`search` verb.
    ///
    /// Includes:
    /// - Scoped capabilities (have required `role: scope` params)
    /// - Non-primary unscoped capabilities (have required params but aren't the first)
    pub fn named_query_capabilities(&self, entity: &str) -> Vec<&CapabilitySchema> {
        let primary_q = self.primary_query_capability(entity).map(|c| &c.name);
        let primary_s = self.primary_search_capability(entity).map(|c| &c.name);
        self.capabilities
            .values()
            .filter(|cap| {
                cap.domain == entity
                    && matches!(cap.kind, CapabilityKind::Query | CapabilityKind::Search)
                    && Some(&cap.name) != primary_q
                    && Some(&cap.name) != primary_s
            })
            .collect()
    }

    /// Validates [`RelationMaterialization::QueryScoped`] / [`QueryScopedBindings`]: `capability` must
    /// resolve to a `query` or `search` capability whose [`CapabilitySchema::domain`] equals `target_entity`
    /// and whose object input declares every name in `required_param_names`.
    pub fn validate_chain_materialize_capability(
        &self,
        parent_entity: &str,
        relation: &str,
        target_entity: &str,
        capability: &CapabilityName,
        required_param_names: &[&str],
    ) -> Result<(), SchemaError> {
        let err = |detail: String| {
            Err(SchemaError::RelationMaterializeCapabilityInvalid {
                entity: parent_entity.to_string(),
                relation: relation.to_string(),
                target: target_entity.to_string(),
                capability: capability.to_string(),
                detail,
            })
        };
        let Some(cap) = self.get_capability(capability.as_str()) else {
            return err("no such capability name".into());
        };
        if cap.domain.as_str() != target_entity {
            return err(format!(
                "capability is declared on entity '{}' but relation targets '{}'",
                cap.domain, target_entity
            ));
        }
        if !matches!(cap.kind, CapabilityKind::Query | CapabilityKind::Search) {
            return err(format!(
                "capability kind must be query or search (got {:?})",
                cap.kind
            ));
        }
        let Some(fields) = cap.object_params() else {
            return err(
                "capability has no object-typed input parameters; query_scoped materialization requires them"
                    .into(),
            );
        };
        for name in required_param_names {
            if !fields.iter().any(|f| f.name == *name) {
                return err(format!(
                    "object input does not declare materialize parameter `{name}`"
                ));
            }
        }
        Ok(())
    }

    /// Find the first query or search capability for `entity` that declares a parameter
    /// named `param_name`. Used for heuristics outside chain materialization (e.g. reverse
    /// traversal discovery). Chain edges must use explicit [`RelationMaterialization::QueryScoped::capability`].
    pub fn find_capability_by_param(
        &self,
        entity: &str,
        param_name: &CapabilityParamName,
    ) -> Option<&CapabilitySchema> {
        for kind in [CapabilityKind::Query, CapabilityKind::Search] {
            for cap in self.find_capabilities(entity, kind) {
                if cap
                    .object_params()
                    .is_some_and(|fields| fields.iter().any(|f| f.name == param_name.as_str()))
                {
                    return Some(cap);
                }
            }
        }
        None
    }

    /// Find a query or search capability on `entity` whose input object declares **every**
    /// name in `param_names` (superset allowed).
    pub fn find_capability_owning_all_params(
        &self,
        entity: &str,
        param_names: &[CapabilityParamName],
    ) -> Option<&CapabilitySchema> {
        if param_names.is_empty() {
            return None;
        }
        use std::collections::HashSet;
        let required: HashSet<&str> = param_names.iter().map(|p| p.as_str()).collect();
        for kind in [CapabilityKind::Query, CapabilityKind::Search] {
            for cap in self.find_capabilities(entity, kind) {
                let Some(fields) = cap.object_params() else {
                    continue;
                };
                let mut ok = true;
                for p in &required {
                    if !fields.iter().any(|f| f.name == *p) {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    return Some(cap);
                }
            }
        }
        None
    }

    /// Resolve the effective field set a capability provides, applying defaults when
    /// `provides` is empty (backward-compatible).
    ///
    /// - Explicit `provides` list → use it directly
    /// - Empty + `Get` / `Query` / `Search` → all entity fields in [`Self::default_ordered_entity_field_names`]
    ///   order (same default as [`Self::effective_ordered_response_fields`])
    /// - Empty + `Create` / `Update` / `Delete` / `Action` → empty (may only return `id`)
    pub fn effective_provides(&self, cap: &CapabilitySchema) -> Vec<String> {
        if !cap.provides.is_empty() {
            return cap.provides.clone();
        }
        match cap.kind {
            CapabilityKind::Get | CapabilityKind::Query | CapabilityKind::Search => self
                .get_entity(cap.domain.as_str())
                .map(Self::default_ordered_entity_field_names)
                .unwrap_or_default(),
            CapabilityKind::Create
            | CapabilityKind::Update
            | CapabilityKind::Delete
            | CapabilityKind::Action => {
                // Write capabilities may return a partial entity; declare `provides`
                // explicitly if the response contains fields you want to cache.
                vec![]
            }
        }
    }

    /// Build a reverse index mapping each entity field to the capabilities that provide it.
    ///
    /// Used by the runtime's auto-resolution path: when a projection requests a field that
    /// is absent from the cache, the engine looks up which capability to invoke.
    ///
    /// Result: `field_name → Vec<capability_name>` in priority order:
    /// `Get` first (most specific), then `Action`, then `Query`/`Search` (least specific).
    pub fn field_providers(&self, entity: &str) -> IndexMap<String, Vec<String>> {
        let mut index: IndexMap<String, Vec<String>> = IndexMap::new();

        // Priority ordering: Get > Action > Query/Search (so the most specific provider
        // is tried first when multiple capabilities cover the same field).
        let priority_order = [
            CapabilityKind::Get,
            CapabilityKind::Action,
            CapabilityKind::Create,
            CapabilityKind::Update,
            CapabilityKind::Query,
            CapabilityKind::Search,
        ];

        for kind in priority_order {
            for cap in self.find_capabilities(entity, kind) {
                let provided = self.effective_provides(cap);
                if provided.is_empty() {
                    continue;
                }
                for field in provided {
                    index.entry(field).or_default().push(cap.name.to_string());
                }
            }
        }

        index
    }
}

impl CapabilitySchema {
    /// Object-typed input parameters for this capability, if any.
    ///
    /// Returns `None` when there is no input schema or the input is not `InputType::Object`.
    pub fn object_params(&self) -> Option<&[InputFieldSchema]> {
        self.input_schema
            .as_ref()
            .and_then(|input| match &input.input_type {
                InputType::Object { fields, .. } => Some(fields.as_slice()),
                _ => None,
            })
    }

    /// Whether this capability has at least one required parameter with `role: scope`.
    ///
    /// Scoped capabilities (e.g. `GET /classes/{class_index}/spells`) use the scope
    /// param in the URL path. In the CLI they get named subcommands (not the generic
    /// `query` verb) because they require a parent-entity pivot.
    pub fn has_required_scope_param(&self) -> bool {
        self.object_params().is_some_and(|fields| {
            fields
                .iter()
                .any(|f| f.required && matches!(f.role, Some(ParameterRole::Scope)))
        })
    }

    /// Whether this capability has at least one required parameter (any role).
    pub fn has_any_required_param(&self) -> bool {
        self.object_params()
            .is_some_and(|fields| fields.iter().any(|f| f.required))
    }

    /// See [`template_domain_exemplar_requires_entity_anchor`].
    #[inline]
    pub fn domain_exemplar_requires_entity_anchor(&self) -> bool {
        template_domain_exemplar_requires_entity_anchor(&self.mapping.template.0)
    }

    /// See [`template_invoke_requires_explicit_anchor_id`].
    #[inline]
    pub fn invoke_requires_explicit_anchor_id(&self) -> bool {
        template_invoke_requires_explicit_anchor_id(&self.mapping.template.0)
    }
}

impl Default for CGS {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod capability_index_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn find_capabilities_index_matches_linear_scan() {
        let p = Path::new("../../fixtures/schemas/petstore");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("petstore");
        let kinds = [
            CapabilityKind::Query,
            CapabilityKind::Search,
            CapabilityKind::Get,
            CapabilityKind::Create,
            CapabilityKind::Update,
            CapabilityKind::Delete,
            CapabilityKind::Action,
        ];
        for ent in cgs.entities.keys() {
            let e = ent.as_str();
            for k in kinds {
                let indexed: Vec<_> = cgs
                    .find_capabilities(e, k)
                    .into_iter()
                    .map(|c| c.name.to_string())
                    .collect();
                let linear: Vec<_> = cgs
                    .capabilities
                    .values()
                    .filter(|cap| cap.domain.as_str() == e && cap.kind == k)
                    .map(|c| c.name.to_string())
                    .collect();
                assert_eq!(indexed, linear, "entity={e} kind={k:?}");
            }
        }
    }

    #[test]
    fn projection_prompt_field_prefixes_is_single_full_list() {
        let p = Path::new("../../fixtures/schemas/petstore");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("petstore");
        let Some(ent) = cgs.get_entity("Pet") else {
            panic!("missing Pet entity");
        };
        let prefixes = cgs.projection_prompt_field_prefixes("Pet", ent);
        assert_eq!(prefixes.len(), 1, "expected one full projection exemplar");
        let cap = cgs
            .resolved_primary_get_for_projection("Pet", ent)
            .expect("Pet should have primary get for projection");
        let f = cgs.effective_ordered_response_fields(cap);
        assert_eq!(prefixes[0], f);
        assert_eq!(
            cgs.domain_projection_heading_fields("Pet", ent).as_deref(),
            Some(f.as_slice())
        );
    }

    #[test]
    fn domain_projection_heading_fields_linear_issue_despite_singleton_get_exemplar() {
        let p = Path::new("../../apis/linear");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("linear");
        let Some(ent) = cgs.get_entity("Issue") else {
            panic!("missing Issue entity");
        };
        let wire = cgs
            .domain_projection_heading_fields("Issue", ent)
            .expect("Linear Issue should expose heading projection fields");
        assert_eq!(
            wire,
            vec![
                "id".to_string(),
                "identifier".to_string(),
                "title".to_string(),
                "description".to_string(),
                "parent".to_string(),
                "team".to_string(),
                "project".to_string(),
                "assignee".to_string(),
                "state".to_string(),
                "cycle".to_string(),
            ]
        );
        assert_eq!(
            cgs.projection_prompt_field_prefixes("Issue", ent),
            vec![wire.clone()]
        );
    }

    #[test]
    fn chain_materialize_capability_rejects_wrong_domain() {
        let p = Path::new("../../apis/clickup");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("clickup");
        let cap: CapabilityName = "task_query".into();
        let err = cgs
            .validate_chain_materialize_capability("Team", "spaces", "Space", &cap, &["team_id"])
            .expect_err("task_query is on Task, not Space");
        assert!(
            matches!(
                err,
                SchemaError::RelationMaterializeCapabilityInvalid { .. }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn chain_materialize_capability_accepts_named_query() {
        let p = Path::new("../../apis/clickup");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("clickup");
        let cap: CapabilityName = "space_query".into();
        cgs.validate_chain_materialize_capability("Team", "spaces", "Space", &cap, &["team_id"])
            .expect("space_query lists Space rows scoped by team_id");
    }

    #[test]
    fn template_binding_helpers_linear_issue_get() {
        let p = Path::new("../../apis/linear");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("linear");
        let cap = cgs.capabilities.get("issue_get").expect("issue_get");
        assert!(
            cap.domain_exemplar_requires_entity_anchor(),
            "GraphQL variables.id must force DOMAIN anchor exemplar"
        );
        assert!(
            cap.invoke_requires_explicit_anchor_id(),
            "invoke parse must not default id to 0"
        );
    }
}

#[cfg(test)]
mod oauth_extension_tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn gmail_linear_jira_load_with_oauth_block() {
        for dir in [
            "../../apis/gmail",
            "../../apis/linear",
            "../../apis/jira",
            "../../apis/github",
            "../../apis/twitter",
        ] {
            let p = Path::new(dir);
            if !p.exists() {
                continue;
            }
            let cgs = crate::loader::load_schema_dir(p).unwrap_or_else(|e| panic!("{dir}: {e}"));
            let oauth = cgs.oauth.as_ref().expect("oauth section");
            assert!(!oauth.provider.trim().is_empty());
            assert!(!oauth.scopes.is_empty());
        }
    }

    #[test]
    fn oauth_capability_satisfied_any_of() {
        let p = Path::new("../../apis/linear");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("linear");
        let mut granted = HashSet::new();
        granted.insert("read".to_string());
        assert_eq!(
            cgs.oauth_capability_satisfied("issue_get", &granted),
            Some(true)
        );
        assert_eq!(
            cgs.oauth_capability_satisfied("issue_create", &granted),
            Some(false)
        );
        granted.insert("issues:create".to_string());
        assert_eq!(
            cgs.oauth_capability_satisfied("issue_create", &granted),
            Some(true)
        );
        assert_eq!(
            cgs.oauth_capability_satisfied("unknown_cap", &granted),
            None
        );
    }

    #[test]
    fn oauth_json_round_trip() {
        let p = Path::new("../../apis/gmail");
        if !p.exists() {
            return;
        }
        let cgs = crate::loader::load_schema_dir(p).expect("gmail");
        let json = serde_json::to_string(&cgs).expect("serialize");
        let back: CGS = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cgs.oauth, back.oauth);
    }

    #[test]
    fn scope_requirement_all_of_nested() {
        let req = ScopeRequirement {
            any_of: vec![],
            all_of: vec![
                ScopeRequirement {
                    any_of: vec!["a".into(), "b".into()],
                    all_of: vec![],
                },
                ScopeRequirement {
                    any_of: vec!["c".into()],
                    all_of: vec![],
                },
            ],
        };
        let mut g = HashSet::new();
        g.insert("a".into());
        assert!(!req.satisfied_by(&g));
        g.insert("c".into());
        assert!(req.satisfied_by(&g));
    }
}
