//! Symbol tuning for LLM prompts: opaque `e#` / `m#` / `p#` tokens — each `p#` is glossed **once**, on the
//! line before its first use in **DOMAIN**; **DOMAIN** gives entity/method examples (including `e#` per block),
//! `;;` descriptions (with a short **type** prefix like `date · …` / `bool · …` from CGS), comma-separated
//! `optional params: …` / `[scope …]` before the prose description (` — `), when present (required args appear in the expression).
//!
//! [`SymbolMap`] is built from the same entity slice as [`crate::prompt_render`] uses. Call
//! [`expand_path_symbols`] on model output **before** [`crate::expr_parser::parse`].
//!
//! **Caching (execute / MCP):** for a fixed loaded [`CGS`] (`catalog_cgs_hash_hex`), almost all DOMAIN
//! symbol structure is stable. [`DomainExposureSession`] memoizes [`SymbolMap`] behind
//! [`DomainExposureSession::symbol_map_arc`] and clears that cache whenever [`DomainExposureSession::expose_entities`]
//! runs so wave indices stay consistent. Per-request variance is mostly the append-only entity list and
//! the derived `e#` / `m#` / `p#` table.
//!
//! **Cross-session reuse (one process):** [`SymbolMapCrossRequestCache`] (bounded LRU; capacity from
//! `PLASM_SYMBOL_MAP_LRU_CAP`, default `64`, set `0` to disable) deduplicates identical [`SymbolMap`]
//! snapshots when the catalog fingerprint and exposure rows match a recent session.

use crate::domain_term::{
    method_ref_for_domain_segment, resolve_parameter_slot, DomainTerm, EntityRef, ParameterSlot,
    Symbol,
};
use crate::identity::CapabilityName;
use crate::identity::EntityName;
use crate::schema::{
    capability_method_label_kebab, ArrayItemsSchema, CapabilitySchema, InputFieldSchema, InputType,
    ParameterRole, StringSemantics, CGS,
};
use crate::CapabilityKind;
use crate::FieldType;
use indexmap::IndexMap;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

/// Which entities drive DOMAIN / symbol-map slicing (REPL `--focus`, eval, HTTP execute sessions).
#[derive(Clone, Copy, Debug, Default)]
pub enum FocusSpec<'a> {
    /// Full schema (no entity subset).
    #[default]
    All,
    /// One seed entity plus its 2-hop neighbourhood (existing behaviour).
    Single(&'a str),
    /// Union of neighbourhoods for several seeds (same CGS).
    Seeds(&'a [&'a str]),
    /// **Exact** entity list only (no 2-hop union). Used with [`DomainExposureSession`] so DOMAIN and
    /// execution expand use the same monotonic `e#` / `m#` / `p#` as more of the graph is exposed.
    SeedsExact(&'a [&'a str]),
}

impl<'a> FocusSpec<'a> {
    #[inline]
    pub fn from_optional(focus: Option<&'a str>) -> Self {
        match focus {
            None => FocusSpec::All,
            Some(s) => FocusSpec::Single(s),
        }
    }
}

/// How an identifier is bound in the CGS: entity field, declared relation, or capability parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentRole {
    EntityField,
    Relation { target: EntityName },
    CapabilityParam { capability: CapabilityName },
}

/// Typed metadata for one identifier on one entity. Replaces the stringly-typed
/// `build_ident_gloss_map` + `build_ident_type_map` pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentMetadata {
    /// Registry catalog row (`entry_id`) owning this slot — distinguishes federated graphs that reuse entity names.
    pub catalog_entry_id: String,
    pub field_type: FieldType,
    /// When [`FieldType::String`], optional [`StringSemantics`] for DOMAIN `p#` type gloss.
    pub string_semantics: Option<StringSemantics>,
    /// Element typing when [`FieldType::Array`] (from CGS `items:`); drives `array[…]` gloss.
    pub array_items: Option<ArrayItemsSchema>,
    /// When [`FieldType::Select`] / [`FieldType::MultiSelect`], CGS `allowed_values` for DOMAIN gloss.
    pub allowed_values: Option<Vec<String>>,
    pub role: IdentRole,
    /// CGS field / parameter / relation name (DOMAIN gloss fallback when `description` is empty).
    pub wire_name: String,
    pub description: String,
    pub entity: EntityName,
}

/// Key for [`IdentMetadata`] maps: `(registry entry_id, CGS entity, wire name)`.
pub type IdentMetaKey = (String, EntityName, String);
use std::fmt::Write;

/// Same 2-hop focus neighbourhood as prompt rendering: `Some(set)` when focus is set.
#[inline]
fn field_is_filter_like_gloss(f: &InputFieldSchema) -> bool {
    !matches!(
        f.role,
        Some(ParameterRole::Search)
            | Some(ParameterRole::Sort)
            | Some(ParameterRole::SortDirection)
            | Some(ParameterRole::ResponseControl)
    )
}

/// Union of [`build_focus_set`] for each seed (same rules as single focus).
pub fn build_focus_set_union<'a>(cgs: &'a CGS, seeds: &[&'a str]) -> HashSet<&'a str> {
    let mut u = HashSet::new();
    for s in seeds {
        if let Some(set) = build_focus_set(cgs, Some(*s)) {
            u.extend(set);
        }
    }
    u
}

pub fn build_focus_set<'a>(cgs: &'a CGS, focus: Option<&'a str>) -> Option<HashSet<&'a str>> {
    let f = focus?;
    let mut s = HashSet::new();
    s.insert(f);
    if let Some(ent) = cgs.get_entity(f) {
        for field in ent.fields.values() {
            if let FieldType::EntityRef { target } = &field.field_type {
                s.insert(target.as_str());
            }
        }
        for rel in ent.relations.values() {
            s.insert(rel.target_resource.as_str());
        }
    }
    for (ename, ent) in &cgs.entities {
        for field in ent.fields.values() {
            if let FieldType::EntityRef { target } = &field.field_type {
                if target.as_str() == f {
                    s.insert(ename.as_str());
                }
            }
        }
    }
    Some(s)
}

/// `(full_entities_in_prompt, dim_entity_names)` — mirrors [`crate::prompt_render`].
pub fn entity_slices_for_render<'a>(
    cgs: &'a CGS,
    focus: FocusSpec<'a>,
) -> (Vec<&'a str>, Vec<&'a str>) {
    if let FocusSpec::SeedsExact(seeds) = focus {
        let mut full = Vec::new();
        for s in seeds.iter().copied() {
            if let Some(ent) = cgs.get_entity(s) {
                if !ent.abstract_entity {
                    full.push(s);
                }
            }
        }
        // `SeedsExact` matches [`DomainExposureSession::entities`] only (no 2-hop neighbourhood).
        // Exposure-bundle rendering ignores `_dim_entities` for this focus mode, so skip the full-schema
        // scan that built `dim` for legacy All/Single/Seeds slices.
        return (full, Vec::new());
    }

    let focus_set: Option<HashSet<&'a str>> = match focus {
        FocusSpec::All => None,
        FocusSpec::Single(s) => build_focus_set(cgs, Some(s)),
        FocusSpec::Seeds(seeds) => {
            if seeds.is_empty() {
                None
            } else {
                Some(build_focus_set_union(cgs, seeds))
            }
        }
        FocusSpec::SeedsExact(_) => unreachable!("handled above"),
    };
    let full_entities: Vec<&str> = cgs
        .entities
        .iter()
        .filter(|(n, ent)| {
            if ent.abstract_entity {
                return false;
            }
            focus_set
                .as_ref()
                .map(|s| s.contains(n.as_str()))
                .unwrap_or(true)
        })
        .map(|(n, _)| n.as_str())
        .collect();
    let dim_entities: Vec<&str> = cgs
        .entities
        .iter()
        .filter(|(n, ent)| {
            if ent.abstract_entity {
                return false;
            }
            focus_set
                .as_ref()
                .map(|s| !s.contains(n.as_str()))
                .unwrap_or(false)
        })
        .map(|(n, _)| n.as_str())
        .collect();
    (full_entities, dim_entities)
}

/// Full + dim entity name slices when [`DomainExposureSession`] spans multiple loaded [`crate::schema::CGS`] graphs.
pub fn entity_slices_for_render_federated<'a>(
    cgs_layers: &[&'a CGS],
    exposure: &'a DomainExposureSession,
) -> (Vec<&'a str>, Vec<&'a str>) {
    if cgs_layers.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let refs: Vec<&'a str> = exposure.entities.iter().map(|s| s.as_str()).collect();
    let mut full: Vec<&str> = Vec::new();
    let mut full_set: HashSet<&str> = HashSet::new();
    for &name in &refs {
        let ok = cgs_layers.iter().any(|c| {
            c.get_entity(name)
                .map(|e| !e.abstract_entity)
                .unwrap_or(false)
        });
        if ok {
            full.push(name);
            full_set.insert(name);
        }
    }
    let mut dim_set: HashSet<&str> = HashSet::new();
    for cgs in cgs_layers {
        for (n, ent) in &cgs.entities {
            if ent.abstract_entity || full_set.contains(n.as_str()) {
                continue;
            }
            dim_set.insert(n.as_str());
        }
    }
    let mut dim: Vec<&str> = dim_set.into_iter().collect();
    dim.sort();
    (full, dim)
}

/// Same `p#` name set as [`SymbolMap::build`] (entity fields + relations + capability inputs for `full_entities`).
pub(crate) fn collect_ident_names(cgs: &CGS, full_entities: &[&str]) -> BTreeSet<String> {
    let full_set: HashSet<&str> = full_entities.iter().copied().collect();
    let mut idents: BTreeSet<String> = BTreeSet::new();
    for e in full_entities {
        let Some(ent) = cgs.get_entity(e) else {
            continue;
        };
        for (k, _) in &ent.fields {
            idents.insert(k.as_str().to_string());
        }
        for (k, _) in &ent.relations {
            idents.insert(k.as_str().to_string());
        }
    }
    for dom in &full_set {
        let Some(names) = cgs.capability_names_by_domain().get(*dom) else {
            continue;
        };
        for cap_name in names {
            let Some(cap) = cgs.capabilities.get(cap_name) else {
                continue;
            };
            let Some(is) = &cap.input_schema else {
                continue;
            };
            let InputType::Object { fields, .. } = &is.input_type else {
                continue;
            };
            for f in fields {
                idents.insert(f.name.clone());
            }
        }
    }
    idents
}

/// Stable fingerprint for slot **identity**: catalog row (`catalog_entry_id`), owning entity,
/// structural type (field type, semantics, array/items, allowed values, role), wire name, and
/// description. Same-shaped slots on **different** entities or catalogs receive **distinct** opaque
/// `p#` tokens; allocation remains append-only within a session.
pub(crate) fn slot_allocation_fingerprint(meta: &IdentMetadata) -> String {
    let role_tag = match &meta.role {
        IdentRole::EntityField => "ef".to_string(),
        IdentRole::Relation { target } => format!("rel:{}", target.as_str()),
        IdentRole::CapabilityParam { capability } => {
            format!("cap:{}|{}", meta.entity.as_str(), capability.as_str(),)
        }
    };
    let ft = serde_json::to_string(&meta.field_type).unwrap_or_else(|_| "\"?\"".to_string());
    let sem = serde_json::to_string(&meta.string_semantics).unwrap_or_else(|_| "null".to_string());
    let ai = serde_json::to_string(&meta.array_items).unwrap_or_else(|_| "null".to_string());
    let av = serde_json::to_string(&meta.allowed_values).unwrap_or_else(|_| "null".to_string());
    let desc = meta.description.trim();
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        meta.catalog_entry_id.as_str(),
        meta.entity.as_str(),
        role_tag,
        meta.wire_name.as_str(),
        ft,
        sem,
        ai,
        av,
        desc,
    )
}

