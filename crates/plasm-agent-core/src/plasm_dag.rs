//! Plasm **program** compiler: multi-line bindings, postfix transforms (`.limit`, `.sort`, …),
//! and final roots, lowered to the internal plan JSON executed by [`crate::plasm_plan_run`].
//!
//! Surface path expressions ([`plasm_core::expr_parser`]) remain the leaf language; this module
//! stitches labels, postfix transforms, and `=>` derives into a single coherent program surface.

use crate::execute_session::ExecuteSession;
use crate::plasm_plan::{
    AggregateFunction, ComputeOp, EffectClass, FieldPath, OutputName, PlanExprIr, PlanNodeKind,
    PlanRelationTraversal, PlanValue, QualifiedEntityKey, RelationCardinality,
    RelationSourceCardinality, SyntheticFieldSchema, SyntheticResultSchema, SyntheticValueKind,
};
use crate::plasm_plan_run::parse_plasm_surface_line;
use plasm_core::Expr;
use plasm_core::PromptPipelineConfig;
use plasm_core::SymbolMapCrossRequestCache;
use plasm_core::expr_parser::{PlasmPostfixOp, peel_postfix_suffixes};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct DagNode {
    id: String,
    expr: String,
    source: DagNodeSource,
    singleton: bool,
    page_size: Option<usize>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum DagNodeSource {
    Surface {
        parsed: plasm_core::expr_parser::ParsedExpr,
        kind: PlanNodeKind,
        qualified_entity: QualifiedEntityKey,
        effect_class: EffectClass,
        result_shape: crate::plasm_plan::ResultShape,
        uses_result: Vec<serde_json::Value>,
    },
    /// CGS relation traversal compiled from `bound_label.relation…` (substitutes bound anchor Plasm).
    RelationTraversal {
        source_label: String,
        /// Expanded Plasm used as the continuation anchor for nested `label.…` bindings.
        expanded_plasm: String,
        parsed: plasm_core::expr_parser::ParsedExpr,
        plan_relation: PlanRelationTraversal,
        qualified_entity: QualifiedEntityKey,
        effect_class: EffectClass,
        result_shape: crate::plasm_plan::ResultShape,
    },
    Data(PlanValue),
    Compute {
        source: String,
        op: ComputeOp,
        schema: SyntheticResultSchema,
    },
    Derive {
        source: String,
        value: PlanValue,
        inputs: Vec<serde_json::Value>,
    },
    ForEach {
        source: String,
        parsed_template: serde_json::Value,
        display_expr: String,
        effect_kind: PlanNodeKind,
        qualified_entity: QualifiedEntityKey,
        uses_result: Vec<serde_json::Value>,
    },
}

#[derive(Debug)]
struct CompileState<'a> {
    nodes: Vec<DagNode>,
    labels: BTreeMap<String, usize>,
    pipeline: &'a PromptPipelineConfig,
    cross_cache: Option<&'a SymbolMapCrossRequestCache>,
}

impl<'a> CompileState<'a> {
    fn new(
        pipeline: &'a PromptPipelineConfig,
        cross_cache: Option<&'a SymbolMapCrossRequestCache>,
    ) -> Self {
        Self {
            nodes: Vec::new(),
            labels: BTreeMap::new(),
            pipeline,
            cross_cache,
        }
    }

    fn insert(&mut self, node: DagNode) -> Result<(), String> {
        if self.labels.contains_key(&node.id) {
            return Err(format!("duplicate Plasm program node label {:?}", node.id));
        }
        self.labels.insert(node.id.clone(), self.nodes.len());
        self.nodes.push(node);
        Ok(())
    }

    fn get(&self, id: &str) -> Option<&DagNode> {
        self.labels.get(id).and_then(|i| self.nodes.get(*i))
    }

    fn contains(&self, id: &str) -> bool {
        self.labels.contains_key(id)
    }
}

pub fn is_plasm_dag_candidate(expressions: &[String]) -> bool {
    if expressions.len() != 1 {
        return false;
    }
    let src = expressions[0].trim();
    src.lines().any(|line| {
        let line = strip_comment(line).trim();
        !line.is_empty() && split_assignment(line).is_some()
    }) || src.contains("=>")
        || peel_postfix_suffixes(src)
            .map(|(_, ops)| !ops.is_empty())
            .unwrap_or(false)
}

pub fn split_bare_plasm_roots(src: &str) -> Option<Vec<String>> {
    let src = src.trim();
    if src.is_empty() || src.contains("=>") {
        return None;
    }
    let stmts = parse_statements(src).ok()?;
    if stmts.len() != 1 {
        return None;
    }
    let sole = stmts[0].trim();
    if split_assignment(sole).is_some() {
        return None;
    }
    let parts = split_top_level(sole, ',').ok()?;
    if parts.len() <= 1 {
        return None;
    }
    let parts: Vec<String> = parts
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (parts.len() > 1).then_some(parts)
}

pub fn compile_plasm_dag_to_plan(
    pipeline: &PromptPipelineConfig,
    symbol_map_cross_cache: Option<&SymbolMapCrossRequestCache>,
    session: &ExecuteSession,
    name: &str,
    source: &str,
) -> Result<serde_json::Value, String> {
    compile_plasm_dag_to_plan_inner(pipeline, symbol_map_cross_cache, session, name, source)
        .map_err(|err| {
            flattened_program_newline_diagnostic(source)
                .map(|hint| format!("{hint}\n\nOriginal parse error: {err}"))
                .unwrap_or(err)
        })
}

fn compile_plasm_dag_to_plan_inner(
    pipeline: &PromptPipelineConfig,
    symbol_map_cross_cache: Option<&SymbolMapCrossRequestCache>,
    session: &ExecuteSession,
    name: &str,
    source: &str,
) -> Result<serde_json::Value, String> {
    let mut state = CompileState::new(pipeline, symbol_map_cross_cache);
    let statements = parse_statements(source)?;
    if statements.is_empty() {
        return Err("Plasm program is empty".to_string());
    }
    let mut final_roots: Option<Vec<String>> = None;
    for stmt in statements {
        if let Some((id, rhs)) = split_assignment(&stmt) {
            validate_label(id)?;
            for node in compile_node_expr(session, &state, id, rhs.trim())? {
                state.insert(node)?;
            }
        } else {
            let stmt = stmt.trim();
            if stmt.starts_with("return ") {
                return Err(
                    "return is not Plasm syntax; write bare comma-separated final roots (e.g. `a, b`, not `return a, b`)"
                        .to_string(),
                );
            }
            final_roots = Some(split_return_list(stmt, &mut state, session)?);
        }
    }
    let roots = final_roots.ok_or_else(|| {
        "Plasm program needs a final line of bare roots (comma-separated expressions or node labels)"
            .to_string()
    })?;
    if roots.is_empty() {
        return Err("Plasm program final roots list is empty".to_string());
    }
    let nodes = state
        .nodes
        .iter()
        .map(node_to_json)
        .collect::<Result<Vec<_>, _>>()?;
    let return_value = if roots.len() == 1 {
        json!({ "kind": "node", "node": roots[0] })
    } else {
        json!({ "kind": "parallel", "nodes": roots })
    };
    Ok(json!({
        "version": 1,
        "kind": "program",
        "name": name,
        "nodes": nodes,
        "return": return_value,
        "metadata": { "language": "plasm-dag" }
    }))
}

/// One line of surface Plasm (or `a, b` at top level) as a one-line program plan — same shape as
/// [`compile_plasm_dag_to_plan`], so the MCP and HTTP runtimes can always execute through the plan runner.
pub fn compile_plasm_surface_line_to_plan(
    pipeline: &PromptPipelineConfig,
    symbol_map_cross_cache: Option<&SymbolMapCrossRequestCache>,
    session: &ExecuteSession,
    name: &str,
    line: &str,
) -> Result<serde_json::Value, String> {
    let mut state = CompileState::new(pipeline, symbol_map_cross_cache);
    let trimmed = line.trim();
    if trimmed.starts_with("return ") {
        return Err(
            "return is not Plasm syntax; write bare comma-separated roots (e.g. `a, b`, not `return a, b`)"
                .to_string(),
        );
    }
    let roots = split_return_list(trimmed, &mut state, session)?;
    if roots.is_empty() {
        return Err("expression is empty".to_string());
    }
    let nodes = state
        .nodes
        .iter()
        .map(node_to_json)
        .collect::<Result<Vec<_>, _>>()?;
    let return_value = if roots.len() == 1 {
        json!({ "kind": "node", "node": &roots[0] })
    } else {
        json!({ "kind": "parallel", "nodes": roots })
    };
    Ok(json!({
        "version": 1,
        "kind": "program",
        "name": name,
        "nodes": nodes,
        "return": return_value,
        "metadata": { "language": "plasm-dag" }
    }))
}

/// Trailing `.singleton()` / `.page_size(n)` in postfix peel order become flags on the final node.
fn split_tail_postfix_flags(
    mut ops: Vec<PlasmPostfixOp>,
) -> (Vec<PlasmPostfixOp>, bool, Option<usize>) {
    let mut singleton = false;
    let mut page_size = None;
    while let Some(last) = ops.last() {
        match last {
            PlasmPostfixOp::Singleton => {
                singleton = true;
                ops.pop();
            }
            PlasmPostfixOp::PageSize(n) => {
                page_size = Some(*n);
                ops.pop();
            }
            _ => break,
        }
    }
    (ops, singleton, page_size)
}

