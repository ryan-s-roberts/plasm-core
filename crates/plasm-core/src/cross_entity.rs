use crate::{CapabilityKind, CompOp, EntityDef, FieldType, Predicate, Value, CGS};

/// Result of analysing a dot-path predicate like `pet.status = available`.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossEntityPredicate {
    /// EntityRef field on the source entity that bridges to the foreign entity.
    pub ref_field: String,
    /// Name of the foreign entity (e.g. `Pet`).
    pub foreign_entity: String,
    /// The predicate to evaluate on the foreign entity (e.g. `status = available`).
    pub foreign_predicate: Predicate,
}

/// Strategy the executor should use for a cross-entity predicate.
#[derive(Debug, Clone, PartialEq)]
pub enum CrossEntityStrategy {
    /// Push-left: query the foreign entity first, collect IDs, then filter source.
    /// `query(Pet, status=available) → ids → query(Order, petId IN ids)`
    PushLeft {
        cross: CrossEntityPredicate,
        /// Name of the query capability parameter on the source entity that accepts the FK.
        source_fk_param: String,
    },
    /// Pull-right: query source, then client-side filter each row by fetching foreign entity.
    /// Always valid but potentially N+1.
    PullRight { cross: CrossEntityPredicate },
}

/// Analyse a predicate for cross-entity dot-path references.
///
/// A comparison field like `pet.status` is cross-entity if:
/// 1. The prefix (`pet`) matches an EntityRef field on `source_entity` (case-insensitive), and
/// 2. The suffix (`status`) is a valid field on the target entity.
///
/// Returns the decomposed cross-entity predicates found at the top level of the predicate tree.
pub fn extract_cross_entity_predicates(
    predicate: &Predicate,
    source_entity: &EntityDef,
    cgs: &CGS,
) -> Vec<CrossEntityPredicate> {
    let mut results = Vec::new();
    collect_cross_entity(predicate, source_entity, cgs, &mut results);
    results
}

fn collect_cross_entity(
    predicate: &Predicate,
    source_entity: &EntityDef,
    cgs: &CGS,
    out: &mut Vec<CrossEntityPredicate>,
) {
    match predicate {
        Predicate::Comparison { field, op, value } => {
            if let Some(dot_pos) = field.find('.') {
                let prefix = &field[..dot_pos];
                let suffix = &field[dot_pos + 1..];
                if suffix.is_empty() {
                    return;
                }
                if let Some(cross) =
                    resolve_cross_entity_field(prefix, suffix, *op, value, source_entity, cgs)
                {
                    out.push(cross);
                }
            }
        }
        Predicate::And { args } | Predicate::Or { args } => {
            for arg in args {
                collect_cross_entity(arg, source_entity, cgs, out);
            }
        }
        Predicate::Not { predicate: inner } => {
            collect_cross_entity(inner, source_entity, cgs, out);
        }
        _ => {}
    }
}

fn resolve_cross_entity_field(
    prefix: &str,
    suffix: &str,
    op: CompOp,
    value: &Value,
    source_entity: &EntityDef,
    cgs: &CGS,
) -> Option<CrossEntityPredicate> {
    let prefix_lower = prefix.to_lowercase();

    // Find the EntityRef field whose name or target entity matches the prefix.
    for (field_name, field_schema) in &source_entity.fields {
        let FieldType::EntityRef { target } = &field_schema.field_type else {
            continue;
        };

        let field_lower = field_name.to_lowercase();
        let target_lower = target.to_lowercase();

        // Match: prefix is the field name (e.g. "petId"), field name without "Id"/"_id" suffix,
        // or the target entity name itself.
        let matches = field_lower == prefix_lower
            || target_lower == prefix_lower
            || field_lower.strip_suffix("id").unwrap_or("") == prefix_lower
            || field_lower.strip_suffix("_id").unwrap_or("") == prefix_lower;

        if !matches {
            continue;
        }

        // Verify the suffix field exists on the target entity.
        let target_entity = cgs.get_entity(target)?;
        if !target_entity.fields.contains_key(suffix) {
            continue;
        }

        return Some(CrossEntityPredicate {
            ref_field: field_name.as_str().to_string(),
            foreign_entity: target.to_string(),
            foreign_predicate: Predicate::Comparison {
                field: suffix.to_string(),
                op,
                value: value.clone(),
            },
        });
    }
    None
}