/// Stable key for one concrete slot occurrence (entity field, relation, or capability param).
/// Unlike [`slot_allocation_fingerprint`], this keeps entity ownership so scoped symbol maps can
/// rebuild exact `(entity, slot)` bindings even when several occurrences intentionally share one
/// opaque `p#`.
fn slot_occurrence_key(meta: &IdentMetadata) -> String {
    match &meta.role {
        IdentRole::EntityField => format!(
            "ef|{}|{}|{}",
            meta.catalog_entry_id.as_str(),
            meta.entity.as_str(),
            meta.wire_name
        ),
        IdentRole::Relation { target } => format!(
            "rel|{}|{}|{}|{}",
            meta.catalog_entry_id.as_str(),
            meta.entity.as_str(),
            meta.wire_name,
            target.as_str()
        ),
        IdentRole::CapabilityParam { capability } => format!(
            "cap|{}|{}|{}|{}",
            meta.catalog_entry_id.as_str(),
            meta.entity.as_str(),
            capability.as_str(),
            meta.wire_name
        ),
    }
}

/// One [`IdentMetadata`] per slot (entity field, relation, or capability input field) visible for
/// `full_entities` — used for fingerprint-based `p#` allocation (not wire-name-only).
fn collect_slot_metas(
    cgs: &CGS,
    full_entities: &[&str],
    catalog_entry_id: &str,
) -> Vec<IdentMetadata> {
    let full_set: HashSet<&str> = full_entities.iter().copied().collect();
    let mut out = Vec::new();
    let cid = catalog_entry_id.to_string();
    for &ename in full_entities {
        let Some(ent) = cgs.get_entity(ename) else {
            continue;
        };
        let en = EntityName::from(ename.to_string());
        for (fname, f) in &ent.fields {
            out.push(IdentMetadata {
                catalog_entry_id: cid.clone(),
                field_type: f.field_type.clone(),
                string_semantics: f.string_semantics,
                array_items: f.array_items.clone(),
                allowed_values: f.allowed_values.clone(),
                role: IdentRole::EntityField,
                wire_name: fname.as_str().to_string(),
                description: f.description.clone(),
                entity: en.clone(),
            });
        }
        for (rname, r) in &ent.relations {
            out.push(IdentMetadata {
                catalog_entry_id: cid.clone(),
                field_type: FieldType::EntityRef {
                    target: r.target_resource.clone(),
                },
                string_semantics: None,
                array_items: None,
                allowed_values: None,
                role: IdentRole::Relation {
                    target: r.target_resource.clone(),
                },
                wire_name: rname.as_str().to_string(),
                description: r.description.clone(),
                entity: en.clone(),
            });
        }
    }
    for dom in &full_set {
        let Some(names) = cgs.capability_names_by_domain().get(*dom) else {
            continue;
        };
        for cap_name in names {
            let Some(cap) = cgs.capabilities.get(cap_name) else {
                continue;
            };
            let Some(is) = &cap.input_schema else {
                continue;
            };
            let InputType::Object { fields, .. } = &is.input_type else {
                continue;
            };
            let en = cap.domain.clone();
            for f in fields {
                out.push(IdentMetadata {
                    catalog_entry_id: cid.clone(),
                    field_type: f.field_type.clone(),
                    string_semantics: f.string_semantics,
                    array_items: f.array_items.clone(),
                    allowed_values: f.allowed_values.clone(),
                    role: IdentRole::CapabilityParam {
                        capability: cap.name.clone(),
                    },
                    wire_name: f.name.clone(),
                    description: f.description.clone().unwrap_or_default(),
                    entity: en.clone(),
                });
            }
        }
    }
    out
}

/// Build typed metadata for all (entity, ident) pairs in the full-entity slice.
/// Replaces the global first-wins `build_ident_gloss_map` + `build_ident_type_map` pipeline.
pub(crate) fn build_ident_metadata(
    cgs: &CGS,
    full_entities: &[&str],
) -> HashMap<IdentMetaKey, IdentMetadata> {
    let full_set: HashSet<&str> = full_entities.iter().copied().collect();
    let mut out: HashMap<IdentMetaKey, IdentMetadata> = HashMap::new();
    let cid = cgs.entry_id.clone().unwrap_or_default();

    for &ename in full_entities {
        let Some(ent) = cgs.get_entity(ename) else {
            continue;
        };
        let en = EntityName::from(ename.to_string());
        for (fname, f) in &ent.fields {
            out.entry((cid.clone(), en.clone(), fname.as_str().to_string()))
                .or_insert_with(|| IdentMetadata {
                    catalog_entry_id: cid.clone(),
                    field_type: f.field_type.clone(),
                    string_semantics: f.string_semantics,
                    array_items: f.array_items.clone(),
                    allowed_values: f.allowed_values.clone(),
                    role: IdentRole::EntityField,
                    wire_name: fname.as_str().to_string(),
                    description: f.description.clone(),
                    entity: en.clone(),
                });
        }
        for (rname, r) in &ent.relations {
            out.entry((cid.clone(), en.clone(), rname.as_str().to_string()))
                .or_insert_with(|| IdentMetadata {
                    catalog_entry_id: cid.clone(),
                    field_type: FieldType::EntityRef {
                        target: r.target_resource.clone(),
                    },
                    string_semantics: None,
                    array_items: None,
                    allowed_values: None,
                    role: IdentRole::Relation {
                        target: r.target_resource.clone(),
                    },
                    wire_name: rname.as_str().to_string(),
                    description: r.description.clone(),
                    entity: en.clone(),
                });
        }
    }
    for dom in &full_set {
        let Some(names) = cgs.capability_names_by_domain().get(*dom) else {
            continue;
        };
        for cap_name in names {
            let Some(cap) = cgs.capabilities.get(cap_name) else {
                continue;
            };
            let Some(is) = &cap.input_schema else {
                continue;
            };
            let InputType::Object { fields, .. } = &is.input_type else {
                continue;
            };
            let en = cap.domain.clone();
            for f in fields {
                out.entry((cid.clone(), en.clone(), f.name.clone()))
                    .or_insert_with(|| IdentMetadata {
                        catalog_entry_id: cid.clone(),
                        field_type: f.field_type.clone(),
                        string_semantics: f.string_semantics,
                        array_items: f.array_items.clone(),
                        allowed_values: f.allowed_values.clone(),
                        role: IdentRole::CapabilityParam {
                            capability: cap.name.clone(),
                        },
                        wire_name: f.name.clone(),
                        description: f.description.clone().unwrap_or_default(),
                        entity: en.clone(),
                    });
            }
        }
    }
    out
}

impl IdentMetadata {
    /// Render the gloss line content (after `p#  ;;  `). The `map` is used to resolve
    /// entity-ref targets to their `e#` symbol when symbol tuning is active.
    pub fn render_gloss(&self, map: Option<&SymbolMap>) -> String {
        let type_label = match &self.role {
            IdentRole::Relation { target } => {
                let hint = match map {
                    Some(m) => format!("=> {}", m.entity_sym(target.as_str())),
                    None => format!("=> {}", target),
                };
                hint
            }
            _ => array_or_scalar_gloss_label(
                &self.field_type,
                &self.array_items,
                self.string_semantics,
                map,
            ),
        };
        // Select / multiselect: show enum values (wire names alone are not actionable).
        if matches!(self.field_type, FieldType::Select | FieldType::MultiSelect) {
            if let Some(ref av) = self.allowed_values {
                if !av.is_empty() {
                    let joined = av.join(", ");
                    let vals = truncate_desc(&joined, 240);
                    return format!("{type_label} · {vals}");
                }
            }
        }
        let desc = self.description.trim();
        if desc.is_empty() {
            match &self.role {
                IdentRole::Relation { target } => {
                    format!("{type_label} \u{00b7} {}", target)
                }
                _ => format!("{type_label} \u{00b7} {}", self.wire_name),
            }
        } else {
            let truncated = truncate_desc(desc, 100);
            format!("{type_label} \u{00b7} {truncated}")
        }
    }
}

/// Single `args: p# …` slot fragment for DOMAIN/TSV compact summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompactArgSlotGloss {
    pub text: String,
    /// When false, keep a full `p#  ;;` gloss row (long enums, `select+`, `multiselect+`, unknown array shape, …).
    pub allows_suppress_standalone_gloss: bool,
    /// Schema `required` — used to order `args:` (required before optional).
    pub required: bool,
}

const MAX_INLINE_ARGS_SELECT_ENUM: usize = 80;

/// Wire label + type + `req`/`opt` in one `args:` segment (conformance-focused; not for long human prose).
pub(crate) fn build_compact_arg_slot_gloss(
    sym: &str,
    wire: &str,
    required: bool,
    meta: &IdentMetadata,
    map: &SymbolMap,
) -> CompactArgSlotGloss {
    let ro = if required { "req" } else { "opt" };
    match &meta.field_type {
        FieldType::EntityRef { target } => {
            let es = map.entity_sym(target.as_str());
            let t = format!("ref:{}", es);
            CompactArgSlotGloss {
                text: format!("{sym} {wire} {t} {ro}"),
                allows_suppress_standalone_gloss: true,
                required,
            }
        }
        FieldType::Select | FieldType::MultiSelect => {
            if let Some(ref av) = meta.allowed_values {
                if !av.is_empty() {
                    let joined = av.join(",");
                    if joined.chars().count() <= MAX_INLINE_ARGS_SELECT_ENUM {
                        let br = match &meta.field_type {
                            FieldType::Select => format!("sel[{}]", joined),
                            _ => format!("msel[{}]", joined),
                        };
                        return CompactArgSlotGloss {
                            text: format!("{sym} {wire} {br} {ro}"),
                            allows_suppress_standalone_gloss: true,
                            required,
                        };
                    }
                }
            }
            let t = if matches!(&meta.field_type, FieldType::Select) {
                "select+"
            } else {
                "multiselect+"
            };
            CompactArgSlotGloss {
                text: format!("{sym} {wire} {t} {ro}"),
                allows_suppress_standalone_gloss: false,
                required,
            }
        }
        FieldType::Array => {
            if let Some(ref items) = meta.array_items {
                let inner = array_element_gloss_label(items, Some(map));
                CompactArgSlotGloss {
                    text: format!("{sym} {wire} array[{inner}] {ro}"),
                    allows_suppress_standalone_gloss: true,
                    required,
                }
            } else {
                CompactArgSlotGloss {
                    text: format!("{sym} {wire} array+ {ro}"),
                    allows_suppress_standalone_gloss: false,
                    required,
                }
            }
        }
        _ => {
            let t = array_or_scalar_gloss_label(
                &meta.field_type,
                &meta.array_items,
                meta.string_semantics,
                Some(map),
            );
            CompactArgSlotGloss {
                text: format!("{sym} {wire} {t} {ro}"),
                allows_suppress_standalone_gloss: true,
                required,
            }
        }
    }
}

/// Joins compact slot glosses for `  ;;  … args: …` (DOMAIN) and TSV `Meaning`.
/// Required parameters are listed before optional (stable order within each group).
pub(crate) fn join_compact_invocation_arg_fragments(
    fragments: Vec<CompactArgSlotGloss>,
) -> Option<String> {
    if fragments.is_empty() {
        return None;
    }
    let mut indexed: Vec<(usize, CompactArgSlotGloss)> =
        fragments.into_iter().enumerate().collect();
    indexed.sort_by_key(|(i, f)| {
        // Required slots first; preserve original field order within each group.
        (!f.required, *i)
    });
    Some(
        indexed
            .into_iter()
            .map(|(_, f)| f.text)
            .collect::<Vec<_>>()
            .join("; "),
    )
}

// Match [`prompt_render::CAP_LEGEND_SEP`] and em dash in legends (U+2014) without importing `prompt_render`.
const CAP_LEGEND_DOMAIN: &str = "  ;;  ";
const LEGEND_EM_DASH: &str = " — ";