fn postfix_op_to_compute(
    op: &PlasmPostfixOp,
    source: &str,
    id: &str,
    expr_display: &str,
) -> Result<DagNode, String> {
    let mk = |op: ComputeOp, schema: SyntheticResultSchema, singleton: bool| -> DagNode {
        DagNode {
            id: id.to_string(),
            expr: expr_display.to_string(),
            singleton,
            page_size: None,
            source: DagNodeSource::Compute {
                source: source.to_string(),
                op,
                schema,
            },
        }
    };
    match op {
        PlasmPostfixOp::Limit(n) => Ok(mk(
            ComputeOp::Limit { count: *n },
            single_unknown_schema("PlanLimit"),
            *n <= 1,
        )),
        PlasmPostfixOp::Sort { args } => {
            let parts = split_top_level(args, ',')?;
            let key = parts
                .first()
                .ok_or_else(|| "sort(...) requires a field".to_string())?
                .trim();
            let descending = parts
                .get(1)
                .map(|s| s.trim().eq_ignore_ascii_case("desc"))
                .unwrap_or(false);
            Ok(mk(
                ComputeOp::Sort {
                    key: FieldPath::from_dotted(key)?,
                    descending,
                },
                single_unknown_schema("PlanSort"),
                false,
            ))
        }
        PlasmPostfixOp::Aggregate { args } => {
            let aggregates = parse_aggregates(args)?;
            let schema = schema_from_aggregates("PlanAggregate", &aggregates);
            Ok(mk(ComputeOp::Aggregate { aggregates }, schema, true))
        }
        PlasmPostfixOp::GroupBy { args } => {
            let parts = split_top_level(args, ',')?;
            let key = parts
                .first()
                .ok_or_else(|| "group_by(...) requires a key field".to_string())?
                .trim();
            let rest = if parts.len() <= 1 {
                return Err("group_by(...) requires aggregate specs".into());
            } else {
                parts[1..].join(",")
            };
            let aggregates = parse_aggregates(rest.as_str())?;
            let schema = schema_from_aggregates("PlanGroup", &aggregates);
            Ok(mk(
                ComputeOp::GroupBy {
                    key: FieldPath::from_dotted(key)?,
                    aggregates,
                },
                schema,
                false,
            ))
        }
        PlasmPostfixOp::Projection { fields } => {
            let mut map = BTreeMap::new();
            for field in parse_field_list(fields)? {
                map.insert(
                    OutputName::new(field.clone())?,
                    FieldPath::from_dotted(&field)?,
                );
            }
            let schema =
                schema_from_output_fields("PlanProject", map.keys(), SyntheticValueKind::Unknown);
            Ok(mk(ComputeOp::Project { fields: map }, schema, false))
        }
        PlasmPostfixOp::Singleton | PlasmPostfixOp::PageSize(_) => {
            Err("internal: singleton/page_size must be split as tail flags before lowering".into())
        }
    }
}

/// Lower `core` plus postfix ops to one or more [`DagNode`]s (surface base + optional compute chain).
fn compile_postfix_plan(
    session: &ExecuteSession,
    state: &CompileState<'_>,
    binding_id: &str,
    full_rhs: &str,
    core: &str,
    ops: Vec<PlasmPostfixOp>,
) -> Result<Vec<DagNode>, String> {
    let (compute_ops, tail_singleton, tail_page_size) = split_tail_postfix_flags(ops);
    let primary = core.trim();

    if compute_ops.is_empty() {
        if state.contains(primary) && (tail_singleton || tail_page_size.is_some()) {
            return Err(format!(
                "Plasm program `{binding_id}`: `.singleton()` / `.page_size(...)` on bare label `{primary}` is not supported; apply transforms to a surface expression or insert an intermediate binding"
            ));
        }
        let mut node = compile_surface_node(session, state, binding_id, primary)?;
        node.singleton |= tail_singleton;
        node.page_size = tail_page_size.or(node.page_size);
        node.expr = full_rhs.to_string();
        return Ok(vec![node]);
    }

    let mut out: Vec<DagNode> = Vec::new();
    let base_source: String = if state.contains(primary) {
        primary.to_string()
    } else {
        let bid = format!("__plasm_{binding_id}_b0");
        let base = compile_surface_node(session, state, &bid, primary)?;
        out.push(base);
        bid
    };

    let mut cur_source = base_source;
    for (i, op) in compute_ops.iter().enumerate() {
        let nid = if i + 1 == compute_ops.len() {
            binding_id.to_string()
        } else {
            format!("__plasm_{binding_id}_s{i}")
        };
        let mut node = postfix_op_to_compute(op, &cur_source, &nid, full_rhs)?;
        if i + 1 < compute_ops.len() {
            node.expr = format!("{full_rhs}::__step{i}");
        }
        cur_source = nid.clone();
        out.push(node);
    }

    if let Some(last) = out.last_mut() {
        last.singleton |= tail_singleton;
        last.page_size = tail_page_size.or(last.page_size);
        last.expr = full_rhs.to_string();
    }
    Ok(out)
}

fn compile_node_expr(
    session: &ExecuteSession,
    state: &CompileState<'_>,
    id: &str,
    rhs: &str,
) -> Result<Vec<DagNode>, String> {
    if let Some((left, right)) = split_arrow(rhs)? {
        let source = left.trim();
        require_node(state, source)?;
        if looks_like_plasm_effect_template(right) {
            let (expr_for_parse, uses) = rewrite_template_expr(right.trim(), state, Some("_"))?;
            let parsed = parse_plasm_surface_line(
                session,
                state.cross_cache,
                state.pipeline,
                &expr_for_parse,
            )
            .map_err(|e| format!("Plasm program `{id}` template parse: {e}"))?;
            let (kind, qualified, _effect, _shape) = infer_surface_contract(session, &parsed.expr)?;
            if !matches!(
                kind,
                PlanNodeKind::Create
                    | PlanNodeKind::Update
                    | PlanNodeKind::Delete
                    | PlanNodeKind::Action
            ) {
                return Err(format!(
                    "Plasm program `{id}` for_each right side must be a write/side-effect expression"
                ));
            }
            return Ok(vec![DagNode {
                id: id.to_string(),
                expr: rhs.to_string(),
                singleton: false,
                page_size: None,
                source: DagNodeSource::ForEach {
                    source: source.to_string(),
                    parsed_template: expr_template_json(&parsed, &uses)?,
                    display_expr: right.trim().to_string(),
                    effect_kind: kind,
                    qualified_entity: qualified,
                    uses_result: uses,
                },
            }]);
        }
        let (value, inputs) = parse_plan_value_expr(right.trim(), state, Some("_"))?;
        return Ok(vec![DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: false,
            page_size: None,
            source: DagNodeSource::Derive {
                source: source.to_string(),
                value,
                inputs,
            },
        }]);
    }
    if let Some((source, fields, template)) = parse_render(rhs)? {
        if state.contains(source) {
            let columns = parse_field_list(fields)?
                .into_iter()
                .map(OutputName::new)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(vec![DagNode {
                id: id.to_string(),
                expr: rhs.to_string(),
                singleton: true,
                page_size: None,
                source: DagNodeSource::Compute {
                    source: source.to_string(),
                    op: ComputeOp::Render { columns, template },
                    schema: SyntheticResultSchema {
                        entity: Some("PlanRender".to_string()),
                        fields: vec![SyntheticFieldSchema {
                            name: OutputName::new("content".to_string())?,
                            value_kind: SyntheticValueKind::String,
                            source: None,
                        }],
                    },
                },
            }]);
        }
    }

    let (core, ops) =
        peel_postfix_suffixes(rhs).map_err(|e| format!("Plasm program `{id}`: {e}"))?;
    if ops.is_empty() {
        if let Ok(value) = parse_plan_value_expr(rhs, state, None) {
            if looks_like_data_literal(rhs) {
                return Ok(vec![DagNode {
                    id: id.to_string(),
                    expr: rhs.to_string(),
                    singleton: true,
                    page_size: None,
                    source: DagNodeSource::Data(value.0),
                }]);
            }
        }
        return Ok(vec![compile_surface_node(
            session,
            state,
            id,
            core.as_str(),
        )?]);
    }
    compile_postfix_plan(session, state, id, rhs, core.as_str(), ops)
}

/// Longest bound label match so `repos.foo` wins over `repo.foo` when both exist.
fn longest_matching_bound_prefix(expr: &str, state: &CompileState<'_>) -> Option<(String, String)> {
    let expr = expr.trim();
    let mut best: Option<(usize, String, String)> = None;
    for label in state.labels.keys() {
        let prefix = format!("{label}.");
        if expr.starts_with(&prefix) {
            let tail = expr[prefix.len()..].to_string();
            if best.as_ref().is_none_or(|(len, _, _)| label.len() > *len) {
                best = Some((label.len(), label.clone(), tail));
            }
        }
    }
    best.map(|(_, l, t)| (l, t))
}

