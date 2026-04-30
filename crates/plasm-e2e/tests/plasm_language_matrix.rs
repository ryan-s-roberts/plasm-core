//! Hermit-backed matrix: parse → DAG compile → dry validate → live plan run for Plasm programs.
//!
//! This file is the conformance surface for user-visible Plasm syntax against the dedicated
//! `plasm_language_matrix` OpenAPI + CGS fixtures.
//!
//! ## Coverage contract (keep in sync when extending the language)
//!
//! Each [`MatrixRow`] should exercise a **distinct** user-visible construct or sugar called out in
//! [`docs/plasm-language-unification.md`](../../../../docs/plasm-language-unification.md):
//!
//! - Entity roots: bare query, search `~`, get `(id)`, brace predicates `{field=value}`, comparisons.
//! - Postfix: `.limit`, `.sort(field[, dir])` including `asc`/`desc`, `.aggregate` (named + sugar),
//!   `.group_by`, `.singleton()`, `.page_size`, bracket projection `[…]`.
//! - Programs: bindings, node-ref continuation, parallel final roots, `compile_plasm_surface_line_to_plan`
//!   (single-line surface) vs multi-line DAG programs.
//! - Relations: `from_parent_get`, `query_scoped`.
//! - Render: bracket render `<<TAG`, and passing **`.content`** into a typed string slot (`create`).
//! - Effects: create / update / delete / zero-arity action (domain-stripped method label), `for_each`.
//! - DOMAIN: `e#` symbols where applicable.
//!
//! Hermit returns **schema-generated** bodies; live assertions use stable planner markdown cues, not
//! OpenAPI `example` literals. Multi-digit **numeric** `.sort` ordering is covered in
//! `plasm-agent-core` (`plan_sort_compute_orders_integer_scores_numerically`) because Hermit list
//! payloads are not example-stable.
//!
//! **Planning:** dry-run [`DryPlasmPlanEvaluation::node_results`] `ir.expr` JSON is deserialized into
//! typed [`plasm_core::Expr`]; compute stages deserialize into [`plasm_agent::plasm_plan::ComputeOp`].
//! We avoid matching rendered `operation` strings. Where the IR omits host-only flags (for example
//! surface [`page_size`](plasm_agent::plasm_plan::PlanNode::page_size)), we assert structured fields
//! on the compiled plan JSON rather than human-readable plan text.

#[path = "common/hermit_lang_matrix.rs"]
mod hermit_lang_matrix;

#[path = "common/language_matrix.rs"]
mod language_matrix;

use std::collections::BTreeSet;

use plasm_agent::plasm_dag::{compile_plasm_dag_to_plan, compile_plasm_surface_line_to_plan};
use plasm_agent::plasm_plan::{
    parse_plan_value, validate_plan_artifact, AggregateFunction, ComputeOp, ComputeTemplate,
    PlanValue,
};
use plasm_agent::plasm_plan_run::{
    evaluate_validated_plasm_plan_dry, run_validated_plasm_plan, DryPlasmPlanEvaluation,
    PlasmPlanRunResult,
};
use plasm_core::{
    ChainStep, CompOp, EntityKey, Expr, GetExpr, InvokeExpr, Predicate, PromptPipelineConfig,
    QueryExpr, TypedComparisonValue, Value,
};
use plasm_runtime::{ExecutionConfig, ExecutionEngine};

/// Every tag listed here must appear on at least one passing [`MATRIX_ROWS`] entry (`features` column).
const REQUIRED_FEATURE_TAGS: &[&str] = &[
    "entity_query",
    "entity_search",
    "entity_get",
    "predicate_brace_equality",
    "predicate_brace_comparison",
    "postfix_limit",
    "postfix_projection",
    "postfix_sort",
    "postfix_sort_ascending",
    "postfix_aggregate",
    "aggregate_sugar_count",
    "aggregate_sum",
    "postfix_singleton",
    "relation_from_parent_get",
    "relation_query_scoped",
    "bindings_assignment",
    "bind_first_postfix_limit",
    "binding_continuation",
    "parallel_final_roots",
    "bracket_render",
    "bracket_render_content_ref",
    "static_heredoc_binding",
    "derive_map",
    "effect_create",
    "effect_update",
    "effect_delete",
    "effect_action",
    "for_each_effect",
    "domain_symbol_e1",
    "postfix_group_by",
    "pagination_page_size",
    "surface_line_compile",
];

struct MatrixRow {
    id: &'static str,
    program: &'static str,
    /// Use [`compile_plasm_surface_line_to_plan`] for this row (single expression / comma roots).
    surface_line: bool,
    features: &'static [&'static str],
    /// Minimum [`PlasmPlanRunResult::node_results`] length after live run.
    min_node_results: usize,
    /// Each substring must appear in [`PlasmPlanRunResult::run_markdown`].
    expect_markdown_substrings: &'static [&'static str],
}

