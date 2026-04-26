//! Map CGS + [`plasm_core::DomainExposureSession`] into [`crate::delta`] and TypeScript strings.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as FmtWrite;

use indexmap::IndexMap;
use plasm_core::schema::CapabilityKind;
use plasm_core::value::FieldType;
use plasm_core::CgsContext;
use plasm_core::DomainExposureSession;
use plasm_core::FieldSchema;
use plasm_core::InputFieldSchema;
use plasm_core::InputType;
use plasm_core::OutputType;
use plasm_core::CGS;
use serde::Serialize;
use std::sync::Arc;

use crate::delta::{
    CatalogAliasRecord, ExposedSet, FacadeCapability, FacadeDeltaV1, FacadeField,
    FacadeInputParameter, FacadeInvokePreflight, FacadeOutputSurface, FacadeRelation,
    FieldTypeName, QualifiedEntitySurface, TypeScriptCodeArtifacts,
};

/// Inputs for a single `add_code_capabilities` wave.
#[derive(Debug, Clone, Serialize)]
pub struct FacadeGenRequest {
    pub new_symbol_space: bool,
    /// Pairs to consider for *this* invocation: typically `seed_pairs` from the waves.
    pub seed_pairs: Vec<(String, String)>,
    /// Cumulative (entry_id, entity) the client has already in declared TS.
    pub already_emitted: ExposedSet,
    /// Push prelude when the logical code session is fresh.
    pub emit_prelude: bool,
}

/// `entry_id` → safe path segment; stable suffix on collision.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CatalogAliasMap {
    map: BTreeMap<String, String>,
    _inv: BTreeMap<String, String>,
}

impl CatalogAliasMap {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            _inv: BTreeMap::new(),
        }
    }

    pub fn refresh_all<'a, I: IntoIterator<Item = &'a str>>(&mut self, entry_ids: I) {
        self.map.clear();
        self._inv.clear();
        for e in entry_ids {
            self.intern_entry_id(e);
        }
    }

    pub fn intern_entry_id(&mut self, entry_id: &str) -> String {
        if let Some(a) = self.map.get(entry_id) {
            return a.clone();
        }
        let base = to_js_identifier(&slugify_entry_id(entry_id));
        let mut c = 0u32;
        let alias = loop {
            let a = if c == 0 {
                base.clone()
            } else {
                format!("{base}{c}")
            };
            if !self._inv.contains_key(&a) {
                break a;
            }
            c = c.saturating_add(1);
        };
        self._inv.insert(alias.clone(), entry_id.to_string());
        self.map.insert(entry_id.to_string(), alias);
        self.map[entry_id].clone()
    }

    pub fn alias_of(&self, entry_id: &str) -> Option<String> {
        self.map.get(entry_id).cloned()
    }
}

fn slugify_entry_id(s: &str) -> String {
    let t = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let t = t.trim_matches('_');
    if t.is_empty() {
        "api".to_string()
    } else {
        t.to_string()
    }
}

fn to_js_identifier(s: &str) -> String {
    if s.is_empty() {
        return "api".to_string();
    }
    let mut ch = s.chars();
    let first = ch.next().unwrap();
    let mut o = String::new();
    if first.is_ascii_alphabetic() || first == '_' || first == '$' {
        o.push(first);
    } else {
        o.push('_');
        if first.is_ascii_alphanumeric() {
            o.push(first);
        }
    }
    o.extend(ch.map(|c| {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            c
        } else {
            '_'
        }
    }));
    o
}

fn pascal(s: &str) -> String {
    let t = s.replace(['-', '_', '.', '/'], " ");
    let mut out = String::new();
    for w in t.split_whitespace() {
        if w.is_empty() {
            continue;
        }
        let mut c = w.chars();
        if let Some(f) = c.next() {
            for ch in f.to_uppercase() {
                out.push(ch);
            }
            for ch in c {
                for ch2 in ch.to_lowercase() {
                    out.push(ch2);
                }
            }
        }
    }
    if out.is_empty() {
        "Api".to_string()
    } else if out
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
    {
        out
    } else {
        format!("Api{out}")
    }
}

fn e_index_for(exp: &DomainExposureSession, entry_id: &str, entity: &str) -> Option<usize> {
    for (i, (e, rid)) in exp
        .entities
        .iter()
        .zip(exp.entity_catalog_entry_ids.iter())
        .enumerate()
    {
        if e == entity && rid == entry_id {
            return Some(i + 1);
        }
    }
    None
}