fn anchor_expanded_plasm(state: &CompileState<'_>, label: &str) -> Option<String> {
    let node = state.get(label)?;
    match &node.source {
        DagNodeSource::Surface { .. } => Some(node.expr.clone()),
        DagNodeSource::RelationTraversal { expanded_plasm, .. } => Some(expanded_plasm.clone()),
        DagNodeSource::Data(_)
        | DagNodeSource::Compute { .. }
        | DagNodeSource::Derive { .. }
        | DagNodeSource::ForEach { .. } => None,
    }
}

fn relation_source_cardinality_from_bound_node(
    state: &CompileState<'_>,
    source_label: &str,
) -> RelationSourceCardinality {
    let Some(node) = state.get(source_label) else {
        return RelationSourceCardinality::RuntimeCheckedSingleton;
    };
    match &node.source {
        DagNodeSource::Surface { parsed, kind, .. } => {
            if matches!(kind, PlanNodeKind::Get) || matches!(parsed.expr, Expr::Get(_)) {
                RelationSourceCardinality::Single
            } else if matches!(kind, PlanNodeKind::Query | PlanNodeKind::Search) {
                RelationSourceCardinality::Many
            } else {
                RelationSourceCardinality::RuntimeCheckedSingleton
            }
        }
        DagNodeSource::RelationTraversal { .. } => RelationSourceCardinality::Many,
        DagNodeSource::Data(_)
        | DagNodeSource::Compute { .. }
        | DagNodeSource::Derive { .. }
        | DagNodeSource::ForEach { .. } => RelationSourceCardinality::RuntimeCheckedSingleton,
    }
}

/// Resolve relation metadata for a parsed [`Expr::Chain`] (declared CGS relation on the source entity).
fn lookup_relation_chain_meta(
    session: &ExecuteSession,
    chain: &plasm_core::ChainExpr,
) -> Result<(QualifiedEntityKey, RelationCardinality), String> {
    let source_entity = chain.source.primary_entity();
    let ent = session.cgs.get_entity(source_entity).ok_or_else(|| {
        format!("unknown entity `{source_entity}` (Plasm program relation continuation)")
    })?;
    let rel = ent.relations.get(chain.selector.as_str()).ok_or_else(|| {
        format!(
            "entity `{source_entity}` has no relation `{}` — use a declared relation name from DOMAIN",
            chain.selector
        )
    })?;
    let target_ent = rel.target_resource.as_str();
    let entry_id: String = session
        .domain_exposure
        .as_ref()
        .and_then(|e| e.catalog_entry_id_for_entity(target_ent))
        .map(str::to_string)
        .unwrap_or_else(|| session.entry_id.clone());
    let cardinality = match rel.cardinality {
        plasm_core::Cardinality::One => RelationCardinality::One,
        plasm_core::Cardinality::Many => RelationCardinality::Many,
    };
    Ok((
        QualifiedEntityKey {
            entry_id,
            entity: target_ent.to_string(),
        },
        cardinality,
    ))
}

fn compile_surface_node(
    session: &ExecuteSession,
    state: &CompileState<'_>,
    id: &str,
    expr: &str,
) -> Result<DagNode, String> {
    if let Some((label, tail)) = longest_matching_bound_prefix(expr, state) {
        if let Some(anchor_plasm) = anchor_expanded_plasm(state, &label) {
            let expanded = format!("{anchor_plasm}.{tail}");
            let parsed = parse_plasm_surface_line(session, state.cross_cache, state.pipeline, &expanded)
                .map_err(|e| {
                    format!(
                        "Plasm program `{id}` expression parse: {e}\n(hint: `{label}.…` substitutes the Plasm bound to `{label}`; expanded form `{expanded}`)"
                    )
                })?;
            if let Expr::Chain(ref chain) = parsed.expr {
                let (target_qe, rel_cardinality) = lookup_relation_chain_meta(session, chain)?;
                let source_card = relation_source_cardinality_from_bound_node(state, &label);
                let result_shape = match rel_cardinality {
                    RelationCardinality::Many => crate::plasm_plan::ResultShape::List,
                    RelationCardinality::One => crate::plasm_plan::ResultShape::Single,
                };
                let ir = PlanExprIr {
                    expr: serde_json::to_value(&parsed.expr).map_err(|e| e.to_string())?,
                    projection: parsed.projection.clone(),
                    display_expr: Some(expr.to_string()),
                };
                let plan_relation = PlanRelationTraversal {
                    source: label.clone(),
                    relation: chain.selector.clone(),
                    target: target_qe.clone(),
                    cardinality: rel_cardinality,
                    source_cardinality: source_card,
                    expr: expanded.clone(),
                    ir: ir.clone(),
                };
                return Ok(DagNode {
                    id: id.to_string(),
                    expr: expr.to_string(),
                    singleton: false,
                    page_size: None,
                    source: DagNodeSource::RelationTraversal {
                        source_label: label,
                        expanded_plasm: expanded,
                        parsed,
                        plan_relation,
                        qualified_entity: target_qe,
                        effect_class: EffectClass::Read,
                        result_shape,
                    },
                });
            }
            return Err(format!(
                "Plasm program `{id}`: `{label}.…` expands to a non-relation Plasm expression; node-ref continuation currently supports CGS relation chains (`label.<relation>`) only"
            ));
        }
        return Err(format!(
            "Plasm program `{id}`: `{label}` is not a Plasm expression anchor — compute/derive/data/for_each bindings cannot be extended with `{label}.…`; repeat the full taught `plasm_expr` or bind an intermediate surface node"
        ));
    }
    let (rewritten, uses) = rewrite_template_expr(expr, state, None)?;
    let parsed = parse_plasm_surface_line(session, state.cross_cache, state.pipeline, &rewritten)
        .map_err(|e| format!("Plasm program `{id}` expression parse: {e}"))?;
    let (kind, qualified_entity, effect_class, result_shape) =
        infer_surface_contract(session, &parsed.expr)?;
    Ok(DagNode {
        id: id.to_string(),
        expr: expr.to_string(),
        singleton: matches!(parsed.expr, Expr::Get(_)),
        page_size: None,
        source: DagNodeSource::Surface {
            parsed,
            kind,
            qualified_entity,
            effect_class,
            result_shape,
            uses_result: uses,
        },
    })
}

fn node_to_json(node: &DagNode) -> Result<serde_json::Value, String> {
    match &node.source {
        DagNodeSource::Surface {
            parsed,
            kind,
            qualified_entity,
            effect_class,
            result_shape,
            uses_result,
        } => {
            let ir = if uses_result.is_empty() {
                json!({
                    "expr": parsed.expr,
                    "projection": parsed.projection,
                    "display_expr": node.expr,
                })
            } else {
                expr_template_json(parsed, uses_result)?
            };
            let mut obj = json!({
                "id": node.id,
                "kind": kind,
                "expr": node.expr,
                "effect_class": effect_class,
                "result_shape": result_shape,
                "projection": parsed.projection.clone().unwrap_or_default(),
                "predicates": [],
                "depends_on": uses_result.iter().filter_map(|u| u.get("node").and_then(|v| v.as_str()).map(str::to_string)).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>(),
                "uses_result": uses_result,
            });
            if matches!(result_shape, crate::plasm_plan::ResultShape::Page) {
                obj["qualified_entity"] = serde_json::Value::Null;
            } else {
                obj["qualified_entity"] = json!(qualified_entity);
            }
            if uses_result.is_empty() {
                obj["ir"] = ir;
            } else {
                obj["ir_template"] = ir;
            }
            if let Some(n) = node.page_size {
                obj["page_size"] = json!(n);
            }
            Ok(obj)
        }
        DagNodeSource::RelationTraversal {
            source_label,
            parsed,
            plan_relation,
            qualified_entity,
            effect_class,
            result_shape,
            ..
        } => {
            let mut obj = json!({
                "id": node.id,
                "kind": PlanNodeKind::Relation,
                "qualified_entity": qualified_entity,
                "effect_class": effect_class,
                "result_shape": result_shape,
                "projection": parsed.projection.clone().unwrap_or_default(),
                "predicates": [],
                "relation": plan_relation,
                "depends_on": [source_label],
                "uses_result": [{ "node": source_label, "as": "source" }],
            });
            if let Some(n) = node.page_size {
                obj["page_size"] = json!(n);
            }
            Ok(obj)
        }
        DagNodeSource::Data(value) => Ok(json!({
            "id": node.id,
            "kind": "data",
            "effect_class": "artifact_read",
            "result_shape": "artifact",
            "data": value,
            "depends_on": [],
            "uses_result": [],
        })),
        DagNodeSource::Compute { source, op, schema } => Ok(json!({
            "id": node.id,
            "kind": "compute",
            "effect_class": "artifact_read",
            "result_shape": if matches!(op, ComputeOp::Render { .. }) { "single" } else { "list" },
            "compute": {
                "source": source,
                "op": op,
                "schema": schema,
                "page_size": node.page_size,
            },
            "depends_on": [source],
            "uses_result": [{ "node": source, "as": "source" }],
        })),
        DagNodeSource::Derive {
            source,
            value,
            inputs,
        } => {
            let mut depends = vec![source.clone()];
            for input in inputs {
                if let Some(n) = input.get("node").and_then(|v| v.as_str()) {
                    if !depends.iter().any(|d| d == n) {
                        depends.push(n.to_string());
                    }
                }
            }
            Ok(json!({
                "id": node.id,
                "kind": "derive",
                "effect_class": "artifact_read",
                "result_shape": "artifact",
                "depends_on": depends,
                "uses_result": std::iter::once(json!({ "node": source, "as": "_" })).chain(inputs.iter().map(|input| {
                    json!({
                        "node": input.get("node").and_then(|v| v.as_str()).unwrap_or_default(),
                        "as": input.get("alias").and_then(|v| v.as_str()).unwrap_or_default(),
                    })
                })).collect::<Vec<_>>(),
                "derive_template": {
                    "kind": "map",
                    "source": source,
                    "item_binding": "_",
                    "inputs": inputs,
                    "value": value,
                }
            }))
        }
        DagNodeSource::ForEach {
            source,
            parsed_template,
            display_expr,
            effect_kind,
            qualified_entity,
            uses_result,
        } => {
            let mut depends = vec![source.clone()];
            for input in uses_result {
                if let Some(n) = input.get("node").and_then(|v| v.as_str()) {
                    if !depends.iter().any(|d| d == n) {
                        depends.push(n.to_string());
                    }
                }
            }
            Ok(json!({
                "id": node.id,
                "kind": "for_each",
                "effect_class": "side_effect",
                "result_shape": "side_effect_ack",
                "source": source,
                "item_binding": "_",
                "depends_on": depends,
                "uses_result": std::iter::once(json!({ "node": source, "as": "_" })).chain(uses_result.iter().cloned()).collect::<Vec<_>>(),
                "effect_template": {
                    "kind": effect_kind,
                    "qualified_entity": qualified_entity,
                    "expr_template": display_expr,
                    "ir_template": parsed_template,
                    "effect_class": "side_effect",
                    "result_shape": "side_effect_ack",
                    "projection": [],
                    "input_bindings": [],
                }
            }))
        }
    }
}