/// `p#` that appear in a compact `args:` line and whether a standalone `p#` row may be omitted.
pub(crate) fn args_line_suppressible_capability_syms(line: &str) -> Option<HashMap<String, bool>> {
    let after = line.split_once(CAP_LEGEND_DOMAIN).map(|(_, a)| a)?;
    let after_args = after.split_once("args:").map(|(_, a)| a)?;
    let body = after_args
        .split_once(LEGEND_EM_DASH)
        .map(|(a, _)| a)
        .unwrap_or(after_args)
        .trim();
    let mut m = HashMap::new();
    for seg in body.split(';') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let Some(sym) = seg.split_whitespace().next() else {
            continue;
        };
        if !sym.starts_with('p') {
            continue;
        }
        let has_plus = seg.contains("select+")
            || seg.contains("multiselect+")
            || seg.contains("msel+")
            || seg.contains("array+");
        m.insert(sym.to_string(), !has_plus);
    }
    if m.is_empty() {
        None
    } else {
        Some(m)
    }
}

/// Short type label for DOMAIN `p#` gloss (matches [`FieldType`] / capability inputs).
/// Type keyword for a scalar `string` in DOMAIN gloss (`str` vs `markdown`, …).
pub(crate) fn string_semantics_gloss_label(sem: Option<StringSemantics>) -> String {
    let s = sem.unwrap_or(StringSemantics::Short);
    s.gloss_type_keyword().unwrap_or("str").to_string()
}

pub(crate) fn field_type_to_gloss_label(ft: &FieldType) -> String {
    match ft {
        FieldType::Boolean => "bool".to_string(),
        FieldType::Number => "float".to_string(),
        FieldType::Integer => "int".to_string(),
        FieldType::String => "str".to_string(),
        FieldType::Blob => "blob".to_string(),
        FieldType::Uuid => "uuid".to_string(),
        FieldType::Select => "select".to_string(),
        FieldType::MultiSelect => "multiselect".to_string(),
        FieldType::Date => "date".to_string(),
        FieldType::Array => "array".to_string(),
        FieldType::Json => "json".to_string(),
        FieldType::EntityRef { target } => format!("ref:{target}"),
    }
}

fn array_element_gloss_label(ai: &ArrayItemsSchema, map: Option<&SymbolMap>) -> String {
    match &ai.field_type {
        FieldType::EntityRef { target } => {
            let sym = map
                .map(|m| m.entity_sym(target.as_str()))
                .unwrap_or_else(|| target.to_string());
            format!("ref:{sym}")
        }
        _ => field_type_to_gloss_label(&ai.field_type),
    }
}

/// Type prefix for `p#  ;;  …` lines: `array[inner]` when element typing is known, else `array`.
fn array_or_scalar_gloss_label(
    ft: &FieldType,
    items: &Option<ArrayItemsSchema>,
    string_semantics: Option<StringSemantics>,
    map: Option<&SymbolMap>,
) -> String {
    match ft {
        FieldType::Array => match items {
            Some(ai) => format!("array[{}]", array_element_gloss_label(ai, map)),
            None => "array".to_string(),
        },
        FieldType::String => string_semantics_gloss_label(string_semantics),
        FieldType::Blob => "blob".to_string(),
        _ => field_type_to_gloss_label(ft),
    }
}

/// Resolve a schema type string for `ident`, scoped like [`SymbolMap::build`].
/// Prefers **capability** input fields (query filters) over entity fields, then relations.
///
/// Relation names resolve to `=> e#` (when `map` is set) or `=> TargetEntity` — same “points at entity”
/// shape as capability result hints, not `relation→…`.
#[allow(dead_code)]
fn resolve_ident_type_string(
    cgs: &CGS,
    full_entities: &[&str],
    name: &str,
    map: Option<&SymbolMap>,
) -> Option<String> {
    let full_set: HashSet<&str> = full_entities.iter().copied().collect();
    let mut caps: Vec<&CapabilitySchema> = cgs.capabilities.values().collect();
    caps.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    for cap in caps {
        if !full_set.contains(cap.domain.as_str()) {
            continue;
        }
        let Some(is) = &cap.input_schema else {
            continue;
        };
        let InputType::Object { fields, .. } = &is.input_type else {
            continue;
        };
        for f in fields {
            if f.name == name {
                return Some(match f.field_type {
                    FieldType::String => string_semantics_gloss_label(f.string_semantics),
                    FieldType::Blob => "blob".to_string(),
                    _ => field_type_to_gloss_label(&f.field_type),
                });
            }
        }
    }
    for e in full_entities {
        if let Some(ent) = cgs.get_entity(e) {
            if let Some(f) = ent.fields.get(name) {
                return Some(match f.field_type {
                    FieldType::String => string_semantics_gloss_label(f.string_semantics),
                    FieldType::Blob => "blob".to_string(),
                    _ => field_type_to_gloss_label(&f.field_type),
                });
            }
        }
    }
    for e in full_entities {
        if let Some(ent) = cgs.get_entity(e) {
            if let Some(r) = ent.relations.get(name) {
                let target = r.target_resource.as_str();
                let hint = match map {
                    Some(m) => format!("=> {}", m.entity_sym(target)),
                    None => format!("=> {}", target),
                };
                return Some(hint);
            }
        }
    }
    None
}

/// Per-ident short type labels for inline `p#  ;;  …` gloss (parallel to [`build_ident_gloss_map`] descriptions).
#[allow(dead_code)]
pub(crate) fn build_ident_type_map(
    cgs: &CGS,
    full_entities: &[&str],
    map: Option<&SymbolMap>,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for name in collect_ident_names(cgs, full_entities) {
        if let Some(t) = resolve_ident_type_string(cgs, full_entities, name.as_str(), map) {
            out.insert(name, t);
        }
    }
    out
}

/// Bidirectional maps for one prompt/eval slice.
#[derive(Debug, Clone)]
pub struct SymbolMap {
    sym_to_entity: IndexMap<String, String>,
    /// `(registry entry_id, CGS entity name)` → opaque `e#`. Duplicate entity names across catalogs are distinct rows.
    qualified_entity_to_sym: IndexMap<(String, String), String>,
    /// method symbol -> (catalog entry_id, domain entity, kebab label)
    sym_to_method: IndexMap<String, (String, String, String)>,
    /// (registry entry_id or `""`, domain entity, kebab method label)
    method_to_sym: IndexMap<(String, String, String), String>,
    /// Dotted-call alias map: `(path anchor entity, m#)` → kebab when method is registered on another domain
    /// (`Team(42).m22` → kebab when `m22` maps to a child-domain method on a path anchor).
    anchor_scoped_method_sym: HashMap<(String, String), String>,
    sym_to_ident: IndexMap<String, String>,
    /// Wire name → first-assigned `p#` when multiple slots share a label (feedback collapse / legacy).
    ident_to_sym: IndexMap<String, String>,
    /// `(entry_id, entity, field)` → `p#` for entity fields.
    entity_field_to_sym: HashMap<(String, String, String), String>,
    /// `(entry_id, entity, relation_or_ref_name)` → `p#` for relations and entity-ref nav.
    relation_to_sym: HashMap<(String, String, String), String>,
    /// `(entry_id, domain_entity, capability_name, param)` → `p#` for capability inputs.
    cap_param_to_sym: HashMap<(String, String, String, String), String>,
}

impl SymbolMap {
    /// If `token` is a session `e#` symbol (e.g. `e1` from the DOMAIN table), return the canonical entity name.
    #[inline]
    pub fn resolve_session_entity_symbol(&self, token: &str) -> Option<String> {
        self.sym_to_entity.get(token).cloned()
    }

    /// Build maps for all entities in `full_entities` (slice order defines `e1`, `e2`, …).
    ///
    /// This is a thin wrapper around [`DomainExposureSession::new`] + [`DomainExposureSession::to_symbol_map`]:
    /// one code path for `m#` / `p#` assignment and dotted-call alias metadata (execute / REPL / canonical DOMAIN).
    pub fn build(cgs: &CGS, full_entities: &[&str]) -> Self {
        DomainExposureSession::new(cgs, "", full_entities).to_symbol_map()
    }

    /// Structured DOMAIN token when `canonical` is in this map.
    #[inline]
    pub fn try_entity_domain_term(&self, canonical: &str) -> Option<DomainTerm> {
        let mut matches: Vec<_> = self
            .qualified_entity_to_sym
            .iter()
            .filter(|((_, ent), _)| ent.as_str() == canonical)
            .collect();
        matches.sort_by_key(|((eid, _), sym)| (eid.clone(), (*sym).clone()));
        let sym_str = matches.first()?.1;
        let idx = Symbol::parse_index(sym_str, 'e')?;
        Some(DomainTerm::Entity(
            EntityRef {
                name: EntityName::new(canonical),
            },
            idx,
        ))
    }

    /// Method token + CGS [`MethodRef`]; requires `cgs` to attach capability identity.
    #[inline]
    pub fn try_method_domain_term(
        &self,
        cgs: &CGS,
        entity: &str,
        kebab: &str,
    ) -> Option<DomainTerm> {
        let entry_key = cgs.entry_id.as_deref().unwrap_or("");
        let sym_str = self
            .method_to_sym
            .get(&(entry_key.to_string(), entity.to_string(), kebab.to_string()))
            .or_else(|| {
                self.method_to_sym.iter().find_map(|((eid, e, k), s)| {
                    (e == entity && k == kebab && (eid.is_empty() || eid.as_str() == entry_key))
                        .then_some(s)
                })
            })?;
        let idx = Symbol::parse_index(sym_str, 'm')?;
        let mref = method_ref_for_domain_segment(cgs, entity, kebab)?;
        Some(DomainTerm::Method(mref, idx))
    }

    /// Parameter token + [`ParameterSlot`]; `full_entities` must match the slice used to build this map.
    #[inline]
    pub fn try_ident_domain_term(
        &self,
        cgs: &CGS,
        full_entities: &[&str],
        name: &str,
    ) -> Option<DomainTerm> {
        let slot = resolve_parameter_slot(cgs, full_entities, name)?;
        let entry_key = cgs.entry_id.as_deref().unwrap_or("");
        let sym_str = match &slot {
            ParameterSlot::EntityField { entity, field } => self.entity_field_to_sym.get(&(
                entry_key.to_string(),
                entity.as_str().to_string(),
                field.clone(),
            )),
            ParameterSlot::Relation { entity, name: rel } => self.relation_to_sym.get(&(
                entry_key.to_string(),
                entity.as_str().to_string(),
                rel.clone(),
            )),
            ParameterSlot::CapabilityInput {
                domain,
                capability,
                param,
            } => self.cap_param_to_sym.get(&(
                entry_key.to_string(),
                domain.as_str().to_string(),
                capability.as_str().to_string(),
                param.clone(),
            )),
        }?;
        let idx = Symbol::parse_index(sym_str, 'p')?;
        Some(DomainTerm::Parameter(slot, idx))
    }

    /// Opaque `e#` string — prefer [`Self::try_entity_domain_term`] then [`std::fmt::Display`] on [`DomainTerm`](crate::domain_term::DomainTerm) when threading DOMAIN state.
    #[inline]
    pub fn entity_sym(&self, canonical: &str) -> String {
        self.try_entity_domain_term(canonical)
            .map(|t| t.to_string())
            .unwrap_or_else(|| canonical.to_string())
    }

    /// Opaque `p#` for an **entity field** (scoped; preferred over [`Self::ident_sym`] when the entity is known).
    #[inline]
    pub fn ident_sym_entity_field(&self, entity: &str, field: &str) -> String {
        let mut v: Vec<_> = self
            .entity_field_to_sym
            .iter()
            .filter(|((_, e, f), _)| e.as_str() == entity && f.as_str() == field)
            .collect();
        v.sort_by_key(|((a, b, c), s)| (a.clone(), b.clone(), c.clone(), (*s).clone()));
        v.first()
            .map(|(_, s)| (*s).clone())
            .unwrap_or_else(|| field.to_string())
    }