fn map_field_type(ft: &FieldType, f: &FieldSchema) -> (FieldTypeName, Option<String>) {
    let hint = f.value_format.as_ref().map(|v| format!("{v:?}"));
    let name = match ft {
        FieldType::String => {
            if matches!(f.string_semantics, Some(plasm_core::StringSemantics::Blob)) {
                FieldTypeName::Blob
            } else {
                FieldTypeName::String
            }
        }
        FieldType::Number => FieldTypeName::Number,
        FieldType::Integer => FieldTypeName::Integer,
        FieldType::Boolean => FieldTypeName::Boolean,
        FieldType::Uuid => FieldTypeName::Uuid,
        FieldType::Blob => FieldTypeName::Blob,
        FieldType::Date => FieldTypeName::Date,
        FieldType::Array => FieldTypeName::Array,
        FieldType::Json => FieldTypeName::Json,
        FieldType::Select => FieldTypeName::Select,
        FieldType::MultiSelect => FieldTypeName::MultiSelect,
        FieldType::EntityRef { .. } => FieldTypeName::EntityRef,
    };
    (name, hint)
}

fn field_type_name_for_value(ft: &FieldType) -> FieldTypeName {
    match ft {
        FieldType::String => FieldTypeName::String,
        FieldType::Number => FieldTypeName::Number,
        FieldType::Integer => FieldTypeName::Integer,
        FieldType::Boolean => FieldTypeName::Boolean,
        FieldType::Uuid => FieldTypeName::Uuid,
        FieldType::Blob => FieldTypeName::Blob,
        FieldType::Date => FieldTypeName::Date,
        FieldType::Array => FieldTypeName::Array,
        FieldType::Json => FieldTypeName::Json,
        FieldType::Select => FieldTypeName::Select,
        FieldType::MultiSelect => FieldTypeName::MultiSelect,
        FieldType::EntityRef { .. } => FieldTypeName::EntityRef,
    }
}

fn capability_input_parameters(c: &plasm_core::CapabilitySchema) -> Vec<FacadeInputParameter> {
    let Some(is) = c.input_schema.as_ref() else {
        return vec![];
    };
    let InputType::Object { fields, .. } = &is.input_type else {
        return vec![];
    };
    fields.iter().map(input_field_to_param).collect()
}

fn input_field_to_param(f: &InputFieldSchema) -> FacadeInputParameter {
    let array_item_type = f
        .array_items
        .as_ref()
        .map(|a| format!("{:?}", a.field_type));
    let description = f.description.as_ref().and_then(|d| {
        let t = d.trim();
        (!t.is_empty()).then(|| d.clone())
    });
    FacadeInputParameter {
        name: f.name.clone(),
        r#type: field_type_name_for_value(&f.field_type),
        required: f.required,
        entity_ref_target: match &f.field_type {
            FieldType::EntityRef { target } => Some(target.to_string()),
            _ => None,
        },
        role: f.role.map(|r| format!("{r:?}")),
        allowed_values: f.allowed_values.clone(),
        array_item_type,
        description,
    }
}

fn capability_output_surface(c: &plasm_core::CapabilitySchema) -> Option<FacadeOutputSurface> {
    let os = c.output_schema.as_ref()?;
    Some(match &os.output_type {
        OutputType::Entity { entity_type } => FacadeOutputSurface {
            type_tag: "entity".to_string(),
            entity_type: Some(entity_type.to_string()),
        },
        OutputType::Collection { entity_type, .. } => FacadeOutputSurface {
            type_tag: "collection".to_string(),
            entity_type: Some(entity_type.to_string()),
        },
        OutputType::SideEffect { .. } => FacadeOutputSurface {
            type_tag: "side_effect".to_string(),
            entity_type: None,
        },
        OutputType::Status { .. } => FacadeOutputSurface {
            type_tag: "status".to_string(),
            entity_type: None,
        },
        OutputType::Custom { .. } => FacadeOutputSurface {
            type_tag: "custom".to_string(),
            entity_type: None,
        },
    })
}

fn find_entitydef<'a>(cgs: &'a CGS, entity: &str) -> Option<(&'a plasm_core::EntityDef, &'a str)> {
    cgs.entities
        .get_key_value(entity)
        .map(|(n, d)| (d, n.as_str()))
        .or_else(|| {
            cgs.entities.iter().find_map(|(n, d)| {
                d.expression_aliases
                    .iter()
                    .any(|a| a == entity)
                    .then_some((d, n.as_str()))
            })
        })
}

