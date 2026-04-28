use crate::cgs_federation::FederationDispatch;
use crate::{
    ArrayItemsSchema, CapabilityKind, ChainExpr, ChainStep, CompOp, CreateExpr, DeleteExpr,
    EntityDef, EntityKey, Expr, FieldType, GetExpr, InputFieldSchema, InvokeExpr, PageExpr,
    Predicate, QueryExpr, RelationMaterialization, RelationSchema, TypeError, Value, CGS,
};
use std::collections::HashSet;

/// Human-facing “what to write instead of `$`” for LLM corrections.
fn expected_type_phrase_for_placeholder(field_type: &FieldType) -> String {
    match field_type {
        FieldType::EntityRef { target } => format!(
            "a real id or reference for `{target}` (`$` in examples is only a stand-in, not a wire value)"
        ),
        FieldType::Uuid => {
            "a UUID string in standard form — never the literal `$`".into()
        }
        FieldType::String | FieldType::Date => {
            "a concrete string for this slot (quotes if needed) — never the literal `$`".into()
        }
        FieldType::Blob => {
            "a base64 or attachment-shaped value for this slot — never the literal `$`".into()
        }
        FieldType::Integer => "a concrete integer — never the literal `$`".into(),
        FieldType::Number => "a concrete number — never the literal `$`".into(),
        FieldType::Boolean => "`true` or `false` — never `$`".into(),
        FieldType::Select => {
            "one of the allowed values the schema lists for this field — never `$`".into()
        }
        FieldType::Array | FieldType::MultiSelect | FieldType::Json => format!(
            "a value matching {:?} for this slot — never the literal `$`",
            field_type
        ),
    }
}

fn domain_placeholder_literal_error(
    field: impl Into<String>,
    field_type: &FieldType,
    description: Option<&str>,
) -> TypeError {
    let description = description
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    TypeError::DomainPlaceholderLiteral {
        field: field.into(),
        expected_type: expected_type_phrase_for_placeholder(field_type),
        description,
    }
}

fn validate_array_item_value(
    value: &Value,
    spec: &ArrayItemsSchema,
    path: &str,
) -> Result<(), TypeError> {
    if value.is_domain_example_placeholder() {
        return Ok(());
    }
    if !value.is_compatible_with_field_type(&spec.field_type) {
        return Err(TypeError::IncompatibleValue {
            field: path.to_string(),
            value_type: value.type_name().to_string(),
            field_type: format!("{:?}", spec.field_type),
        });
    }
    if matches!(spec.field_type, FieldType::Select) {
        if let (Some(allowed), Some(sv)) = (&spec.allowed_values, value.as_str()) {
            if !allowed.contains(&sv.to_string()) {
                return Err(TypeError::IncompatibleValue {
                    field: path.to_string(),
                    value_type: format!("'{sv}' (not in allowed values)"),
                    field_type: format!("select with values: {:?}", allowed),
                });
            }
        }
    }
    Ok(())
}

fn validate_typed_array_value(
    value: &Value,
    spec: &ArrayItemsSchema,
    path: &str,
) -> Result<(), TypeError> {
    let Some(arr) = value.as_array() else {
        return Err(TypeError::IncompatibleValue {
            field: path.to_string(),
            value_type: value.type_name().to_string(),
            field_type: "array".to_string(),
        });
    };
    for (i, el) in arr.iter().enumerate() {
        validate_array_item_value(el, spec, &format!("{path}[{i}]"))?;
    }
    Ok(())
}

fn validate_multiselect_value(
    value: &Value,
    allowed: &[String],
    path: &str,
) -> Result<(), TypeError> {
    let Some(arr) = value.as_array() else {
        return Err(TypeError::IncompatibleValue {
            field: path.to_string(),
            value_type: value.type_name().to_string(),
            field_type: "multi_select (array)".to_string(),
        });
    };
    for (i, el) in arr.iter().enumerate() {
        if el.is_domain_example_placeholder() {
            continue;
        }
        let Some(sv) = el.as_str() else {
            return Err(TypeError::IncompatibleValue {
                field: format!("{path}[{i}]"),
                value_type: el.type_name().to_string(),
                field_type: "multi_select element (expected string)".to_string(),
            });
        };
        if !allowed.contains(&sv.to_string()) {
            return Err(TypeError::IncompatibleValue {
                field: format!("{path}[{i}]"),
                value_type: format!("'{sv}' (not in allowed values)"),
                field_type: format!("multi_select with values: {:?}", allowed),
            });
        }
    }
    Ok(())
}

fn type_check_page(page: &PageExpr) -> Result<(), TypeError> {
    if crate::PagingHandle::parse(page.handle.as_str()).is_err() {
        return Err(TypeError::IncompatibleValue {
            field: "handle".to_string(),
            value_type: page.handle.as_str().to_string(),
            field_type: "opaque paging handle: plain `pg1` (HTTP) or `s0_pg1` (MCP logical session slot + sequence)".to_string(),
        });
    }
    if let Some(limit) = page.limit {
        if limit == 0 {
            return Err(TypeError::IncompatibleValue {
                field: "limit".to_string(),
                value_type: "0".to_string(),
                field_type: "positive integer".to_string(),
            });
        }
    }
    Ok(())
}

/// Union of `object` parameters from every Query and Search capability on `entity`.
fn union_query_and_search_params(cgs: &CGS, entity: &str) -> Vec<InputFieldSchema> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for kind in [CapabilityKind::Query, CapabilityKind::Search] {
        for cap in cgs.find_capabilities(entity, kind) {
            if let Some(fields) = cap.object_params() {
                for f in fields {
                    if seen.insert(f.name.clone()) {
                        out.push(f.clone());
                    }
                }
            }
        }
    }
    out
}

/// Type-check an expression against a CGS schema.
pub fn type_check_expr(expr: &Expr, cgs: &CGS) -> Result<(), TypeError> {
    match expr {
        Expr::Query(query) => type_check_query(query, cgs),
        Expr::Get(get) => type_check_get(get, cgs),
        Expr::Create(create) => type_check_create(create, cgs),
        Expr::Delete(delete) => type_check_delete(delete, cgs),
        Expr::Invoke(invoke) => type_check_invoke(invoke, cgs),
        Expr::Chain(chain) => type_check_chain(chain, cgs),
        Expr::Page(page) => type_check_page(page),
    }
}