/// How a structured heredoc closing line was recognized (tagged `TAG` line) — mirrors
/// [`plasm_core::expr_parser::value`] so program staging matches surface parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeredocCloseLineKind {
    LineOnly,
    GluedSuffix,
}

/// Tagged heredoc close: trim matches `TAG` alone, or `TAG` + optional ASCII ws + one of `)` `,` `}`.
fn tagged_heredoc_close_kind(line_slice: &str, tag: &str) -> Option<(HeredocCloseLineKind, usize)> {
    let leading_ws = line_slice.len() - line_slice.trim_start().len();
    let t = line_slice.trim();
    if t == tag {
        return Some((HeredocCloseLineKind::LineOnly, leading_ws));
    }
    if !t.starts_with(tag) {
        return None;
    }
    let after = &t[tag.len()..];
    let after = after.trim_start();
    if after.len() == 1 {
        let b = after.as_bytes()[0];
        if matches!(b, b')' | b',' | b'}') {
            return Some((HeredocCloseLineKind::GluedSuffix, leading_ws));
        }
    }
    None
}

enum HeredocOpener {
    /// Parsed `<<TAG` but the opener line does not yet contain the required newline after `TAG`
    /// (physical line split — accumulate more lines into the same statement).
    Incomplete {
        tag: String,
    },
    Complete {
        tag: String,
        body_start: usize,
    },
}

#[inline]
fn is_tagged_heredoc_opener_start(bytes: &[u8], i: usize) -> bool {
    i + 2 <= bytes.len()
        && &bytes[i..i + 2] == b"<<"
        && !(i + 3 <= bytes.len() && bytes[i + 2] == b'<')
}

/// What to do at byte index `i` when the surface scan sees a potential `<<TAG` heredoc.
enum HeredocSurfaceStep {
    /// Not `<<` / `<<<` — caller advances one UTF-8 scalar.
    NotAnOpener,
    /// Jump `i` to this index (past full heredoc).
    SkipTo(usize),
    /// Opener `<<TAG` has no newline after tag on this fragment (physical line continues later).
    OpenerIncomplete { tag: String },
}

/// Unified tagged-heredoc recognition for Plasm program surface scans (`split_top_level`, statement line scan).
fn heredoc_surface_step_at(s: &str, i: usize) -> Result<HeredocSurfaceStep, String> {
    let b = s.as_bytes();
    if !is_tagged_heredoc_opener_start(b, i) {
        return Ok(HeredocSurfaceStep::NotAnOpener);
    }
    match try_parse_tagged_heredoc_opener(s, i)? {
        HeredocOpener::Incomplete { tag } => Ok(HeredocSurfaceStep::OpenerIncomplete { tag }),
        HeredocOpener::Complete { tag, body_start } => {
            let end = skip_tagged_structured_heredoc(s, body_start, &tag)?;
            Ok(HeredocSurfaceStep::SkipTo(end))
        }
    }
}

/// Parse `<<TAG` on the line containing `open_idx` (byte index of first `<`), requiring a newline
/// after the tag on the same line with only ASCII whitespace between tag and newline.
fn try_parse_tagged_heredoc_opener(s: &str, open_idx: usize) -> Result<HeredocOpener, String> {
    let b = s.as_bytes();
    debug_assert!(is_tagged_heredoc_opener_start(b, open_idx));
    let mut p = open_idx + 2;
    if p >= b.len() {
        return Err(
            "tagged heredoc `<<` must be immediately followed by a tag (`TAG` = [A-Za-z_][A-Za-z0-9_]*) and a newline after the tag on the same line".into(),
        );
    }
    if !(b[p].is_ascii_alphabetic() || b[p] == b'_') {
        return Err(
            "tagged heredoc `<<` must be immediately followed by a tag (`TAG` = [A-Za-z_][A-Za-z0-9_]*) and a newline after the tag on the same line".into(),
        );
    }
    let tag_start = p;
    p += 1;
    while p < b.len() && (b[p].is_ascii_alphanumeric() || b[p] == b'_') {
        p += 1;
    }
    let tag = s[tag_start..p].to_string();
    let Some(line_end_rel) = s[open_idx..].find('\n') else {
        return Ok(HeredocOpener::Incomplete { tag });
    };
    let line_end = open_idx + line_end_rel;
    let tail = s[p..line_end].trim();
    if !tail.is_empty() {
        return Err(format!(
            "tagged heredoc `<<{tag}` opener must be only `<<{tag}` then optional ASCII spaces/tabs before the newline; do not put text (or `#` comments) after the tag on the opener line"
        ));
    }
    Ok(HeredocOpener::Complete {
        tag,
        body_start: line_end + 1,
    })
}

fn skip_tagged_structured_heredoc(s: &str, body_start: usize, tag: &str) -> Result<usize, String> {
    let mut pos = body_start;
    while pos <= s.len() {
        let line_end = s[pos..].find('\n').map(|r| pos + r).unwrap_or(s.len());
        let line_slice = &s[pos..line_end];
        if let Some((kind, leading_ws)) = tagged_heredoc_close_kind(line_slice, tag) {
            return Ok(match kind {
                HeredocCloseLineKind::LineOnly => {
                    if line_end < s.len() {
                        line_end + 1
                    } else {
                        s.len()
                    }
                }
                HeredocCloseLineKind::GluedSuffix => pos + leading_ws + tag.len(),
            });
        }
        if line_end >= s.len() {
            return Err(format!("unterminated tagged heredoc <<{tag}"));
        }
        pos = line_end + 1;
    }
    Err(format!("unterminated tagged heredoc <<{tag}"))
}

/// One physical line is a complete Plasm program statement, **unless** it opens a tagged heredoc
/// whose closing `TAG` line has not yet been seen (then we accumulate further physical lines).
#[derive(Debug)]
enum PhysicalLineStmtState {
    Complete,
    AwaitingHeredocClose { tag: String },
}

fn scan_physical_line_stmt_state(line: &str) -> Result<PhysicalLineStmtState, String> {
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut quote = None::<char>;
    while i < line.len() {
        let c = line[i..]
            .chars()
            .next()
            .ok_or_else(|| "invalid UTF-8 boundary".to_string())?;
        let cl = c.len_utf8();
        if quote.is_none() {
            match heredoc_surface_step_at(line, i)? {
                HeredocSurfaceStep::NotAnOpener => {}
                HeredocSurfaceStep::OpenerIncomplete { tag } => {
                    return Ok(PhysicalLineStmtState::AwaitingHeredocClose { tag });
                }
                HeredocSurfaceStep::SkipTo(next) => {
                    i = next;
                    continue;
                }
            }
        }
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            _ => {}
        }
        i += cl;
    }
    if depth != 0 {
        return Err(format!(
            "unbalanced delimiters in Plasm program line `{line}`"
        ));
    }
    Ok(PhysicalLineStmtState::Complete)
}