fn build_entity_surface(
    cgs: &CGS,
    exp: &DomainExposureSession,
    entry_id: &str,
    entity: &str,
    cat_alias: &str,
) -> Option<QualifiedEntitySurface> {
    let (edef, ename) = find_entitydef(cgs, entity)?;
    let eidx = e_index_for(exp, entry_id, ename);
    let mut fields: Vec<FacadeField> = Vec::new();
    for (n, f) in &edef.fields {
        let (t, vfmt) = map_field_type(&f.field_type, f);
        let select_values = if matches!(f.field_type, FieldType::Select | FieldType::MultiSelect) {
            f.allowed_values.clone()
        } else {
            None
        };
        let entity_ref_target = match &f.field_type {
            FieldType::EntityRef { target } => Some(target.to_string()),
            _ => None,
        };
        fields.push(FacadeField {
            name: n.to_string(),
            description: non_empty_description(f.description.as_str()),
            r#type: t,
            required: f.required,
            value_format: vfmt,
            select_values,
            entity_ref_target,
        });
    }
    let mut rels: Vec<FacadeRelation> = Vec::new();
    for (n, r) in &edef.relations {
        rels.push(FacadeRelation {
            name: n.to_string(),
            description: non_empty_description(r.description.as_str()),
            target: r.target_resource.to_string(),
            cardinality: match r.cardinality {
                plasm_core::schema::Cardinality::One => "one",
                plasm_core::schema::Cardinality::Many => "many",
            }
            .to_string(),
            materialize: r.materialize.as_ref().map(|m| format!("{m:?}")),
        });
    }
    let mut caps: Vec<FacadeCapability> = Vec::new();
    for (cn, c) in &cgs.capabilities {
        if c.domain.as_str() != ename {
            continue;
        }
        let (ec, rs) = cap_effect_and_shape(c);
        let is_ack = c.kind == CapabilityKind::Action
            && c.output_schema
                .as_ref()
                .is_some_and(|o| matches!(&o.output_type, OutputType::SideEffect { .. }));
        caps.push(FacadeCapability {
            name: cn.to_string(),
            description: non_empty_description(c.description.as_str()),
            kind: cap_kind_name(c.kind),
            effect_class: ec,
            result_shape: rs,
            provides: c.provides.clone(),
            is_side_effect_ack: is_ack,
            input_parameters: capability_input_parameters(c),
            output: capability_output_surface(c),
            invoke_preflight: c.invoke_preflight.as_ref().map(|p| FacadeInvokePreflight {
                hydrate_capability: p.hydrate_capability.to_string(),
                env_prefix: p.env_prefix.clone(),
            }),
        });
    }
    Some(QualifiedEntitySurface {
        entry_id: entry_id.to_string(),
        catalog_alias: cat_alias.to_string(),
        entity: ename.to_string(),
        description: non_empty_description(edef.description.as_str()),
        e_index: eidx,
        key_vars: edef.key_vars.iter().map(|k| k.to_string()).collect(),
        fields,
        relations: rels,
        capabilities: caps,
    })
}

fn cap_kind_name(k: CapabilityKind) -> String {
    match k {
        CapabilityKind::Query => "query",
        CapabilityKind::Search => "search",
        CapabilityKind::Get => "get",
        CapabilityKind::Create => "create",
        CapabilityKind::Update => "update",
        CapabilityKind::Delete => "delete",
        CapabilityKind::Action => "action",
    }
    .to_string()
}

fn cap_effect_and_shape(c: &plasm_core::CapabilitySchema) -> (String, String) {
    use CapabilityKind::*;
    match c.kind {
        Query | Search => ("read".to_string(), "list".to_string()),
        Get => ("read".to_string(), "single".to_string()),
        Create | Update | Delete => ("write".to_string(), "mutation_result".to_string()),
        Action => {
            if c.output_schema
                .as_ref()
                .is_some_and(|o| matches!(&o.output_type, OutputType::SideEffect { .. }))
                || c.provides.is_empty()
            {
                ("side_effect".to_string(), "side_effect_ack".to_string())
            } else {
                ("write".to_string(), "mutation_result".to_string())
            }
        }
    }
}

fn field_type_to_ts(f: &FacadeField, cat_alias: &str) -> String {
    match f.r#type {
        FieldTypeName::String
        | FieldTypeName::Uuid
        | FieldTypeName::Date
        | FieldTypeName::Select => "string".to_string(),
        FieldTypeName::Json => "unknown".to_string(),
        FieldTypeName::Number | FieldTypeName::Integer => "number".to_string(),
        FieldTypeName::Boolean => "boolean".to_string(),
        FieldTypeName::Blob => "unknown /* blob / attachment */".to_string(),
        FieldTypeName::MultiSelect => "readonly string[]".to_string(),
        FieldTypeName::Array => "unknown[]".to_string(),
        FieldTypeName::EntityRef => {
            let target = f.entity_ref_target.as_deref().unwrap_or("string");
            format!(r#"Plasm.EntityRef<"{cat}", "{target}">"#, cat = cat_alias)
        }
        FieldTypeName::Unknown => "unknown".to_string(),
    }
}

const CODE_MODE_RUNTIME_BOOTSTRAP_REF: &str = "code-mode-quickjs-runtime-v1";