fn surface_exprs(dry: &DryPlasmPlanEvaluation) -> Vec<Expr> {
    dry.node_results
        .iter()
        .filter_map(|nr| {
            let ev = nr.get("ir")?.get("expr")?;
            serde_json::from_value(ev.clone()).ok()
        })
        .collect()
}

fn relation_exprs(dry: &DryPlasmPlanEvaluation) -> Vec<Expr> {
    dry.node_results
        .iter()
        .filter_map(|nr| {
            let ev = nr.get("execution_contract")?.get("ir")?;
            serde_json::from_value(ev.clone()).ok()
        })
        .collect()
}

fn compute_templates(dry: &DryPlasmPlanEvaluation) -> Vec<ComputeTemplate> {
    dry.node_results
        .iter()
        .filter_map(|nr| {
            nr.get("compute")
                .and_then(|c| serde_json::from_value::<ComputeTemplate>(c.clone()).ok())
        })
        .collect()
}

fn tcv_string(v: &TypedComparisonValue) -> Option<String> {
    match v.to_value() {
        Value::String(s) => Some(s),
        Value::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

fn json_contains_selector_field(v: &serde_json::Value, want: &str) -> bool {
    match v {
        serde_json::Value::Object(map) => {
            if map.get("selector").and_then(|s| s.as_str()) == Some(want) {
                return true;
            }
            map.values().any(|x| json_contains_selector_field(x, want))
        }
        serde_json::Value::Array(a) => a.iter().any(|x| json_contains_selector_field(x, want)),
        _ => false,
    }
}

fn plan_has_relation_named(plan: &serde_json::Value, relation: &str) -> bool {
    let Some(nodes) = plan.get("nodes").and_then(|n| n.as_array()) else {
        return false;
    };
    nodes.iter().any(|n| {
        n.get("kind").and_then(|k| k.as_str()) == Some("relation")
            && n.pointer("/relation/relation").and_then(|x| x.as_str()) == Some(relation)
    })
}

fn plan_ir_contains_selector(plan: &serde_json::Value, want: &str) -> bool {
    let Some(nodes) = plan.get("nodes").and_then(|n| n.as_array()) else {
        return false;
    };
    nodes.iter().any(|n| {
        [n.get("ir"), n.get("ir_template")]
            .into_iter()
            .flatten()
            .any(|ir| json_contains_selector_field(ir, want))
    })
}

fn json_value_contains_substring(v: &serde_json::Value, needle: &str) -> bool {
    match v {
        serde_json::Value::String(s) => s.contains(needle),
        serde_json::Value::Array(a) => a.iter().any(|x| json_value_contains_substring(x, needle)),
        serde_json::Value::Object(o) => {
            o.values().any(|x| json_value_contains_substring(x, needle))
        }
        _ => false,
    }
}

fn tcv_integer(v: &TypedComparisonValue) -> Option<i64> {
    match v.to_value() {
        Value::Integer(n) => Some(n),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn first_query(exprs: &[Expr]) -> Result<&QueryExpr, String> {
    for e in exprs {
        if let Expr::Query(q) = e {
            return Ok(q);
        }
    }
    Err("expected a Query IR node".into())
}

fn plan_surface_page_size(plan: &serde_json::Value) -> Option<u64> {
    let nodes = plan.get("nodes")?.as_array()?;
    for n in nodes {
        let kind = n.get("kind")?.as_str()?;
        if matches!(
            kind,
            "query" | "search" | "get" | "create" | "update" | "delete" | "action"
        ) {
            return n.get("page_size")?.as_u64();
        }
    }
    None
}

#[allow(clippy::too_many_lines)]
fn assert_planning_ir(
    row: &MatrixRow,
    dry: &DryPlasmPlanEvaluation,
    plan: &serde_json::Value,
) -> Result<(), String> {
    let surfaces = surface_exprs(dry);
    let computes = compute_templates(dry);
    let rel = relation_exprs(dry);

    match row.id {
        "lang_query_all" => {
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" {
                return Err(format!("expected LangItem query, got {:?}", q.entity));
            }
            if q.predicate.is_some() {
                return Err(format!(
                    "expected unpredicated query, got {:?}",
                    q.predicate
                ));
            }
            if q.capability_name.is_some() {
                return Err("expected implicit query capability".into());
            }
            if !computes.is_empty() {
                return Err(format!(
                    "expected no compute stages, got {}",
                    computes.len()
                ));
            }
        }
        "lang_surface_line_limit" | "lang_bind_first_limit" => {
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" || q.predicate.is_some() {
                return Err(format!("unexpected query IR: {q:?}"));
            }
            let want = if row.id == "lang_surface_line_limit" {
                2usize
            } else {
                3usize
            };
            let Some(ComputeOp::Limit { count }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Limit { .. }))
            else {
                return Err(format!("expected Limit compute, got {:?}", computes));
            };
            if *count != want {
                return Err(format!("expected limit {want}, got {count}"));
            }
        }
        "lang_search" => {
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" {
                return Err(format!("expected LangItem, got {:?}", q.entity));
            }
            let Some(cap) = q.capability_name.as_ref() else {
                return Err("search query should pin a Search capability".into());
            };
            if cap.as_str() != "langitem_search" {
                return Err(format!("expected langitem_search capability, got {cap}"));
            }
            let Some(pred) = q.predicate.as_ref() else {
                return Err("expected search predicate".into());
            };
            let Predicate::Comparison {
                field,
                op: CompOp::Eq,
                value,
            } = pred
            else {
                return Err(format!("expected equality predicate, got {pred:?}"));
            };
            if field != "q" {
                return Err(format!("expected search field q, got {field}"));
            }
            if tcv_string(value).as_deref() != Some("Alpha") {
                return Err(format!(
                    "expected Alpha search text, got {:?}",
                    tcv_string(value)
                ));
            }
        }
        "lang_get_by_id" => {
            if !surfaces
                .iter()
                .any(|e| expr_contains_get_langitem(e, Some("i1")))
            {
                return Err(format!(
                    "expected LangItem(i1) Get IR, got {:?}",
                    surfaces.first()
                ));
            }
        }
        "lang_predicate_brace_owner" => {
            let q = first_query(&surfaces)?;
            // `capability_name` may be inferred later in the pipeline; brace IR stability is the predicate.
            let Some(pred) = q.predicate.as_ref() else {
                return Err("expected owner predicate".into());
            };
            let Predicate::Comparison {
                field,
                op: CompOp::Eq,
                value,
            } = pred
            else {
                return Err(format!("expected owner eq, got {pred:?}"));
            };
            if field != "owner" || tcv_string(value).as_deref() != Some("alice") {
                return Err(format!("unexpected predicate: {pred:?}"));
            }
        }
        "lang_predicate_brace_score_cmp" => {
            let q = first_query(&surfaces)?;
            if q.capability_name.as_ref().map(|c| c.as_str()) == Some("langitem_query_owner") {
                return Err("score comparison must not route to langitem_query_owner".into());
            }
            let Some(pred) = q.predicate.as_ref() else {
                return Err("expected comparison predicate".into());
            };
            let Predicate::Comparison {
                field,
                op: CompOp::Gt,
                value,
            } = pred
            else {
                return Err(format!("expected score gt, got {pred:?}"));
            };
            if field != "score" || tcv_integer(value) != Some(1) {
                return Err(format!("unexpected predicate: {pred:?}"));
            }
        }
        "lang_limit_projection" => {
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" {
                return Err(format!("expected LangItem, got {:?}", q.entity));
            }
            let Some(ComputeOp::Limit { count: 1 }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Limit { .. }))
            else {
                return Err(format!("expected Limit(1), got {:?}", computes));
            };
        }
        "lang_sort_limit" => {
            let Some(ComputeOp::Sort {
                descending: true, ..
            }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Sort { .. }))
            else {
                return Err(format!("expected descending Sort, got {:?}", computes));
            };
            let Some(ComputeOp::Limit { count: 2 }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Limit { .. }))
            else {
                return Err(format!("expected Limit(2), got {:?}", computes));
            };
        }
        "lang_sort_asc" => {
            let Some(ComputeOp::Sort {
                descending: false, ..
            }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Sort { .. }))
            else {
                return Err(format!("expected ascending Sort, got {:?}", computes));
            };
            let Some(ComputeOp::Limit { count: 3 }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Limit { .. }))
            else {
                return Err(format!("expected Limit(3), got {:?}", computes));
            };
        }
        "lang_aggregate" => {
            let Some(ComputeTemplate {
                op: ComputeOp::Aggregate { aggregates },
                ..
            }) = computes
                .iter()
                .find(|c| matches!(c.op, ComputeOp::Aggregate { .. }))
            else {
                return Err(format!("expected Aggregate compute, got {:?}", computes));
            };
            let Some(spec) = aggregates.iter().find(|a| a.name.as_str() == "n") else {
                return Err(format!(
                    "expected aggregate binding n, got {:?}",
                    aggregates
                ));
            };
            if spec.function != AggregateFunction::Count || spec.field.is_some() {
                return Err(format!("unexpected aggregate spec: {spec:?}"));
            }
        }
        "lang_aggregate_sugar_count" => {
            let Some(ComputeTemplate {
                op: ComputeOp::Aggregate { aggregates },
                ..
            }) = computes
                .iter()
                .find(|c| matches!(c.op, ComputeOp::Aggregate { .. }))
            else {
                return Err(format!("expected Aggregate compute, got {:?}", computes));
            };
            let Some(spec) = aggregates.iter().find(|a| a.name.as_str() == "count") else {
                return Err(format!(
                    "expected sugar binding count, got {:?}",
                    aggregates
                ));
            };
            if spec.function != AggregateFunction::Count || spec.field.is_some() {
                return Err(format!("unexpected aggregate spec: {spec:?}"));
            }
        }
        "lang_aggregate_sum" => {
            let Some(ComputeTemplate {
                op: ComputeOp::Aggregate { aggregates },
                ..
            }) = computes
                .iter()
                .find(|c| matches!(c.op, ComputeOp::Aggregate { .. }))
            else {
                return Err(format!("expected Aggregate compute, got {:?}", computes));
            };
            let Some(spec) = aggregates.iter().find(|a| a.name.as_str() == "t") else {
                return Err(format!(
                    "expected aggregate binding t, got {:?}",
                    aggregates
                ));
            };
            if spec.function != AggregateFunction::Sum {
                return Err(format!("expected sum, got {:?}", spec.function));
            }
            if spec.field.as_ref().is_none_or(|p| p.dotted() != "score") {
                return Err(format!("expected sum(score), got {:?}", spec.field));
            }
        }
        "lang_group_by" => {
            let Some(ComputeTemplate {
                op: ComputeOp::GroupBy { key, aggregates },
                ..
            }) = computes
                .iter()
                .find(|c| matches!(c.op, ComputeOp::GroupBy { .. }))
            else {
                return Err(format!("expected GroupBy compute, got {:?}", computes));
            };
            if key.dotted() != "owner" {
                return Err(format!("expected group key owner, got {}", key.dotted()));
            }
            let Some(spec) = aggregates.iter().find(|a| a.name.as_str() == "n") else {
                return Err(format!("expected aggregate n, got {:?}", aggregates));
            };
            if spec.function != AggregateFunction::Count {
                return Err(format!("unexpected aggregate: {spec:?}"));
            }
        }
        "lang_relation_lines" => {
            if !surfaces
                .iter()
                .any(|e| expr_contains_get_langitem(e, Some("i1")))
            {
                return Err(format!(
                    "expected LangItem(i1) in surface IR (possibly under Chain), got {:?}",
                    surfaces
                ));
            }
            let pool: Vec<&Expr> = surfaces.iter().chain(rel.iter()).collect();
            // `from_parent_get` often lowers through `.lines` chain navigation; LangLine may appear in
            // the explicit continuation rather than as a bare `Query { entity: LangLine }` root.
            if !pool
                .iter()
                .copied()
                .any(|e| expr_chain_selects_lines(e) || expr_mentions_langline(e))
            {
                return Err(format!(
                    "expected `.lines` chain and/or LangLine IR, got surfaces={surfaces:?} rel={rel:?}"
                ));
            }
        }
        "lang_query_singleton" => {
            let Some(ComputeOp::Limit { count: 5 }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Limit { .. }))
            else {
                return Err(format!("expected Limit(5), got {:?}", computes));
            };
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" || q.predicate.is_some() {
                return Err(format!(
                    "expected bare LangItem query before singleton tail, got {q:?}"
                ));
            }
            // `.singleton()` is primarily a runtime cardinality proof + relation constraint; it does not
            // reliably surface as `result_shape: single` on serialized plan nodes for every lowering.
        }
        "lang_relation_tags_scoped" => {
            if !surfaces
                .iter()
                .any(|e| expr_contains_get_langitem(e, Some("i1")) || expr_chain_selects_tags(e))
            {
                return Err(format!(
                    "expected LangItem(i1) and/or `.tags` chain surface, got {:?}",
                    surfaces
                ));
            }
            if !surfaces.iter().any(expr_chain_selects_tags) {
                return Err(format!(
                    "expected `.tags` chain selector in IR, got {:?}",
                    surfaces
                ));
            }
        }
        "lang_bindings_render" => {
            let Some(ComputeTemplate {
                op: ComputeOp::Render { .. },
                ..
            }) = computes
                .iter()
                .find(|c| matches!(c.op, ComputeOp::Render { .. }))
            else {
                return Err(format!("expected Render compute, got {:?}", computes));
            };
        }
        "lang_render_content_into_create" => {
            let has_create_node = plan
                .get("nodes")
                .and_then(|n| n.as_array())
                .into_iter()
                .flatten()
                .any(|n| n.get("kind").and_then(|k| k.as_str()) == Some("create"));
            if !has_create_node {
                return Err(format!(
                    "expected a plan `create` node (Create may be staged with `ir_template`, not dry `ir.expr`), got {:?}",
                    plan.get("nodes")
                ));
            }
            if !computes
                .iter()
                .any(|c| matches!(c.op, ComputeOp::Render { .. }))
            {
                return Err("expected bracket Render compute before create".into());
            }
        }
        "lang_heredoc_binding" => {
            let mut saw_literal = false;
            for nr in &dry.node_results {
                if nr.get("kind").and_then(|k| k.as_str()) != Some("data") {
                    continue;
                }
                let Some(data_v) = nr.get("data") else {
                    continue;
                };
                if json_value_contains_substring(data_v, "hello-matrix") {
                    saw_literal = true;
                    break;
                }
            }
            if !saw_literal {
                return Err("expected data node carrying hello-matrix payload".into());
            }
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" {
                return Err(format!(
                    "expected LangItem query binding, got {:?}",
                    q.entity
                ));
            }
        }
        "lang_derive_map_parallel" => {
            let mut saw_map_object = false;
            for nr in &dry.node_results {
                if nr.get("kind").and_then(|k| k.as_str()) != Some("derive") {
                    continue;
                }
                let Some(v) = nr.get("value") else {
                    continue;
                };
                let Ok(pv) = serde_json::from_value::<PlanValue>(v.clone()) else {
                    continue;
                };
                if let PlanValue::Object { fields } = pv {
                    if fields.contains_key("t") {
                        saw_map_object = true;
                        break;
                    }
                }
            }
            if !saw_map_object {
                return Err("expected derive map object with field t".into());
            }
            let q = first_query(&surfaces)?;
            if q.capability_name.as_ref().map(|c| c.as_str()) != Some("langitem_search") {
                return Err(format!(
                    "expected search capability on hits root, got {:?}",
                    q.capability_name
                ));
            }
        }
        "lang_binding_continuation" => {
            if !surfaces
                .iter()
                .any(|e| expr_contains_get_langitem(e, Some("i1")) || expr_chain_selects_tags(e))
            {
                return Err(format!(
                    "expected LangItem(i1) Get and/or `.tags` navigation surface, got {:?}",
                    surfaces
                ));
            }
            if !surfaces.iter().any(expr_chain_selects_tags)
                && !plan_ir_contains_selector(plan, "tags")
                && !plan_has_relation_named(plan, "tags")
            {
                return Err(format!(
                    "expected `.tags` navigation (surface IR, plan selector walk, or relation node), surfaces={surfaces:?}"
                ));
            }
        }
        "lang_effect_create_literal" => {
            let Some(Expr::Create(c)) = surfaces.iter().find(|e| matches!(e, Expr::Create(_)))
            else {
                return Err(format!("expected Create, got {:?}", surfaces));
            };
            if c.capability.as_str() != "langitem_create" || c.entity != "LangItem" {
                return Err(format!("unexpected create: {:?}", c.capability));
            }
        }
        "lang_effect_update" => {
            let Some(Expr::Invoke(InvokeExpr { capability, .. })) =
                surfaces.iter().find(|e| matches!(e, Expr::Invoke(_)))
            else {
                return Err(format!("expected Invoke IR, got {:?}", surfaces));
            };
            if capability.as_str() != "langitem_update" {
                return Err(format!("expected langitem_update, got {capability}"));
            }
        }
        "lang_effect_action_ping" => {
            let Some(Expr::Invoke(InvokeExpr { capability, .. })) =
                surfaces.iter().find(|e| matches!(e, Expr::Invoke(_)))
            else {
                return Err(format!("expected Invoke IR, got {:?}", surfaces));
            };
            if capability.as_str() != "langitem_ping" {
                return Err(format!("expected langitem_ping, got {capability}"));
            }
        }
        "lang_effect_delete" => {
            let Some(Expr::Delete(d)) = surfaces.iter().find(|e| matches!(e, Expr::Delete(_)))
            else {
                return Err(format!("expected Delete IR, got {:?}", surfaces));
            };
            if d.capability.as_str() != "langitem_delete" {
                return Err(format!("expected langitem_delete, got {:?}", d.capability));
            }
        }
        "lang_for_each_update" => {
            // CGS `update` capabilities lower to `Expr::Invoke`, which [`infer_surface_contract`]
            // classifies as [`PlanNodeKind::Action`] (not `Update`) in the plan DAG.
            let matches_fe_invoke = |nr: &serde_json::Value| {
                nr.get("kind").and_then(|k| k.as_str()) == Some("for_each")
                    && nr.pointer("/effect_template/kind").and_then(|k| k.as_str())
                        == Some("action")
            };
            let dry_ok = dry.node_results.iter().any(matches_fe_invoke);
            let plan_ok = plan
                .get("nodes")
                .and_then(|n| n.as_array())
                .into_iter()
                .flatten()
                .any(matches_fe_invoke);
            if !dry_ok && !plan_ok {
                return Err(
                    "expected `for_each` with effect_template.kind action (invoke/update surface)"
                        .into(),
                );
            }
            let Some(ComputeOp::Limit { count: 2 }) = computes
                .iter()
                .map(|c| &c.op)
                .find(|o| matches!(o, ComputeOp::Limit { .. }))
            else {
                return Err(format!(
                    "expected Limit(2) before for_each, got {:?}",
                    computes
                ));
            };
        }
        "lang_domain_symbol_page_size" => {
            let q = first_query(&surfaces)?;
            if q.entity != "LangItem" || q.predicate.is_some() {
                return Err(format!("unexpected query IR: {q:?}"));
            }
            let ps = plan_surface_page_size(plan).ok_or_else(|| {
                "expected plan surface page_size field (IR omits host paging cap)".to_string()
            })?;
            if ps != 10 {
                return Err(format!("expected page_size 10, got {ps}"));
            }
        }
        other => {
            return Err(format!(
                "internal: add IR planning asserts for matrix row {other}"
            ));
        }
    }
    Ok(())
}