fn parse_statements(src: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut pending_tag: Option<String> = None;

    for raw in src.lines() {
        let w = strip_comment(raw);
        if pending_tag.is_some() {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(w);
            let last = cur.lines().last().unwrap_or("");
            if tagged_heredoc_close_kind(last, pending_tag.as_deref().unwrap()).is_some() {
                out.push(cur.trim_end().to_string());
                cur.clear();
                pending_tag = None;
            }
        } else {
            if w.trim().is_empty() {
                continue;
            }
            cur.clear();
            cur.push_str(w);
            match scan_physical_line_stmt_state(&cur)? {
                PhysicalLineStmtState::Complete => {
                    out.push(cur.trim_end().to_string());
                    cur.clear();
                }
                PhysicalLineStmtState::AwaitingHeredocClose { tag } => {
                    pending_tag = Some(tag);
                }
            }
        }
    }

    if pending_tag.is_some() {
        return Err(
            "unterminated tagged heredoc (missing closing `TAG` line, or missing newline after `<<TAG` on the opener line)".into(),
        );
    }
    if !cur.is_empty() {
        return Err("unterminated Plasm program statement (unexpected trailing fragment)".into());
    }
    Ok(out)
}

fn strip_comment(line: &str) -> &str {
    line.split_once(";;").map_or(line, |(left, _)| left)
}

fn flattened_program_newline_diagnostic(src: &str) -> Option<String> {
    let line = src.trim();
    if line.is_empty() || line.contains('\n') || line.contains("<<") {
        return None;
    }
    let (_id, rhs) = split_assignment(line)?;
    if has_flattened_assignment_boundary(rhs) || has_flattened_final_root_boundary(rhs) {
        Some(
            "Plasm program statements must be separated by real newline characters (U+000A) in the `program` string. Do not separate bindings or final roots with spaces. Send one physical line per binding, then final roots on their own line, e.g. `repo = e2(...)\\ncommits = e1{p4=repo}.limit(20)\\ncommits`."
                .to_string(),
        )
    } else {
        None
    }
}

fn has_flattened_assignment_boundary(s: &str) -> bool {
    let mut depth = 0i32;
    let mut quote = None::<char>;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            '=' if quote.is_none() && depth == 0 => {
                let before_eq = &s[..i];
                let before_trimmed = before_eq.trim_end();
                let token_start = before_trimmed
                    .char_indices()
                    .rev()
                    .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx + ch.len_utf8()))
                    .unwrap_or(0);
                let label = &before_trimmed[token_start..];
                if token_start > 0 && is_valid_label(label) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn has_flattened_final_root_boundary(s: &str) -> bool {
    let mut depth = 0i32;
    let mut quote = None::<char>;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            c if quote.is_none() && depth == 0 && c.is_whitespace() => {
                let left = s[..i].trim();
                let right = s[i..].trim();
                if !left.is_empty() && starts_like_statement_or_root(right) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn starts_like_statement_or_root(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    if matches!(first, 'e' | 'p' | 'm') {
        let mut chars = s.chars();
        chars.next();
        if matches!(chars.next(), Some(c) if c.is_ascii_digit()) {
            return true;
        }
    }
    let token = s
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | '[' | '{' | '.' | '='))
        .next()
        .unwrap_or_default();
    is_valid_label(token)
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            '=' if quote.is_none() && depth == 0 => {
                let left = line[..i].trim();
                let right = line[i + 1..].trim();
                if is_valid_label(left) && !right.is_empty() {
                    return Some((left, right));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_arrow(line: &str) -> Result<Option<(&str, &str)>, String> {
    split_token_top_level(line, "=>")
}

fn split_token_top_level<'a>(
    line: &'a str,
    token: &str,
) -> Result<Option<(&'a str, &'a str)>, String> {
    let mut depth = 0i32;
    let mut quote = None;
    let bytes = line.as_bytes();
    let token_b = token.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = line[i..].chars().next().ok_or("invalid UTF-8 boundary")?;
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            _ => {}
        }
        if quote.is_none() && depth == 0 && bytes[i..].starts_with(token_b) {
            return Ok(Some((&line[..i], &line[i + token.len()..])));
        }
        i += c.len_utf8();
    }
    Ok(None)
}

fn split_return_list(
    line: &str,
    state: &mut CompileState<'_>,
    session: &ExecuteSession,
) -> Result<Vec<String>, String> {
    let mut roots = Vec::new();
    for part in split_top_level(line, ',')? {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if state.contains(part) {
            roots.push(part.to_string());
        } else {
            let id = format!("return_{}", roots.len() + 1);
            for node in compile_node_expr(session, state, &id, part)? {
                state.insert(node)?;
            }
            roots.push(id);
        }
    }
    Ok(roots)
}

fn split_top_level(s: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote = None::<char>;
    let mut i = 0usize;
    while i < s.len() {
        let c = s[i..]
            .chars()
            .next()
            .ok_or_else(|| "invalid UTF-8 boundary".to_string())?;
        let cl = c.len_utf8();
        if quote.is_none() {
            match heredoc_surface_step_at(s, i)? {
                HeredocSurfaceStep::NotAnOpener => {}
                HeredocSurfaceStep::OpenerIncomplete { .. } => {
                    return Err(
                        "tagged heredoc `<<TAG` must have a newline immediately after the tag on the opener line (hard newline; do not squash `<<TAG` with the body on one line)".into(),
                    );
                }
                HeredocSurfaceStep::SkipTo(next) => {
                    i = next;
                    continue;
                }
            }
        }
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            _ if c == delimiter && quote.is_none() && depth == 0 => {
                out.push(&s[start..i]);
                start = i + cl;
            }
            _ => {}
        }
        i += cl;
    }
    if depth != 0 {
        return Err(format!("unbalanced delimiters in `{s}`"));
    }
    out.push(&s[start..]);
    Ok(out)
}

fn validate_label(label: &str) -> Result<(), String> {
    if !is_valid_label(label) || matches!(label, "_" | "$" | "return") {
        return Err(format!("invalid Plasm program label `{label}`"));
    }
    Ok(())
}

fn is_valid_label(label: &str) -> bool {
    let mut chars = label.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !looks_like_domain_symbol(label)
}

fn looks_like_domain_symbol(label: &str) -> bool {
    let mut chars = label.chars();
    matches!(chars.next(), Some('e' | 'p' | 'm'))
        && matches!(chars.next(), Some(c) if c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_digit())
}

fn require_node(state: &CompileState<'_>, node: &str) -> Result<(), String> {
    if state.contains(node) {
        Ok(())
    } else {
        Err(format!("unknown Plasm program node `{node}`"))
    }
}

fn parse_projection(rhs: &str) -> Result<Option<(&str, &str)>, String> {
    let Some(open) = rhs.rfind('[') else {
        return Ok(None);
    };
    if !rhs.ends_with(']') {
        return Ok(None);
    }
    Ok(Some((&rhs[..open], &rhs[open + 1..rhs.len() - 1])))
}

fn parse_render(rhs: &str) -> Result<Option<(&str, &str, String)>, String> {
    let Some((head, rest)) = rhs.split_once("<<") else {
        return Ok(None);
    };
    let head = head.trim();
    if head.is_empty() {
        // Heredoc-only / value RHS starts with `<<TAG` — not `source[field] <<TAG` render.
        return Ok(None);
    }
    let Some((source, fields)) = parse_projection(head)? else {
        return Err("render syntax must be source[field,...] <<TAG".to_string());
    };
    let mut lines = rest.lines();
    let tag = lines
        .next()
        .map(str::trim)
        .ok_or_else(|| "render heredoc missing tag".to_string())?;
    let body = lines.collect::<Vec<_>>().join("\n");
    let end = body
        .rfind(&format!("\n{tag}"))
        .or_else(|| (body.trim() == tag).then_some(0))
        .ok_or_else(|| format!("render heredoc <<{tag} is not closed"))?;
    let template = if end == 0 {
        String::new()
    } else {
        body[..end].to_string()
    };
    Ok(Some((source, fields, template)))
}

fn parse_field_list(fields: &str) -> Result<Vec<String>, String> {
    let out = split_top_level(fields, ',')?
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if out.is_empty() {
        return Err("field list must be non-empty".to_string());
    }
    Ok(out)
}

fn parse_aggregates(args: &str) -> Result<Vec<crate::plasm_plan::AggregateSpec>, String> {
    split_top_level(args, ',')?
        .into_iter()
        .map(|raw| {
            let (name, rhs) = raw
                .split_once('=')
                .ok_or_else(|| format!("aggregate spec `{raw}` must be name=function(field)"))?;
            let name = OutputName::new(name.trim().to_string())?;
            let rhs = rhs.trim();
            if rhs == "count" {
                return Ok(crate::plasm_plan::AggregateSpec {
                    name,
                    function: crate::plasm_plan::AggregateFunction::Count,
                    field: None,
                });
            }
            let open = rhs
                .find('(')
                .ok_or_else(|| format!("aggregate function `{rhs}` must call a field"))?;
            let func = &rhs[..open];
            let field = rhs[open + 1..]
                .strip_suffix(')')
                .ok_or_else(|| format!("aggregate function `{rhs}` must end with `)`"))?;
            let function = match func {
                "sum" => AggregateFunction::Sum,
                "avg" => AggregateFunction::Avg,
                "min" => AggregateFunction::Min,
                "max" => AggregateFunction::Max,
                other => return Err(format!("unknown aggregate function `{other}`")),
            };
            Ok(crate::plasm_plan::AggregateSpec {
                name,
                function,
                field: Some(FieldPath::from_dotted(field.trim())?),
            })
        })
        .collect()
}