fn non_empty_description(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn jsdoc_comment(s: &Option<String>, indent: &str) -> String {
    let Some(s) = s.as_deref() else {
        return String::new();
    };
    let one_line = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("*/", "* /");
    if one_line.is_empty() {
        String::new()
    } else {
        format!("{indent}/** {one_line} */\n")
    }
}

fn input_param_type_to_ts(param: &FacadeInputParameter, cat_alias: &str) -> String {
    let field = FacadeField {
        name: param.name.clone(),
        description: param.description.clone(),
        r#type: param.r#type,
        required: param.required,
        value_format: None,
        select_values: param.allowed_values.clone(),
        entity_ref_target: param.entity_ref_target.clone(),
    };
    field_type_to_ts(&field, cat_alias)
}

fn search_input_fields_ts(entity: &QualifiedEntitySurface) -> String {
    let Some(cap) = entity.capabilities.iter().find(|c| c.kind == "search") else {
        return "    q: string;\n".to_string();
    };
    if cap.input_parameters.is_empty() {
        return "    q: string;\n".to_string();
    }
    cap.input_parameters
        .iter()
        .map(|param| {
            let q = if param.required { "" } else { "?" };
            let ts = input_param_type_to_ts(param, entity.catalog_alias.as_str());
            format!("    {}{q}: {ts};\n", param.name)
        })
        .collect::<String>()
}

fn capability_input_fields_ts(
    params: &[FacadeInputParameter],
    cat_alias: &str,
    indent: &str,
) -> String {
    params
        .iter()
        .map(|param| {
            let q = if param.required { "" } else { "?" };
            let ts = input_param_type_to_ts(param, cat_alias);
            format!("{indent}{}{q}: {ts};\n", param.name)
        })
        .collect::<String>()
}

fn capability_input_type_alias_ts(
    type_name: &str,
    params: &[FacadeInputParameter],
    cat_alias: &str,
) -> Option<String> {
    if params.is_empty() {
        return None;
    }
    Some(format!(
        "  type {type_name} = {{\n{}  }};\n",
        capability_input_fields_ts(params, cat_alias, "    ")
    ))
}

fn has_required_input(params: &[FacadeInputParameter]) -> bool {
    params.iter().any(|param| param.required)
}

fn input_arg_optional(params: &[FacadeInputParameter]) -> &'static str {
    if has_required_input(params) {
        ""
    } else {
        "?"
    }
}

fn entity_get_key_type_ts(entity: &QualifiedEntitySurface) -> String {
    if entity.key_vars.len() <= 1 {
        "string".to_string()
    } else {
        let fields = entity
            .key_vars
            .iter()
            .map(|key| format!("{}: string", serde_json::Value::String(key.clone())))
            .collect::<Vec<_>>()
            .join("; ");
        format!("string | {{ {fields} }}")
    }
}

fn capability_suffix(cap: &FacadeCapability) -> String {
    pascal(&cap.name)
}

fn relation_methods_ts(
    entity: &QualifiedEntitySurface,
    available_entities: &BTreeSet<String>,
) -> String {
    entity
        .relations
        .iter()
        .filter(|rel| available_entities.contains(rel.target.as_str()))
        .map(|rel| {
            let name = to_js_identifier(rel.name.as_str());
            format!(
                "    /** Follow relation `{}` to `{}` ({}). */\n    {}(): {}NodeHandle;\n",
                rel.name, rel.target, rel.cardinality, name, rel.target
            )
        })
        .collect::<String>()
}

fn action_method_overloads_ts(
    entity: &QualifiedEntitySurface,
    caps: &[&FacadeCapability],
) -> String {
    let mut out = String::new();
    for cap in caps {
        let input_type = if cap.input_parameters.is_empty() {
            "Record<string, unknown>".to_string()
        } else {
            format!("{}{}Input", entity.entity, capability_suffix(cap))
        };
        let optional = input_arg_optional(&cap.input_parameters);
        let cap_name = serde_json::to_string(&cap.name).unwrap_or_else(|_| "\"\"".to_string());
        out.push_str(&format!(
            "action(name: {cap_name}, input{optional}: {input_type}): Plasm.PlanEffect; "
        ));
    }
    if out.is_empty() {
        out.push_str("action(name: string, input?: Record<string, unknown>): Plasm.PlanEffect;");
    }
    out
}