    /// Opaque `p#` for a **relation** (or entity-ref nav segment) on `entity`.
    #[inline]
    pub fn ident_sym_relation(&self, entity: &str, relation: &str) -> String {
        let mut v: Vec<_> = self
            .relation_to_sym
            .iter()
            .filter(|((_, e, r), _)| e.as_str() == entity && r.as_str() == relation)
            .collect();
        v.sort_by_key(|((a, b, c), s)| (a.clone(), b.clone(), c.clone(), (*s).clone()));
        v.first()
            .map(|(_, s)| (*s).clone())
            .unwrap_or_else(|| relation.to_string())
    }

    /// Opaque `p#` for a **capability input** parameter (domain entity + capability + param name).
    #[inline]
    pub fn ident_sym_cap_param(
        &self,
        domain_entity: &str,
        capability: &str,
        param: &str,
    ) -> String {
        let mut v: Vec<_> = self
            .cap_param_to_sym
            .iter()
            .filter(|((_, dom, cap, p), _)| {
                dom.as_str() == domain_entity && cap.as_str() == capability && p.as_str() == param
            })
            .collect();
        v.sort_by_key(|((a, b, c, d), s)| {
            (a.clone(), b.clone(), c.clone(), d.clone(), (*s).clone())
        });
        v.first()
            .map(|(_, s)| (*s).clone())
            .unwrap_or_else(|| param.to_string())
    }

    /// `p#` by wire name alone only when all concrete slots with that wire name resolve to the same
    /// symbol. Returns `None` for ambiguous names like `id` when different entities map them to
    /// different `p#` tokens.
    pub fn ident_sym_unambiguous(&self, name: &str) -> Option<String> {
        let mut resolved: Option<&String> = None;
        for ((_, _, field), sym) in &self.entity_field_to_sym {
            if field != name {
                continue;
            }
            match resolved {
                None => resolved = Some(sym),
                Some(prev) if prev == sym => {}
                Some(_) => return None,
            }
        }
        for ((_, _, relation), sym) in &self.relation_to_sym {
            if relation != name {
                continue;
            }
            match resolved {
                None => resolved = Some(sym),
                Some(prev) if prev == sym => {}
                Some(_) => return None,
            }
        }
        for ((_, _, _, param), sym) in &self.cap_param_to_sym {
            if param != name {
                continue;
            }
            match resolved {
                None => resolved = Some(sym),
                Some(prev) if prev == sym => {}
                Some(_) => return None,
            }
        }
        resolved
            .cloned()
            .or_else(|| self.ident_to_sym.get(name).cloned())
    }

    /// Opaque `p#` by wire name alone — **ambiguous** when the same label names distinct slots; prefer
    /// [`Self::ident_sym_entity_field`], [`Self::ident_sym_cap_param`], or [`Self::ident_sym_relation`].
    #[inline]
    pub fn ident_sym(&self, name: &str) -> String {
        self.ident_to_sym
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// Resolve `p#` → canonical field name. Returns `None` if `sym` is not a known `p#` token.
    pub fn resolve_ident<'a>(&'a self, sym: &str) -> Option<&'a str> {
        self.sym_to_ident.get(sym).map(|s| s.as_str())
    }

    /// If `sym` maps a capability input parameter, return the `(capability domain entity, param wire)`.
    pub fn capability_param_key_for_p_sym(&self, sym: &str) -> Option<(EntityName, String)> {
        for ((_eid, dom, _cap, pname), s) in &self.cap_param_to_sym {
            if s == sym {
                return Some((EntityName::from(dom.as_str()), pname.clone()));
            }
        }
        None
    }

    /// Rewrite canonical entity and field names in a short snippet into opaque `e#` / `p#` tokens for
    /// LLM-facing recovery text (inverse of [`expand_path_symbols`] for identifier-shaped spans).
    pub fn collapse_tokens_for_feedback(&self, input: &str) -> String {
        let mut keys: Vec<String> = self
            .qualified_entity_to_sym
            .keys()
            .map(|(_, ent)| ent.clone())
            .collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        keys.dedup();
        let mut s = scan_replace(input, &keys, |k| {
            self.try_entity_domain_term(k)
                .map(|t| t.to_string())
                .unwrap_or_else(|| k.to_string())
        });
        let mut idents: Vec<String> = self.ident_to_sym.keys().cloned().collect();
        idents.sort_by_key(|k| std::cmp::Reverse(k.len()));
        s = scan_replace(&s, &idents, |id| {
            self.ident_sym_unambiguous(id)
                .unwrap_or_else(|| id.to_string())
        });
        s
    }

    /// Opaque `m#` string — prefer [`Self::try_method_domain_term`] when `cgs` is available.
    #[inline]
    pub fn method_sym(&self, entity: &str, kebab: &str) -> String {
        self.method_to_sym
            .iter()
            .find(|((_, e, k), _)| e == entity && k == kebab)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| kebab.to_string())
    }

    /// If `label` is an opaque method token `m#` (digits), return the kebab method label for parse.
    #[inline]
    pub fn resolve_method_symbol_token(&self, label: &str) -> Option<&str> {
        self.resolve_method_symbol_pair(label)
            .map(|(_, kebab)| kebab)
    }

    /// `m#` → `(domain entity name, kebab label)` — disambiguates duplicate kebabs (e.g. `space_query` vs `task_query` both `query`).
    #[inline]
    pub fn resolve_method_symbol_pair(&self, label: &str) -> Option<(&str, &str)> {
        let rest = label.strip_prefix('m')?;
        if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        self.sym_to_method
            .get(label)
            .map(|(_, d, k)| (d.as_str(), k.as_str()))
    }

    /// `[scope …]` fragment for DOMAIN `;;` legends only (no `optional params:` list).
    /// For [`CapabilityKind::Query`], returns empty (scope is not shown for query-style capabilities).
    pub(crate) fn capability_scope_legend_gloss(&self, cap: &CapabilitySchema) -> String {
        const MAX_SIG: usize = 96;
        let Some(is) = &cap.input_schema else {
            return String::new();
        };
        let InputType::Object { fields, .. } = &is.input_type else {
            return String::new();
        };
        if cap.kind == CapabilityKind::Query {
            return String::new();
        }
        let mut scope_parts: Vec<String> = Vec::new();
        let domain = cap.domain.as_str();
        let cap_name = cap.name.as_str();
        for f in fields {
            if !matches!(f.role, Some(ParameterRole::Scope)) {
                continue;
            }
            if let FieldType::EntityRef { target } = &f.field_type {
                let ps = self.ident_sym_cap_param(domain, cap_name, f.name.as_str());
                let es = self.entity_sym(target.as_str());
                scope_parts.push(format!("{ps}→{es}"));
            } else {
                scope_parts.push(self.ident_sym_cap_param(domain, cap_name, f.name.as_str()));
            }
        }
        if scope_parts.is_empty() {
            return String::new();
        }
        let s = format!("[scope {}]", scope_parts.join(", "));
        crate::utf8_trunc::truncate_utf8_owned_with_ellipsis(s, MAX_SIG)
    }

    /// Optional / scope parameter symbols for DOMAIN `;;` legends. Required parameters are omitted — they
    /// are already shown in the example expression. For [`CapabilityKind::Query`], omits `[scope …]`.
    /// When compact `args:` is present, prefer [`capability_scope_legend_gloss`] + `args:` instead (no duplicate list).
    pub(crate) fn capability_input_signature_gloss(&self, cap: &CapabilitySchema) -> String {
        const MAX_SIG: usize = 96;
        let Some(is) = &cap.input_schema else {
            return String::new();
        };
        let InputType::Object { fields, .. } = &is.input_type else {
            return String::new();
        };
        let mut scope_s = self.capability_scope_legend_gloss(cap);
        let mut optional_parts: Vec<String> = Vec::new();
        let domain = cap.domain.as_str();
        let cap_name = cap.name.as_str();
        for f in fields {
            if matches!(f.role, Some(ParameterRole::Scope)) {
                continue;
            }
            if !field_is_filter_like_gloss(f) {
                continue;
            }
            let sym = self.ident_sym_cap_param(domain, cap_name, f.name.as_str());
            if f.required {
                continue;
            }
            optional_parts.push(sym);
        }
        if !optional_parts.is_empty() {
            if !scope_s.is_empty() {
                scope_s.push(' ');
            }
            let _ = write!(
                &mut scope_s,
                "optional params: {}",
                optional_parts.join(", ")
            );
        }
        if scope_s.is_empty() {
            return scope_s;
        }
        crate::utf8_trunc::truncate_utf8_owned_with_ellipsis(scope_s, MAX_SIG)
    }

    /// Reserved for future SYMBOL MAP content; **FIELDS** moved inline into **DOMAIN** (see [`build_ident_gloss_map`]).
    pub fn format_legend(&self, _cgs: &CGS) -> String {
        String::new()
    }

    /// Human-readable gloss for a field token `p#` (same rules as the former **FIELDS** block).
    ///
    /// When `ident_types` is set, emits `type · description` (type from CGS; description from [`build_ident_gloss_map`]).
    /// Relations use `=> e# · …` (target entity symbol), not `relation→…`.
    #[allow(dead_code)]
    pub fn field_gloss_display(
        &self,
        sym: &str,
        ident_gloss: &HashMap<String, String>,
        ident_types: Option<&HashMap<String, String>>,
    ) -> String {
        const MAX_DESC: usize = 100;
        let name = self
            .sym_to_ident
            .get(sym)
            .map(|s| s.as_str())
            .unwrap_or(sym);
        let desc = ident_gloss
            .get(name)
            .map(|d| d.as_str().trim())
            .filter(|d| !d.is_empty())
            .map(|d| truncate_desc(d, MAX_DESC))
            .unwrap_or_else(|| name.to_string())
            .replace('\t', " ");
        let ty = ident_types
            .and_then(|m| m.get(name))
            .map(|s| s.as_str().trim())
            .filter(|t| !t.is_empty());
        match ty {
            Some(t) => format!("{t} · {desc}"),
            None => desc,
        }
    }
}

/// Merge entity field, relation, and capability-parameter descriptions (first wins per name).
#[allow(dead_code)]
pub fn build_ident_gloss_map(cgs: &CGS) -> HashMap<String, String> {
    let mut ident_gloss: HashMap<String, String> = HashMap::new();
    for e in cgs.entities.values() {
        for (fname, f) in &e.fields {
            if !f.description.is_empty() {
                ident_gloss
                    .entry(fname.as_str().to_string())
                    .or_insert_with(|| f.description.clone());
            }
        }
    }
    for e in cgs.entities.values() {
        for r in e.relations.values() {
            if !r.description.is_empty() {
                ident_gloss
                    .entry(r.name.as_str().to_string())
                    .or_insert_with(|| r.description.clone());
            }
        }
    }
    for cap in cgs.capabilities.values() {
        let Some(is) = &cap.input_schema else {
            continue;
        };
        let InputType::Object { fields, .. } = &is.input_type else {
            continue;
        };
        for f in fields {
            if let Some(d) = &f.description {
                if !d.is_empty() {
                    ident_gloss
                        .entry(f.name.clone())
                        .or_insert_with(|| d.clone());
                }
            }
        }
    }
    ident_gloss
}