/// Type-check using per-entity [`CGS`] from [`FederationDispatch`] (fallback: primary session graph).
pub fn type_check_expr_federated(
    expr: &Expr,
    fed: &FederationDispatch,
    fallback: &CGS,
) -> Result<(), TypeError> {
    let cgs_for = |entity: &str| fed.resolve_cgs(entity, fallback);
    match expr {
        Expr::Query(query) => type_check_query(query, cgs_for(query.entity.as_str())),
        Expr::Get(get) => type_check_get(get, cgs_for(get.reference.entity_type.as_str())),
        Expr::Create(create) => type_check_create(create, cgs_for(create.entity.as_str())),
        Expr::Page(page) => type_check_page(page),
        Expr::Delete(delete) => {
            type_check_delete(delete, cgs_for(delete.target.entity_type.as_str()))
        }
        Expr::Invoke(invoke) => {
            type_check_invoke(invoke, cgs_for(invoke.target.entity_type.as_str()))
        }
        Expr::Chain(chain) => type_check_chain_federated(chain, fed, fallback),
    }
}

/// Resolve chain selector against source entity into `(target_entity_name, relation_if_declared)`.
fn resolve_chain_target<'a>(
    source_entity_name: &str,
    source_entity: &'a EntityDef,
    selector: &str,
) -> Result<(String, Option<&'a RelationSchema>), TypeError> {
    if let Some(field) = source_entity.fields.get(selector) {
        let target = match &field.field_type {
            FieldType::EntityRef { target } => target.to_string(),
            other => {
                return Err(TypeError::IncompatibleOperator {
                    field: selector.to_string(),
                    op: "chain (EntityRef navigation)".to_string(),
                    field_type: format!("{:?} (expected EntityRef or relation)", other),
                });
            }
        };
        return Ok((target, None));
    }
    if let Some(rel) = source_entity.relations.get(selector) {
        return Ok((rel.target_resource.to_string(), Some(rel)));
    }
    Err(TypeError::FieldNotFound {
        field: selector.to_string(),
        entity: source_entity_name.to_string(),
    })
}

/// `AutoGet` is admissible when target has `Get`, or when this is a many relation whose
/// materialization is a scoped query fanout (`query_scoped` / `query_scoped_bindings`).
fn ensure_chain_auto_get_admissible(
    source_entity_name: &str,
    selector: &str,
    target_entity_name: &str,
    relation: Option<&RelationSchema>,
    cgs_target: &CGS,
) -> Result<(), TypeError> {
    if cgs_target
        .find_capability(target_entity_name, crate::CapabilityKind::Get)
        .is_some()
    {
        return Ok(());
    }

    let scoped_many_materialization = relation.is_some_and(|rel| {
        rel.cardinality == crate::Cardinality::Many
            && matches!(
                rel.materialize.as_ref(),
                Some(RelationMaterialization::QueryScoped { .. })
                    | Some(RelationMaterialization::QueryScopedBindings { .. })
            )
    });
    if scoped_many_materialization {
        return Ok(());
    }

    Err(TypeError::ChainTargetMissingGet {
        source_entity: source_entity_name.to_string(),
        selector: selector.to_string(),
        target_entity: target_entity_name.to_string(),
    })
}

fn type_check_chain_federated(
    chain: &ChainExpr,
    fed: &FederationDispatch,
    fallback: &CGS,
) -> Result<(), TypeError> {
    type_check_expr_federated(&chain.source, fed, fallback)?;

    let source_entity_name = chain.source.primary_entity();
    let cgs_src = fed.resolve_cgs(source_entity_name, fallback);
    let source_entity =
        cgs_src
            .get_entity(source_entity_name)
            .ok_or_else(|| TypeError::EntityNotFound {
                entity: source_entity_name.to_string(),
            })?;

    let (target_entity_name, relation) =
        resolve_chain_target(source_entity_name, source_entity, chain.selector.as_str())?;

    let cgs_tgt = fed.resolve_cgs(&target_entity_name, fallback);
    cgs_tgt
        .get_entity(&target_entity_name)
        .ok_or_else(|| TypeError::EntityNotFound {
            entity: target_entity_name.clone(),
        })?;

    match &chain.step {
        ChainStep::AutoGet => ensure_chain_auto_get_admissible(
            source_entity_name,
            chain.selector.as_str(),
            &target_entity_name,
            relation,
            cgs_tgt,
        )?,
        ChainStep::Explicit { expr } => {
            type_check_expr_federated(expr, fed, fallback)?;
        }
    }

    Ok(())
}

/// Type-check a query expression.
pub fn type_check_query(query: &QueryExpr, cgs: &CGS) -> Result<(), TypeError> {
    // Check that the entity exists
    let entity = cgs
        .get_entity(&query.entity)
        .ok_or_else(|| TypeError::EntityNotFound {
            entity: query.entity.to_string(),
        })?;

    // Predicate variables may reference entity fields OR any query/search capability
    // parameter for this entity. When `capability_name` is set, use that capability
    // only; otherwise union parameters from all Query + Search caps (schemas often
    // expose multiple scoped queries with different params, e.g. `team_id` vs `space_id`).
    let cap_params: Vec<InputFieldSchema> = if let Some(name) = query.capability_name.as_deref() {
        cgs.get_capability(name)
            .and_then(|cap| cap.object_params().map(|f| f.to_vec()))
            .unwrap_or_default()
    } else {
        union_query_and_search_params(cgs, &query.entity)
    };

    // Type-check the predicate against entity fields + capability parameters
    if let Some(predicate) = &query.predicate {
        type_check_predicate(predicate, entity, &cap_params, cgs)?;
    }

    // Validate projection fields (projections are always entity fields)
    if let Some(projection) = &query.projection {
        for field_name in projection {
            if !entity.fields.contains_key(field_name.as_str()) {
                return Err(TypeError::FieldNotFound {
                    field: field_name.clone(),
                    entity: query.entity.to_string(),
                });
            }
        }
    }

    Ok(())
}