const TS_PRELUDE: &str = r#"declare namespace Plasm {
  export type EntityRef<Api extends string, E extends string, K = string> = K | PlanValueExpr | (PlanValueExpr & {
    readonly __plasmEntityRef: true;
    readonly api: Api;
    readonly entity: E;
    readonly key: K;
  });
  export type PlanBrand<Name extends string> = { readonly __plasmPlanBrands?: readonly Name[] };
  export type PlanSource = PlanBrand<"PlanSource">;
  export type PlanEffect = PlanStep & PlanBrand<"PlanEffect">;
  export type PlanBuilder = PlanSource & PlanBrand<"PlanBuilder">;
  export type BoundPlanHandle = PlanSource & PlanBrand<"BoundPlanHandle">;
  export type PlanValueExpr = PlanBrand<"PlanValueExpr">;
  export type PlanNodeHandle = BoundPlanHandle & { readonly __planNodeId: string };
  export type PlanValue<T = unknown> =
    | { readonly kind: "literal"; readonly value: T }
    | { readonly kind: "helper"; readonly name: string; readonly args?: readonly unknown[]; readonly display?: string }
    | { readonly kind: "symbol"; readonly path: string }
    | { readonly kind: "binding_symbol"; readonly binding: string; readonly path?: readonly string[] }
    | { readonly kind: "node_symbol"; readonly node: string; readonly alias: string; readonly path?: readonly string[]; readonly cardinality?: PlanInputCardinality }
    | { readonly kind: "template"; readonly template: string; readonly input_bindings?: readonly PlanInputBinding[] }
    | { readonly kind: "array"; readonly items: readonly PlanValue[] }
    | { readonly kind: "object"; readonly fields: Record<string, PlanValue> };
  export type PlanInputCardinality = "auto" | "singleton";
  export type RelationCardinality = "one" | "many";
  export type RelationSourceCardinality = "single" | "many" | "runtime_checked_singleton";
  export type PlanInputBinding = { readonly from: string; readonly to: string; readonly node?: string; readonly alias?: string; readonly cardinality?: PlanInputCardinality };
  export type PlanDataInput = { readonly node: string; readonly alias: string; readonly cardinality?: PlanInputCardinality };
  export type PredicateOp = "eq" | "ne" | "lt" | "lte" | "gt" | "gte" | "contains" | "in" | "exists";
  export type PlanPredicate = {
    readonly field_path: readonly string[];
    readonly op: PredicateOp;
    readonly value: PlanValue;
  };
  export type PlanStep = {
    readonly kind: string;
    readonly qualified_entity?: { readonly entry_id: string; readonly entity: string };
    readonly expr?: string;
    readonly expr_template?: string;
    readonly effect_class: "read" | "write" | "side_effect" | "artifact_read";
    readonly result_shape: "list" | "single" | "mutation_result" | "side_effect_ack" | "page" | "artifact";
    readonly projection?: readonly string[];
    readonly predicates?: readonly PlanPredicate[];
    readonly input_bindings?: readonly PlanInputBinding[];
    readonly data?: PlanValue;
    readonly derive_template?: {
      readonly kind: "map" | "data";
      readonly source?: string;
      readonly item_binding?: string;
      readonly inputs?: readonly PlanDataInput[];
      readonly value: PlanValue;
    };
    readonly compute?: {
      readonly source: string;
      readonly op: PlanComputeOp;
      readonly schema: SyntheticResultSchema;
      readonly page_size?: number;
    };
    readonly relation?: {
      readonly source: string;
      readonly relation: string;
      readonly target: { readonly entry_id: string; readonly entity: string };
      readonly cardinality: RelationCardinality;
      readonly source_cardinality: RelationSourceCardinality;
      readonly expr: string;
    };
  };
  export type SyntheticValueKind = "null" | "boolean" | "integer" | "number" | "string" | "array" | "object" | "unknown";
  export type SyntheticResultSchema = {
    readonly entity?: string;
    readonly fields: readonly { readonly name: string; readonly value_kind: SyntheticValueKind; readonly source?: readonly string[] }[];
  };
  export type AggregateFunction = "count" | "sum" | "avg" | "min" | "max";
  export type AggregateSpec = { readonly name: string; readonly function: AggregateFunction; readonly field?: readonly string[] };
  export type PlanComputeOp =
    | { readonly kind: "project"; readonly fields: Record<string, readonly string[]> }
    | { readonly kind: "filter"; readonly predicates: readonly PlanPredicate[] }
    | { readonly kind: "group_by"; readonly key: readonly string[]; readonly aggregates: readonly AggregateSpec[] }
    | { readonly kind: "aggregate"; readonly aggregates: readonly AggregateSpec[] }
    | { readonly kind: "sort"; readonly key: readonly string[]; readonly descending?: boolean }
    | { readonly kind: "limit"; readonly count: number }
    | { readonly kind: "table_from_matrix"; readonly columns: readonly string[]; readonly has_header?: boolean };
  export type FieldPredicateBuilder<T = unknown> = {
    eq(value: T): PlanPredicate;
    ne(value: T): PlanPredicate;
    lt(value: T): PlanPredicate;
    lte(value: T): PlanPredicate;
    gt(value: T): PlanPredicate;
    gte(value: T): PlanPredicate;
    contains(value: T): PlanPredicate;
    in(value: readonly T[]): PlanPredicate;
  };
  export type Symbolic<T = unknown> = T & PlanValueExpr & { readonly __bindingPath: string; readonly __plasmExpr: string };
  export type TemplateValue = PlanValueExpr & { readonly __plasmExpr: string; readonly __planValue: PlanValue; readonly input_bindings: readonly PlanInputBinding[] };
  export type ProjectionValue = Symbolic<unknown>;
  export type PlanReturnSource = PlanNodeHandle | PlanBuilder | PlanEffect;
  export type PlanReturnable = PlanReturnSource | readonly PlanReturnSource[] | Record<string, PlanReturnSource>;
}
declare class Plan {
  static return(value: Plasm.PlanReturnable): string;
  static data(value: unknown): Plasm.PlanNodeHandle;
  static singleton<T extends Plasm.PlanNodeHandle>(source: T): T;
  static map<T, R>(source: Plasm.PlanSource, fn: (item: Plasm.Symbolic<T>) => R): Plasm.PlanNodeHandle;
  static project<T>(source: Plasm.PlanSource, spec: Record<string, (item: Plasm.Symbolic<T>) => Plasm.ProjectionValue> | readonly string[]): Plasm.PlanNodeHandle;
  static filter<T>(source: Plasm.PlanSource, ...predicates: readonly Plasm.PlanPredicate[]): Plasm.PlanNodeHandle;
  /** Aggregates over the full logical source collection. Returned result views may be paged. */
  static aggregate(source: Plasm.PlanSource, aggregates: readonly Plasm.AggregateSpec[]): Plasm.PlanNodeHandle;
  /** Groups the full logical source collection. Returned result views may be paged. */
  static groupBy<T>(source: Plasm.PlanSource, keyFn: (item: Plasm.Symbolic<T>) => Plasm.ProjectionValue): { count(name?: string): Plasm.PlanNodeHandle; aggregate(aggregates: readonly Plasm.AggregateSpec[]): Plasm.PlanNodeHandle };
  static sort<T>(source: Plasm.PlanSource, keyFn: (item: Plasm.Symbolic<T>) => Plasm.ProjectionValue, direction?: "asc" | "desc"): Plasm.PlanNodeHandle;
  /** Semantic truncation of the DAG collection, not ordinary result pagination. */
  static limit(source: Plasm.PlanSource, count: number): Plasm.PlanNodeHandle;
  static table(source: Plasm.PlanSource, spec: { readonly columns: readonly string[]; readonly hasHeader?: boolean }): Plasm.PlanNodeHandle;
}
declare function field<T = unknown>(name: string): Plasm.FieldPredicateBuilder<T>;
declare function daysAgo(days: number): Plasm.PlanValueExpr & { readonly __plasmExpr: string; readonly __planValue: Plasm.PlanValue<string> };
declare function template(strings: TemplateStringsArray, ...values: readonly unknown[]): Plasm.TemplateValue;
declare function forEach<T>(source: Plasm.PlanSource, fn: (item: Plasm.Symbolic<T>) => Plasm.PlanEffect): Plasm.PlanNodeHandle;
"#;