/// Left-to-right `p#` tokens in an expression fragment (after stripping prompt annotations).
pub fn field_syms_in_expr(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < expr.len() {
        if expr.as_bytes().get(i) == Some(&b'p') && ident_boundary_left(expr, i) {
            let mut end = i + 1;
            while end < expr.len() {
                let c = expr[end..].chars().next().unwrap();
                if c.is_ascii_digit() {
                    end += c.len_utf8();
                } else {
                    break;
                }
            }
            if end > i + 1 {
                let next = expr[end..].chars().next();
                if next.is_none() || !ident_continue(next.unwrap()) {
                    out.push(expr[i..end].to_string());
                    i = end;
                    continue;
                }
            }
        }
        i += expr[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

/// `p#` tokens for inline gloss: expression first (left-to-right), then the `;;` suffix (`optional params: …` /
/// `[scope …]` then description, so optional-only params in `..` still get a gloss line).
pub fn field_syms_for_domain_line(line: &str) -> Vec<String> {
    let expr = strip_prompt_expression_annotations(line);
    let mut ordered: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for sym in field_syms_in_expr(&expr) {
        if seen.insert(sym.clone()) {
            ordered.push(sym);
        }
    }
    if let Some((_, rest)) = line.split_once("  ;;  ") {
        for sym in field_syms_in_expr(rest) {
            if seen.insert(sym.clone()) {
                ordered.push(sym);
            }
        }
    }
    ordered
}

fn truncate_desc(s: &str, max: usize) -> String {
    let t = s.trim();
    crate::utf8_trunc::truncate_utf8_bytes_with_ellipsis(t, max)
}

/// Expand symbolic path text to canonical identifiers for the parser.
pub fn expand_path_symbols(input: &str, map: &SymbolMap) -> String {
    let mut s = replace_sym_tokens(input, map, SymPhase::Entity);
    s = replace_sym_tokens(&s, map, SymPhase::Ident);
    s = expand_method_tokens(&s, map);
    s
}

enum SymPhase {
    Entity,
    Ident,
}

fn replace_sym_tokens(input: &str, map: &SymbolMap, phase: SymPhase) -> String {
    use std::collections::HashMap;
    let lookup: HashMap<String, String> = match phase {
        SymPhase::Entity => map
            .sym_to_entity
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect(),
        SymPhase::Ident => map
            .sym_to_ident
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect(),
    };
    let mut syms: Vec<String> = lookup.keys().cloned().collect();
    syms.sort_by_key(|k| std::cmp::Reverse(k.len()));
    scan_replace(input, &syms, |sym| {
        lookup.get(sym).cloned().unwrap_or_else(|| sym.to_string())
    })
}

fn expand_method_tokens(input: &str, map: &SymbolMap) -> String {
    let mut syms: Vec<String> = map.sym_to_method.keys().cloned().collect();
    syms.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut in_string = false;
    let mut escape = false;

    while i < input.len() {
        let ch = input[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += ch_len;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += ch_len;
            continue;
        }
        let mut advanced = false;
        if ch == '.' && i + 1 < input.len() {
            let rest = &input[i + 1..];
            if let Some(sym) = syms.iter().find(|s| rest.starts_with(*s)) {
                let sym_len = sym.len();
                let after = i + 1 + sym_len;
                let boundary_ok =
                    after >= input.len() || !ident_continue(input[after..].chars().next().unwrap());
                if boundary_ok {
                    if let Some((_, ent, kebab)) = map.sym_to_method.get(sym.as_str()) {
                        if let Some(left_ent) = find_entity_before_dot(input, i) {
                            if left_ent == *ent {
                                out.push('.');
                                out.push_str(kebab);
                                i += 1 + sym_len;
                                advanced = true;
                            } else if let Some(sk) =
                                map.anchor_scoped_method_sym.get(&(left_ent, sym.clone()))
                            {
                                out.push('.');
                                out.push_str(sk);
                                i += 1 + sym_len;
                                advanced = true;
                            }
                        }
                    }
                }
            }
        }
        if advanced {
            continue;
        }
        out.push(ch);
        i += ch_len;
    }
    out
}

fn find_entity_before_dot(s: &str, dot_idx: usize) -> Option<String> {
    if dot_idx == 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = dot_idx - 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        if i == 0 {
            return None;
        }
        i -= 1;
    }
    if bytes[i] == b')' {
        return entity_before_dot_after_close_paren(s, i);
    }
    // `Entity.method()` / `Entity.rel` — identifier immediately before `.`
    let end = i + 1;
    let mut start = end;
    while start > 0 {
        let c = bytes[start - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            start -= 1;
        } else {
            break;
        }
    }
    (start < end).then(|| s[start..end].to_string())
}

/// `Something(…).method` — resolve `Something` from the `)` at `close_paren_idx`.
fn entity_before_dot_after_close_paren(s: &str, close_paren_idx: usize) -> Option<String> {
    let bytes = s.as_bytes();
    let mut depth = 1usize;
    let mut j = close_paren_idx;
    while j > 0 {
        j -= 1;
        match bytes[j] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    let (start, end) = entity_ident_before_open_paren(s, j)?;
                    return Some(s[start..end].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn entity_ident_before_open_paren(s: &str, open_paren: usize) -> Option<(usize, usize)> {
    if open_paren == 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut end = open_paren;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 {
        let c = bytes[start - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end {
        None
    } else {
        Some((start, end))
    }
}

fn scan_replace(
    input: &str,
    syms_sorted_long_first: &[String],
    canon: impl Fn(&str) -> String,
) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut in_string = false;
    let mut escape = false;

    while i < input.len() {
        let ch = input[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += ch_len;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += ch_len;
            continue;
        }
        let mut replaced = false;
        if ident_boundary_left(input, i) {
            for sym in syms_sorted_long_first {
                if input[i..].starts_with(sym) {
                    let after = i + sym.len();
                    let boundary_ok = after >= input.len()
                        || !ident_continue(input[after..].chars().next().unwrap());
                    if boundary_ok {
                        out.push_str(&canon(sym));
                        i = after;
                        replaced = true;
                        break;
                    }
                }
            }
        }
        if !replaced {
            out.push(ch);
            i += ch_len;
        }
    }
    out
}

fn ident_boundary_left(s: &str, i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = s[..i].chars().next_back().unwrap();
    !ident_continue(prev)
}

fn ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Build a [`DomainExposureSession`] from REPL/eval [`FocusSpec`], using the **same** `e#`/`m#`/`p#`
/// rules as HTTP/MCP execute: **sorted** entity names, **no** 2-hop neighbourhood expansion.
///
/// This keeps REPL and execute symbol indices aligned when the same seed set is used (`Single(s)`
/// ≡ one seed, `Seeds` ≡ sorted list). Use multiple seeds or incremental exposure if you need more
/// entities in DOMAIN.
pub fn domain_exposure_session_from_focus(
    cgs: &CGS,
    focus: FocusSpec<'_>,
) -> DomainExposureSession {
    match focus {
        FocusSpec::All => {
            let mut names: Vec<&str> = cgs
                .entities
                .iter()
                .filter(|(_, ent)| !ent.abstract_entity)
                .map(|(n, _)| n.as_str())
                .collect();
            names.sort();
            DomainExposureSession::new(cgs, "", &names)
        }
        FocusSpec::Single(s) => DomainExposureSession::new(cgs, "", &[s]),
        FocusSpec::Seeds(seeds) => {
            if seeds.is_empty() {
                return domain_exposure_session_from_focus(cgs, FocusSpec::All);
            }
            let mut v: Vec<&str> = seeds.to_vec();
            v.sort();
            v.dedup();
            DomainExposureSession::new(cgs, "", &v)
        }
        FocusSpec::SeedsExact(seeds) => {
            if seeds.is_empty() {
                return domain_exposure_session_from_focus(cgs, FocusSpec::All);
            }
            let mut v: Vec<&str> = seeds.to_vec();
            v.sort();
            v.dedup();
            DomainExposureSession::new(cgs, "", &v)
        }
    }
}

/// When `symbol_tuning` is true (same as [`crate::prompt_render::RenderConfig::uses_symbols`]: **compact** or **tsv** [`crate::prompt_render::PromptRenderMode`]), build the map used for prompts and pre-parse expansion.
pub fn symbol_map_for_prompt(
    cgs: &CGS,
    focus: FocusSpec<'_>,
    symbol_tuning: bool,
) -> Option<SymbolMap> {
    if !symbol_tuning {
        return None;
    }
    Some(domain_exposure_session_from_focus(cgs, focus).to_symbol_map())
}

/// Owned entity names for prompt surface metrics and DOMAIN line counts, plus optional
/// [`DomainExposureSession`] when `symbol_tuning` is true (execute-parity slice; mirrors symbolic render modes); otherwise names from
/// [`entity_slices_for_render`] (2-hop for `Single` / `Seeds` when not exact).
pub fn resolve_prompt_surface_entities(
    cgs: &CGS,
    focus: FocusSpec<'_>,
    symbol_tuning: bool,
) -> (Vec<String>, Option<DomainExposureSession>) {
    if symbol_tuning {
        let exp = domain_exposure_session_from_focus(cgs, focus);
        let names = exp.entities.clone();
        (names, Some(exp))
    } else {
        let (full, _) = entity_slices_for_render(cgs, focus);
        let names = full.iter().map(|s| (*s).to_string()).collect();
        (names, None)
    }
}

/// Monotonic `e#` / `m#` / `p#` assignment as an execute/MCP session exposes more entity names from
/// the CGS graph. Indices only **append** — existing symbols never change when new domains appear.
#[derive(Debug)]
pub struct DomainExposureSession {
    /// Entities included in symbol space (order = `e1`, `e2`, …).
    pub entities: Vec<String>,
    /// Catalog registry `entry_id` for each row in [`Self::entities`] (same length). Disambiguates
    /// which [`crate::CgsContext`] owns the CGS entity when multiple catalogs are federated.
    pub entity_catalog_entry_ids: Vec<String>,
    /// Owning [`CGS`] per catalog `entry_id` (same keys as [`Self::entity_catalog_entry_ids`] values).
    catalog_cgs: IndexMap<String, Arc<CGS>>,
    sym_to_entity: IndexMap<String, String>,
    /// `(registry entry_id, CGS entity name)` → opaque `e#`.
    qualified_entity_to_sym: IndexMap<(String, String), String>,
    method_to_sym: IndexMap<(String, String, String), String>,
    sym_to_method: IndexMap<String, (String, String, String)>,
    ident_to_sym: IndexMap<String, String>,
    sym_to_ident: IndexMap<String, String>,
    anchor_scoped_method_sym: HashMap<(String, String), String>,
    /// Fingerprint → opaque `p#` (append-only; never renumbered).
    slot_fingerprint_to_sym: IndexMap<String, String>,
    /// Fingerprint → slot metadata (append-only; stable gloss / lookup rebuild).
    fingerprint_meta: IndexMap<String, IdentMetadata>,
    /// Concrete slot occurrence → metadata. Preserves every `(entity, slot)` binding even when
    /// multiple occurrences intentionally share one fingerprint / `p#`.
    slot_occurrence_meta: IndexMap<String, IdentMetadata>,
    entity_field_to_sym: HashMap<(String, String, String), String>,
    relation_to_sym: HashMap<(String, String, String), String>,
    cap_param_to_sym: HashMap<(String, String, String, String), String>,
    /// Memoized [`SymbolMap`] for this session; cleared in [`Self::expose_entities`].
    symbol_map_cache: RwLock<Option<Arc<SymbolMap>>>,
}

impl Clone for DomainExposureSession {
    fn clone(&self) -> Self {
        Self {
            entities: self.entities.clone(),
            entity_catalog_entry_ids: self.entity_catalog_entry_ids.clone(),
            catalog_cgs: self.catalog_cgs.clone(),
            sym_to_entity: self.sym_to_entity.clone(),
            qualified_entity_to_sym: self.qualified_entity_to_sym.clone(),
            method_to_sym: self.method_to_sym.clone(),
            sym_to_method: self.sym_to_method.clone(),
            ident_to_sym: self.ident_to_sym.clone(),
            sym_to_ident: self.sym_to_ident.clone(),
            anchor_scoped_method_sym: self.anchor_scoped_method_sym.clone(),
            slot_fingerprint_to_sym: self.slot_fingerprint_to_sym.clone(),
            fingerprint_meta: self.fingerprint_meta.clone(),
            slot_occurrence_meta: self.slot_occurrence_meta.clone(),
            entity_field_to_sym: self.entity_field_to_sym.clone(),
            relation_to_sym: self.relation_to_sym.clone(),
            cap_param_to_sym: self.cap_param_to_sym.clone(),
            symbol_map_cache: RwLock::new(None),
        }
    }
}

impl DomainExposureSession {
    /// First wave: assign symbols for `entity_names_in_order` (typically sorted seeds from the client).
    /// `catalog_entry_id` is the registry row for this graph (`""` when not using a multi-entry catalog).
    pub fn new(cgs: &CGS, catalog_entry_id: &str, entity_names_in_order: &[&str]) -> Self {
        let mut s = Self {
            entities: Vec::new(),
            entity_catalog_entry_ids: Vec::new(),
            catalog_cgs: IndexMap::new(),
            sym_to_entity: IndexMap::new(),
            qualified_entity_to_sym: IndexMap::new(),
            method_to_sym: IndexMap::new(),
            sym_to_method: IndexMap::new(),
            ident_to_sym: IndexMap::new(),
            sym_to_ident: IndexMap::new(),
            anchor_scoped_method_sym: HashMap::new(),
            slot_fingerprint_to_sym: IndexMap::new(),
            fingerprint_meta: IndexMap::new(),
            slot_occurrence_meta: IndexMap::new(),
            entity_field_to_sym: HashMap::new(),
            relation_to_sym: HashMap::new(),
            cap_param_to_sym: HashMap::new(),
            symbol_map_cache: RwLock::new(None),
        };
        let arc = Arc::new(cgs.clone());
        s.expose_entities(&[cgs], arc, catalog_entry_id, entity_names_in_order);
        s
    }

    /// Expose more entity names (e.g. next hop in the graph). Skips unknown or duplicate names.
    /// `cgs_layers` must include every [`CGS`] that contributes to this session (federated: all catalogs).
    /// `catalog_entry_id` identifies which catalog row these `names` belong to.
    pub fn expose_entities(
        &mut self,
        cgs_layers: &[&CGS],
        owning_cgs: Arc<CGS>,
        catalog_entry_id: &str,
        names: &[&str],
    ) {
        if cgs_layers.is_empty() {
            return;
        }
        self.catalog_cgs
            .insert(catalog_entry_id.to_string(), owning_cgs.clone());
        *self
            .symbol_map_cache
            .write()
            .expect("symbol_map_cache lock poisoned") = None;
        for n in names {
            let qkey = (catalog_entry_id.to_string(), (*n).to_string());
            if self.qualified_entity_to_sym.contains_key(&qkey) {
                continue;
            }
            let Some(ent) = owning_cgs.get_entity(n) else {
                continue;
            };
            if ent.abstract_entity {
                continue;
            }
            let i = self.entities.len() + 1;
            let sym = format!("e{i}");
            self.entities.push((*n).to_string());
            self.entity_catalog_entry_ids
                .push(catalog_entry_id.to_string());
            self.qualified_entity_to_sym.insert(qkey, sym.clone());
            self.sym_to_entity.insert(sym, (*n).to_string());
        }
        self.assign_new_methods_and_idents(cgs_layers);
        self.rebuild_anchor_scoped_method_labels(cgs_layers);
    }

    fn assign_new_methods_and_idents(&mut self, cgs_layers: &[&CGS]) {
        let full_refs: Vec<String> = self.entities.clone();
        let full_refs_str: Vec<&str> = full_refs.iter().map(|s| s.as_str()).collect();

        let mut new_triples: Vec<(String, String, String)> = Vec::new();
        for (dom, entry_id) in full_refs.iter().zip(self.entity_catalog_entry_ids.iter()) {
            let dom = dom.as_str();
            let entry_id = entry_id.as_str();
            let Some(cgs) = self.catalog_cgs.get(entry_id) else {
                continue;
            };
            let Some(names) = cgs.capability_names_by_domain().get(dom) else {
                continue;
            };
            for cap_name in names {
                let Some(cap) = cgs.capabilities.get(cap_name) else {
                    continue;
                };
                let kebab = capability_method_label_kebab(cap);
                let triple = (entry_id.to_string(), cap.domain.to_string(), kebab.clone());
                if !self.method_to_sym.contains_key(&triple) {
                    new_triples.push(triple);
                }
            }
        }
        new_triples.sort();
        let mut next_m = self.sym_to_method.len() + 1;
        for triple in new_triples {
            let sym = format!("m{next_m}");
            next_m += 1;
            self.method_to_sym.insert(triple.clone(), sym.clone());
            self.sym_to_method.insert(sym, triple);
        }

        self.assign_new_slot_symbols(cgs_layers, &full_refs_str);
    }

    fn assign_new_slot_symbols(&mut self, _cgs_layers: &[&CGS], full_refs: &[&str]) {
        let mut collected: Vec<IdentMetadata> = Vec::new();
        let full_set: HashSet<&str> = full_refs.iter().copied().collect();
        for i in 0..self.entities.len() {
            let dom = self.entities[i].as_str();
            if !full_set.contains(dom) {
                continue;
            }
            let entry_id = self.entity_catalog_entry_ids[i].as_str();
            let Some(cgs) = self.catalog_cgs.get(entry_id) else {
                continue;
            };
            collected.extend(collect_slot_metas(cgs.as_ref(), &[dom], entry_id));
        }
        let mut by_fp: IndexMap<String, IdentMetadata> = IndexMap::new();
        for m in collected {
            self.slot_occurrence_meta
                .entry(slot_occurrence_key(&m))
                .or_insert_with(|| m.clone());
            let fp = slot_allocation_fingerprint(&m);
            by_fp.entry(fp).or_insert(m);
        }
        for (fp, meta) in &by_fp {
            self.fingerprint_meta
                .entry(fp.clone())
                .or_insert_with(|| meta.clone());
        }
        let mut new_fps: Vec<String> = by_fp
            .keys()
            .filter(|fp| !self.slot_fingerprint_to_sym.contains_key(*fp))
            .cloned()
            .collect();
        new_fps.sort();
        let mut next_p = self.slot_fingerprint_to_sym.len() + 1;
        for fp in new_fps {
            let sym = format!("p{next_p}");
            next_p += 1;
            self.slot_fingerprint_to_sym.insert(fp, sym);
        }
        self.rebuild_parameter_symbol_maps();
    }

    fn rebuild_parameter_symbol_maps(&mut self) {
        self.entity_field_to_sym.clear();
        self.relation_to_sym.clear();
        self.cap_param_to_sym.clear();
        self.sym_to_ident.clear();
        self.ident_to_sym.clear();
        let mut fp_order: Vec<String> = self.slot_fingerprint_to_sym.keys().cloned().collect();
        fp_order.sort();
        for fp in fp_order {
            let Some(meta) = self.fingerprint_meta.get(&fp) else {
                continue;
            };
            let Some(sym) = self.slot_fingerprint_to_sym.get(&fp) else {
                continue;
            };
            self.sym_to_ident
                .insert(sym.clone(), meta.wire_name.clone());
            self.ident_to_sym
                .entry(meta.wire_name.clone())
                .or_insert_with(|| sym.clone());
            match &meta.role {
                IdentRole::EntityField => {
                    self.entity_field_to_sym.insert(
                        (
                            meta.catalog_entry_id.clone(),
                            meta.entity.as_str().to_string(),
                            meta.wire_name.clone(),
                        ),
                        sym.clone(),
                    );
                }
                IdentRole::Relation { .. } => {
                    self.relation_to_sym.insert(
                        (
                            meta.catalog_entry_id.clone(),
                            meta.entity.as_str().to_string(),
                            meta.wire_name.clone(),
                        ),
                        sym.clone(),
                    );
                }
                IdentRole::CapabilityParam { capability } => {
                    self.cap_param_to_sym.insert(
                        (
                            meta.catalog_entry_id.clone(),
                            meta.entity.as_str().to_string(),
                            capability.as_str().to_string(),
                            meta.wire_name.clone(),
                        ),
                        sym.clone(),
                    );
                }
            }
        }
        for meta in self.slot_occurrence_meta.values() {
            let fp = slot_allocation_fingerprint(meta);
            let Some(sym) = self.slot_fingerprint_to_sym.get(&fp) else {
                continue;
            };
            match &meta.role {
                IdentRole::EntityField => {
                    self.entity_field_to_sym.insert(
                        (
                            meta.catalog_entry_id.clone(),
                            meta.entity.as_str().to_string(),
                            meta.wire_name.clone(),
                        ),
                        sym.clone(),
                    );
                }
                IdentRole::Relation { .. } => {
                    self.relation_to_sym.insert(
                        (
                            meta.catalog_entry_id.clone(),
                            meta.entity.as_str().to_string(),
                            meta.wire_name.clone(),
                        ),
                        sym.clone(),
                    );
                }
                IdentRole::CapabilityParam { capability } => {
                    self.cap_param_to_sym.insert(
                        (
                            meta.catalog_entry_id.clone(),
                            meta.entity.as_str().to_string(),
                            capability.as_str().to_string(),
                            meta.wire_name.clone(),
                        ),
                        sym.clone(),
                    );
                }
            }
        }
    }

    fn rebuild_anchor_scoped_method_labels(&mut self, cgs_layers: &[&CGS]) {
        let _ = cgs_layers;
        self.anchor_scoped_method_sym.clear();
        let full_refs: Vec<&str> = self.entities.iter().map(|s| s.as_str()).collect();
        let full_set: HashSet<&str> = full_refs.iter().copied().collect();
        for i in 0..self.entities.len() {
            let dom = self.entities[i].as_str();
            if !full_set.contains(dom) {
                continue;
            }
            let entry_id = self.entity_catalog_entry_ids[i].as_str();
            let Some(cgs) = self.catalog_cgs.get(entry_id) else {
                continue;
            };
            let Some(names) = cgs.capability_names_by_domain().get(dom) else {
                continue;
            };
            for cap_name in names {
                let Some(cap) = cgs.capabilities.get(cap_name) else {
                    continue;
                };
                if cap.kind != CapabilityKind::Create {
                    continue;
                }
                let pvars =
                    crate::schema::path_var_names_from_mapping_json(&cap.mapping.template.0);
                if pvars.len() != 1 {
                    continue;
                }
                let pv = pvars[0].as_str();
                let Some(anchor_lower) = pv.strip_suffix("_id") else {
                    continue;
                };
                let Some(anchor_entity) = cgs
                    .entities
                    .keys()
                    .find(|e| e.as_str().to_lowercase() == anchor_lower)
                else {
                    continue;
                };
                let anchor = anchor_entity.as_str();
                if !full_set.contains(anchor) {
                    continue;
                }
                if cap.domain.as_str() == anchor {
                    continue;
                }
                let Some(is) = cap.input_schema.as_ref() else {
                    continue;
                };
                let InputType::Object { fields, .. } = &is.input_type else {
                    continue;
                };
                let req: Vec<_> = fields.iter().filter(|f| f.required).collect();
                if req.len() != 1 || req[0].field_type != FieldType::String {
                    continue;
                }
                let kebab = capability_method_label_kebab(cap);
                let Some(sym) = self.method_to_sym.get(&(
                    entry_id.to_string(),
                    cap.domain.to_string(),
                    kebab.clone(),
                )) else {
                    continue;
                };
                self.anchor_scoped_method_sym
                    .insert((anchor.to_string(), sym.clone()), kebab);
            }
        }
    }

    fn build_symbol_map_snapshot(&self) -> SymbolMap {
        SymbolMap {
            sym_to_entity: self.sym_to_entity.clone(),
            qualified_entity_to_sym: self.qualified_entity_to_sym.clone(),
            sym_to_method: self.sym_to_method.clone(),
            method_to_sym: self.method_to_sym.clone(),
            anchor_scoped_method_sym: self.anchor_scoped_method_sym.clone(),
            sym_to_ident: self.sym_to_ident.clone(),
            ident_to_sym: self.ident_to_sym.clone(),
            entity_field_to_sym: self.entity_field_to_sym.clone(),
            relation_to_sym: self.relation_to_sym.clone(),
            cap_param_to_sym: self.cap_param_to_sym.clone(),
        }
    }

    /// [`IdentMetadata`] for `full_entities`, aligned with this session’s slot table (avoids a second CGS walk).
    pub(crate) fn ident_metadata_for_exposure_entities(
        &self,
        full_entities: &[&str],
    ) -> HashMap<IdentMetaKey, IdentMetadata> {
        let set: HashSet<&str> = full_entities.iter().copied().collect();
        let mut out = HashMap::new();
        for meta in self.slot_occurrence_meta.values() {
            if !set.contains(meta.entity.as_str()) {
                continue;
            }
            let k = (
                meta.catalog_entry_id.clone(),
                meta.entity.clone(),
                meta.wire_name.clone(),
            );
            out.entry(k).or_insert_with(|| meta.clone());
        }
        out
    }

    /// Shared [`SymbolMap`] for this exposure session (memoized until the next [`Self::expose_entities`]).
    pub fn symbol_map_arc(&self) -> Arc<SymbolMap> {
        self.symbol_map_arc_cross(None, None).0
    }

    /// Like [`Self::symbol_map_arc`], plus an optional process-wide LRU keyed by [`SymbolMapCacheKey`]
    /// (same schema + exposure rows → reuse the snapshot across HTTP/MCP sessions).
    ///
    /// Second return is `Some(true)` / `Some(false)` when the cross-request LRU was consulted
    /// (cache hit vs miss); `None` when this call used session-local memo or built without cross cache.
    pub fn symbol_map_arc_cross(
        &self,
        cross: Option<&SymbolMapCrossRequestCache>,
        key: Option<SymbolMapCacheKey>,
    ) -> (Arc<SymbolMap>, Option<bool>) {
        {
            let r = self
                .symbol_map_cache
                .read()
                .expect("symbol_map_cache lock poisoned");
            if let Some(arc) = r.as_ref() {
                return (Arc::clone(arc), None);
            }
        }
        let (built, lru_hit) = if let (Some(cache), Some(k)) = (cross, key) {
            if cache.is_enabled() {
                let (arc, hit) =
                    cache.get_or_insert_tracked(k, || self.build_symbol_map_snapshot());
                (arc, Some(hit))
            } else {
                (Arc::new(self.build_symbol_map_snapshot()), None)
            }
        } else {
            (Arc::new(self.build_symbol_map_snapshot()), None)
        };
        let mut w = self
            .symbol_map_cache
            .write()
            .expect("symbol_map_cache lock poisoned");
        *w = Some(Arc::clone(&built));
        (built, lru_hit)
    }

    /// Snapshot for [`expand_path_symbols`] — matches DOMAIN lines for this session.
    pub fn to_symbol_map(&self) -> SymbolMap {
        (*self.symbol_map_arc()).clone()
    }

    /// Registry `entry_id` for an exposed **entity name** (aligned with `e#` / DOMAIN table order).
    ///
    /// In federated sessions, each exposed row is tied to one loaded catalog; this is the
    /// authoritative owning id for that symbol row. Returns `None` if `entity` is not in
    /// [`Self::entities`].
    pub fn catalog_entry_id_for_entity(&self, entity: &str) -> Option<&str> {
        self.entities
            .iter()
            .zip(self.entity_catalog_entry_ids.iter())
            .find(|(e, _)| e.as_str() == entity)
            .map(|(_, id)| id.as_str())
    }
}

fn hash_exposure_session_rows(exposure: &DomainExposureSession) -> u64 {
    let mut h = DefaultHasher::new();
    for (e, row) in exposure
        .entities
        .iter()
        .zip(&exposure.entity_catalog_entry_ids)
    {
        e.hash(&mut h);
        row.hash(&mut h);
    }
    h.finish()
}

/// Fingerprint for [`SymbolMapCrossRequestCache`]: pinned catalogs + exposed entity rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SymbolMapCacheKey {
    catalogs_fingerprint: u64,
    exposure_fingerprint: u64,
}

/// Cache key for a single-catalog session (`entry_id` + [`CGS::catalog_cgs_hash_hex`] + exposure rows).
pub fn symbol_map_cache_key_single_catalog(
    cgs: &CGS,
    exposure: &DomainExposureSession,
) -> SymbolMapCacheKey {
    let mut ch = DefaultHasher::new();
    cgs.entry_id.as_deref().unwrap_or("").hash(&mut ch);
    cgs.catalog_cgs_hash_hex().hash(&mut ch);
    SymbolMapCacheKey {
        catalogs_fingerprint: ch.finish(),
        exposure_fingerprint: hash_exposure_session_rows(exposure),
    }
}

/// Cache key when expression parse spans multiple [`CGS`] layers (federation).
pub fn symbol_map_cache_key_federated(
    layers: &[&CGS],
    exposure: &DomainExposureSession,
) -> SymbolMapCacheKey {
    let mut parts: Vec<String> = layers
        .iter()
        .map(|c| {
            format!(
                "{}:{}",
                c.entry_id.as_deref().unwrap_or(""),
                c.catalog_cgs_hash_hex()
            )
        })
        .collect();
    parts.sort();
    let mut ch = DefaultHasher::new();
    for p in &parts {
        p.hash(&mut ch);
    }
    SymbolMapCacheKey {
        catalogs_fingerprint: ch.finish(),
        exposure_fingerprint: hash_exposure_session_rows(exposure),
    }
}

/// Cross-request LRU of [`SymbolMap`] snapshots (bounded; disabled when capacity is `0`).
pub struct SymbolMapCrossRequestCache {
    cap: usize,
    inner: RwLock<IndexMap<SymbolMapCacheKey, Arc<SymbolMap>>>,
}

impl std::fmt::Debug for SymbolMapCrossRequestCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolMapCrossRequestCache")
            .field("cap", &self.cap)
            .finish_non_exhaustive()
    }
}

impl SymbolMapCrossRequestCache {
    pub const ENV_CAP: &'static str = "PLASM_SYMBOL_MAP_LRU_CAP";

    pub fn new(capacity: usize) -> Self {
        Self {
            cap: capacity,
            inner: RwLock::new(IndexMap::new()),
        }
    }

    pub fn from_env() -> Self {
        let cap = std::env::var(Self::ENV_CAP)
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .unwrap_or(64);
        Self::new(cap)
    }

    pub fn is_enabled(&self) -> bool {
        self.cap > 0
    }

    /// Remove all cached [`SymbolMap`] snapshots (e.g. after replacing API catalog plugins on disk).
    pub fn clear(&self) {
        let mut map = self
            .inner
            .write()
            .expect("SymbolMapCrossRequestCache lock poisoned");
        map.clear();
    }

    pub fn get_or_insert(
        &self,
        key: SymbolMapCacheKey,
        build: impl FnOnce() -> SymbolMap,
    ) -> Arc<SymbolMap> {
        self.get_or_insert_tracked(key, build).0
    }

    /// Returns `(snapshot, cache_hit)` where `cache_hit` is true iff an existing LRU entry was reused.
    pub fn get_or_insert_tracked(
        &self,
        key: SymbolMapCacheKey,
        build: impl FnOnce() -> SymbolMap,
    ) -> (Arc<SymbolMap>, bool) {
        if !self.is_enabled() {
            return (Arc::new(build()), false);
        }
        let mut map = self
            .inner
            .write()
            .expect("SymbolMapCrossRequestCache lock poisoned");
        if let Some(arc) = map.shift_remove(&key) {
            map.insert(key, Arc::clone(&arc));
            return (arc, true);
        }
        let arc = Arc::new(build());
        while map.len() >= self.cap {
            let Some(k) = map.keys().next().cloned() else {
                break;
            };
            map.shift_remove(&k);
        }
        map.insert(key, Arc::clone(&arc));
        (arc, false)
    }
}

/// Expand using a [`DomainExposureSession`] snapshot (HTTP execute / MCP); ignores [`FocusSpec`]. When `symbol_tuning` is false, only annotation stripping runs (canonical / tests).
pub fn expand_expr_for_domain_session(
    input: &str,
    session: &DomainExposureSession,
    symbol_tuning: bool,
) -> String {
    let stripped = strip_prompt_expression_annotations(input);
    if !symbol_tuning {
        return stripped;
    }
    let map = session.symbol_map_arc();
    expand_path_symbols(&stripped, map.as_ref())
}

/// Strip human-only suffixes from pasted prompt examples (`;;` comment may include `=>` result type,
/// legacy `=>` before `;;`, `->` relation target hint).
pub fn strip_prompt_expression_annotations(input: &str) -> String {
    let trimmed = input.trim();
    // Expression is always before the first `;;` (result type now lives inside the comment).
    let no_cap = trimmed.split("  ;;  ").next().unwrap_or(trimmed).trim();
    // Legacy lines: `expr  =>  [e#]  ;;  …`
    let no_gloss = no_cap
        .rsplit_once("  =>  ")
        .map(|(a, _)| a.trim())
        .unwrap_or(no_cap);
    let no_or = no_gloss.split(" or ").next().unwrap_or(no_gloss).trim();
    let expr_only = no_or
        .split_once(" -> ")
        .map(|(a, _)| a.trim())
        .unwrap_or(no_or);
    expr_only.to_string()
}

/// Rebuild-or-skip expansion for interactive / HTTP paths (reconstructs the map each call). `symbol_tuning` matches [`crate::prompt_render::RenderConfig::uses_symbols`].
pub fn expand_expr_for_parse(
    input: &str,
    cgs: &CGS,
    focus: FocusSpec<'_>,
    symbol_tuning: bool,
) -> String {
    let stripped = strip_prompt_expression_annotations(input);
    if !symbol_tuning {
        return stripped;
    }
    let exposure = domain_exposure_session_from_focus(cgs, focus);
    expand_expr_for_domain_session(&stripped, &exposure, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load_schema_dir;

    #[test]
    fn slot_allocation_fingerprint_splits_same_wire_different_field_types() {
        let en = EntityName::from("N".to_string());
        let meta = |ft: FieldType| IdentMetadata {
            catalog_entry_id: String::new(),
            field_type: ft,
            string_semantics: None,
            array_items: None,
            allowed_values: None,
            role: IdentRole::EntityField,
            wire_name: "id".into(),
            description: "same desc".into(),
            entity: en.clone(),
        };
        assert_ne!(
            slot_allocation_fingerprint(&meta(FieldType::Integer)),
            slot_allocation_fingerprint(&meta(FieldType::String)),
        );
        let mut a = meta(FieldType::Integer);
        a.entity = EntityName::from("Alpha".to_string());
        let mut b = meta(FieldType::Integer);
        b.entity = EntityName::from("Beta".to_string());
        assert_ne!(
            slot_allocation_fingerprint(&a),
            slot_allocation_fingerprint(&b),
            "same-shaped field on different entities must not share a p# fingerprint"
        );
    }

    #[test]
    fn overshow_entity_scoped_slot_maps_split_incompatible_id_slots() {
        let dir = std::path::Path::new("../../fixtures/schemas/overshow_tools");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let map = symbol_map_for_prompt(&cgs, FocusSpec::All, true).expect("symbol map");
        let capture_item_id = map.ident_sym_entity_field("CaptureItem", "id");
        let profile_id = map.ident_sym_entity_field("Profile", "id");
        let pipeline_snapshot_id = map.ident_sym_entity_field("PipelineSnapshot", "id");
        assert_ne!(
            capture_item_id, profile_id,
            "same-shaped `id` fields on different entities must not share one p#"
        );
        assert_ne!(
            capture_item_id, pipeline_snapshot_id,
            "entity-scoped lookup must not fall back to the wrong legacy bare-name `id` symbol"
        );
    }

    #[test]
    fn overshow_unambiguous_ident_lookup_rejects_ambiguous_id_name() {
        let dir = std::path::Path::new("../../fixtures/schemas/overshow_tools");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let map = symbol_map_for_prompt(&cgs, FocusSpec::All, true).expect("symbol map");
        assert_eq!(
            map.ident_sym_unambiguous("workers"),
            Some(map.ident_sym_entity_field("PipelineSnapshot", "workers"))
        );
        assert_eq!(
            map.ident_sym_unambiguous("id"),
            None,
            "bare `id` should not collapse to a single p# when both int and str id slots exist"
        );
    }

    #[test]
    fn domain_exposure_session_keeps_entity_symbols_stable_across_waves() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let mut s = DomainExposureSession::new(&cgs, "", &["Pet"]);
        let pet_sym = s.to_symbol_map().entity_sym("Pet");
        s.expose_entities(&[&cgs], Arc::new(cgs.clone()), "", &["Store"]);
        assert_eq!(pet_sym, s.to_symbol_map().entity_sym("Pet"));
        assert_ne!(pet_sym, s.to_symbol_map().entity_sym("Store"));
    }

    /// `m#` / `p#` append-only invariants: adding a second entity must not renumber existing method
    /// or field slot symbols for the first entity.
    #[test]
    fn domain_exposure_session_keeps_method_and_field_symbols_stable_across_waves() {
        let dir = std::path::Path::new("../../fixtures/schemas/overshow_tools");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let mut s = DomainExposureSession::new(&cgs, "", &["Profile"]);
        let map0 = s.to_symbol_map();
        let display_p = map0.ident_sym_entity_field("Profile", "display_name");
        let get_m = map0.method_sym("Profile", "get");
        s.expose_entities(&[&cgs], Arc::new(cgs.clone()), "", &["RecordedContent"]);
        let map1 = s.to_symbol_map();
        assert_eq!(
            map1.ident_sym_entity_field("Profile", "display_name"),
            display_p
        );
        assert_eq!(map1.method_sym("Profile", "get"), get_m);
    }

    #[test]
    fn ident_metadata_from_exposure_matches_build_ident_metadata() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let sesh = DomainExposureSession::new(&cgs, "", &["Pet", "Store"]);
        let full_refs: Vec<&str> = sesh.entities.iter().map(|s| s.as_str()).collect();
        let from_exp = sesh.ident_metadata_for_exposure_entities(&full_refs);
        let mut from_build = HashMap::new();
        for &e in &full_refs {
            from_build.extend(build_ident_metadata(&cgs, &[e]));
        }
        assert_eq!(from_exp, from_build);
    }

    #[test]
    fn symbol_map_cross_request_cache_reuses_snapshot() {
        let cache = SymbolMapCrossRequestCache::new(8);
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let exp = DomainExposureSession::new(&cgs, "", &["Pet"]);
        let key = symbol_map_cache_key_single_catalog(&cgs, &exp);
        let (a, h1) = exp.symbol_map_arc_cross(Some(&cache), Some(key));
        assert_eq!(h1, Some(false));
        let exp2 = DomainExposureSession::new(&cgs, "", &["Pet"]);
        let (b, h2) = exp2.symbol_map_arc_cross(Some(&cache), Some(key));
        assert_eq!(h2, Some(true));
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn symbol_map_cross_request_cache_clear_drops_lru_entries() {
        let cache = SymbolMapCrossRequestCache::new(8);
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let exp = DomainExposureSession::new(&cgs, "", &["Pet"]);
        let key = symbol_map_cache_key_single_catalog(&cgs, &exp);
        let (_, h1) = exp.symbol_map_arc_cross(Some(&cache), Some(key));
        assert_eq!(h1, Some(false));
        cache.clear();
        let exp2 = DomainExposureSession::new(&cgs, "", &["Pet"]);
        let (_, h2) = exp2.symbol_map_arc_cross(Some(&cache), Some(key));
        assert_eq!(h2, Some(false));
    }

    #[test]
    fn petstore_roundtrip_expand() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let (full, _) = entity_slices_for_render(&cgs, FocusSpec::All);
        let map = SymbolMap::build(&cgs, &full);
        let expr = format!("{}(42)", map.entity_sym("Pet"));
        let back = expand_path_symbols(&expr, &map);
        assert_eq!(back, "Pet(42)");
    }

    #[test]
    fn method_expand_requires_entity_context() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let (full, _) = entity_slices_for_render(&cgs, FocusSpec::All);
        let map = SymbolMap::build(&cgs, &full);
        let pet = map.entity_sym("Pet");
        let m = map.method_sym("Pet", "upload-image");
        if m == "upload-image" {
            return;
        }
        let expr = format!("{}(1).{}()", pet, m);
        let back = expand_path_symbols(&expr, &map);
        assert!(back.contains("upload-image"), "got {back}");
    }

    #[test]
    fn field_syms_in_expr_order() {
        assert_eq!(
            field_syms_in_expr(r#"e1(42).m22(p37="x",p18=1)"#),
            vec!["p37".to_string(), "p18".to_string()]
        );
        assert_eq!(
            field_syms_in_expr("e4{p61=e1(42)}"),
            vec!["p61".to_string()]
        );
        assert!(field_syms_in_expr("e1(42)").is_empty());
    }

    #[test]
    fn field_syms_for_domain_line_includes_optional_from_legend() {
        let line = r#"e1(42).m22(p37=$,..)  ;;  optional params: p18, p17 — Create a goal"#;
        assert_eq!(
            field_syms_for_domain_line(line),
            vec!["p37".to_string(), "p18".to_string(), "p17".to_string(),]
        );
    }

    #[test]
    fn strip_prompt_annotations_result_inside_comment() {
        assert_eq!(
            strip_prompt_expression_annotations("e1  ;;  => [e1]  List all accessible workspaces"),
            "e1"
        );
        assert_eq!(
            strip_prompt_expression_annotations(
                "e6{p26=e5(42), p1=true}  ;;  => [e6]  optional params: p1 — List lists"
            ),
            "e6{p26=e5(42), p1=true}"
        );
    }

    #[test]
    fn strip_prompt_annotations_legacy_result_before_comment() {
        assert_eq!(
            strip_prompt_expression_annotations("e1  =>  [e1]  ;;  List all accessible workspaces"),
            "e1"
        );
    }

    #[test]
    fn render_gloss_capability_param_uses_wire_name_without_description() {
        let m = IdentMetadata {
            catalog_entry_id: String::new(),
            field_type: FieldType::String,
            string_semantics: None,
            array_items: None,
            allowed_values: None,
            role: IdentRole::CapabilityParam {
                capability: CapabilityName::from("test_cap".to_string()),
            },
            wire_name: "payment_method_id".to_string(),
            description: String::new(),
            entity: EntityName::from("Order".to_string()),
        };
        assert_eq!(m.render_gloss(None), "str · payment_method_id");
    }

    #[test]
    fn render_gloss_capability_param_uses_description_when_set() {
        let m = IdentMetadata {
            catalog_entry_id: String::new(),
            field_type: FieldType::String,
            string_semantics: None,
            array_items: None,
            allowed_values: None,
            role: IdentRole::CapabilityParam {
                capability: CapabilityName::from("test_cap".to_string()),
            },
            wire_name: "payment_method_id".to_string(),
            description: "Payment method".to_string(),
            entity: EntityName::from("Order".to_string()),
        };
        assert_eq!(m.render_gloss(None), "str · Payment method");
    }

    #[test]
    fn render_gloss_string_semantics_markdown_replaces_str_label() {
        let m = IdentMetadata {
            catalog_entry_id: String::new(),
            field_type: FieldType::String,
            string_semantics: Some(StringSemantics::Markdown),
            array_items: None,
            allowed_values: None,
            role: IdentRole::CapabilityParam {
                capability: CapabilityName::from("test_cap".to_string()),
            },
            wire_name: "body".to_string(),
            description: String::new(),
            entity: EntityName::from("Issue".to_string()),
        };
        assert_eq!(m.render_gloss(None), "markdown · body");
    }

    #[test]
    fn render_gloss_array_param_shows_element_type() {
        let m = IdentMetadata {
            catalog_entry_id: String::new(),
            field_type: FieldType::Array,
            string_semantics: None,
            array_items: Some(ArrayItemsSchema {
                field_type: FieldType::EntityRef {
                    target: EntityName::from("Variant".to_string()),
                },
                value_format: None,
                allowed_values: None,
            }),
            allowed_values: None,
            role: IdentRole::CapabilityParam {
                capability: CapabilityName::from("exchange_delivered_order_items".to_string()),
            },
            wire_name: "item_ids".to_string(),
            description: String::new(),
            entity: EntityName::from("Order".to_string()),
        };
        assert_eq!(m.render_gloss(None), "array[ref:Variant] · item_ids");
    }

    #[test]
    fn render_gloss_select_shows_allowed_values_not_wire_name() {
        let m = IdentMetadata {
            catalog_entry_id: String::new(),
            field_type: FieldType::Select,
            string_semantics: None,
            array_items: None,
            allowed_values: Some(vec![
                "completed".to_string(),
                "reopened".to_string(),
                "not_planned".to_string(),
                "duplicate".to_string(),
            ]),
            role: IdentRole::EntityField,
            wire_name: "state_reason".to_string(),
            description: String::new(),
            entity: EntityName::from("Issue".to_string()),
        };
        assert_eq!(
            m.render_gloss(None),
            "select · completed, reopened, not_planned, duplicate"
        );
    }

    #[test]
    fn build_ident_metadata_includes_scalar_kinds() {
        let dir = std::path::Path::new("../../apis/clickup");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let (full, _) = entity_slices_for_render(&cgs, FocusSpec::All);
        let meta = build_ident_metadata(&cgs, &full);
        assert!(
            meta.values()
                .any(|m| matches!(m.field_type, crate::FieldType::Date)),
            "expected Date field type in metadata"
        );
        assert!(
            meta.values()
                .any(|m| matches!(m.field_type, crate::FieldType::Boolean)),
            "expected Boolean field type in metadata"
        );
    }

    #[test]
    fn domain_term_entity_roundtrips_display_with_symbol_map() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let (full, _) = entity_slices_for_render(&cgs, FocusSpec::All);
        let map = SymbolMap::build(&cgs, &full);
        let dt = map.try_entity_domain_term("Pet").expect("Pet in map");
        assert_eq!(dt.to_string(), map.entity_sym("Pet"));
        assert!(matches!(dt, crate::DomainTerm::Entity(_, _)));
    }

    #[test]
    fn domain_term_method_matches_symbol_map_when_cgs_resolves() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).unwrap();
        let (full, _) = entity_slices_for_render(&cgs, FocusSpec::All);
        let map = SymbolMap::build(&cgs, &full);
        let kebab = "upload-image";
        let m_str = map.method_sym("Pet", kebab);
        if m_str == kebab {
            return;
        }
        let dt = map
            .try_method_domain_term(&cgs, "Pet", kebab)
            .expect("method domain term");
        assert_eq!(dt.to_string(), m_str);
        assert!(matches!(dt, crate::DomainTerm::Method(_, _)));
    }

    #[test]
    fn args_line_suppressible_marks_select_plus_for_extra_gloss() {
        let line = "e1.m1()  ;;  args: p1 x str req; p2 y select+ opt — d";
        let m = super::args_line_suppressible_capability_syms(line).expect("m");
        assert_eq!(m.get("p1"), Some(&true));
        assert_eq!(m.get("p2"), Some(&false));
    }

    #[test]
    fn federation_duplicate_entity_name_allocates_distinct_e_symbols() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let mut cgs_a = load_schema_dir(dir).unwrap();
        cgs_a.entry_id = Some("alpha".into());
        let mut cgs_b = cgs_a.clone();
        cgs_b.entry_id = Some("beta".into());
        let arc_b = std::sync::Arc::new(cgs_b);
        let mut s = DomainExposureSession::new(&cgs_a, "alpha", &["Pet"]);
        s.expose_entities(&[arc_b.as_ref()], arc_b.clone(), "beta", &["Pet"]);
        assert_eq!(s.entities.len(), 2);
        let map = s.to_symbol_map();
        let sa = map
            .qualified_entity_to_sym
            .get(&("alpha".into(), "Pet".into()))
            .expect("alpha Pet");
        let sb = map
            .qualified_entity_to_sym
            .get(&("beta".into(), "Pet".into()))
            .expect("beta Pet");
        assert_ne!(sa, sb);
    }
}