fn get_simple_id(g: &GetExpr) -> Option<&str> {
    match &g.reference.key {
        EntityKey::Simple(id) => Some(id.as_str()),
        EntityKey::Compound(_) => None,
    }
}

fn expr_chain_selects_lines(e: &Expr) -> bool {
    chain_selector_matches(e, "lines")
}

fn expr_chain_selects_tags(e: &Expr) -> bool {
    chain_selector_matches(e, "tags")
}

fn chain_selector_matches(e: &Expr, want_sel: &str) -> bool {
    match e {
        Expr::Chain(c) if c.selector == want_sel => true,
        Expr::Chain(c) => {
            chain_selector_matches(&c.source, want_sel)
                || matches!(
                    &c.step,
                    ChainStep::Explicit { expr } if chain_selector_matches(expr, want_sel)
                )
        }
        _ => false,
    }
}

fn expr_contains_get_langitem(e: &Expr, want_id: Option<&str>) -> bool {
    match e {
        Expr::Get(g) if g.reference.entity_type == "LangItem" => {
            want_id.is_none_or(|id| get_simple_id(g) == Some(id))
        }
        Expr::Chain(c) => expr_contains_get_langitem(&c.source, want_id),
        _ => false,
    }
}

fn expr_mentions_langline(e: &Expr) -> bool {
    match e {
        Expr::Query(q) => q.entity == "LangLine",
        Expr::Chain(c) => {
            expr_mentions_langline(&c.source)
                || matches!(
                    &c.step,
                    ChainStep::Explicit { expr } if expr_mentions_langline(expr)
                )
        }
        _ => false,
    }
}