fn parse_plan_value_expr(
    raw: &str,
    state: &CompileState<'_>,
    row_binding: Option<&str>,
) -> Result<(PlanValue, Vec<serde_json::Value>), String> {
    let raw = raw.trim();
    if raw.starts_with('{') && raw.ends_with('}') {
        let mut inputs = Vec::new();
        let mut fields = BTreeMap::new();
        for part in split_top_level(&raw[1..raw.len() - 1], ',')? {
            let (k, v) = part
                .split_once(':')
                .ok_or_else(|| format!("object field `{part}` must be key: value"))?;
            let (value, child_inputs) = parse_plan_value_expr(v, state, row_binding)?;
            inputs.extend(child_inputs);
            fields.insert(k.trim().to_string(), value);
        }
        return Ok((PlanValue::Object { fields }, dedupe_inputs(inputs)));
    }
    if raw.starts_with('[') && raw.ends_with(']') {
        let mut inputs = Vec::new();
        let mut items = Vec::new();
        for part in split_top_level(&raw[1..raw.len() - 1], ',')? {
            let (value, child_inputs) = parse_plan_value_expr(part, state, row_binding)?;
            inputs.extend(child_inputs);
            items.push(value);
        }
        return Ok((PlanValue::Array { items }, dedupe_inputs(inputs)));
    }
    if let Some(path) = raw.strip_prefix("_.") {
        return Ok((
            PlanValue::BindingSymbol {
                binding: row_binding.unwrap_or("_").to_string(),
                path: path.split('.').map(str::to_string).collect(),
            },
            Vec::new(),
        ));
    }
    if let Some((node, path)) = raw.split_once('.') {
        if let Some(dep) = state.get(node) {
            return Ok((
                PlanValue::NodeSymbol {
                    node: node.to_string(),
                    alias: node.to_string(),
                    path: path.split('.').map(str::to_string).collect(),
                },
                vec![json!({
                    "node": node,
                    "alias": node,
                    "cardinality": if dep.singleton { "auto" } else { "singleton" }
                })],
            ));
        }
    }
    if state.contains(raw) {
        return Ok((
            PlanValue::EntityRefKey {
                api: String::new(),
                entity: String::new(),
                key: Box::new(PlanValue::Symbol {
                    path: raw.to_string(),
                }),
            },
            Vec::new(),
        ));
    }
    if raw.starts_with("<<") {
        return Ok((
            PlanValue::Template {
                template: raw.to_string(),
                input_bindings: Vec::new(),
            },
            Vec::new(),
        ));
    }
    let value = parse_literal(raw)?;
    Ok((PlanValue::Literal { value }, Vec::new()))
}

fn parse_literal(raw: &str) -> Result<serde_json::Value, String> {
    if raw.starts_with('"') || raw == "null" || raw == "true" || raw == "false" {
        return serde_json::from_str(raw).map_err(|e| format!("literal `{raw}`: {e}"));
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Ok(json!(n));
    }
    if let Ok(n) = raw.parse::<f64>() {
        return Ok(json!(n));
    }
    Ok(json!(raw))
}

fn rewrite_template_expr(
    expr: &str,
    state: &CompileState<'_>,
    row_binding: Option<&str>,
) -> Result<(String, Vec<serde_json::Value>), String> {
    let mut rewritten = expr.to_string();
    let mut uses = Vec::new();
    for node in &state.nodes {
        for path in find_node_paths(expr, &node.id) {
            let sentinel = format!("__plasm_dag_node_{}_{}__", node.id, path.replace('.', "_"));
            rewritten =
                rewritten.replace(&format!("{}.{}", node.id, path), &format!("\"{sentinel}\""));
            uses.push(json!({
                "node": node.id,
                "as": node.id,
            }));
        }
        rewritten = replace_bare_node_value(&rewritten, &node.id, node.expr.as_str());
    }
    if let Some(_binding) = row_binding {
        for path in find_node_paths(expr, "_") {
            let sentinel = format!("__plasm_dag_binding_{}__", path.replace('.', "_"));
            rewritten = rewritten.replace(&format!("_.{path}"), &format!("\"{sentinel}\""));
        }
    }
    Ok((rewritten, dedupe_uses(uses)))
}

fn find_node_paths(expr: &str, node: &str) -> Vec<String> {
    let needle = format!("{node}.");
    let mut out = Vec::new();
    let mut rest = expr;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        let path: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !path.is_empty() {
            out.push(path.trim_end_matches('.').to_string());
        }
        rest = &after[path.len()..];
    }
    out
}

fn replace_bare_node_value(expr: &str, node: &str, replacement: &str) -> String {
    let mut out = expr.to_string();
    for prefix in ["=", "(", ","] {
        out = out.replace(
            &format!("{prefix}{node}"),
            &format!("{prefix}{replacement}"),
        );
    }
    out
}

fn expr_template_json(
    parsed: &plasm_core::expr_parser::ParsedExpr,
    uses: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(&parsed.expr).map_err(|e| e.to_string())?;
    replace_sentinels(&mut value);
    Ok(json!({
        "expr": value,
        "projection": parsed.projection,
        "display_expr": crate::expr_display::expr_display(&parsed.expr),
        "input_bindings": uses.iter().map(|u| {
            json!({
                "from": u.get("as").and_then(|v| v.as_str()).unwrap_or_default(),
                "to": u.get("as").and_then(|v| v.as_str()).unwrap_or_default(),
            })
        }).collect::<Vec<_>>()
    }))
}

fn replace_sentinels(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) if s.starts_with("__plasm_dag_node_") => {
            let raw = s
                .trim_start_matches("__plasm_dag_node_")
                .trim_end_matches("__");
            let mut parts = raw.split('_');
            let node = parts.next().unwrap_or_default().to_string();
            let path = parts.map(str::to_string).collect::<Vec<_>>();
            *value = json!({ "__plasm_hole": { "kind": "node_input", "node": node, "alias": node, "path": path } });
        }
        serde_json::Value::String(s) if s.starts_with("__plasm_dag_binding_") => {
            let raw = s
                .trim_start_matches("__plasm_dag_binding_")
                .trim_end_matches("__");
            let path = raw.split('_').map(str::to_string).collect::<Vec<_>>();
            *value = json!({ "__plasm_hole": { "kind": "binding", "binding": "_", "path": path } });
        }
        serde_json::Value::Array(items) => {
            for item in items {
                replace_sentinels(item);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                replace_sentinels(item);
            }
        }
        _ => {}
    }
}

fn dedupe_uses(uses: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut seen = BTreeSet::new();
    uses.into_iter()
        .filter(|u| {
            let key = format!(
                "{}:{}",
                u.get("node").and_then(|v| v.as_str()).unwrap_or_default(),
                u.get("as").and_then(|v| v.as_str()).unwrap_or_default()
            );
            seen.insert(key)
        })
        .collect()
}

fn dedupe_inputs(inputs: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut seen = BTreeSet::new();
    inputs
        .into_iter()
        .filter(|u| {
            let key = format!(
                "{}:{}",
                u.get("node").and_then(|v| v.as_str()).unwrap_or_default(),
                u.get("alias").and_then(|v| v.as_str()).unwrap_or_default()
            );
            seen.insert(key)
        })
        .collect()
}

fn infer_surface_contract(
    session: &ExecuteSession,
    expr: &Expr,
) -> Result<
    (
        PlanNodeKind,
        QualifiedEntityKey,
        EffectClass,
        crate::plasm_plan::ResultShape,
    ),
    String,