/// Public entry: build `facade_delta` and TypeScript fragments.
pub fn build_code_facade(
    req: &FacadeGenRequest,
    domain_exposure: &DomainExposureSession,
    ctx_by_entry: &IndexMap<String, Arc<CgsContext>>,
) -> (FacadeDeltaV1, TypeScriptCodeArtifacts) {
    // Which pairs are newly declared in this call?
    let to_emit: Vec<(String, String)> = req
        .seed_pairs
        .iter()
        .filter(|(e, o)| {
            if req.new_symbol_space {
                return true;
            }
            !req.already_emitted
                .contains(&(e.to_string(), o.to_string()))
        })
        .cloned()
        .collect();

    if to_emit.is_empty() {
        return (
            FacadeDeltaV1 {
                version: 1,
                catalog_entry_ids: vec![],
                catalog_aliases: vec![],
                qualified_entities: vec![],
                collision_notes: vec![],
            },
            TypeScriptCodeArtifacts {
                agent_prelude: String::new(),
                agent_namespace_body: String::new(),
                agent_loaded_apis: String::new(),
                runtime_bootstrap_ref: Some(CODE_MODE_RUNTIME_BOOTSTRAP_REF.to_string()),
                declarations_unchanged: true,
                added_catalog_aliases: vec![],
            },
        );
    }

    let mut alias_map = CatalogAliasMap::new();
    for k in ctx_by_entry.keys() {
        alias_map.intern_entry_id(k);
    }
    for (e, _) in &to_emit {
        alias_map.intern_entry_id(e);
    }

    let added_catalog: HashSet<String> = to_emit.iter().map(|(a, _)| a.clone()).collect();
    let mut by_ent: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (e, ent) in &to_emit {
        by_ent.entry(ent.clone()).or_default().insert(e.clone());
    }
    let mut collision: Vec<String> = Vec::new();
    for (ent, set) in &by_ent {
        if set.len() > 1 {
            collision.push(format!(
                "entity {ent} appears under multiple entry_ids: {}",
                set.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }

    let mut qsurfaces: Vec<QualifiedEntitySurface> = Vec::new();
    for (entry_id, ent) in &to_emit {
        if let Some(ctx) = ctx_by_entry.get(entry_id) {
            let cgs: &CGS = ctx.cgs.as_ref();
            let al = alias_map
                .alias_of(entry_id)
                .unwrap_or_else(|| "api".to_string());
            if let Some(s) = build_entity_surface(cgs, domain_exposure, entry_id, ent, al.as_str())
            {
                qsurfaces.push(s);
            }
        }
    }
    qsurfaces.sort_by(|a, b| a.entry_id.cmp(&b.entry_id).then(a.entity.cmp(&b.entity)));

    let mut catalog_aliases: Vec<CatalogAliasRecord> = added_catalog
        .iter()
        .filter_map(|eid| {
            let a = alias_map.alias_of(eid)?;
            Some(CatalogAliasRecord {
                entry_id: eid.clone(),
                alias: a.clone(),
                namespace: pascal(&a),
            })
        })
        .collect();
    catalog_aliases.sort_by(|a, b| a.entry_id.cmp(&b.entry_id));

    let mut catalog_entry_ids: Vec<String> = added_catalog.iter().cloned().collect();
    catalog_entry_ids.sort();

    let fac = FacadeDeltaV1 {
        version: 1,
        catalog_entry_ids,
        catalog_aliases,
        qualified_entities: qsurfaces.clone(),
        collision_notes: collision,
    };

    // TypeScript: one `declare namespace <Pascal(alias)>` per catalog touched, with interfaces
    // for new entities; `LoadedApis` one line per entry for this wave
    let mut namespace_body = String::new();
    for r in &qsurfaces {
        let available_entities: BTreeSet<String> = qsurfaces
            .iter()
            .filter(|s| s.entry_id == r.entry_id)
            .map(|s| s.entity.clone())
            .chain(
                req.already_emitted
                    .iter()
                    .filter(|(entry_id, _)| entry_id == &r.entry_id)
                    .map(|(_, entity)| entity.clone()),
            )
            .collect();
        let ns = pascal(r.catalog_alias.as_str());
        let e_s = r
            .e_index
            .map(|i| i.to_string())
            .unwrap_or_else(|| "?".to_string());
        writeln!(&mut namespace_body, "declare namespace {ns} {{").ok();
        writeln!(
            &mut namespace_body,
            "  type Api = \"{alias}\";",
            alias = r.catalog_alias
        )
        .ok();
        let _ = writeln!(
            &mut namespace_body,
            "  // {entity} — session index e{e_s} (entry_id: {eid})",
            entity = r.entity,
            eid = r.entry_id
        );
        namespace_body.push_str(&jsdoc_comment(&r.description, "  "));
        let _ = writeln!(
            &mut namespace_body,
            "  /** Row surface for this catalog version. Optional fields may be absent/null when a list/search endpoint returns a summary shape; use get(...) or an explicit modeled relation when detail hydration is required. CGS relations are not ordinary row fields unless this interface declares them. */"
        );
        writeln!(&mut namespace_body, "  interface {e}Row {{", e = r.entity).ok();
        for f in &r.fields {
            let q = if f.required { "" } else { "?" };
            let ts = field_type_to_ts(f, r.catalog_alias.as_str());
            namespace_body.push_str(&jsdoc_comment(&f.description, "    "));
            if !f.required {
                let _ = writeln!(
                    &mut namespace_body,
                    "    /** Optional in CGS; may be absent or null in summary/list rows. */"
                );
            }
            let _ = writeln!(
                &mut namespace_body,
                "    {}{q}: {ts};",
                f.name,
                q = q,
                ts = ts
            );
        }
        let _ = writeln!(&mut namespace_body, "  }}");
        let relation_methods = relation_methods_ts(r, &available_entities);
        let _ = writeln!(
            &mut namespace_body,
            "  interface {e}NodeHandle extends Plasm.PlanNodeHandle {{\n{relation_methods}    /** Select fields to include in typed projection metadata. */\n    select(...fields: Array<keyof {e}Row & string>): this;\n  }}\n  interface {e}QueryBuilder extends {e}NodeHandle {{\n    /** Add structured predicates that the host preserves for dry-run and execution reports. */\n    where(...predicates: Plasm.PlanPredicate[]): this;\n  }}",
            e = r.entity
        );
        if r.capabilities.iter().any(|c| c.kind == "search") {
            let _ = writeln!(
                &mut namespace_body,
                "  type {e}SearchInput = string | {{\n{}  }};\n  interface {e}SearchBuilder extends {e}QueryBuilder {{}}",
                search_input_fields_ts(r),
                e = r.entity
            );
        }
        if let Some(cap) = r.capabilities.iter().find(|c| c.kind == "query") {
            if !cap.input_parameters.is_empty() {
                let _ = writeln!(
                    &mut namespace_body,
                    "  type {e}QueryInput = Partial<{e}Row> & {{\n{}  }};",
                    capability_input_fields_ts(
                        &cap.input_parameters,
                        r.catalog_alias.as_str(),
                        "    "
                    ),
                    e = r.entity
                );
            }
        }
        if let Some(cap) = r.capabilities.iter().find(|c| c.kind == "create") {
            if let Some(alias) = capability_input_type_alias_ts(
                &format!("{}CreateInput", r.entity),
                &cap.input_parameters,
                r.catalog_alias.as_str(),
            ) {
                namespace_body.push_str(&alias);
            }
        }
        for cap in r.capabilities.iter().filter(|c| c.kind == "action") {
            if let Some(alias) = capability_input_type_alias_ts(
                &format!("{}{}Input", r.entity, capability_suffix(cap)),
                &cap.input_parameters,
                r.catalog_alias.as_str(),
            ) {
                namespace_body.push_str(&alias);
            }
        }
        let _ = writeln!(
            &mut namespace_body,
            "  interface {e}Entity {{",
            e = r.entity
        );
        if let Some(cap) = r.capabilities.iter().find(|c| c.kind == "query") {
            namespace_body.push_str(&jsdoc_comment(&cap.description, "    "));
            let query_input_type = if cap.input_parameters.is_empty() {
                format!("Partial<{}Row>", r.entity)
            } else {
                format!("{}QueryInput", r.entity)
            };
            let optional = if has_required_input(&cap.input_parameters) {
                ""
            } else {
                "?"
            };
            let _ = writeln!(
                &mut namespace_body,
                "    /** Logical collection over all enumerable rows; result rendering/artifacts may page the returned view. Use Plan.limit(...) for semantic truncation. */\n    query(filters{optional}: {query_input_type}): {e}QueryBuilder;",
                e = r.entity,
                optional = optional,
                query_input_type = query_input_type
            );
        }
        if let Some(cap) = r.capabilities.iter().find(|c| c.kind == "search") {
            namespace_body.push_str(&jsdoc_comment(&cap.description, "    "));
            let _ = writeln!(
                &mut namespace_body,
                "    search(input: {e}SearchInput): {e}SearchBuilder;",
                e = r.entity
            );
        }
        if let Some(cap) = r.capabilities.iter().find(|c| c.kind == "get") {
            namespace_body.push_str(&jsdoc_comment(&cap.description, "    "));
        }
        let _ = writeln!(
            &mut namespace_body,
            "    /** Single-row detail read when this catalog models a get capability; nested fields may still be nullable if the upstream API omits them. */\n    get(id: {key_type}): {e}NodeHandle;",
            e = r.entity,
            key_type = entity_get_key_type_ts(r)
        );
        if let Some(cap) = r.capabilities.iter().find(|c| c.kind == "create") {
            namespace_body.push_str(&jsdoc_comment(&cap.description, "    "));
            let create_input_type = if cap.input_parameters.is_empty() {
                "Record<string, unknown>".to_string()
            } else {
                format!("{}CreateInput", r.entity)
            };
            let optional = input_arg_optional(&cap.input_parameters);
            let _ = writeln!(
                &mut namespace_body,
                "    create(input{optional}: {create_input_type}): Plasm.PlanEffect;",
                optional = optional,
                create_input_type = create_input_type
            );
        } else {
            let _ = writeln!(
                &mut namespace_body,
                "    create(input?: Record<string, unknown>): Plasm.PlanEffect;"
            );
        }
        let action_caps = r
            .capabilities
            .iter()
            .filter(|c| c.kind == "action")
            .collect::<Vec<_>>();
        for cap in &action_caps {
            namespace_body.push_str(&jsdoc_comment(&cap.description, "    "));
        }
        let action_methods = action_method_overloads_ts(r, &action_caps);
        let _ = writeln!(
            &mut namespace_body,
            "    ref(id: unknown): {{ {action_methods} }};"
        );
        let _ = writeln!(&mut namespace_body, "  }}\n}}\n");
    }
    // LoadedApis: shallow merge
    let mut la = String::new();
    let mut by_cat: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for r in &qsurfaces {
        by_cat
            .entry(r.catalog_alias.clone())
            .or_default()
            .insert(r.entity.clone());
    }
    for (a, ent) in by_cat {
        let ns = pascal(&a);
        let _ = writeln!(&mut la, "  {a}: {{", a = a);
        for e in &ent {
            let _ = writeln!(
                &mut la,
                "    {e}: {ns}.{e}Entity & {{ row: {ns}.{e}Row }};",
                e = e
            );
        }
        let _ = writeln!(&mut la, "  }}");
    }
    let loaded = if !la.is_empty() {
        format!(
            "declare interface LoadedApis {{\n{}}}\n\
             /** Catalog-qualified `plasm.<apiAlias>.<Entity>` (see each wave’s `namespace_body`). */\n\
             declare const plasm: LoadedApis;\n",
            la
        )
    } else {
        String::new()
    };
    let agent_prelude = if req.emit_prelude {
        TS_PRELUDE.to_string()
    } else {
        String::new()
    };
    let mut added_aliases: Vec<String> = added_catalog.iter().cloned().collect();
    added_aliases.sort();

    (
        fac,
        TypeScriptCodeArtifacts {
            agent_prelude,
            agent_namespace_body: namespace_body,
            agent_loaded_apis: loaded,
            runtime_bootstrap_ref: Some(CODE_MODE_RUNTIME_BOOTSTRAP_REF.to_string()),
            declarations_unchanged: false,
            added_catalog_aliases: added_aliases,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_pretty() {
        assert_eq!(pascal("github"), "Github");
        assert_eq!(pascal("omdb_api"), "OmdbApi");
        assert_eq!(pascal("123catalog"), "Api123catalog");
    }
}