fn assert_row(row: &MatrixRow, out: &PlasmPlanRunResult) -> Result<(), String> {
    if out.node_results.len() < row.min_node_results {
        return Err(format!(
            "row {}: expected at least {} node_results, got {}",
            row.id,
            row.min_node_results,
            out.node_results.len()
        ));
    }
    let md = out.run_markdown.as_deref().unwrap_or("");
    for sub in row.expect_markdown_substrings {
        if !md.contains(sub) {
            return Err(format!(
                "row {}: run_markdown missing substring {sub:?} (len {}):\n{md}",
                row.id,
                md.len()
            ));
        }
    }
    Ok(())
}

const MATRIX_ROWS: &[MatrixRow] = &[
    MatrixRow {
        id: "lang_query_all",
        program: "LangItem",
        surface_line: false,
        features: &["entity_query"],
        min_node_results: 1,
        expect_markdown_substrings: &["Query(LangItem", "```"],
    },
    MatrixRow {
        id: "lang_surface_line_limit",
        program: "LangItem.limit(2)",
        surface_line: true,
        features: &["surface_line_compile", "postfix_limit"],
        min_node_results: 1,
        expect_markdown_substrings: &["compute", "PlanLimit"],
    },
    MatrixRow {
        id: "lang_bind_first_limit",
        program: "items = LangItem\nitems.limit(3)",
        surface_line: false,
        features: &["bind_first_postfix_limit", "postfix_limit"],
        min_node_results: 2,
        expect_markdown_substrings: &["compute", "PlanLimit"],
    },
    MatrixRow {
        id: "lang_search",
        program: r#"LangItem~"Alpha""#,
        surface_line: false,
        features: &["entity_search"],
        min_node_results: 1,
        expect_markdown_substrings: &["langitem_search", "filtered"],
    },
    MatrixRow {
        id: "lang_get_by_id",
        program: r#"LangItem("i1")"#,
        surface_line: false,
        features: &["entity_get"],
        min_node_results: 1,
        expect_markdown_substrings: &["Get(LangItem", "i1"],
    },
    MatrixRow {
        id: "lang_predicate_brace_owner",
        program: r#"LangItem{owner="alice"}"#,
        surface_line: false,
        features: &["predicate_brace_equality"],
        min_node_results: 1,
        // Routed to `langitem_query_owner` in CGS; planner markdown is still `Query(LangItem filtered)`.
        expect_markdown_substrings: &["Query(LangItem", "filtered"],
    },
    MatrixRow {
        id: "lang_predicate_brace_score_cmp",
        program: "LangItem{score>1}",
        surface_line: false,
        features: &["predicate_brace_comparison"],
        min_node_results: 1,
        expect_markdown_substrings: &["Query(LangItem", "filtered"],
    },
    MatrixRow {
        id: "lang_limit_projection",
        program: "LangItem.limit(1)[id,title]",
        surface_line: false,
        features: &["postfix_limit", "postfix_projection"],
        min_node_results: 1,
        expect_markdown_substrings: &["compute", "projection"],
    },
    MatrixRow {
        id: "lang_sort_limit",
        program: "LangItem.sort(score, desc).limit(2)[id,score]",
        surface_line: false,
        features: &["postfix_sort"],
        min_node_results: 1,
        expect_markdown_substrings: &["score", "projection"],
    },
    MatrixRow {
        id: "lang_sort_asc",
        program: "LangItem.sort(score, asc).limit(3)[id,score]",
        surface_line: false,
        features: &["postfix_sort", "postfix_sort_ascending"],
        min_node_results: 1,
        expect_markdown_substrings: &["score", "projection"],
    },
    MatrixRow {
        id: "lang_aggregate",
        program: "LangItem.aggregate(n=count)",
        surface_line: false,
        features: &["postfix_aggregate"],
        min_node_results: 1,
        expect_markdown_substrings: &["PlanAggregate", "n"],
    },
    MatrixRow {
        id: "lang_aggregate_sugar_count",
        program: "LangItem.aggregate(count)",
        surface_line: false,
        features: &["aggregate_sugar_count", "postfix_aggregate"],
        min_node_results: 1,
        expect_markdown_substrings: &["PlanAggregate", "count"],
    },
    MatrixRow {
        id: "lang_aggregate_sum",
        program: "LangItem.aggregate(t=sum(score))",
        surface_line: false,
        features: &["aggregate_sum", "postfix_aggregate"],
        min_node_results: 1,
        // Aggregate label `sum(...)` is not spelled in short markdown; binding `t` is stable.
        expect_markdown_substrings: &["PlanAggregate", "t"],
    },
    MatrixRow {
        id: "lang_group_by",
        program: "LangItem.group_by(owner, n=count)",
        surface_line: false,
        features: &["postfix_group_by"],
        min_node_results: 1,
        expect_markdown_substrings: &["PlanGroup", "key"],
    },
    MatrixRow {
        id: "lang_relation_lines",
        program: r#"LangItem("i1").lines[id,note]"#,
        surface_line: false,
        features: &["relation_from_parent_get"],
        min_node_results: 1,
        expect_markdown_substrings: &["compute", "note"],
    },
    MatrixRow {
        id: "lang_query_singleton",
        program: "LangItem.limit(5).singleton()",
        surface_line: false,
        features: &["postfix_singleton"],
        min_node_results: 1,
        expect_markdown_substrings: &["PlanLimit", "langmatrix"],
    },
    MatrixRow {
        id: "lang_relation_tags_scoped",
        program: r#"LangItem("i1").tags"#,
        surface_line: false,
        features: &["relation_query_scoped"],
        min_node_results: 1,
        expect_markdown_substrings: &["tags", "LangTag"],
    },
    MatrixRow {
        id: "lang_bindings_render",
        program: r#"hdr = LangItem("i1")[id,title] <<MD
# {{ rows | length }} row(s): {% for r in rows %}{{ r.id }}{% endfor %}
MD
hdr"#,
        surface_line: false,
        features: &["bindings_assignment", "bracket_render"],
        min_node_results: 2,
        expect_markdown_substrings: &["row(s)", "```"],
    },
    MatrixRow {
        id: "lang_render_content_into_create",
        program: r#"hdr = LangItem.limit(1)[title] <<PLASM_TITLE_PIPE
{{ rows[0].title }}
PLASM_TITLE_PIPE
LangItem.create(title=hdr.content, score=0, owner="render-pipe-owner")"#,
        surface_line: false,
        features: &[
            "bracket_render_content_ref",
            "effect_create",
            "bracket_render",
        ],
        min_node_results: 2,
        expect_markdown_substrings: &["Create(LangItem", "render-pipe-owner"],
    },
    MatrixRow {
        id: "lang_heredoc_binding",
        program: r#"note = <<PLASM_LANG_MATRIX_EOF
hello-matrix
PLASM_LANG_MATRIX_EOF
one = LangItem.limit(1)[title]
one, note"#,
        surface_line: false,
        features: &["static_heredoc_binding", "parallel_final_roots"],
        min_node_results: 2,
        expect_markdown_substrings: &["hello-matrix", "parallel"],
    },
    MatrixRow {
        id: "lang_derive_map_parallel",
        program: r#"hits = LangItem~"Alpha"
sumry = hits[id,title]
cards = sumry => { t: _.title }
sumry, cards"#,
        surface_line: false,
        features: &["derive_map", "parallel_final_roots"],
        min_node_results: 3,
        expect_markdown_substrings: &["derive", "parallel["],
    },
    MatrixRow {
        id: "lang_binding_continuation",
        program: r#"root = LangItem("i1")
tags = root.tags
tags"#,
        surface_line: false,
        features: &["binding_continuation"],
        min_node_results: 2,
        expect_markdown_substrings: &["tags", "LangTag"],
    },
    MatrixRow {
        id: "lang_effect_create_literal",
        program: r#"LangItem.create(title="MatrixCreated", score=7, owner="bot")"#,
        surface_line: false,
        features: &["effect_create"],
        min_node_results: 1,
        expect_markdown_substrings: &["Create(LangItem", "MatrixCreated"],
    },
    MatrixRow {
        id: "lang_effect_update",
        program: r#"LangItem("i1").update(title="MatrixPatch", score=42, owner="alice")"#,
        surface_line: false,
        features: &["effect_update"],
        min_node_results: 1,
        expect_markdown_substrings: &["langitem_update", "42"],
    },
    MatrixRow {
        id: "lang_effect_action_ping",
        program: r#"LangItem("i1").ping()"#,
        surface_line: false,
        features: &["effect_action"],
        min_node_results: 1,
        expect_markdown_substrings: &["langitem_ping", "Invoke"],
    },
    MatrixRow {
        id: "lang_effect_delete",
        program: r#"LangItem("i2").delete()"#,
        surface_line: false,
        features: &["effect_delete"],
        min_node_results: 1,
        expect_markdown_substrings: &["Delete(LangItem", "i2"],
    },
    MatrixRow {
        id: "lang_for_each_update",
        program: r#"items = LangItem.limit(2)[id,title,owner]
sync = items => LangItem(_.id).update(score=3, title=_.title, owner=_.owner)
sync"#,
        surface_line: false,
        features: &["for_each_effect"],
        min_node_results: 2,
        expect_markdown_substrings: &["for_each", "calls"],
    },
    MatrixRow {
        id: "lang_domain_symbol_page_size",
        program: "e1.page_size(10)",
        surface_line: false,
        features: &["domain_symbol_e1", "pagination_page_size"],
        min_node_results: 1,
        expect_markdown_substrings: &["langmatrix.LangItem", "Query(LangItem"],
    },
];