> {
    let (mut kind, entity, effect, shape) = infer_surface_contract_from_expr(expr)?;
    if let Expr::Query(q) = expr {
        if let Some(capability_name) = q.capability_name.as_ref() {
            if let Some(cap) = session.cgs.capabilities.get(capability_name.as_str()) {
                if cap.kind == plasm_core::CapabilityKind::Search {
                    kind = PlanNodeKind::Search;
                }
            }
        }
    }
    let entry_id = session
        .domain_exposure
        .as_ref()
        .and_then(|e| e.catalog_entry_id_for_entity(entity.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| session.entry_id.clone());
    Ok((kind, QualifiedEntityKey { entry_id, entity }, effect, shape))
}

fn infer_surface_contract_from_expr(
    expr: &Expr,
) -> Result<
    (
        PlanNodeKind,
        String,
        EffectClass,
        crate::plasm_plan::ResultShape,
    ),
    String,
> {
    let (kind, entity, effect, shape) = match expr {
        Expr::Query(q) => (
            PlanNodeKind::Query,
            q.entity.as_str().to_string(),
            EffectClass::Read,
            crate::plasm_plan::ResultShape::List,
        ),
        Expr::Get(g) => (
            PlanNodeKind::Get,
            g.reference.entity_type.as_str().to_string(),
            EffectClass::Read,
            crate::plasm_plan::ResultShape::Single,
        ),
        Expr::Create(c) => (
            PlanNodeKind::Create,
            c.entity.as_str().to_string(),
            EffectClass::Write,
            crate::plasm_plan::ResultShape::MutationResult,
        ),
        Expr::Delete(d) => (
            PlanNodeKind::Delete,
            d.target.entity_type.as_str().to_string(),
            EffectClass::Write,
            crate::plasm_plan::ResultShape::SideEffectAck,
        ),
        Expr::Invoke(i) => (
            PlanNodeKind::Action,
            i.target.entity_type.as_str().to_string(),
            EffectClass::SideEffect,
            crate::plasm_plan::ResultShape::SideEffectAck,
        ),
        Expr::Chain(_) => (
            PlanNodeKind::Query,
            "Chain".to_string(),
            EffectClass::Read,
            crate::plasm_plan::ResultShape::List,
        ),
        Expr::Page(_) => (
            PlanNodeKind::Query,
            "__page__".to_string(),
            EffectClass::Read,
            crate::plasm_plan::ResultShape::Page,
        ),
    };
    Ok((kind, entity, effect, shape))
}

fn schema_from_output_fields<'a>(
    entity: &str,
    fields: impl Iterator<Item = &'a OutputName>,
    kind: SyntheticValueKind,
) -> SyntheticResultSchema {
    SyntheticResultSchema {
        entity: Some(entity.to_string()),
        fields: fields
            .map(|name| SyntheticFieldSchema {
                name: name.clone(),
                value_kind: kind,
                source: None,
            })
            .collect(),
    }
}

fn schema_from_aggregates(
    entity: &str,
    aggregates: &[crate::plasm_plan::AggregateSpec],
) -> SyntheticResultSchema {
    SyntheticResultSchema {
        entity: Some(entity.to_string()),
        fields: aggregates
            .iter()
            .map(|agg| SyntheticFieldSchema {
                name: agg.name.clone(),
                value_kind: if agg.function == AggregateFunction::Count {
                    SyntheticValueKind::Integer
                } else {
                    SyntheticValueKind::Number
                },
                source: None,
            })
            .collect(),
    }
}

fn single_unknown_schema(entity: &str) -> SyntheticResultSchema {
    SyntheticResultSchema {
        entity: Some(entity.to_string()),
        fields: vec![SyntheticFieldSchema {
            name: OutputName::new("value".to_string()).expect("constant non-empty"),
            value_kind: SyntheticValueKind::Unknown,
            source: None,
        }],
    }
}

fn looks_like_data_literal(rhs: &str) -> bool {
    let t = rhs.trim_start();
    t.starts_with('{') || t.starts_with('[') || t.starts_with('"') || t.starts_with("<<")
}

