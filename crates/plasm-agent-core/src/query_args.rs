use clap::ArgMatches;
use plasm_core::{CapabilitySchema, Predicate};

use crate::input_field_cli::{build_field_args, extend_query_predicates_for_field, FieldArgHelp};

/// Generate typed clap `Arg`s for a query capability's declared `parameters:`.
///
/// Each `InputFieldSchema` becomes exactly **one** flag — no operator suffixes.
/// APIs that need range queries (`due_date_gt`, `due_date_lt`) declare them as
/// separate parameters in domain.yaml.
///
/// No `parameters:` on the capability → empty vec (no filter flags; pagination
/// flags are added separately by the CLI builder).
pub fn build_query_param_args(cap: &CapabilitySchema) -> Vec<clap::Arg> {
    build_field_args(cap, FieldArgHelp::Query)
}

/// Extract matched query arguments back into a typed `Predicate`.
///
/// Walks the capability's declared parameters, reads the clap match for each,
/// and assembles `Predicate::Eq` comparisons. Range/inequality semantics are
/// left to APIs that expose distinct `_gt` / `_lt` parameters — each is an
/// equality comparison against its own named flag.
pub fn args_to_query_predicate(matches: &ArgMatches, cap: &CapabilitySchema) -> Option<Predicate> {
    let fields = cap.object_params()?;

    let mut comparisons = Vec::new();

    let registered_ids: std::collections::HashSet<String> =
        matches.ids().map(|id| id.as_str().to_string()).collect();

    for field in fields {
        if !registered_ids.contains(&field.name) {
            continue;
        }
        extend_query_predicates_for_field(matches, field, &mut comparisons);
    }

    match comparisons.len() {
        0 => None,
        1 => Some(comparisons.remove(0)),
        _ => Some(Predicate::and(comparisons)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;
    use plasm_core::{
        CapabilityKind, CapabilityMapping, CompOp, InputFieldSchema, InputSchema, InputType,
        InputValidation, Value,
    };

    fn make_query_cap(params: Vec<InputFieldSchema>) -> CapabilitySchema {
        CapabilitySchema {
            name: "thing_query".into(),
            description: String::new(),
            kind: CapabilityKind::Query,
            domain: "Thing".into(),
            mapping: CapabilityMapping {
                template: serde_json::json!({}).into(),
            },
            input_schema: Some(InputSchema {
                input_type: InputType::Object {
                    fields: params,
                    additional_fields: false,
                },
                validation: InputValidation::default(),
                description: None,
                examples: vec![],
            }),
            output_schema: None,
            provides: vec![],
            scope_aggregate_key_policy: Default::default(),
            invoke_preflight: None,
        }
    }

    fn no_params_cap() -> CapabilitySchema {
        CapabilitySchema {
            name: "index_query".into(),
            description: String::new(),
            kind: CapabilityKind::Query,
            domain: "Thing".into(),
            mapping: CapabilityMapping {
                template: serde_json::json!({}).into(),
            },
            input_schema: None,
            output_schema: None,
            provides: vec![],
            scope_aggregate_key_policy: Default::default(),
            invoke_preflight: None,
        }
    }

    fn parse_query(args: &[&str], cap: &CapabilitySchema) -> ArgMatches {
        let mut cmd = Command::new("test");
        for arg in build_query_param_args(cap) {
            cmd = cmd.arg(arg);
        }
        cmd.try_get_matches_from(args).unwrap()
    }

    #[test]
    fn no_params_gives_empty_flags() {
        let cap = no_params_cap();
        let flags = build_query_param_args(&cap);
        assert!(flags.is_empty());
    }

    #[test]
    fn no_params_gives_no_predicate() {
        let cap = no_params_cap();
        let cmd = Command::new("test");
        let matches = cmd.try_get_matches_from(["test"]).unwrap();
        let pred = args_to_query_predicate(&matches, &cap);
        assert!(pred.is_none());
    }

    #[test]
    fn select_param_generates_typed_flag() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "status".into(),
            field_type: plasm_core::FieldType::Select,
            value_format: None,
            required: true,
            allowed_values: Some(vec!["available".into(), "pending".into(), "sold".into()]),
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let flags = build_query_param_args(&cap);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].get_id(), "status");

        let matches = parse_query(&["test", "--status", "available"], &cap);
        let pred = args_to_query_predicate(&matches, &cap).unwrap();
        if let Predicate::Comparison { field, op, value } = pred {
            assert_eq!(field, "status");
            assert_eq!(op, CompOp::Eq);
            assert_eq!(value, Value::String("available".into()));
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn rejects_invalid_select_value() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "status".into(),
            field_type: plasm_core::FieldType::Select,
            value_format: None,
            required: false,
            allowed_values: Some(vec!["available".into()]),
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let mut cmd = Command::new("test");
        for arg in build_query_param_args(&cap) {
            cmd = cmd.arg(arg);
        }
        assert!(cmd
            .try_get_matches_from(["test", "--status", "BOGUS"])
            .is_err());
    }

    #[test]
    fn required_param_enforced() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "team_id".into(),
            field_type: plasm_core::FieldType::String,
            value_format: None,
            required: true,
            allowed_values: None,
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let mut cmd = Command::new("test");
        for arg in build_query_param_args(&cap) {
            cmd = cmd.arg(arg);
        }
        assert!(cmd.try_get_matches_from(["test"]).is_err());
    }

    #[test]
    fn multiple_params_become_and_predicate() {
        let cap = make_query_cap(vec![
            InputFieldSchema {
                name: "archived".into(),
                field_type: plasm_core::FieldType::Boolean,
                value_format: None,
                required: false,
                allowed_values: None,
                array_items: None,
                string_semantics: None,
                description: None,
                default: None,
                role: None,
            },
            InputFieldSchema {
                name: "team_id".into(),
                field_type: plasm_core::FieldType::String,
                value_format: None,
                required: false,
                allowed_values: None,
                array_items: None,
                string_semantics: None,
                description: None,
                default: None,
                role: None,
            },
        ]);
        let matches = parse_query(&["test", "--archived", "--team_id", "abc"], &cap);
        let pred = args_to_query_predicate(&matches, &cap).unwrap();
        assert!(matches!(pred, Predicate::And { .. }));
    }

    #[test]
    fn no_flags_given_returns_none_predicate() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "status".into(),
            field_type: plasm_core::FieldType::Select,
            value_format: None,
            required: false,
            allowed_values: Some(vec!["available".into()]),
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let matches = parse_query(&["test"], &cap);
        assert!(args_to_query_predicate(&matches, &cap).is_none());
    }

    #[test]
    fn generates_select_with_possible_values() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "region".into(),
            field_type: plasm_core::FieldType::Select,
            value_format: None,
            required: false,
            allowed_values: Some(vec!["EMEA".into(), "APAC".into(), "AMER".into()]),
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let matches = parse_query(&["test", "--region", "EMEA"], &cap);
        let pred = args_to_query_predicate(&matches, &cap).unwrap();
        if let Predicate::Comparison { field, op, value } = &pred {
            assert_eq!(field, "region");
            assert_eq!(*op, CompOp::Eq);
            assert_eq!(*value, Value::String("EMEA".into()));
        } else {
            panic!("expected Comparison, got {:?}", pred);
        }
    }

    #[test]
    fn number_param_flag() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "revenue".into(),
            field_type: plasm_core::FieldType::Number,
            value_format: None,
            required: false,
            allowed_values: None,
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let matches = parse_query(&["test", "--revenue", "1000"], &cap);
        let pred = args_to_query_predicate(&matches, &cap).unwrap();
        if let Predicate::Comparison { field, op, value } = &pred {
            assert_eq!(field, "revenue");
            assert_eq!(*op, CompOp::Eq);
            assert_eq!(*value, Value::Float(1000.0));
        } else {
            panic!("expected Comparison, got {:?}", pred);
        }
    }

    #[test]
    fn no_filters_returns_none() {
        let cap = make_query_cap(vec![InputFieldSchema {
            name: "region".into(),
            field_type: plasm_core::FieldType::Select,
            value_format: None,
            required: false,
            allowed_values: Some(vec!["EMEA".into()]),
            array_items: None,
            string_semantics: None,
            description: None,
            default: None,
            role: None,
        }]);
        let matches = parse_query(&["test"], &cap);
        assert!(args_to_query_predicate(&matches, &cap).is_none());
    }
}