/// Type-check a get expression.
pub fn type_check_get(get: &GetExpr, cgs: &CGS) -> Result<(), TypeError> {
    let entity =
        cgs.get_entity(&get.reference.entity_type)
            .ok_or_else(|| TypeError::EntityNotFound {
                entity: get.reference.entity_type.to_string(),
            })?;

    let en = get.reference.entity_type.to_string();
    match &get.reference.key {
        EntityKey::Simple(_) => {
            if entity.key_vars.len() > 1 {
                return Err(TypeError::RefKeyMismatch {
                    entity: en,
                    message: format!(
                        "compound key {:?} required; use named form Entity(key=value, ...)",
                        entity.key_vars
                    ),
                });
            }
        }
        EntityKey::Compound(m) => {
            if entity.key_vars.len() <= 1 {
                return Err(TypeError::RefKeyMismatch {
                    entity: en,
                    message: "simple id form expected for this entity".into(),
                });
            }
            let expected: std::collections::BTreeSet<String> = entity
                .key_vars
                .iter()
                .map(|k| k.as_str().to_string())
                .collect();
            let got: std::collections::BTreeSet<String> = m.keys().cloned().collect();
            if expected != got {
                return Err(TypeError::RefKeyMismatch {
                    entity: en,
                    message: format!("expected keys {:?}, got {:?}", entity.key_vars, got),
                });
            }
        }
    }

    // Bare `$` in Get keys is allowed for DOMAIN teaching lines; [`ExecutionEngine`] rejects it
    // before any network I/O so the literal never reaches backends.

    Ok(())
}

/// Type-check a create expression.
pub fn type_check_create(create: &CreateExpr, cgs: &CGS) -> Result<(), TypeError> {
    cgs.get_entity(&create.entity)
        .ok_or_else(|| TypeError::EntityNotFound {
            entity: create.entity.to_string(),
        })?;

    let capability =
        cgs.get_capability(&create.capability)
            .ok_or_else(|| TypeError::CapabilityNotFound {
                capability: create.capability.to_string(),
            })?;

    if let Some(input_schema) = &capability.input_schema {
        validate_capability_input(&create.input, input_schema)?;
    }

    Ok(())
}

/// Type-check a delete expression.
pub fn type_check_delete(delete: &DeleteExpr, cgs: &CGS) -> Result<(), TypeError> {
    cgs.get_entity(&delete.target.entity_type)
        .ok_or_else(|| TypeError::EntityNotFound {
            entity: delete.target.entity_type.to_string(),
        })?;

    cgs.get_capability(&delete.capability)
        .ok_or_else(|| TypeError::CapabilityNotFound {
            capability: delete.capability.to_string(),
        })?;

    Ok(())
}