fn looks_like_plasm_effect_template(rhs: &str) -> bool {
    // Distinguish for-each side effects from `source => { … }` derive. `.m#` (DOMAIN methods) and
    // all readable verbs must register here—`.label(`, `.update(`, etc.—not just `.m`.
    rhs.contains(".m")
        || rhs.contains("=>")
        || rhs.contains(".update(")
        || rhs.contains(".create(")
        || rhs.contains(".delete(")
        || rhs.contains(".label(")
        || rhs.contains(".invoke(")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasm_plan_run::evaluate_plasm_plan_dry;
    use plasm_core::{CGS, CgsContext, DomainExposureSession, PromptPipelineConfig, load_schema};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_session() -> ExecuteSession {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cgs = Arc::new(
            load_schema(&root.join("tests/fixtures/execute_tiny")).expect("load execute_tiny"),
        );
        let mut ctxs = indexmap::IndexMap::new();
        ctxs.insert(
            "acme".into(),
            Arc::new(CgsContext::entry("acme", cgs.clone())),
        );
        let exp = DomainExposureSession::new(cgs.as_ref(), "acme", &["Product", "Category"]);
        ExecuteSession::new(
            "ph".into(),
            "p".into(),
            cgs.clone(),
            ctxs,
            "acme".into(),
            String::new(),
            String::new(),
            None,
            vec!["Product".into(), "Category".into()],
            Some(exp),
            None,
            None,
            cgs.catalog_cgs_hash_hex(),
        )
    }

    /// `tests/fixtures/scoped_create_tiny` — `product_update` (PATCH) + `product_label` (action) + `product_query` + relations.
    fn test_session_scoped_with_actions() -> ExecuteSession {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cgs = Arc::new(
            load_schema(&root.join("tests/fixtures/scoped_create_tiny"))
                .expect("load scoped_create_tiny"),
        );
        let mut ctxs = indexmap::IndexMap::new();
        ctxs.insert(
            "acme".into(),
            Arc::new(CgsContext::entry("acme", cgs.clone())),
        );
        let exp = DomainExposureSession::new(cgs.as_ref(), "acme", &["Product", "Category"]);
        ExecuteSession::new(
            "ph".into(),
            "p".into(),
            cgs.clone(),
            ctxs,
            "acme".into(),
            String::new(),
            String::new(),
            None,
            vec!["Product".into(), "Category".into()],
            Some(exp),
            None,
            None,
            cgs.catalog_cgs_hash_hex(),
        )
    }

    /// Primary session `entry_id` is `github`, but `Category` was exposed from `linear` in DOMAIN
    /// — plan `qualified_entity` must use the owning catalog, not the lexicographic primary.
    #[test]
    fn federated_surface_qualified_entity_matches_exposure_catalog() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cgs = Arc::new(
            load_schema(&root.join("tests/fixtures/execute_tiny")).expect("load execute_tiny"),
        );
        let mut ctxs = indexmap::IndexMap::new();
        ctxs.insert(
            "github".into(),
            Arc::new(CgsContext::entry("github", cgs.clone())),
        );
        ctxs.insert(
            "linear".into(),
            Arc::new(CgsContext::entry("linear", cgs.clone())),
        );
        let layers: Vec<&CGS> = vec![cgs.as_ref(), cgs.as_ref()];
        let mut exp = DomainExposureSession::new(cgs.as_ref(), "github", &["Product"]);
        exp.expose_entities(&layers, cgs.as_ref(), "linear", &["Category"]);
        let session = ExecuteSession::new(
            "ph".into(),
            "p".into(),
            cgs.clone(),
            ctxs,
            "github".into(),
            String::new(),
            String::new(),
            None,
            vec!["Product".into(), "Category".into()],
            Some(exp),
            None,
            None,
            cgs.catalog_cgs_hash_hex(),
        );
        let plan = compile_plasm_surface_line_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "t",
            "Category",
        )
        .expect("compile");
        let qe = &plan["nodes"][0]["qualified_entity"];
        assert_eq!(qe["entry_id"], "linear", "{plan}");
        assert_eq!(qe["entity"], "Category");
    }

    #[test]
    fn splits_bare_comma_plasm_roots() {
        let roots = split_bare_plasm_roots("Product, Product~\"bolt\"").expect("split");
        assert_eq!(roots, vec!["Product", "Product~\"bolt\""]);
    }

    #[test]
    fn compiles_bound_query_limit_render_dag_to_valid_plan() {
        let session = test_session();
        let source = r#"products = Product
top = products.limit(1)
doc = top[id,name] <<MD
{{ rows | length }} product
MD
doc"#;
        let plan = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "native",
            source,
        )
        .expect("compile");
        let dry = evaluate_plasm_plan_dry(&session, &plan).expect("dry");
        assert_eq!(dry.node_results.len(), 3);
        assert_eq!(plan["nodes"].as_array().map(Vec::len), Some(3));
    }

    /// `plasm-oss/README.md` §4: search + project + `=>` map derive + parallel final roots.
    #[test]
    fn readme_plasm_dag_search_project_derive_parallel_roots_compiles() {
        let session = test_session();
        let source = r#"search = Product~"bolt hardware"
summary = search[id, name]
cards = summary => { blurb: { id: _.id, name: _.name } }
summary, cards"#;
        let plan = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "readme",
            source,
        )
        .expect("compile");
        let dry = evaluate_plasm_plan_dry(&session, &plan).expect("dry");
        assert_eq!(dry.node_results.len(), 3, "{dry:?}");
        assert_eq!(
            plan["return"],
            json!({ "kind": "parallel", "nodes": ["summary", "cards"] })
        );
    }

    /// `plasm-oss/README.md` §4 — complex: Jinja public reply + `for_each` **update** + **case comment**
    /// (render `content` piped into `add_support_reply`). Same shape in federated sessions.
    #[test]
    fn readme_plasm_dag_jinja_for_each_category_and_product_compiles() {
        let session = test_session_scoped_with_actions();
        let source = r#"bucket = Category("c1")
header = bucket[id, name] <<MD
# **{% for r in rows %}{{ r.name }}{% endfor %}** (`{% for r in rows %}{{ r.id }}{% endfor %}`)
MD
matches = Product{owner="acme", repo="ingest", active=true}
candidates = matches.limit(3)
brief = candidates[id, name, category_id] <<MD
Hi there — we’ve pulled **{{ rows | length }}** line item(s) in `acme/ingest` and matched them against the **category** we show in the other root. Here’s what we’re doing next:

{% for r in rows %}
- We’re reconciling **`{{ r.name }}`** (`{{ r.id }}`) to the category you asked us to use.
{% endfor %}

Thanks for your patience — the team
MD
synced = candidates => Product(_.id).update(category_id=bucket.id)
posted = Product("p-helpline-case-1").add-support-reply(message=brief.content)
header, brief, synced, posted"#;
        let plan = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "readme-complex",
            source,
        )
        .expect("compile");
        let dry = evaluate_plasm_plan_dry(&session, &plan).expect("dry");
        assert_eq!(plan["return"]["kind"], "parallel");
        let kinds: Vec<_> = plan["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .map(|n| n["kind"].as_str().unwrap_or(""))
            .collect();
        assert!(
            kinds.contains(&"for_each"),
            "expected a for_each side-effect node, got {kinds:?}\n{plan:#}"
        );
        assert!(
            dry.node_results.len() >= 6,
            "expected category +2 renders + query/limit + for_each + case reply, got {dry:?}"
        );
    }

    #[test]
    fn rejects_return_prefixed_surface_line() {
        let session = test_session();
        let err = compile_plasm_surface_line_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "t",
            "return Product, Category",
        )
        .expect_err("return prefix");
        assert!(
            err.contains("return is not Plasm syntax"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn rejects_return_prefixed_final_roots_in_dag() {
        let session = test_session();
        let source = "products = Product\nreturn products";
        let err = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "x",
            source,
        )
        .expect_err("return");
        assert!(
            err.contains("return is not Plasm syntax"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn surface_line_plan_compiles_e1_with_page_size() {
        let session = test_session();
        let plan = compile_plasm_surface_line_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "t",
            "e1.page_size(100)",
        )
        .expect("compile");
        assert_eq!(plan["nodes"].as_array().map(|a| a.len()), Some(1));
        let dry = evaluate_plasm_plan_dry(&session, &plan).expect("dry");
        assert!(!dry.node_results.is_empty());
    }

    #[test]
    fn flattened_dag_bindings_get_newline_diagnostic() {
        let session = test_session();
        let source = r#"repo = e2("c1") commits = e1{p3=repo}.limit(20) commits"#;
        let err = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "flattened",
            source,
        )
        .expect_err("flattened input should fail with diagnostic");
        assert!(
            err.contains("real newline characters") && err.contains("Original parse error"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn flattened_dag_assignment_then_root_gets_newline_diagnostic() {
        let session = test_session();
        let source = r#"repo = e2("c1") e1{p3=repo}.sort(p2, desc).page_size(20)"#;
        let err = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "flattened-root",
            source,
        )
        .expect_err("flattened input should fail with diagnostic");
        assert!(
            err.contains("Do not separate bindings or final roots with spaces"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn flattened_dag_diagnostic_does_not_mask_heredoc_newline_errors() {
        let session = test_session();
        let source = "body = <<B hello B";
        let err = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "heredoc-flat",
            source,
        )
        .expect_err("bad heredoc should fail");
        assert!(
            !err.contains("Do not separate bindings or final roots with spaces"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn split_top_level_does_not_split_commas_inside_tagged_heredoc() {
        let parts = split_top_level("fn(<<T\na,b,c\nT\n), bar", ',').expect("split");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("a,b,c"), "{:?}", parts[0]);
        assert_eq!(parts[1].trim(), "bar");
    }

    #[test]
    fn parse_statements_errors_on_squashed_heredoc_opener() {
        let err = parse_statements("body = <<B # junk").expect_err("err");
        assert!(
            err.contains("opener") || err.contains("tag") || err.contains("newline"),
            "{err}"
        );
    }

    #[test]
    fn parse_statements_glued_heredoc_close() {
        let stmts = parse_statements("x = <<H\none\nH)").expect("parse");
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].contains("<<H"), "{:?}", stmts[0]);
    }

    #[test]
    fn split_bare_plasm_roots_multiline_heredoc_second_root() {
        let src = "Product, <<TXT\n,\nTXT";
        let roots = split_bare_plasm_roots(src).expect("roots");
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].trim(), "Product");
        assert!(
            roots[1].contains("<<TXT") && roots[1].contains(','),
            "{:?}",
            roots[1]
        );
    }

    #[test]
    fn multiline_heredoc_binding_then_parallel_roots_compiles() {
        let session = test_session();
        let source = "body = <<T\nhello\nT\nProduct, Category";
        let plan = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "heredoc-roots",
            source,
        )
        .expect("compile");
        assert_eq!(plan["return"]["kind"], "parallel");
    }

    fn github_repository_commit_session() -> ExecuteSession {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cgs = Arc::new(load_schema(&root.join("../../apis/github")).expect("load github"));
        let mut ctxs = indexmap::IndexMap::new();
        ctxs.insert(
            "github".into(),
            Arc::new(CgsContext::entry("github", cgs.clone())),
        );
        let exp = DomainExposureSession::new(cgs.as_ref(), "github", &["Repository", "Commit"]);
        ExecuteSession::new(
            "ph".into(),
            "p".into(),
            cgs.clone(),
            ctxs,
            "github".into(),
            String::new(),
            String::new(),
            None,
            vec!["Repository".into(), "Commit".into()],
            Some(exp),
            None,
            None,
            cgs.catalog_cgs_hash_hex(),
        )
    }

    /// `repo.<relation>` continues the bound repository Plasm and compiles to a `kind: relation` plan node.
    #[test]
    fn compiles_bound_node_ref_relation_chain_dag_to_valid_plan() {
        let session = github_repository_commit_session();
        let source = r#"repo = Repository(owner="ryan-s-roberts", repo="plasm-core")
commits = repo.commits
commits"#;
        let plan = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "github-node-ref-rel",
            source,
        )
        .expect("compile");
        let nodes = plan["nodes"].as_array().expect("nodes");
        assert_eq!(nodes.len(), 2, "{plan:#}");
        let rel = &nodes[1];
        assert_eq!(rel["kind"], "relation");
        assert_eq!(rel["relation"]["source"], "repo");
        assert_eq!(rel["relation"]["relation"], "commits");
        assert_eq!(rel["relation"]["target"]["entity"], "Commit");
        assert_eq!(rel["uses_result"][0]["node"], "repo");
        let dry = evaluate_plasm_plan_dry(&session, &plan).expect("dry");
        assert_eq!(
            dry.node_results[1]["simulation"]["kind"],
            "relation_traversal"
        );
    }

    #[test]
    fn compiles_node_ref_relation_limit_and_project() {
        let session = github_repository_commit_session();
        let source = r#"repo = Repository(owner="ryan-s-roberts", repo="plasm-core")
commits = repo.commits
limited = commits.limit(20)
projected = limited[sha,message]
projected"#;
        let plan = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "github-chain-limit-project",
            source,
        )
        .expect("compile");
        assert_eq!(plan["nodes"].as_array().map(Vec::len), Some(4));
        let dry = evaluate_plasm_plan_dry(&session, &plan).expect("dry");
        assert_eq!(dry.node_results.len(), 4, "{dry:?}");
    }

    #[test]
    fn rejects_continuation_from_compute_projection_anchor() {
        let session = github_repository_commit_session();
        let source = r#"repo = Repository(owner="ryan-s-roberts", repo="plasm-core")
trimmed = repo[id]
bad = trimmed.commits"#;
        let err = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "non-anchor",
            source,
        )
        .expect_err("compute projection is not a Plasm anchor");
        assert!(
            err.contains("not a Plasm expression anchor"),
            "unexpected: {err}"
        );
    }

    /// Direct postfix `.limit` on a surface expression must compile with the same plan shape as
    /// bind-first `label = expr` then `label.limit(n)` (unified language contract).
    #[test]
    fn direct_surface_limit_equivalent_to_bind_first_two_node_plan() {
        let session = github_repository_commit_session();
        let bind_first = r#"commits = Repository(owner="ryan-s-roberts", repo="plasm-core").commits
x = commits.limit(2)
x"#;
        let direct = r#"Repository(owner="ryan-s-roberts", repo="plasm-core").commits.limit(2)"#;
        let p1 = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "bind-first-limit",
            bind_first,
        )
        .expect("bind-first");
        let p2 = compile_plasm_dag_to_plan(
            &PromptPipelineConfig::default(),
            None,
            &session,
            "direct-limit",
            direct,
        )
        .expect("direct");
        let n1 = p1["nodes"].as_array().expect("nodes");
        let n2 = p2["nodes"].as_array().expect("nodes");
        assert_eq!(n1.len(), 2, "{p1:#}");
        assert_eq!(n2.len(), 2, "{p2:#}");
        let last1 = &n1[1];
        let last2 = &n2[1];
        assert_eq!(last1["kind"], "compute");
        assert_eq!(last2["kind"], "compute");
        let op1 = &last1["compute"]["op"];
        let op2 = &last2["compute"]["op"];
        assert_eq!(op1["kind"], "limit", "{op1:#}");
        assert_eq!(op2["kind"], "limit", "{op2:#}");
        assert_eq!(op1["count"], 2);
        assert_eq!(op2["count"], 2);
    }
}