#[tokio::test]
async fn plasm_language_matrix_cgs_templates_validate() {
    let cgs = language_matrix::load_language_matrix_cgs();
    plasm_compile::validate_cgs_capability_templates(&cgs).expect("capability CML templates");
}

#[tokio::test]
async fn plasm_language_matrix_live_runs() {
    let base = hermit_lang_matrix::language_matrix_hermit_base_url().await;
    let cgs = language_matrix::load_language_matrix_cgs();
    plasm_compile::validate_cgs_capability_templates(&cgs).expect("templates");

    let es = language_matrix::matrix_execute_session(cgs.clone());
    let engine = ExecutionEngine::new(ExecutionConfig {
        base_url: Some(base.clone()),
        ..Default::default()
    })
    .expect("ExecutionEngine");
    let st = language_matrix::matrix_host_state(engine, cgs);

    let mut tags_seen: BTreeSet<String> = BTreeSet::new();

    for row in MATRIX_ROWS {
        let plan_json = if row.surface_line {
            compile_plasm_surface_line_to_plan(
                &PromptPipelineConfig::default(),
                None,
                &es,
                row.id,
                row.program,
            )
        } else {
            compile_plasm_dag_to_plan(
                &PromptPipelineConfig::default(),
                None,
                &es,
                row.id,
                row.program,
            )
        }
        .unwrap_or_else(|e| panic!("row {} compile: {e}", row.id));

        let plan = parse_plan_value(&plan_json)
            .unwrap_or_else(|e| panic!("row {} parse_plan_value: {e}", row.id));
        let validated = validate_plan_artifact(&plan)
            .unwrap_or_else(|e| panic!("row {} validate_plan_artifact: {e}", row.id));

        let dry = evaluate_validated_plasm_plan_dry(&es, &validated)
            .unwrap_or_else(|e| panic!("row {} evaluate_validated_plasm_plan_dry: {e}", row.id));
        assert_planning_ir(row, &dry, &plan_json)
            .unwrap_or_else(|e| panic!("row {} planning IR: {e}", row.id));

        let live = run_validated_plasm_plan(
            &es,
            &st,
            es.prompt_hash.as_str(),
            "matrix_sess",
            &validated,
            true,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("row {} run_validated_plasm_plan: {e}", row.id));

        assert_row(row, &live).unwrap_or_else(|e| panic!("row {} assertion: {e}", row.id));
        for t in row.features {
            tags_seen.insert((*t).to_string());
        }
    }

    let required: BTreeSet<String> = REQUIRED_FEATURE_TAGS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let missing: Vec<_> = required.difference(&tags_seen).cloned().collect();
    assert!(
        missing.is_empty(),
        "missing required feature tag coverage: {missing:?}"
    );
}