/// Type-check an invoke expression.
pub fn type_check_invoke(invoke: &InvokeExpr, cgs: &CGS) -> Result<(), TypeError> {
    // Check that the target entity exists
    cgs.get_entity(&invoke.target.entity_type)
        .ok_or_else(|| TypeError::EntityNotFound {
            entity: invoke.target.entity_type.to_string(),
        })?;

    // Check that the capability exists
    let capability =
        cgs.get_capability(&invoke.capability)
            .ok_or_else(|| TypeError::CapabilityNotFound {
                capability: invoke.capability.to_string(),
            })?;

    // Validate input against capability input schema if present
    if let Some(input_schema) = &capability.input_schema {
        if let Some(input) = &invoke.input {
            validate_capability_input(input, input_schema)?;
        } else if !matches!(input_schema.input_type, crate::InputType::None) {
            // Object bodies with only optional fields may be omitted (empty object implied).
            match &input_schema.input_type {
                crate::InputType::Object { fields, .. } if fields.iter().all(|f| !f.required) => {}
                _ => {
                    return Err(TypeError::InputRequired {
                        capability: invoke.capability.to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}

/// Type-check a chain expression (Kleisli EntityRef navigation).
///
/// Validates: source expression is well-typed, selector names an EntityRef field on the
/// source entity, and the target entity exists with:
/// - `Get` capability for regular `AutoGet`, or
/// - `query_scoped`/`query_scoped_bindings` many-relation materialization on the selector
///   (fanout query traversal), or
/// - an explicit continuation expression that is itself well-typed.
pub fn type_check_chain(chain: &ChainExpr, cgs: &CGS) -> Result<(), TypeError> {
    type_check_expr(&chain.source, cgs)?;

    let source_entity_name = chain.source.primary_entity();
    let source_entity =
        cgs.get_entity(source_entity_name)
            .ok_or_else(|| TypeError::EntityNotFound {
                entity: source_entity_name.to_string(),
            })?;

    // Resolve target via EntityRef field or declared relation.
    let (target_entity_name, relation) =
        resolve_chain_target(source_entity_name, source_entity, chain.selector.as_str())?;

    cgs.get_entity(&target_entity_name)
        .ok_or_else(|| TypeError::EntityNotFound {
            entity: target_entity_name.clone(),
        })?;

    match &chain.step {
        ChainStep::AutoGet => ensure_chain_auto_get_admissible(
            source_entity_name,
            chain.selector.as_str(),
            &target_entity_name,
            relation,
            cgs,
        )?,
        ChainStep::Explicit { expr } => {
            type_check_expr(expr, cgs)?;
        }
    }

    Ok(())
}

/// Type-check a predicate against an entity schema and (optionally) capability parameters.
///
/// A predicate variable is valid when it names:
/// - A declared capability parameter for this query/search (when listed in `cap_params`), OR
/// - Else an entity field (full type + operator + allowed_values checking).
///
/// Capability parameters win on name clashes so filter enums can be wider than row fields
/// (e.g. `state=all` on list endpoints vs `Issue.state` ∈ {open, closed}).
///
/// The `cap_params` slice is empty for non-query expressions (get, create, etc.).
pub fn type_check_predicate(
    predicate: &Predicate,
    entity: &EntityDef,
    cap_params: &[InputFieldSchema],
    cgs: &CGS,
) -> Result<(), TypeError> {
    match predicate {
        Predicate::True | Predicate::False => Ok(()),

        Predicate::Comparison { field, op, value } => {
            type_check_comparison(field, *op, value, entity, cap_params)
        }

        Predicate::And { args } | Predicate::Or { args } => {
            for arg in args {
                type_check_predicate(arg, entity, cap_params, cgs)?;
            }
            Ok(())
        }

        Predicate::Not { predicate } => type_check_predicate(predicate, entity, cap_params, cgs),

        Predicate::ExistsRelation {
            relation,
            predicate,
        } => type_check_relation(relation, predicate.as_deref(), entity, cgs),
    }
}

/// Type-check a field comparison.
///
/// Resolution order:
/// 1. Capability parameter (when `cap_params` is non-empty and names a match) — `allowed_values`
///    for select/multiselect; operator checks relaxed (HTTP query/body slots).
/// 2. Entity field — full type checking (operator compatibility, value type, allowed_values).
/// 3. Neither — `FieldNotFound` error.
///
/// Capability parameters are checked **first** so names like `state` can differ from the entity
/// field enum (e.g. GitHub list filters accept `all` while `Issue.state` is only `open`/`closed`).
fn type_check_comparison(
    field_name: &str,
    op: CompOp,
    value: &Value,
    entity: &EntityDef,
    cap_params: &[InputFieldSchema],
) -> Result<(), TypeError> {
    // DOMAIN teaching form: `p#=` with no RHS parses as null — skip strict checks so
    // brace-query witness lines need not repeat concrete literals.
    if matches!(value, Value::Null) {
        return Ok(());
    }

    // Capability parameters (scope/filter HTTP slots) allow `$` in DOMAIN teaching lines.
    // Check these **before** entity fields: names like `owner`/`repo` are often both entity
    // fields and query scope parameters (e.g. `RepositoryTag{owner=$,repo=$}`).
    if value.is_domain_example_placeholder() && cap_params.iter().any(|p| p.name == field_name) {
        return Ok(());
    }

    // Reject `$` for comparisons that target entity fields only (not a cap param name).
    if value.is_domain_example_placeholder() {
        if let Some(f) = entity.fields.get(field_name) {
            return Err(domain_placeholder_literal_error(
                field_name,
                &f.field_type,
                Some(f.description.as_str()),
            ));
        }
    }

    // ── 1. Capability parameter ───────────────────────────────────────────────
    // Capability params are typed HTTP inputs. Their `role` (search, sort,
    // response_control, scope, filter) is semantic metadata; at the type-checking
    // level we enforce the declared `field_type` (same matrix as entity fields via
    // [`Value::is_compatible_with_field_type`]) plus structured rules below.
    // We do NOT enforce operator compatibility — the operator is just a hint for
    // how the CLI flag was built; the CML template determines the actual HTTP encoding.
    if let Some(param) = cap_params.iter().find(|p| p.name == field_name) {
        // Enforce allowed_values for select-typed params
        if matches!(param.field_type, FieldType::Select) {
            if let (Some(av), Some(sv)) = (&param.allowed_values, value.as_str()) {
                if !av.contains(&sv.to_string()) {
                    return Err(TypeError::IncompatibleValue {
                        field: field_name.to_string(),
                        value_type: format!("'{}' (not in allowed values)", sv),
                        field_type: format!("select with values: {:?}", av),
                    });
                }
            }
        }
        if matches!(param.field_type, FieldType::Array) {
            if let Some(spec) = param.array_items.as_ref() {
                validate_typed_array_value(value, spec, field_name)?;
            }
        }
        if matches!(param.field_type, FieldType::MultiSelect) {
            if let Some(av) = param.allowed_values.as_deref() {
                validate_multiselect_value(value, av, field_name)?;
            }
        }
        if !value.is_compatible_with_field_type(&param.field_type) {
            return Err(TypeError::IncompatibleValue {
                field: field_name.to_string(),
                value_type: value.type_name().to_string(),
                field_type: format!("{:?}", param.field_type),
            });
        }
        return Ok(());
    }

    // ── 2. Entity field ──────────────────────────────────────────────────────
    if let Some(field) = entity.fields.get(field_name) {
        // Operator compatibility
        if !field.field_type.compatible_operators().contains(&op) {
            return Err(TypeError::IncompatibleOperator {
                field: field_name.to_string(),
                op: format!("{:?}", op),
                field_type: format!("{:?}", field.field_type),
            });
        }

        if op == CompOp::Exists {
            return Ok(());
        }

        if matches!(field.field_type, FieldType::Array) {
            let Some(spec) = field.array_items.as_ref() else {
                return Err(TypeError::IncompatibleValue {
                    field: field_name.to_string(),
                    value_type: value.type_name().to_string(),
                    field_type: "array (missing items schema)".to_string(),
                });
            };
            return validate_typed_array_value(value, spec, field_name);
        }
        if matches!(field.field_type, FieldType::MultiSelect) {
            let allowed = field.allowed_values.as_deref().unwrap_or(&[]);
            return validate_multiselect_value(value, allowed, field_name);
        }

        if !value.is_compatible_with_field_type(&field.field_type) {
            return Err(TypeError::IncompatibleValue {
                field: field_name.to_string(),
                value_type: value.type_name().to_string(),
                field_type: format!("{:?}", field.field_type),
            });
        }

        if matches!(field.field_type, FieldType::Select) {
            if let (Some(allowed_values), Some(string_val)) =
                (&field.allowed_values, value.as_str())
            {
                if !allowed_values.contains(&string_val.to_string()) {
                    return Err(TypeError::IncompatibleValue {
                        field: field_name.to_string(),
                        value_type: format!("'{}' (not in allowed values)", string_val),
                        field_type: format!("select with values: {:?}", allowed_values),
                    });
                }
            }
        }

        return Ok(());
    }

    // ── 3. Unknown field ──────────────────────────────────────────────────────
    Err(TypeError::FieldNotFound {
        field: field_name.to_string(),
        entity: entity.name.to_string(),
    })
}

/// Type-check a relation predicate.
fn type_check_relation(
    relation_name: &str,
    predicate: Option<&Predicate>,
    entity: &EntityDef,
    cgs: &CGS,
) -> Result<(), TypeError> {
    // Check that the relation exists
    let relation =
        entity
            .relations
            .get(relation_name)
            .ok_or_else(|| TypeError::RelationNotFound {
                relation: relation_name.to_string(),
                entity: entity.name.to_string(),
            })?;

    // Check that the target entity exists
    let target_entity =
        cgs.get_entity(&relation.target_resource)
            .ok_or_else(|| TypeError::EntityNotFound {
                entity: relation.target_resource.to_string(),
            })?;

    // Type-check the nested predicate against the target entity.
    // Relation predicates filter target-entity fields; capability params do not apply here.
    if let Some(pred) = predicate {
        type_check_predicate(pred, target_entity, &[], cgs).map_err(|err| {
            TypeError::RecursiveError {
                relation: relation_name.to_string(),
                source: Box::new(err),
            }
        })?;
    }

    Ok(())
}

/// Validate input against capability input schema
fn validate_capability_input(
    input: &Value,
    input_schema: &crate::InputSchema,
) -> Result<(), TypeError> {
    validate_input_type(input, &input_schema.input_type, "")?;
    validate_input_constraints(input, &input_schema.validation)?;
    Ok(())
}

/// Validate a value against an input type specification
fn validate_input_type(
    value: &Value,
    input_type: &crate::InputType,
    path: &str,
) -> Result<(), TypeError> {
    let path_label = || {
        if path.is_empty() {
            "input".to_string()
        } else {
            path.to_string()
        }
    };

    if matches!(value, Value::PlasmInputRef(_)) {
        return Ok(());
    }

    match input_type {
        crate::InputType::None => {
            if value.is_domain_example_placeholder() {
                return Err(TypeError::DomainPlaceholderLiteral {
                    field: path_label(),
                    expected_type: "this action expects no request body — remove `$` entirely"
                        .into(),
                    description: None,
                });
            }
            if !matches!(value, Value::Null) {
                return Err(TypeError::IncompatibleValue {
                    field: path.to_string(),
                    value_type: value.type_name().to_string(),
                    field_type: "none (no input expected)".to_string(),
                });
            }
        }

        crate::InputType::Value {
            field_type,
            allowed_values,
        } => {
            if value.is_domain_example_placeholder() {
                return Err(domain_placeholder_literal_error(
                    path_label(),
                    field_type,
                    None,
                ));
            }
            if !value.is_compatible_with_field_type(field_type) {
                return Err(TypeError::IncompatibleValue {
                    field: path.to_string(),
                    value_type: value.type_name().to_string(),
                    field_type: format!("{:?}", field_type),
                });
            }

            // Check allowed values for select types
            if let (Some(allowed), Some(string_val)) = (allowed_values, value.as_str()) {
                if !allowed.contains(&string_val.to_string()) {
                    return Err(TypeError::IncompatibleValue {
                        field: path.to_string(),
                        value_type: format!("'{}' (not in allowed values)", string_val),
                        field_type: format!("select with values: {:?}", allowed),
                    });
                }
            }
        }

        crate::InputType::Object {
            fields,
            additional_fields,
        } => {
            if value.is_domain_example_placeholder() {
                return Err(TypeError::DomainPlaceholderLiteral {
                    field: path_label(),
                    expected_type: "an object with the fields the prompt lists for this action (e.g. `{name: …}`) — never the bare `$` token".into(),
                    description: None,
                });
            }
            let Some(object) = value.as_object() else {
                return Err(TypeError::IncompatibleValue {
                    field: path.to_string(),
                    value_type: value.type_name().to_string(),
                    field_type: "object".to_string(),
                });
            };

            // Validate required fields
            for field_schema in fields {
                let field_path = if path.is_empty() {
                    field_schema.name.clone()
                } else {
                    format!("{}.{}", path, field_schema.name)
                };

                match object.get(&field_schema.name) {
                    Some(field_value) => {
                        if !field_value.is_domain_example_placeholder() {
                            match &field_schema.field_type {
                                FieldType::Array => {
                                    let Some(spec) = field_schema.array_items.as_ref() else {
                                        return Err(TypeError::IncompatibleValue {
                                            field: field_path.clone(),
                                            value_type: field_value.type_name().to_string(),
                                            field_type: "array (missing items schema)".to_string(),
                                        });
                                    };
                                    validate_typed_array_value(field_value, spec, &field_path)?;
                                }
                                FieldType::MultiSelect => {
                                    let allowed =
                                        field_schema.allowed_values.as_deref().unwrap_or(&[]);
                                    validate_multiselect_value(field_value, allowed, &field_path)?;
                                }
                                _ => {
                                    if !field_value
                                        .is_compatible_with_field_type(&field_schema.field_type)
                                    {
                                        return Err(TypeError::IncompatibleValue {
                                            field: field_path.clone(),
                                            value_type: field_value.type_name().to_string(),
                                            field_type: format!("{:?}", field_schema.field_type),
                                        });
                                    }

                                    if let (Some(allowed), Some(str_val)) =
                                        (&field_schema.allowed_values, field_value.as_str())
                                    {
                                        if !allowed.contains(&str_val.to_string()) {
                                            return Err(TypeError::IncompatibleValue {
                                                field: field_path,
                                                value_type: format!(
                                                    "'{}' (not in allowed values)",
                                                    str_val
                                                ),
                                                field_type: format!(
                                                    "select with values: {:?}",
                                                    allowed
                                                ),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        if field_schema.required {
                            return Err(TypeError::FieldNotFound {
                                field: field_path,
                                entity: "input object".to_string(),
                            });
                        }
                    }
                }
            }

            // Check for unexpected fields if additional_fields is false
            if !additional_fields {
                let defined_fields: std::collections::HashSet<_> =
                    fields.iter().map(|f| &f.name).collect();

                for object_field in object.keys() {
                    if !defined_fields.contains(object_field) {
                        return Err(TypeError::FieldNotFound {
                            field: format!("{}.{}", path, object_field),
                            entity: "additional fields not allowed".to_string(),
                        });
                    }
                }
            }
        }

        crate::InputType::Array {
            element_type,
            min_length,
            max_length,
        } => {
            if value.is_domain_example_placeholder() {
                return Err(TypeError::DomainPlaceholderLiteral {
                    field: path_label(),
                    expected_type: "an array `[...]` as the examples show for this parameter — not the bare `$` token".into(),
                    description: None,
                });
            }
            let Some(array) = value.as_array() else {
                return Err(TypeError::IncompatibleValue {
                    field: path.to_string(),
                    value_type: value.type_name().to_string(),
                    field_type: "array".to_string(),
                });
            };

            // Check length constraints
            if let Some(min) = min_length {
                if array.len() < *min {
                    return Err(TypeError::IncompatibleValue {
                        field: path.to_string(),
                        value_type: format!("array of length {}", array.len()),
                        field_type: format!("array with min length {}", min),
                    });
                }
            }

            if let Some(max) = max_length {
                if array.len() > *max {
                    return Err(TypeError::IncompatibleValue {
                        field: path.to_string(),
                        value_type: format!("array of length {}", array.len()),
                        field_type: format!("array with max length {}", max),
                    });
                }
            }

            // Validate each element
            for (i, element) in array.iter().enumerate() {
                let element_path = format!("{}[{}]", path, i);
                validate_input_type(element, element_type, &element_path)?;
            }
        }

        crate::InputType::Union { variants } => {
            if value.is_domain_example_placeholder() {
                return Err(TypeError::DomainPlaceholderLiteral {
                    field: path_label(),
                    expected_type: format!(
                        "a concrete value matching the example for this capability in the prompt — never copy `$` from teaching lines (union of {} shapes)",
                        variants.len()
                    ),
                    description: None,
                });
            }
            // Input must match at least one variant
            for variant in variants {
                if validate_input_type(value, variant, path).is_ok() {
                    return Ok(()); // Found compatible variant
                }
            }

            return Err(TypeError::IncompatibleValue {
                field: path.to_string(),
                value_type: value.type_name().to_string(),
                field_type: format!("union of {} variants", variants.len()),
            });
        }
    }

    Ok(())
}

/// Validate input constraints
fn validate_input_constraints(
    input: &Value,
    validation: &crate::InputValidation,
) -> Result<(), TypeError> {
    // Check null allowance
    if matches!(input, Value::Null) && !validation.allow_null {
        return Err(TypeError::IncompatibleValue {
            field: "input".to_string(),
            value_type: "null".to_string(),
            field_type: "non-null value required".to_string(),
        });
    }

    // Apply validation predicates
    for predicate in &validation.predicates {
        validate_input_predicate(input, predicate)?;
    }

    // Apply cross-field rules for object inputs
    if let Value::Object(obj) = input {
        for rule in &validation.cross_field_rules {
            validate_cross_field_rule(obj, rule)?;
        }
    }

    Ok(())
}

/// Validate a specific input predicate
fn validate_input_predicate(
    input: &Value,
    predicate: &crate::ValidationPredicate,
) -> Result<(), TypeError> {
    let value = extract_field_by_path(input, &predicate.field_path)?;

    let valid = match predicate.operator {
        crate::ValidationOp::MinLength => {
            let min = predicate.value.as_number().unwrap_or(0.0) as usize;
            match &value {
                Value::String(s) => s.len() >= min,
                Value::Array(a) => a.len() >= min,
                _ => false,
            }
        }

        crate::ValidationOp::MaxLength => {
            let max = predicate.value.as_number().unwrap_or(f64::MAX) as usize;
            match &value {
                Value::String(s) => s.len() <= max,
                Value::Array(a) => a.len() <= max,
                _ => false,
            }
        }

        crate::ValidationOp::MinValue => {
            if let (Some(n), Some(min)) = (value.as_number(), predicate.value.as_number()) {
                n >= min
            } else {
                false
            }
        }

        crate::ValidationOp::MaxValue => {
            if let (Some(n), Some(max)) = (value.as_number(), predicate.value.as_number()) {
                n <= max
            } else {
                false
            }
        }

        crate::ValidationOp::Pattern => {
            // Simplified pattern matching - would use regex in full implementation
            match (&value, &predicate.value) {
                (Value::String(s), Value::String(pattern)) => s.contains(pattern),
                _ => false,
            }
        }

        crate::ValidationOp::CustomFunction => {
            // Custom functions would be implemented in full system
            true // Always pass for POC
        }

        crate::ValidationOp::DependsOn => {
            // Dependency validation would check related fields
            true // Always pass for POC
        }
    };

    if !valid {
        return Err(TypeError::IncompatibleValue {
            field: predicate.field_path.clone(),
            value_type: value.type_name().to_string(),
            field_type: predicate.error_message.clone(),
        });
    }

    Ok(())
}

/// Extract field value by dot-notation path
fn extract_field_by_path(value: &Value, path: &str) -> Result<Value, TypeError> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;

    for part in parts {
        match current {
            Value::Object(obj) => {
                current = obj.get(part).ok_or_else(|| TypeError::FieldNotFound {
                    field: path.to_string(),
                    entity: "input object".to_string(),
                })?;
            }
            _ => {
                return Err(TypeError::IncompatibleValue {
                    field: path.to_string(),
                    value_type: current.type_name().to_string(),
                    field_type: "object (for field access)".to_string(),
                });
            }
        }
    }

    Ok(current.clone())
}

/// Validate cross-field rules
fn validate_cross_field_rule(
    object: &indexmap::IndexMap<String, Value>,
    rule: &crate::CrossFieldRule,
) -> Result<(), TypeError> {
    let present_fields: Vec<_> = rule
        .fields
        .iter()
        .filter(|&field| {
            object.contains_key(field) && !matches!(object.get(field), Some(Value::Null))
        })
        .collect();

    let valid = match rule.rule_type {
        crate::CrossFieldRuleType::AtLeastOne => !present_fields.is_empty(),
        crate::CrossFieldRuleType::ExactlyOne => present_fields.len() == 1,
        crate::CrossFieldRuleType::AllOrNone => {
            present_fields.is_empty() || present_fields.len() == rule.fields.len()
        }
        crate::CrossFieldRuleType::Implies => {
            // If first field is present, second must be too
            if rule.fields.len() >= 2 {
                let first_present = object.contains_key(&rule.fields[0]);
                let second_present = object.contains_key(&rule.fields[1]);
                !first_present || second_present
            } else {
                true
            }
        }
        crate::CrossFieldRuleType::MutuallyExclusive => present_fields.len() <= 1,
    };

    if !valid {
        return Err(TypeError::IncompatibleValue {
            field: rule.fields.join(", "),
            value_type: format!("fields present: {:?}", present_fields),
            field_type: rule.error_message.clone(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load_schema_dir;
    use crate::{
        CapabilityKind, CapabilityMapping, CapabilitySchema, Cardinality, ChainExpr, EntityDef,
        EntityFieldName, Expr, FieldSchema, GetExpr, Predicate, QueryExpr, RelationSchema,
        ResourceSchema,
    };

    fn create_test_schema() -> CGS {
        let mut cgs = CGS::new();

        let account = ResourceSchema {
            name: "Account".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: vec![
                FieldSchema {
                    name: "id".into(),
                    description: String::new(),
                    field_type: FieldType::String,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "name".into(),
                    description: String::new(),
                    field_type: FieldType::String,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "revenue".into(),
                    description: String::new(),
                    field_type: FieldType::Number,
                    value_format: None,
                    allowed_values: None,
                    required: false,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "region".into(),
                    description: String::new(),
                    field_type: FieldType::Select,
                    value_format: None,
                    allowed_values: Some(vec![
                        "EMEA".to_string(),
                        "APAC".to_string(),
                        "AMER".to_string(),
                    ]),
                    required: false,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
            ],
            relations: vec![RelationSchema {
                name: "contacts".into(),
                description: String::new(),
                target_resource: "Contact".into(),
                cardinality: Cardinality::Many,
                materialize: None,
            }],
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };

        let contact = ResourceSchema {
            name: "Contact".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: vec![
                FieldSchema {
                    name: "id".into(),
                    description: String::new(),
                    field_type: FieldType::String,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "name".into(),
                    description: String::new(),
                    field_type: FieldType::String,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "role".into(),
                    description: String::new(),
                    field_type: FieldType::Select,
                    value_format: None,
                    allowed_values: Some(vec!["Manager".to_string(), "Employee".to_string()]),
                    required: false,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
            ],
            relations: vec![],
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };

        cgs.add_resource(account).unwrap();
        cgs.add_resource(contact).unwrap();

        cgs
    }

    fn create_chain_test_schema() -> CGS {
        let mut cgs = CGS::new();

        cgs.add_resource(ResourceSchema {
            name: "Order".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: vec![
                FieldSchema {
                    name: "id".into(),
                    description: String::new(),
                    field_type: FieldType::Integer,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "petId".into(),
                    description: String::new(),
                    field_type: FieldType::EntityRef {
                        target: "Pet".into(),
                    },
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "quantity".into(),
                    description: String::new(),
                    field_type: FieldType::Integer,
                    value_format: None,
                    allowed_values: None,
                    required: false,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
            ],
            relations: vec![],
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        })
        .unwrap();

        cgs.add_resource(ResourceSchema {
            name: "Pet".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: vec![
                FieldSchema {
                    name: "id".into(),
                    description: String::new(),
                    field_type: FieldType::Integer,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "name".into(),
                    description: String::new(),
                    field_type: FieldType::String,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
            ],
            relations: vec![],
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        })
        .unwrap();

        let get_pet = CapabilitySchema {
            name: "pet_get".into(),
            description: String::new(),
            kind: CapabilityKind::Get,
            domain: "Pet".into(),
            mapping: CapabilityMapping {
                template: serde_json::json!({
                    "method": "GET",
                    "path": [{"type": "literal", "value": "pet"}, {"type": "var", "name": "id"}],
                })
                .into(),
            },
            input_schema: None,
            output_schema: None,
            provides: vec![],
            scope_aggregate_key_policy: Default::default(),
            invoke_preflight: None,
        };
        cgs.add_capability(get_pet).unwrap();

        let get_order = CapabilitySchema {
            name: "order_get".into(),
            description: String::new(),
            kind: CapabilityKind::Get,
            domain: "Order".into(),
            mapping: CapabilityMapping {
                template: serde_json::json!({
                    "method": "GET",
                    "path": [{"type": "literal", "value": "store"}, {"type": "literal", "value": "order"}, {"type": "var", "name": "id"}],
                })
                .into(),
            },
            input_schema: None,
            output_schema: None,
            provides: vec![],
            scope_aggregate_key_policy: Default::default(),
            invoke_preflight: None,
        };
        cgs.add_capability(get_order).unwrap();

        cgs
    }

    #[test]
    fn test_valid_query() {
        let cgs = create_test_schema();
        let query = QueryExpr::filtered(
            "Account",
            Predicate::and(vec![
                Predicate::eq("region", "EMEA"),
                Predicate::gt("revenue", 1000.0),
            ]),
        );

        assert!(type_check_query(&query, &cgs).is_ok());
    }

    #[test]
    fn test_invalid_field() {
        let cgs = create_test_schema();
        let query = QueryExpr::filtered("Account", Predicate::eq("invalid_field", "value"));

        let result = type_check_query(&query, &cgs);
        assert!(result.is_err());
        matches!(result.unwrap_err(), TypeError::FieldNotFound { .. });
    }

    #[test]
    fn test_incompatible_operator() {
        let cgs = create_test_schema();
        let query = QueryExpr::filtered("Account", Predicate::gt("name", "value")); // string with >

        let result = type_check_query(&query, &cgs);
        assert!(result.is_err());
        matches!(result.unwrap_err(), TypeError::IncompatibleOperator { .. });
    }

    #[test]
    fn test_relation_predicate() {
        let cgs = create_test_schema();
        let query = QueryExpr::filtered(
            "Account",
            Predicate::exists_relation("contacts", Some(Predicate::eq("role", "Manager"))),
        );

        assert!(type_check_query(&query, &cgs).is_ok());
    }

    #[test]
    fn test_invalid_relation() {
        let cgs = create_test_schema();
        let query = QueryExpr::filtered(
            "Account",
            Predicate::exists_relation("invalid_relation", None),
        );

        let result = type_check_query(&query, &cgs);
        assert!(result.is_err());
        matches!(result.unwrap_err(), TypeError::RelationNotFound { .. });
    }

    #[test]
    fn test_chain_valid_auto_get() {
        let cgs = create_chain_test_schema();
        let chain = ChainExpr::auto_get(Expr::Get(GetExpr::new("Order", "5")), "petId");
        assert!(type_check_chain(&chain, &cgs).is_ok());
    }

    #[test]
    fn test_chain_rejects_non_entity_ref_field() {
        let cgs = create_chain_test_schema();
        let chain = ChainExpr::auto_get(Expr::Get(GetExpr::new("Order", "5")), "quantity");
        let err = type_check_chain(&chain, &cgs).unwrap_err();
        assert!(matches!(err, TypeError::IncompatibleOperator { .. }));
    }

    #[test]
    fn test_chain_rejects_unknown_field() {
        let cgs = create_chain_test_schema();
        let chain = ChainExpr::auto_get(Expr::Get(GetExpr::new("Order", "5")), "nonexistent");
        let err = type_check_chain(&chain, &cgs).unwrap_err();
        assert!(matches!(err, TypeError::FieldNotFound { .. }));
    }

    #[test]
    fn test_chain_rejects_missing_get_capability() {
        let mut cgs = CGS::new();
        cgs.add_resource(ResourceSchema {
            name: "A".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: vec![
                FieldSchema {
                    name: "id".into(),
                    description: String::new(),
                    field_type: FieldType::String,
                    value_format: None,
                    allowed_values: None,
                    required: true,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
                FieldSchema {
                    name: "b_id".into(),
                    description: String::new(),
                    field_type: FieldType::EntityRef { target: "B".into() },
                    value_format: None,
                    allowed_values: None,
                    required: false,
                    array_items: None,
                    string_semantics: None,
                    agent_presentation: None,
                    mime_type_hint: None,
                    attachment_media: None,
                    wire_path: None,
                    derive: None,
                },
            ],
            relations: vec![],
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        })
        .unwrap();
        cgs.add_resource(ResourceSchema {
            name: "B".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: vec![FieldSchema {
                name: "id".into(),
                description: String::new(),
                field_type: FieldType::String,
                value_format: None,
                allowed_values: None,
                required: true,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            }],
            relations: vec![],
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        })
        .unwrap();
        // No Get capability for B
        let chain = ChainExpr::auto_get(Expr::Get(GetExpr::new("A", "1")), "b_id");
        let err = type_check_chain(&chain, &cgs).unwrap_err();
        assert!(matches!(err, TypeError::ChainTargetMissingGet { .. }));
    }

    #[test]
    fn test_chain_allows_query_scoped_many_relation_without_target_get() {
        let dir = std::path::Path::new("../../fixtures/schemas/overshow_tools");
        if !dir.exists() {
            return;
        }
        let cgs = load_schema_dir(dir).expect("load overshow_tools fixture");
        let chain =
            ChainExpr::auto_get(Expr::Get(GetExpr::new("Profile", "$")), "recorded_matches");
        type_check_chain(&chain, &cgs).expect(
            "query_scoped many relation traversal should type-check without requiring target Get",
        );
    }

    #[test]
    fn test_literal_dollar_typechecks_in_get_for_domain_examples() {
        let cgs = create_test_schema();
        type_check_expr(&Expr::Get(GetExpr::new("Account", "$")), &cgs).unwrap();
    }

    #[test]
    fn test_literal_dollar_rejected_in_query_predicate() {
        let cgs = create_test_schema();
        let q = QueryExpr::filtered("Account", Predicate::eq("name", "$"));
        let err = type_check_query(&q, &cgs).unwrap_err();
        assert!(matches!(err, TypeError::DomainPlaceholderLiteral { .. }));
    }

    /// Scoped queries often reuse names like `owner`/`repo` as HTTP scope params and as entity fields
    /// (GitHub `RepositoryTag`). DOMAIN lines use `$` in those slots; they must type-check.
    #[test]
    fn placeholder_dollar_ok_when_scope_param_shadows_entity_field() {
        use crate::schema::{FieldSchema, InputFieldSchema, ParameterRole};
        use indexmap::IndexMap;

        let mut fields = IndexMap::new();
        fields.insert(
            EntityFieldName::from("owner"),
            FieldSchema {
                name: "owner".into(),
                description: String::new(),
                field_type: FieldType::String,
                value_format: None,
                allowed_values: None,
                required: true,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            },
        );
        let entity = EntityDef {
            name: "Tag".into(),
            description: String::new(),
            id_field: "name".into(),
            id_format: None,
            id_from: None,
            fields,
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let cap_params = [InputFieldSchema {
            name: "owner".to_string(),
            field_type: FieldType::String,
            value_format: None,
            required: true,
            allowed_values: None,
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: Some(ParameterRole::Scope),
        }];
        let pred = Predicate::eq("owner", "$");
        let cgs = CGS::new();
        type_check_predicate(&pred, &entity, &cap_params, &cgs).unwrap();
    }

    /// Regression: query predicate `state` must use the **capability** enum (e.g. `all` for
    /// GitHub list filters), not the entity field (`Issue.state` is only open/closed).
    #[test]
    fn query_select_extra_values_ok_when_capability_shadows_entity_field() {
        use indexmap::IndexMap;

        let mut fields = IndexMap::new();
        fields.insert(
            EntityFieldName::from("state"),
            FieldSchema {
                name: "state".into(),
                description: String::new(),
                field_type: FieldType::Select,
                value_format: None,
                allowed_values: Some(vec!["open".into(), "closed".into()]),
                required: true,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            },
        );
        let entity = EntityDef {
            name: "Issue".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields,
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let cap_params = [InputFieldSchema {
            name: "state".to_string(),
            field_type: FieldType::Select,
            value_format: None,
            required: false,
            allowed_values: Some(vec!["open".into(), "closed".into(), "all".into()]),
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }];
        let pred = Predicate::eq("state", "all");
        let cgs = CGS::new();
        type_check_predicate(&pred, &entity, &cap_params, &cgs).unwrap();
    }

    /// Boolean query/body capability parameters must not accept strings (e.g. mailbox labels
    /// mistaken for Gmail `includeSpamTrash`).
    #[test]
    fn capability_boolean_param_rejects_string() {
        use indexmap::IndexMap;

        let entity = EntityDef {
            name: "Message".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: IndexMap::new(),
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let cap_params = [InputFieldSchema {
            name: "includeSpamTrash".to_string(),
            field_type: FieldType::Boolean,
            value_format: None,
            required: false,
            allowed_values: None,
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }];
        let pred = Predicate::eq("includeSpamTrash", "INBOX");
        let cgs = CGS::new();
        let err = type_check_predicate(&pred, &entity, &cap_params, &cgs).unwrap_err();
        assert!(
            matches!(err, TypeError::IncompatibleValue { ref field, .. } if field == "includeSpamTrash"),
            "expected IncompatibleValue for includeSpamTrash, got {err:?}"
        );
    }

    /// String cap params reject wrong scalar kinds; note integers are **allowed** for `String`
    /// in [`Value::is_compatible_with_field_type`] (same as entity fields — numeric id literals).
    #[test]
    fn capability_string_param_rejects_boolean() {
        use indexmap::IndexMap;

        let entity = EntityDef {
            name: "Message".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: IndexMap::new(),
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let cap_params = [InputFieldSchema {
            name: "q".to_string(),
            field_type: FieldType::String,
            value_format: None,
            required: false,
            allowed_values: None,
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }];
        let pred = Predicate::eq("q", true);
        let cgs = CGS::new();
        let err = type_check_predicate(&pred, &entity, &cap_params, &cgs).unwrap_err();
        assert!(
            matches!(err, TypeError::IncompatibleValue { ref field, .. } if field == "q"),
            "expected IncompatibleValue for q, got {err:?}"
        );
    }

    #[test]
    fn capability_integer_param_rejects_string() {
        use indexmap::IndexMap;

        let entity = EntityDef {
            name: "Issue".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: IndexMap::new(),
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let cap_params = [InputFieldSchema {
            name: "limit".to_string(),
            field_type: FieldType::Integer,
            value_format: None,
            required: false,
            allowed_values: None,
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }];
        let pred = Predicate::eq("limit", "10");
        let cgs = CGS::new();
        let err = type_check_predicate(&pred, &entity, &cap_params, &cgs).unwrap_err();
        assert!(
            matches!(err, TypeError::IncompatibleValue { ref field, .. } if field == "limit"),
            "expected IncompatibleValue for limit, got {err:?}"
        );
    }

    #[test]
    fn test_chain_expr_serde_roundtrip() {
        let chain = ChainExpr::auto_get(Expr::Get(GetExpr::new("Order", "5")), "petId");
        let expr = Expr::Chain(chain);
        let json = serde_json::to_string(&expr).unwrap();
        let parsed: Expr = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, parsed);
    }
}