/// Choose the optimal execution strategy for a cross-entity predicate.
///
/// **Push-left** is preferred when there exists a query capability on the source entity
/// whose parameters include the FK field — this allows server-side filtering.
/// Otherwise falls back to **pull-right** (client-side N+1 filter).
pub fn choose_strategy(
    cross: &CrossEntityPredicate,
    source_entity_name: &str,
    cgs: &CGS,
) -> CrossEntityStrategy {
    // Check if the source entity's Query capability has the FK field as a parameter.
    if let Some(query_cap) = cgs.find_capability(source_entity_name, CapabilityKind::Query) {
        if let Some(ref input) = query_cap.input_schema {
            if let crate::InputType::Object { ref fields, .. } = input.input_type {
                for f in fields {
                    if f.name == cross.ref_field {
                        return CrossEntityStrategy::PushLeft {
                            cross: cross.clone(),
                            source_fk_param: f.name.clone(),
                        };
                    }
                }
            }
        }
    }

    CrossEntityStrategy::PullRight {
        cross: cross.clone(),
    }
}

/// Rewrite a predicate by stripping out cross-entity comparisons (dot-paths).
/// Returns the remaining local-only predicate (or None if everything was cross-entity).
pub fn strip_cross_entity_comparisons(
    predicate: &Predicate,
    source_entity: &EntityDef,
    cgs: &CGS,
) -> Option<Predicate> {
    match predicate {
        Predicate::Comparison { field, .. } => {
            if field.contains('.') {
                let dot_pos = field.find('.').unwrap();
                let prefix = &field[..dot_pos];
                let suffix = &field[dot_pos + 1..];
                if resolve_cross_entity_field(
                    prefix,
                    suffix,
                    CompOp::Eq,
                    &Value::Null,
                    source_entity,
                    cgs,
                )
                .is_some()
                {
                    return None;
                }
            }
            Some(predicate.clone())
        }
        Predicate::And { args } => {
            let remaining: Vec<Predicate> = args
                .iter()
                .filter_map(|a| strip_cross_entity_comparisons(a, source_entity, cgs))
                .collect();
            match remaining.len() {
                0 => None,
                1 => Some(remaining.into_iter().next().unwrap()),
                _ => Some(Predicate::And { args: remaining }),
            }
        }
        Predicate::Or { args } => {
            let remaining: Vec<Predicate> = args
                .iter()
                .filter_map(|a| strip_cross_entity_comparisons(a, source_entity, cgs))
                .collect();
            if remaining.len() != args.len() {
                // Can't partially strip from OR — keep the whole thing
                Some(predicate.clone())
            } else {
                Some(Predicate::Or { args: remaining })
            }
        }
        other => Some(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityMapping, CapabilitySchema, FieldSchema, InputFieldSchema, InputSchema, InputType,
        InputValidation, ResourceSchema,
    };

    fn petstore_cgs() -> CGS {
        let mut cgs = CGS::new();

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
                FieldSchema {
                    name: "status".into(),
                    description: String::new(),
                    field_type: FieldType::Select,
                    value_format: None,
                    allowed_values: Some(vec!["available".into(), "pending".into(), "sold".into()]),
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

        cgs.add_capability(CapabilitySchema {
            name: "pet_query".into(),
            description: String::new(),
            kind: CapabilityKind::Query,
            domain: "Pet".into(),
            mapping: CapabilityMapping {
                template: serde_json::json!({"method": "GET", "path": [{"type": "literal", "value": "pet"}]}).into(),
            },
            input_schema: Some(InputSchema {
                input_type: InputType::Object {
                    fields: vec![InputFieldSchema {
                        name: "status".into(),
                        field_type: FieldType::Select,
                        value_format: None,
                        required: false,
                        allowed_values: Some(vec!["available".into(), "pending".into(), "sold".into()]),
                        array_items: None,
                        string_semantics: None,
                        description: None,
                        default: None,
                        role: None,
                }],
                    additional_fields: true,
                },
                validation: InputValidation::default(),
                description: None,
                examples: vec![],
            }),
            output_schema: None,
            provides: vec![],
            scope_aggregate_key_policy: Default::default(),
            invoke_preflight: None,
        }).unwrap();

        cgs.add_capability(CapabilitySchema {
            name: "order_query".into(),
            description: String::new(),
            kind: CapabilityKind::Query,
            domain: "Order".into(),
            mapping: CapabilityMapping {
                template: serde_json::json!({"method": "GET", "path": [{"type": "literal", "value": "store"}, {"type": "literal", "value": "order"}]}).into(),
            },
            input_schema: Some(InputSchema {
                input_type: InputType::Object {
                    fields: vec![InputFieldSchema {
                        name: "petId".into(),
                        field_type: FieldType::EntityRef { target: "Pet".into() },
                        value_format: None,
                        required: false,
                        allowed_values: None,
                        array_items: None,
                        string_semantics: None,
                        description: None,
                        default: None,
                        role: None,
                }],
                    additional_fields: true,
                },
                validation: InputValidation::default(),
                description: None,
                examples: vec![],
            }),
            output_schema: None,
            provides: vec![],
            scope_aggregate_key_policy: Default::default(),
            invoke_preflight: None,
        }).unwrap();

        cgs.validate()
            .expect("petstore_cgs fixture must satisfy CGS::validate");
        cgs
    }

    #[test]
    fn extracts_dot_path_predicate() {
        let cgs = petstore_cgs();
        let order = cgs.get_entity("Order").unwrap();
        let pred = Predicate::eq("pet.status", "available");
        let crosses = extract_cross_entity_predicates(&pred, order, &cgs);
        assert_eq!(crosses.len(), 1);
        assert_eq!(crosses[0].ref_field, "petId");
        assert_eq!(crosses[0].foreign_entity, "Pet");
    }

    #[test]
    fn choose_push_left_when_fk_param_exists() {
        let cgs = petstore_cgs();
        let order = cgs.get_entity("Order").unwrap();
        let pred = Predicate::eq("pet.status", "available");
        let crosses = extract_cross_entity_predicates(&pred, order, &cgs);
        let strategy = choose_strategy(&crosses[0], "Order", &cgs);
        assert!(matches!(strategy, CrossEntityStrategy::PushLeft { .. }));
    }

    #[test]
    fn strips_cross_entity_from_and() {
        let cgs = petstore_cgs();
        let order = cgs.get_entity("Order").unwrap();
        let pred = Predicate::and(vec![
            Predicate::eq("pet.status", "available"),
            Predicate::eq("quantity", 5),
        ]);
        let remaining = strip_cross_entity_comparisons(&pred, order, &cgs);
        assert!(remaining.is_some());
        if let Some(Predicate::Comparison { field, .. }) = &remaining {
            assert_eq!(field, "quantity");
        } else {
            panic!("expected single Comparison, got {:?}", remaining);
        }
    }

    #[test]
    fn matches_target_entity_name_as_prefix() {
        let cgs = petstore_cgs();
        let order = cgs.get_entity("Order").unwrap();
        // "Pet.name" should also resolve through petId → Pet
        let pred = Predicate::eq("Pet.name", "Fido");
        let crosses = extract_cross_entity_predicates(&pred, order, &cgs);
        assert_eq!(crosses.len(), 1);
        assert_eq!(crosses[0].foreign_entity, "Pet");
        assert_eq!(crosses[0].ref_field, "petId");
    }

    #[test]
    fn ignores_non_cross_entity_dots() {
        let cgs = petstore_cgs();
        let order = cgs.get_entity("Order").unwrap();
        let pred = Predicate::eq("some.random.path", "x");
        let crosses = extract_cross_entity_predicates(&pred, order, &cgs);
        assert!(crosses.is_empty());
    }
}
