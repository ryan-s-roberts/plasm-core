//! Code Mode MCP helpers: parse Plasm effect plans and optional QuickJS bootstrap string.

use plasm_core::cgs_federation::FederationDispatch;
use plasm_core::expr_parser::{parse, parse_with_cgs_layers, ParseError, ParsedExpr};
use plasm_core::expr_simulation_bindings;
use plasm_core::render_intent_with_projection;
use plasm_core::render_intent_with_projection_federated;
use plasm_core::type_check_expr;
use plasm_core::type_check_expr_federated;
use plasm_core::TypeError;
use plasm_core::CGS;

use crate::code_mode_plan::{
    parse_plan_value, validate_plan_artifact, AggregateFunction, BindingName, ComputeOp,
    ComputeTemplate, EffectClass, EffectTemplate, FieldPath, InputAlias, Plan, PlanDataInput,
    PlanNode, PlanNodeId, PlanNodeKind, PlanValue, QualifiedEntityKey, ValidatedPlan,
    ValidatedPlanDataInput, ValidatedPlanNode,
};
use crate::execute_session::ExecuteSession;
use crate::expr_display::expr_display_resolved;
use crate::expr_display::expr_display_resolved_federated;
use crate::http_execute::{
    archive_code_mode_result_snapshot, execute_code_mode_plasm_line, execute_session_run_markdown,
    publish_code_mode_result_steps, PublishedResultStep,
};
use crate::incoming_auth::TenantPrincipal;
use crate::mcp_plasm_meta::PlasmMetaIndex;
use crate::server_state::PlasmHostState;
use crate::trace_hub::McpPlasmTraceSink;
use crate::trace_sink_emit::PlasmTraceContext;
use indexmap::IndexMap;
use plasm_core::{EntityName, Ref, Value};
use plasm_runtime::{
    CachedEntity, EntityCompleteness, ExecutionResult, ExecutionSource, ExecutionStats,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Parse a Plasm line to [`ParsedExpr`] (surface IR + optional projection) for the active session.
pub fn parse_parsed_expr_for_session(
    session: &ExecuteSession,
    line: &str,
) -> Result<ParsedExpr, ParseError> {
    if session.contexts_by_entry.len() <= 1 {
        return parse(line, session.cgs.as_ref());
    }
    let exp = match session.domain_exposure.as_ref() {
        Some(e) => e,
        None => {
            return parse(line, session.cgs.as_ref());
        }
    };
    let layers: Vec<&CGS> = session
        .contexts_by_entry
        .values()
        .map(|c| c.cgs.as_ref())
        .collect();
    let sym = exp.to_symbol_map();
    parse_with_cgs_layers(line, &layers, sym)
}

/// Type-check a parsed line against the session CGS (federated when multiple catalogs are loaded).
pub fn typecheck_parsed_for_session(
    session: &ExecuteSession,
    pe: &ParsedExpr,
) -> Result<(), TypeError> {
    if session.contexts_by_entry.len() <= 1 {
        return type_check_expr(&pe.expr, session.cgs.as_ref());
    }
    let Some(exposure) = session.domain_exposure.as_ref() else {
        return type_check_expr(&pe.expr, session.cgs.as_ref());
    };
    let fed =
        FederationDispatch::from_contexts_and_exposure(session.contexts_by_entry.clone(), exposure);
    type_check_expr_federated(&pe.expr, &fed, session.cgs.as_ref())
}

/// Simulated execution step: human **intent**, compact **il** (query `cap=` from schema), and **bindings** JSON, without HTTP or the `plasm` tool.
pub fn dry_run_simulation_for_session(
    session: &ExecuteSession,
    pe: &ParsedExpr,
) -> (String, String, serde_json::Value) {
    let intent = if session.contexts_by_entry.len() <= 1 {
        render_intent_with_projection(&pe.expr, pe.projection.as_deref(), session.cgs.as_ref())
    } else {
        match session.domain_exposure.as_ref() {
            None => render_intent_with_projection(
                &pe.expr,
                pe.projection.as_deref(),
                session.cgs.as_ref(),
            ),
            Some(exposure) => {
                let fed = FederationDispatch::from_contexts_and_exposure(
                    session.contexts_by_entry.clone(),
                    exposure,
                );
                render_intent_with_projection_federated(
                    &pe.expr,
                    pe.projection.as_deref(),
                    &fed,
                    session.cgs.as_ref(),
                )
            }
        }
    };
    let il = if session.contexts_by_entry.len() <= 1 {
        expr_display_resolved(&pe.expr, session.cgs.as_ref())
    } else {
        match session.domain_exposure.as_ref() {
            None => expr_display_resolved(&pe.expr, session.cgs.as_ref()),
            Some(exposure) => {
                let fed = FederationDispatch::from_contexts_and_exposure(
                    session.contexts_by_entry.clone(),
                    exposure,
                );
                expr_display_resolved_federated(&pe.expr, &fed, session.cgs.as_ref())
            }
        }
    };
    (intent, il, expr_simulation_bindings(&pe.expr))
}

/// Parse a single Plasm path expression string against the active execute session (federated or single).
pub fn parse_plasm_line_for_session(
    session: &ExecuteSession,
    line: &str,
) -> Result<(), ParseError> {
    parse_parsed_expr_for_session(session, line).map(|_| ())
}

/// Optional MCP `plasm` run hooks: meta index, distributed trace, hub sink. Pass when
/// `run: true` and the caller must match the MCP `execute` tool (same as batch `plasm` tracing).
pub struct CodeModePlasmRunHooks<'a> {
    pub meta_index: &'a mut PlasmMetaIndex,
    pub trace: PlasmTraceContext,
    pub sink: McpPlasmTraceSink,
}

/// Outcome of [`run_code_mode_plan`]: the same `node_results` / optional run payload shape as the MCP
/// `execute` tool (fenced JSON), without Markdown framing.
#[derive(Debug)]
pub struct CodeModePlanRunResult {
    pub version: serde_json::Value,
    /// One entry per `plan.nodes[]` with `ir`, `simulation`, and optional `id`.
    pub node_results: Vec<serde_json::Value>,
    pub graph_summary: serde_json::Value,
    /// Set when `run` is `true` and the engine returns Markdown (HTTP-backed run path).
    pub run_markdown: Option<String>,
    /// Optional `CallToolResult` `_meta` map (typically includes `plasm` steps when run snapshots exist).
    pub run_plasm_meta: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Dry-run a code-mode plan: validate, type-check, simulation JSON per node, and collect surface
/// `expr` strings in plan order (for `plasm` batch runs). [`run_code_mode_plan`] and MCP `execute`
/// both use this as the single authoritative parse/typecheck path.
#[derive(Debug)]
pub struct DryCodeModePlanEvaluation {
    pub version: serde_json::Value,
    pub name: Option<String>,
    pub plan: Plan,
    pub topological_order: Vec<String>,
    pub node_results: Vec<serde_json::Value>,
    /// One Plasm `expr` string per simple executable node. Staged Plans may dry-run but cannot use
    /// the legacy batch runner.
    pub expression_strings: Vec<String>,
    pub can_batch_run: bool,
    pub execution_unsupported: Vec<String>,
    pub graph_summary: serde_json::Value,
}

/// Optional archive/provenance fields shown at the top of compact dry-run text.
pub struct CodePlanDryRunTextMeta<'a> {
    pub plan_name: Option<&'a str>,
    pub plan_handle: &'a str,
    pub plan_uri: &'a str,
    pub canonical_plan_uri: &'a str,
    pub plan_hash: &'a str,
}

/// Parse, validate, and dry-run a typed code-mode `Plan`.
pub fn evaluate_code_mode_plan_dry(
    es: &ExecuteSession,
    plan: &serde_json::Value,
) -> Result<DryCodeModePlanEvaluation, String> {
    let plan = parse_plan_value(plan)?;
    let validated = validate_plan_artifact(&plan)?;
    evaluate_typed_plan_dry(es, &validated)
}

fn evaluate_typed_plan_dry(
    es: &ExecuteSession,
    validated: &ValidatedPlan,
) -> Result<DryCodeModePlanEvaluation, String> {
    let plan = validated.to_raw_plan();
    let plan = &plan;
    let version = serde_json::json!(plan.version);
    let mut out = Vec::new();
    let mut expression_strings: Vec<String> = Vec::new();
    let mut can_batch_run = true;
    let mut execution_unsupported = Vec::new();
    for node_id in &validated.topo {
        let i = *validated
            .node_indices
            .get(node_id)
            .ok_or_else(|| format!("validated node {:?} missing index", node_id.as_str()))?;
        let n = &plan.nodes[i];
        ensure_node_dispatchable(es, n, i)?;
        let inferred_approval = inferred_node_approval(n);
        if inferred_approval.is_some() {
            can_batch_run = false;
            execution_unsupported.push(format!(
                "node {:?} ({}) requires host-inferred approval",
                n.kind, n.id
            ));
        }
        if n.depends_on.is_empty() && n.uses_result.is_empty() && n.kind.has_surface_expr() {
            let expr = n
                .expr
                .as_deref()
                .ok_or_else(|| format!("validated node {} missing expr", n.id))?
                .trim();
            let pe = parse_parsed_expr_for_session(es, expr)
                .map_err(|e| format!("parse error in plan.nodes[{i}]: {e}"))?;
            typecheck_parsed_for_session(es, &pe)
                .map_err(|e| format!("type check in plan.nodes[{i}]: {e}"))?;
            expression_strings.push(expr.to_string());
            let (intent, il, bindings) = dry_run_simulation_for_session(es, &pe);
            out.push(serde_json::json!({
                "index": i,
                "ok": true,
                "id": n.id,
                "kind": n.kind,
                "qualified_entity": n.qualified_entity,
                "effect_class": n.effect_class,
                "result_shape": n.result_shape,
                "projection": n.projection,
                "predicates": n.predicates,
                "approval_gate": inferred_approval,
                "ir": {
                    "expr": pe.expr,
                    "projection": pe.projection
                },
                "type_check": "ok",
                "simulation": {
                    "intent": intent,
                    "il": il,
                    "bindings": bindings
                }
            }));
            continue;
        }

        can_batch_run = false;
        execution_unsupported.push(format!(
            "node {:?} ({}) requires staged execution",
            n.kind, n.id
        ));
        out.push(dry_stage_result(i, n));
    }
    Ok(DryCodeModePlanEvaluation {
        version,
        name: plan.name.clone(),
        plan: plan.clone(),
        topological_order: validated
            .topo
            .iter()
            .map(|id| id.as_str().to_string())
            .collect(),
        node_results: out,
        expression_strings,
        can_batch_run,
        execution_unsupported,
        graph_summary: graph_summary(plan),
    })
}

/// Render the canonical human-facing dry-run form: compact topology, roots, approvals, and returns.
pub fn render_code_mode_plan_dry_text(
    dry: &DryCodeModePlanEvaluation,
    archive: Option<CodePlanDryRunTextMeta<'_>>,
) -> String {
    let mut out = String::new();
    let plan = &dry.plan;
    let summary = &dry.graph_summary;
    let name = archive
        .as_ref()
        .and_then(|a| a.plan_name)
        .or(dry.name.as_deref().or(plan.name.as_deref()))
        .unwrap_or("<unnamed>");
    let roots = json_string_array(summary.get("parallelizable_roots"));
    let approvals = summary
        .get("approval_gates")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);
    let reads = json_string_array(summary.get("read_nodes")).len();
    let writes = json_string_array(summary.get("write_or_side_effect_nodes")).len();
    let staged = dry.execution_unsupported.len();

    let _ = writeln!(out, "code-plan dry-run");
    let _ = writeln!(out, "name: {name}");
    if let Some(a) = archive {
        let _ = writeln!(out, "handle: {} ({})", a.plan_handle, a.plan_uri);
        let _ = writeln!(out, "archive: {}", a.canonical_plan_uri);
        let _ = writeln!(out, "hash: {}", a.plan_hash);
    }
    let _ = writeln!(
        out,
        "nodes: {} total, {} read, {} write/side-effect, {} staged",
        plan.nodes.len(),
        reads,
        writes,
        staged
    );
    let _ = writeln!(
        out,
        "execution: {}",
        if dry.can_batch_run {
            "batchable"
        } else {
            "staged"
        }
    );
    let _ = writeln!(
        out,
        "roots: {}",
        if roots.is_empty() {
            "none".to_string()
        } else {
            roots.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "approvals: {}",
        if approvals == 0 {
            "none".to_string()
        } else {
            approvals.to_string()
        }
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "dag:");

    for (ordinal, id) in dry.topological_order.iter().enumerate() {
        let Some(node) = plan.nodes.iter().find(|n| n.id == *id) else {
            continue;
        };
        let deps = node_dependencies(node);
        let _ = writeln!(
            out,
            "{:02}. {}{} -> {} [{}; {}]",
            ordinal + 1,
            node.id,
            render_dependency_suffix(&deps),
            render_node_operation(node),
            render_effect_class(node.effect_class),
            render_result_shape(node.result_shape)
        );
        let uses = render_uses_result(node);
        if !uses.is_empty() {
            let _ = writeln!(out, "    uses: {}", uses.join(", "));
        }
        if let Some(approval) = inferred_node_approval(node) {
            if let Some(policy) = approval.get("policy_key").and_then(|v| v.as_str()) {
                let _ = writeln!(out, "    approval: {policy}");
            }
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "returns:");
    for line in render_return_lines(&plan.return_value) {
        let _ = writeln!(out, "- {line}");
    }
    out
}

fn json_string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn node_dependencies(node: &PlanNode) -> Vec<String> {
    let mut out = Vec::new();
    push_unique(&mut out, node.depends_on.iter().cloned());
    push_unique(&mut out, node.uses_result.iter().map(|u| u.node.clone()));
    if let Some(source) = &node.source {
        push_unique(&mut out, std::iter::once(source.clone()));
    }
    if let Some(derive) = &node.derive_template {
        if let Some(source) = &derive.source {
            push_unique(&mut out, std::iter::once(source.clone()));
        }
        push_unique(
            &mut out,
            derive.inputs.iter().map(|input| input.node.clone()),
        );
    }
    if let Some(compute) = &node.compute {
        push_unique(&mut out, std::iter::once(compute.source.clone()));
    }
    out
}

fn push_unique(out: &mut Vec<String>, values: impl IntoIterator<Item = String>) {
    for value in values {
        if !out.iter().any(|seen| seen == &value) {
            out.push(value);
        }
    }
}

fn render_dependency_suffix(deps: &[String]) -> String {
    if deps.is_empty() {
        String::new()
    } else {
        format!(" <- {}", deps.join(", "))
    }
}

fn render_uses_result(node: &PlanNode) -> Vec<String> {
    node.uses_result
        .iter()
        .map(|u| format!("{} as {}", u.node, u.r#as))
        .collect()
}

fn render_node_operation(node: &PlanNode) -> String {
    match node.kind {
        PlanNodeKind::Query
        | PlanNodeKind::Search
        | PlanNodeKind::Get
        | PlanNodeKind::Create
        | PlanNodeKind::Update
        | PlanNodeKind::Delete
        | PlanNodeKind::Action => render_surface_operation(node),
        PlanNodeKind::Data => node
            .data
            .as_ref()
            .map(|v| format!("data {}", render_plan_value(v)))
            .unwrap_or_else(|| "data <missing>".to_string()),
        PlanNodeKind::Derive => node
            .derive_template
            .as_ref()
            .map(render_derive_template)
            .unwrap_or_else(|| "derive <missing>".to_string()),
        PlanNodeKind::Compute => node
            .compute
            .as_ref()
            .map(render_compute_template)
            .unwrap_or_else(|| "compute <missing>".to_string()),
        PlanNodeKind::ForEach => {
            let source = node.source.as_deref().unwrap_or("<missing>");
            let binding = node.item_binding.as_deref().unwrap_or("item");
            let template = node
                .effect_template
                .as_ref()
                .map(|t| t.expr_template.as_str())
                .unwrap_or("<missing>");
            format!("for_each {source} as {binding} => {template}")
        }
    }
}

fn render_surface_operation(node: &PlanNode) -> String {
    let entity = node
        .qualified_entity
        .as_ref()
        .map(|q| format!("{}.{}", q.entry_id, q.entity))
        .unwrap_or_else(|| "<unqualified>".to_string());
    let expr = node.expr.as_deref().unwrap_or("<missing>");
    format!("{} {} <= {}", render_kind(node.kind), entity, expr)
}

fn render_derive_template(template: &crate::code_mode_plan::DeriveTemplate) -> String {
    match template.kind {
        crate::code_mode_plan::DeriveKind::Map => {
            let source = template.source.as_deref().unwrap_or("<missing>");
            let binding = template.item_binding.as_deref().unwrap_or("item");
            let inputs = render_data_inputs(&template.inputs);
            let input_suffix = if inputs.is_empty() {
                String::new()
            } else {
                format!(" with {}", inputs.join(", "))
            };
            format!(
                "map {source} as {binding}{input_suffix} => {}",
                render_plan_value(&template.value)
            )
        }
        crate::code_mode_plan::DeriveKind::Data => {
            format!("derive data {}", render_plan_value(&template.value))
        }
    }
}

fn render_data_inputs(inputs: &[PlanDataInput]) -> Vec<String> {
    inputs
        .iter()
        .map(|input| {
            format!(
                "{} as {} {}",
                input.node,
                input.alias,
                render_input_cardinality(input.cardinality)
            )
        })
        .collect()
}

fn render_input_cardinality(cardinality: crate::code_mode_plan::InputCardinality) -> &'static str {
    match cardinality {
        crate::code_mode_plan::InputCardinality::Auto => "auto",
        crate::code_mode_plan::InputCardinality::Singleton => "singleton",
    }
}

fn render_compute_template(compute: &ComputeTemplate) -> String {
    match &compute.op {
        ComputeOp::Project { fields } => {
            let fields = fields
                .iter()
                .map(|(name, path)| format!("{}={}", name.as_str(), path.dotted()))
                .collect::<Vec<_>>()
                .join(", ");
            format!("project {} -> {{{fields}}}", compute.source)
        }
        ComputeOp::Filter { predicates } => {
            let predicates = predicates
                .iter()
                .map(render_predicate)
                .collect::<Vec<_>>()
                .join(", ");
            format!("filter {} where {predicates}", compute.source)
        }
        ComputeOp::GroupBy { key, aggregates } => {
            format!(
                "group_by {} key={} -> {{{}}}",
                compute.source,
                key.dotted(),
                render_aggregates(aggregates)
            )
        }
        ComputeOp::Aggregate { aggregates } => {
            format!(
                "aggregate {} -> {{{}}}",
                compute.source,
                render_aggregates(aggregates)
            )
        }
        ComputeOp::Sort { key, descending } => format!(
            "sort {} by {} {}",
            compute.source,
            key.dotted(),
            if *descending { "desc" } else { "asc" }
        ),
        ComputeOp::Limit { count } => format!("limit {} count={count}", compute.source),
        ComputeOp::TableFromMatrix {
            columns,
            has_header,
        } => format!(
            "table {} columns=[{}] header={has_header}",
            compute.source,
            columns
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn render_aggregates(aggregates: &[crate::code_mode_plan::AggregateSpec]) -> String {
    aggregates
        .iter()
        .map(|agg| {
            let field = agg
                .field
                .as_ref()
                .map(FieldPath::dotted)
                .unwrap_or_else(|| "*".to_string());
            format!(
                "{}={}({field})",
                agg.name.as_str(),
                render_aggregate_function(agg.function)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_predicate(predicate: &crate::code_mode_plan::PlanPredicate) -> String {
    format!(
        "{}{}{}",
        predicate.field_path.join("."),
        render_predicate_op(predicate.op),
        render_plan_value(&predicate.value)
    )
}

fn render_plan_value(value: &PlanValue) -> String {
    match value {
        PlanValue::Literal { value } => render_json_value(value),
        PlanValue::Helper {
            name,
            args,
            display,
        } => display.clone().unwrap_or_else(|| {
            format!(
                "{}({})",
                name,
                args.iter()
                    .map(render_json_value)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        PlanValue::Symbol { path } => format!("${path}"),
        PlanValue::BindingSymbol { binding, path } => {
            let suffix = if path.is_empty() {
                String::new()
            } else {
                format!(".{}", path.join("."))
            };
            format!("${binding}{suffix}")
        }
        PlanValue::NodeSymbol { alias, path, .. } => {
            let suffix = if path.is_empty() {
                String::new()
            } else {
                format!(".{}", path.join("."))
            };
            format!("${alias}{suffix}")
        }
        PlanValue::Template { template, .. } => format!("template`{template}`"),
        PlanValue::Array { items } => {
            let mut rendered = items
                .iter()
                .take(5)
                .map(render_plan_value)
                .collect::<Vec<_>>();
            if items.len() > 5 {
                rendered.push("...".to_string());
            }
            format!("[{}]", rendered.join(", "))
        }
        PlanValue::Object { fields } => {
            let mut rendered = fields
                .iter()
                .take(8)
                .map(|(name, value)| format!("{name}: {}", render_plan_value(value)))
                .collect::<Vec<_>>();
            if fields.len() > 8 {
                rendered.push("...".to_string());
            }
            format!("{{{}}}", rendered.join(", "))
        }
    }
}

fn render_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("{s:?}"),
        serde_json::Value::Array(items) => {
            let mut rendered = items
                .iter()
                .take(5)
                .map(render_json_value)
                .collect::<Vec<_>>();
            if items.len() > 5 {
                rendered.push("...".to_string());
            }
            format!("[{}]", rendered.join(", "))
        }
        serde_json::Value::Object(obj) => {
            let mut rendered = obj
                .iter()
                .take(8)
                .map(|(name, value)| format!("{name}: {}", render_json_value(value)))
                .collect::<Vec<_>>();
            if obj.len() > 8 {
                rendered.push("...".to_string());
            }
            format!("{{{}}}", rendered.join(", "))
        }
        other => other.to_string(),
    }
}

fn render_return_lines(ret: &crate::code_mode_plan::PlanReturn) -> Vec<String> {
    match ret {
        crate::code_mode_plan::PlanReturn::Node(id) => vec![id.clone()],
        crate::code_mode_plan::PlanReturn::Parallel { parallel } => parallel
            .iter()
            .enumerate()
            .map(|(i, id)| format!("parallel[{}] -> {}", i, id))
            .collect(),
        crate::code_mode_plan::PlanReturn::Record(map) => map
            .iter()
            .map(|(name, id)| format!("{} -> {}", name.as_str(), id))
            .collect(),
    }
}

fn render_kind(kind: PlanNodeKind) -> &'static str {
    match kind {
        PlanNodeKind::Query => "query",
        PlanNodeKind::Search => "search",
        PlanNodeKind::Get => "get",
        PlanNodeKind::Create => "create",
        PlanNodeKind::Update => "update",
        PlanNodeKind::Delete => "delete",
        PlanNodeKind::Action => "action",
        PlanNodeKind::Data => "data",
        PlanNodeKind::Derive => "derive",
        PlanNodeKind::Compute => "compute",
        PlanNodeKind::ForEach => "for_each",
    }
}

fn render_effect_class(effect: EffectClass) -> &'static str {
    match effect {
        EffectClass::Read => "read",
        EffectClass::Write => "write",
        EffectClass::SideEffect => "side_effect",
        EffectClass::ArtifactRead => "artifact_read",
    }
}

fn render_result_shape(shape: crate::code_mode_plan::ResultShape) -> &'static str {
    match shape {
        crate::code_mode_plan::ResultShape::List => "list",
        crate::code_mode_plan::ResultShape::Single => "single",
        crate::code_mode_plan::ResultShape::MutationResult => "mutation_result",
        crate::code_mode_plan::ResultShape::SideEffectAck => "side_effect_ack",
        crate::code_mode_plan::ResultShape::Page => "page",
        crate::code_mode_plan::ResultShape::Artifact => "artifact",
    }
}

fn render_aggregate_function(function: AggregateFunction) -> &'static str {
    match function {
        AggregateFunction::Count => "count",
        AggregateFunction::Sum => "sum",
        AggregateFunction::Avg => "avg",
        AggregateFunction::Min => "min",
        AggregateFunction::Max => "max",
    }
}

fn render_predicate_op(op: crate::code_mode_plan::PlanPredicateOp) -> &'static str {
    match op {
        crate::code_mode_plan::PlanPredicateOp::Eq => "=",
        crate::code_mode_plan::PlanPredicateOp::Ne => "!=",
        crate::code_mode_plan::PlanPredicateOp::Lt => "<",
        crate::code_mode_plan::PlanPredicateOp::Lte => "<=",
        crate::code_mode_plan::PlanPredicateOp::Gt => ">",
        crate::code_mode_plan::PlanPredicateOp::Gte => ">=",
        crate::code_mode_plan::PlanPredicateOp::Contains => "~",
        crate::code_mode_plan::PlanPredicateOp::In => " in ",
        crate::code_mode_plan::PlanPredicateOp::Exists => " exists ",
    }
}

fn graph_summary(plan: &Plan) -> serde_json::Value {
    let mut read_nodes = Vec::new();
    let mut write_or_side_effect_nodes = Vec::new();
    let mut derive_nodes = Vec::new();
    let mut template_nodes = Vec::new();
    let mut approval_gates = Vec::new();
    let mut parallelizable_roots = Vec::new();

    for n in &plan.nodes {
        if node_dependencies(n).is_empty() {
            parallelizable_roots.push(n.id.clone());
        }
        match n.effect_class {
            EffectClass::Read => read_nodes.push(n.id.clone()),
            EffectClass::Write | EffectClass::SideEffect => {
                write_or_side_effect_nodes.push(n.id.clone())
            }
            EffectClass::ArtifactRead => derive_nodes.push(n.id.clone()),
        }
        if n.effect_template.is_some() {
            template_nodes.push(n.id.clone());
        }
        if let Some(approval) = inferred_node_approval(n) {
            approval_gates.push(approval);
        }
    }

    serde_json::json!({
        "node_count": plan.nodes.len(),
        "read_nodes": read_nodes,
        "write_or_side_effect_nodes": write_or_side_effect_nodes,
        "derive_nodes": derive_nodes,
        "template_nodes": template_nodes,
        "approval_gates": approval_gates,
        "parallelizable_roots": parallelizable_roots,
    })
}

fn inferred_node_approval(node: &PlanNode) -> Option<serde_json::Value> {
    if let Some(t) = node.effect_template.as_ref() {
        return inferred_template_approval(node, t);
    }
    if node_requires_approval(node.kind, node.effect_class) {
        let q = node.qualified_entity.as_ref()?;
        return Some(approval_gate_json(
            node.id.as_str(),
            q,
            node.kind,
            None,
            node.approval.as_deref(),
        ));
    }
    None
}

fn inferred_template_approval(
    node: &PlanNode,
    template: &EffectTemplate,
) -> Option<serde_json::Value> {
    if !node_requires_approval(template.kind, template.effect_class) {
        return None;
    }
    Some(approval_gate_json(
        node.id.as_str(),
        &template.qualified_entity,
        template.kind,
        action_name_from_template(template.expr_template.as_str()).as_deref(),
        node.approval.as_deref(),
    ))
}

fn node_requires_approval(kind: PlanNodeKind, effect_class: EffectClass) -> bool {
    matches!(
        kind,
        PlanNodeKind::Create | PlanNodeKind::Update | PlanNodeKind::Delete | PlanNodeKind::Action
    ) || matches!(effect_class, EffectClass::Write | EffectClass::SideEffect)
}

fn approval_gate_json(
    node_id: &str,
    q: &QualifiedEntityKey,
    kind: PlanNodeKind,
    action_name: Option<&str>,
    author_label: Option<&str>,
) -> serde_json::Value {
    let operation = action_name.unwrap_or(match kind {
        PlanNodeKind::Create => "create",
        PlanNodeKind::Update => "update",
        PlanNodeKind::Delete => "delete",
        PlanNodeKind::Action => "action",
        PlanNodeKind::Data => "data",
        PlanNodeKind::Query => "query",
        PlanNodeKind::Search => "search",
        PlanNodeKind::Get => "get",
        PlanNodeKind::Derive => "derive",
        PlanNodeKind::Compute => "compute",
        PlanNodeKind::ForEach => "for_each",
    });
    serde_json::json!({
        "node": node_id,
        "required": true,
        "policy_key": format!("{}.{}.{}", q.entry_id, q.entity, operation),
        "entry_id": q.entry_id,
        "entity": q.entity,
        "operation": operation,
        "author_label": author_label,
        "reason": format!("mutating capability {:?} on {}.{}", kind, q.entry_id, q.entity),
    })
}

fn action_name_from_template(expr_template: &str) -> Option<String> {
    let after_ref = expr_template.split(").").nth(1)?;
    let name = after_ref
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or_default()
        .trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn ensure_node_dispatchable(
    es: &ExecuteSession,
    node: &PlanNode,
    index: usize,
) -> Result<(), String> {
    let Some(q) = node.qualified_entity.as_ref() else {
        if es.contexts_by_entry.len() > 1 && node.kind.has_surface_expr() {
            return Err(format!(
                "plan.nodes[{index}] is missing qualified_entity in a federated session"
            ));
        }
        return Ok(());
    };
    let Some(ctx) = es.contexts_by_entry.get(&q.entry_id) else {
        return Err(format!(
            "plan.nodes[{index}].qualified_entity.entry_id {:?} is not loaded in this session",
            q.entry_id
        ));
    };
    if !ctx.cgs.entities.contains_key(q.entity.as_str()) {
        return Err(format!(
            "plan.nodes[{index}].qualified_entity entity {:?} is not present under entry_id {:?}",
            q.entity, q.entry_id
        ));
    }
    Ok(())
}

fn dry_stage_result(index: usize, n: &PlanNode) -> serde_json::Value {
    match n.kind {
        PlanNodeKind::ForEach => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id,
            "kind": n.kind,
            "effect_class": n.effect_class,
            "result_shape": n.result_shape,
            "projection": n.projection,
            "predicates": n.predicates,
            "depends_on": n.depends_on,
            "uses_result": n.uses_result,
            "source": n.source,
            "item_binding": n.item_binding,
            "approval": n.approval,
            "approval_gate": inferred_node_approval(n),
            "data": n.data,
            "derive_template": n.derive_template,
            "effect_template": n.effect_template,
            "simulation": {
                "kind": "template_stage",
                "max_write_set": {
                    "source": n.source,
                    "shape": "one template invocation per source row"
                },
                "execution": "requires phased Plan runner"
            }
        }),
        PlanNodeKind::Derive => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id,
            "kind": n.kind,
            "effect_class": n.effect_class,
            "result_shape": n.result_shape,
            "projection": n.projection,
            "predicates": n.predicates,
            "depends_on": n.depends_on,
            "uses_result": n.uses_result,
            "approval_gate": inferred_node_approval(n),
            "data": n.data,
            "derive_template": n.derive_template,
            "simulation": {
                "kind": "local_derivation",
                "execution": "runs after dependencies are materialized by the phased Plan runner"
            }
        }),
        PlanNodeKind::Compute => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id,
            "kind": n.kind,
            "effect_class": n.effect_class,
            "result_shape": n.result_shape,
            "projection": n.projection,
            "predicates": n.predicates,
            "depends_on": n.depends_on,
            "uses_result": n.uses_result,
            "approval_gate": inferred_node_approval(n),
            "compute": n.compute,
            "simulation": {
                "kind": "deterministic_compute",
                "execution": "materializes a synthetic Plasm result set via the phased Plan runner"
            }
        }),
        _ => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id,
            "kind": n.kind,
            "effect_class": n.effect_class,
            "result_shape": n.result_shape,
            "projection": n.projection,
            "predicates": n.predicates,
            "depends_on": n.depends_on,
            "uses_result": n.uses_result,
            "approval_gate": inferred_node_approval(n),
            "data": n.data,
            "derive_template": n.derive_template,
            "simulation": {
                "kind": "staged_effect",
                "execution": "requires phased Plan runner"
            }
        }),
    }
}

/// Code-mode / program-synthesis **plan** execution: validate each `expr`, materialize the same
/// per-node `simulation` as MCP `execute` (`run: false`), and optionally run the full batch
/// through [`execute_session_run_markdown`] (same as MCP `execute` with `run: true`).
pub async fn run_code_mode_plan(
    es: &ExecuteSession,
    st: &PlasmHostState,
    principal: Option<&TenantPrincipal>,
    prompt_hash: &str,
    session_id: &str,
    plan: &serde_json::Value,
    run: bool,
    mcp_plasm: Option<CodeModePlasmRunHooks<'_>>,
) -> Result<CodeModePlanRunResult, String> {
    let plan_typed = parse_plan_value(plan)?;
    let validated = validate_plan_artifact(&plan_typed)?;
    let dry = evaluate_typed_plan_dry(es, &validated)?;
    if !run {
        return Ok(CodeModePlanRunResult {
            version: dry.version,
            node_results: dry.node_results,
            graph_summary: dry.graph_summary,
            run_markdown: None,
            run_plasm_meta: None,
        });
    }
    if !dry.can_batch_run {
        return run_validated_plan_phased(
            es,
            st,
            prompt_hash,
            session_id,
            &validated,
            dry,
            mcp_plasm,
        )
        .await;
    }
    if dry.expression_strings.is_empty() {
        return Err("run mode: internal error (no expressions collected)".to_string());
    }
    let exprs = dry.expression_strings;
    let run_out = if let Some(mut h) = mcp_plasm {
        execute_session_run_markdown(
            st,
            principal,
            prompt_hash,
            session_id,
            exprs,
            Some(&mut h.meta_index),
            Some(h.trace),
            Some(h.sink),
        )
        .await
    } else {
        execute_session_run_markdown(
            st,
            principal,
            prompt_hash,
            session_id,
            exprs,
            None,
            None,
            None,
        )
        .await
    }
    .map_err(|e| e)?;
    Ok(CodeModePlanRunResult {
        version: dry.version,
        node_results: dry.node_results,
        graph_summary: dry.graph_summary,
        run_markdown: Some(run_out.markdown),
        run_plasm_meta: run_out.tool_meta,
    })
}

#[derive(Debug, Clone)]
struct MaterializedNode {
    result: ExecutionResult,
    artifact: Option<crate::run_artifacts::RunArtifactHandle>,
    display: String,
    projection: Option<Vec<String>>,
}

async fn run_validated_plan_phased(
    es: &ExecuteSession,
    st: &PlasmHostState,
    prompt_hash: &str,
    session_id: &str,
    validated: &ValidatedPlan,
    dry: DryCodeModePlanEvaluation,
    mcp_plasm: Option<CodeModePlasmRunHooks<'_>>,
) -> Result<CodeModePlanRunResult, String> {
    let _ = prompt_hash;
    let mut materialized: BTreeMap<PlanNodeId, MaterializedNode> = BTreeMap::new();
    let mut trace = None;
    let mut sink = None;
    let mut meta_index = None;
    if let Some(hooks) = mcp_plasm {
        trace = Some(hooks.trace);
        sink = Some(hooks.sink);
        meta_index = Some(hooks.meta_index);
    }
    let _ = sink;
    for node_id in &validated.topo {
        let idx = *validated
            .node_indices
            .get(node_id)
            .ok_or_else(|| format!("validated node {:?} missing index", node_id.as_str()))?;
        let node = &validated.nodes[idx];
        let raw_node = node.to_plan_node();
        if inferred_node_approval(&raw_node).is_some() {
            return Err(format!(
                "Plan execution blocked: node {:?} ({}) requires host-inferred approval",
                node.kind(),
                node.id()
            ));
        }
        let mat = match node {
            ValidatedPlanNode::Surface(surface) => {
                let (parsed, result, artifact) = execute_code_mode_plasm_line(
                    st,
                    es,
                    session_id,
                    &surface.expr,
                    trace.as_ref(),
                    idx as i64,
                )
                .await?;
                MaterializedNode {
                    display: crate::expr_display::expr_display(&parsed.expr),
                    projection: parsed.projection,
                    result,
                    artifact,
                }
            }
            ValidatedPlanNode::Data(data) => {
                materialize_synthetic_node(
                    st,
                    es,
                    session_id,
                    &raw_node,
                    plan_value_to_rows(&data.data)?,
                    trace.as_ref(),
                )
                .await?
            }
            ValidatedPlanNode::Derive(derive) => {
                let source_rows = materialized_rows(&materialized, &derive.source)?;
                let input_rows = materialized_singleton_inputs(&materialized, &derive.inputs)?;
                let mut rows = Vec::with_capacity(source_rows.len());
                for row in source_rows {
                    let scope = EvalScope {
                        row: &row,
                        binding: Some(&derive.item_binding),
                    };
                    let inputs = InputEnv { rows: &input_rows };
                    let env = PlanEvalEnv { scope, inputs };
                    rows.push(eval_plan_value(&derive.value, &env)?);
                }
                materialize_synthetic_node(st, es, session_id, &raw_node, rows, trace.as_ref())
                    .await?
            }
            ValidatedPlanNode::Compute(compute) => {
                let rows = eval_compute(&compute.compute, &materialized)?;
                materialize_synthetic_node(st, es, session_id, &raw_node, rows, trace.as_ref())
                    .await?
            }
            ValidatedPlanNode::ForEach(for_each) => {
                return Err(format!(
                    "Plan execution blocked: mutating for_each node {} requires approval receipts",
                    for_each.id
                ));
            }
        };
        materialized.insert(node.id().clone(), mat);
    }

    let return_refs = validated.return_value.refs();
    let mut steps = Vec::new();
    for node_ref in return_refs {
        let mat = materialized
            .get(&PlanNodeId::new(node_ref.as_str().to_string())?)
            .ok_or_else(|| {
                format!(
                    "plan.return materialized node {:?} missing",
                    node_ref.as_str()
                )
            })?;
        steps.push(PublishedResultStep {
            display: mat.display.clone(),
            projection: mat.projection.clone(),
            result: mat.result.clone(),
            artifact: mat.artifact.clone(),
        });
    }
    let out = publish_code_mode_result_steps(es.cgs.as_ref().into(), meta_index, &steps);
    Ok(CodeModePlanRunResult {
        version: dry.version,
        node_results: dry.node_results,
        graph_summary: dry.graph_summary,
        run_markdown: Some(out.markdown),
        run_plasm_meta: out.tool_meta,
    })
}

async fn materialize_synthetic_node(
    st: &PlasmHostState,
    es: &ExecuteSession,
    session_id: &str,
    node: &PlanNode,
    rows: Vec<serde_json::Value>,
    trace: Option<&PlasmTraceContext>,
) -> Result<MaterializedNode, String> {
    let entity = node
        .compute
        .as_ref()
        .and_then(|c| c.schema.entity.clone())
        .unwrap_or_else(|| format!("PlanComputed_{}", node.id));
    let full_entities = json_rows_to_entities(&entity, &rows);
    let request_fingerprints = vec![compute_fingerprint(node, &rows)];
    let full_result = ExecutionResult {
        count: full_entities.len(),
        entities: full_entities.clone(),
        has_more: false,
        pagination_resume: None,
        paging_handle: None,
        source: ExecutionSource::Cache,
        stats: ExecutionStats {
            duration_ms: 0,
            network_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
        },
        request_fingerprints: request_fingerprints.clone(),
    };
    let artifact = archive_code_mode_result_snapshot(
        st,
        es,
        session_id,
        vec![format!("plan.compute({})", node.id)],
        &full_result,
        trace,
    )
    .await?;
    let page_size = node
        .compute
        .as_ref()
        .and_then(|c| c.page_size)
        .unwrap_or(50);
    let (entities, has_more, paging_handle) = if full_entities.len() > page_size {
        let first = full_entities[..page_size].to_vec();
        let handle = es.register_synthetic_paging_continuation(
            crate::execute_session::SyntheticPageCursor {
                node_id: node.id.clone(),
                entity_type: entity.clone(),
                rows: full_entities,
                offset: page_size,
                page_size,
                request_fingerprints: request_fingerprints.clone(),
            },
            trace.and_then(|t| t.logical_session_ref.as_deref()),
        );
        (first, true, Some(handle))
    } else {
        (full_result.entities.clone(), false, None)
    };
    Ok(MaterializedNode {
        display: synthetic_node_display(node),
        projection: synthetic_projection(node),
        result: ExecutionResult {
            count: entities.len(),
            entities,
            has_more,
            pagination_resume: None,
            paging_handle,
            source: ExecutionSource::Cache,
            stats: full_result.stats,
            request_fingerprints,
        },
        artifact: Some(artifact),
    })
}

fn synthetic_node_display(node: &PlanNode) -> String {
    match node.kind {
        PlanNodeKind::Data => format!("plan.data({})", node.id),
        PlanNodeKind::Derive => format!("plan.derive({})", node.id),
        PlanNodeKind::Compute => format!("plan.compute({})", node.id),
        _ => format!("plan.stage({})", node.id),
    }
}

fn materialized_rows(
    materialized: &BTreeMap<PlanNodeId, MaterializedNode>,
    source: &PlanNodeId,
) -> Result<Vec<serde_json::Value>, String> {
    let mat = materialized.get(source).ok_or_else(|| {
        format!(
            "source node {:?} has not been materialized",
            source.as_str()
        )
    })?;
    Ok(mat
        .result
        .entities
        .iter()
        .map(CachedEntity::payload_to_json)
        .collect())
}

fn materialized_singleton_inputs(
    materialized: &BTreeMap<PlanNodeId, MaterializedNode>,
    inputs: &[ValidatedPlanDataInput],
) -> Result<BTreeMap<InputAlias, serde_json::Value>, String> {
    let mut out = BTreeMap::new();
    for input in inputs {
        let mat = materialized.get(&input.node).ok_or_else(|| {
            format!(
                "input node {:?} for alias {:?} has not been materialized",
                input.node.as_str(),
                input.alias.as_str()
            )
        })?;
        if mat.result.has_more || mat.result.entities.len() != 1 {
            return Err(format!(
                "Plan input {:?} for alias {:?} expected one row for {:?} broadcast, got {}",
                input.node.as_str(),
                input.alias.as_str(),
                input.proof,
                mat.result.entities.len()
            ));
        }
        let row = mat
            .result
            .entities
            .first()
            .map(CachedEntity::payload_to_json)
            .ok_or_else(|| {
                format!(
                    "Plan input {:?} for alias {:?} expected one row but was empty",
                    input.node.as_str(),
                    input.alias.as_str()
                )
            })?;
        out.insert(input.alias.clone(), row);
    }
    Ok(out)
}

fn plan_value_to_rows(value: &PlanValue) -> Result<Vec<serde_json::Value>, String> {
    let inputs = BTreeMap::new();
    let scope = EvalScope {
        row: &serde_json::Value::Null,
        binding: None,
    };
    let input_env = InputEnv { rows: &inputs };
    let env = PlanEvalEnv {
        scope,
        inputs: input_env,
    };
    let json = eval_plan_value(value, &env)?;
    Ok(match json {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    })
}

fn eval_compute(
    compute: &ComputeTemplate,
    materialized: &BTreeMap<PlanNodeId, MaterializedNode>,
) -> Result<Vec<serde_json::Value>, String> {
    let source = PlanNodeId::new(compute.source.clone())?;
    let rows = materialized_rows(materialized, &source)?;
    match &compute.op {
        ComputeOp::Project { fields } => rows
            .iter()
            .map(|row| {
                let mut out = serde_json::Map::new();
                for (name, path) in fields {
                    out.insert(
                        name.as_str().to_string(),
                        value_at_path(row, path)
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    );
                }
                Ok(serde_json::Value::Object(out))
            })
            .collect(),
        ComputeOp::Filter { predicates } => rows
            .into_iter()
            .filter(|row| predicates.iter().all(|p| predicate_matches(row, p)))
            .map(Ok)
            .collect(),
        ComputeOp::GroupBy { key, aggregates } => group_rows(&rows, key, aggregates),
        ComputeOp::Aggregate { aggregates } => aggregate_rows(&rows, aggregates),
        ComputeOp::Sort { key, descending } => {
            let mut sorted = rows;
            sorted.sort_by(|a, b| {
                json_sort_key(value_at_path(a, key)).cmp(&json_sort_key(value_at_path(b, key)))
            });
            if *descending {
                sorted.reverse();
            }
            Ok(sorted)
        }
        ComputeOp::Limit { count } => Ok(rows.into_iter().take(*count).collect()),
        ComputeOp::TableFromMatrix {
            columns,
            has_header,
        } => table_from_matrix(&rows, columns, *has_header),
    }
}

struct EvalScope<'a> {
    row: &'a serde_json::Value,
    binding: Option<&'a BindingName>,
}

struct InputEnv<'a> {
    rows: &'a BTreeMap<InputAlias, serde_json::Value>,
}

struct PlanEvalEnv<'a> {
    scope: EvalScope<'a>,
    inputs: InputEnv<'a>,
}

fn eval_plan_value(value: &PlanValue, env: &PlanEvalEnv<'_>) -> Result<serde_json::Value, String> {
    match value {
        PlanValue::Literal { value } => Ok(value.clone()),
        PlanValue::Helper { display, args, .. } => Ok(display
            .as_ref()
            .map(|s| serde_json::Value::String(s.clone()))
            .unwrap_or_else(|| serde_json::Value::Array(args.clone()))),
        PlanValue::Symbol { path } => {
            let path = strip_binding(path, env.scope.binding);
            Ok(value_at_dotted(env.scope.row, path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        PlanValue::BindingSymbol { binding, path } => {
            if env.scope.binding.map(BindingName::as_str) != Some(binding.as_str()) {
                return Err(format!(
                    "binding symbol references unknown binding {binding:?}"
                ));
            }
            Ok(value_at_segments(env.scope.row, path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        PlanValue::NodeSymbol { alias, path, .. } => {
            let alias = InputAlias::new(alias.clone())?;
            let row = env.inputs.rows.get(&alias).ok_or_else(|| {
                format!(
                    "node symbol references missing input alias {:?}",
                    alias.as_str()
                )
            })?;
            Ok(value_at_segments(row, path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        PlanValue::Template { template, .. } => {
            Ok(serde_json::Value::String(render_template(template, env)))
        }
        PlanValue::Array { items } => Ok(serde_json::Value::Array(
            items
                .iter()
                .map(|item| eval_plan_value(item, env))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        PlanValue::Object { fields } => {
            let mut out = serde_json::Map::new();
            for (k, v) in fields {
                out.insert(k.clone(), eval_plan_value(v, env)?);
            }
            Ok(serde_json::Value::Object(out))
        }
    }
}

fn strip_binding<'a>(path: &'a str, binding: Option<&BindingName>) -> &'a str {
    if let Some(binding) = binding {
        let binding = binding.as_str();
        if path == binding {
            return "";
        }
        if let Some(rest) = path.strip_prefix(&format!("{binding}.")) {
            return rest;
        }
    }
    path
}

fn render_template(template: &str, env: &PlanEvalEnv<'_>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let raw_path = &after[..end];
        let rendered = resolve_template_path(raw_path, env)
            .map(json_scalar_display)
            .unwrap_or_default();
        out.push_str(&rendered);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn resolve_template_path<'a>(
    raw_path: &str,
    env: &'a PlanEvalEnv<'_>,
) -> Option<&'a serde_json::Value> {
    if let Some(binding) = env.scope.binding {
        if raw_path == binding.as_str() || raw_path.starts_with(&format!("{binding}.")) {
            return value_at_dotted(env.scope.row, strip_binding(raw_path, Some(binding)));
        }
    }
    let (alias, rest) = raw_path
        .split_once('.')
        .map_or((raw_path, ""), |(alias, rest)| (alias, rest));
    let alias = InputAlias::new(alias.to_string()).ok()?;
    env.inputs
        .rows
        .get(&alias)
        .and_then(|row| value_at_dotted(row, rest))
}

fn value_at_path<'a>(
    row: &'a serde_json::Value,
    path: &FieldPath,
) -> Option<&'a serde_json::Value> {
    let mut cur = row;
    for segment in path.segments() {
        cur = cur.get(segment)?;
    }
    Some(cur)
}

fn value_at_dotted<'a>(row: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    if path.is_empty() {
        return Some(row);
    }
    let mut cur = row;
    for segment in path.split('.').filter(|s| !s.is_empty()) {
        cur = cur.get(segment)?;
    }
    Some(cur)
}

fn value_at_segments<'a>(
    row: &'a serde_json::Value,
    path: &[String],
) -> Option<&'a serde_json::Value> {
    let mut cur = row;
    for segment in path {
        cur = cur.get(segment)?;
    }
    Some(cur)
}

fn predicate_matches(row: &serde_json::Value, pred: &crate::code_mode_plan::PlanPredicate) -> bool {
    let Ok(path) = FieldPath::new(pred.field_path.clone()) else {
        return false;
    };
    let lhs = value_at_path(row, &path).unwrap_or(&serde_json::Value::Null);
    let rhs = match &pred.value {
        PlanValue::Literal { value } => value,
        _ => return false,
    };
    match pred.op {
        crate::code_mode_plan::PlanPredicateOp::Eq => lhs == rhs,
        crate::code_mode_plan::PlanPredicateOp::Ne => lhs != rhs,
        crate::code_mode_plan::PlanPredicateOp::Exists => !lhs.is_null(),
        crate::code_mode_plan::PlanPredicateOp::Contains => lhs
            .as_str()
            .zip(rhs.as_str())
            .map(|(l, r)| l.contains(r))
            .unwrap_or(false),
        crate::code_mode_plan::PlanPredicateOp::In => rhs
            .as_array()
            .map(|items| items.iter().any(|item| item == lhs))
            .unwrap_or(false),
        crate::code_mode_plan::PlanPredicateOp::Lt => json_number(lhs) < json_number(rhs),
        crate::code_mode_plan::PlanPredicateOp::Lte => json_number(lhs) <= json_number(rhs),
        crate::code_mode_plan::PlanPredicateOp::Gt => json_number(lhs) > json_number(rhs),
        crate::code_mode_plan::PlanPredicateOp::Gte => json_number(lhs) >= json_number(rhs),
    }
}

fn group_rows(
    rows: &[serde_json::Value],
    key: &FieldPath,
    aggregates: &[crate::code_mode_plan::AggregateSpec],
) -> Result<Vec<serde_json::Value>, String> {
    let mut groups: BTreeMap<String, Vec<&serde_json::Value>> = BTreeMap::new();
    for row in rows {
        let k = value_at_path(row, key)
            .map(json_scalar_display)
            .unwrap_or_default();
        groups.entry(k).or_default().push(row);
    }
    let mut out = Vec::new();
    for (key_value, rows) in groups {
        let mut obj = serde_json::Map::new();
        obj.insert("key".to_string(), serde_json::Value::String(key_value));
        append_aggregates(&mut obj, &rows, aggregates)?;
        out.push(serde_json::Value::Object(obj));
    }
    Ok(out)
}

fn aggregate_rows(
    rows: &[serde_json::Value],
    aggregates: &[crate::code_mode_plan::AggregateSpec],
) -> Result<Vec<serde_json::Value>, String> {
    let refs = rows.iter().collect::<Vec<_>>();
    let mut obj = serde_json::Map::new();
    append_aggregates(&mut obj, &refs, aggregates)?;
    Ok(vec![serde_json::Value::Object(obj)])
}

fn append_aggregates(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    rows: &[&serde_json::Value],
    aggregates: &[crate::code_mode_plan::AggregateSpec],
) -> Result<(), String> {
    for agg in aggregates {
        let value = match agg.function {
            AggregateFunction::Count => serde_json::json!(rows.len()),
            AggregateFunction::Sum => {
                serde_json::json!(aggregate_numbers(rows, agg.field.as_ref())
                    .iter()
                    .sum::<f64>())
            }
            AggregateFunction::Avg => {
                let nums = aggregate_numbers(rows, agg.field.as_ref());
                serde_json::json!(if nums.is_empty() {
                    0.0
                } else {
                    nums.iter().sum::<f64>() / nums.len() as f64
                })
            }
            AggregateFunction::Min => aggregate_numbers(rows, agg.field.as_ref())
                .into_iter()
                .reduce(f64::min)
                .map(|n| serde_json::json!(n))
                .unwrap_or(serde_json::Value::Null),
            AggregateFunction::Max => aggregate_numbers(rows, agg.field.as_ref())
                .into_iter()
                .reduce(f64::max)
                .map(|n| serde_json::json!(n))
                .unwrap_or(serde_json::Value::Null),
        };
        obj.insert(agg.name.as_str().to_string(), value);
    }
    Ok(())
}

fn aggregate_numbers(rows: &[&serde_json::Value], field: Option<&FieldPath>) -> Vec<f64> {
    rows.iter()
        .filter_map(|row| {
            field
                .and_then(|f| value_at_path(row, f))
                .and_then(json_number)
        })
        .collect()
}

fn table_from_matrix(
    rows: &[serde_json::Value],
    columns: &[crate::code_mode_plan::OutputName],
    has_header: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let matrix = rows
        .first()
        .and_then(|row| row.get("value").or_else(|| row.get("values")))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "table_from_matrix source must contain a value/values array".to_string())?;
    let start = usize::from(has_header);
    Ok(matrix
        .iter()
        .skip(start)
        .filter_map(|row| row.as_array())
        .map(|cells| {
            let mut obj = serde_json::Map::new();
            for (idx, col) in columns.iter().enumerate() {
                obj.insert(
                    col.as_str().to_string(),
                    cells.get(idx).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            serde_json::Value::Object(obj)
        })
        .collect())
}

fn json_number(v: &serde_json::Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_i64().map(|n| n as f64))
}

fn json_scalar_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn json_sort_key(v: Option<&serde_json::Value>) -> String {
    v.map(json_scalar_display).unwrap_or_default()
}

fn json_rows_to_entities(entity: &str, rows: &[serde_json::Value]) -> Vec<CachedEntity> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut fields = IndexMap::new();
            match row {
                serde_json::Value::Object(obj) => {
                    for (k, v) in obj {
                        fields.insert(k.clone(), json_to_plasm_value(v));
                    }
                }
                other => {
                    fields.insert("value".to_string(), json_to_plasm_value(other));
                }
            }
            CachedEntity {
                reference: Ref::new(
                    EntityName::new(entity.to_string()),
                    format!("synthetic-{}", idx + 1),
                ),
                fields,
                relations: IndexMap::new(),
                last_updated: 0,
                version: 1,
                completeness: EntityCompleteness::Complete,
            }
        })
        .collect()
}

fn json_to_plasm_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Integer)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => {
            Value::Array(items.iter().map(json_to_plasm_value).collect())
        }
        serde_json::Value::Object(obj) => Value::Object(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_plasm_value(v)))
                .collect::<IndexMap<_, _>>(),
        ),
    }
}

fn synthetic_projection(node: &PlanNode) -> Option<Vec<String>> {
    node.compute.as_ref().map(|compute| {
        compute
            .schema
            .fields
            .iter()
            .map(|f| f.name.as_str().to_string())
            .collect()
    })
}

fn compute_fingerprint(node: &PlanNode, rows: &[serde_json::Value]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(node.id.as_bytes());
    match serde_json::to_vec(&node.compute) {
        Ok(bytes) => hasher.update(bytes),
        Err(e) => hasher.update(format!("compute-serialization-error:{e}").as_bytes()),
    }
    match serde_json::to_vec(rows) {
        Ok(bytes) => hasher.update(bytes),
        Err(e) => hasher.update(format!("rows-serialization-error:{e}").as_bytes()),
    }
    format!("plan-compute:{}", hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plasm_core::load_schema;
    use plasm_core::CgsContext;
    use plasm_core::DomainExposureSession;
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
        let exp = DomainExposureSession::new(cgs.as_ref(), "acme", &["Product"]);
        ExecuteSession::new(
            "ph".into(),
            "p".into(),
            cgs.clone(),
            ctxs,
            "acme".into(),
            String::new(),
            String::new(),
            None,
            vec!["Product".into()],
            Some(exp),
            None,
            None,
            cgs.catalog_cgs_hash_hex(),
        )
    }

    #[test]
    fn plan_parses_product_query() {
        let s = test_session();
        let pe = parse_parsed_expr_for_session(&s, "Product").expect("parse");
        let v = serde_json::json!({ "expr": pe.expr, "projection": pe.projection });
        assert!(v.get("expr").is_some());
    }

    #[test]
    fn dry_run_typechecks_product_query() {
        let s = test_session();
        let pe = parse_parsed_expr_for_session(&s, "Product").expect("parse");
        typecheck_parsed_for_session(&s, &pe).expect("typecheck");
    }

    #[test]
    fn dry_run_simulation_includes_intent_il_and_bindings() {
        let s = test_session();
        let pe = parse_parsed_expr_for_session(&s, "Product").expect("parse");
        let (intent, il, bindings) = dry_run_simulation_for_session(&s, &pe);
        assert!(
            intent.contains("Query") && intent.contains("Product"),
            "{intent}"
        );
        assert!(il.contains("cap=product_list"), "il must resolve cap: {il}");
        let m = bindings.as_object().expect("object");
        assert_eq!(m.get("op").and_then(|v| v.as_str()), Some("query"));
    }

    #[test]
    fn evaluate_code_mode_plan_dry_matches_single_node() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "read-products",
            "nodes": [{
                "id": "n0",
                "kind": "query",
                "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                "expr": "Product",
                "effect_class": "read",
                "result_shape": "list"
            }],
            "return": "n0"
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(dry.expression_strings, vec!["Product".to_string()]);
        assert_eq!(dry.node_results.len(), 1);
    }

    #[test]
    fn dry_run_text_renders_dependency_dag_snapshot() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "product-summary",
            "nodes": [
                {
                    "id": "products",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "summary",
                    "kind": "compute",
                    "effect_class": "artifact_read",
                    "result_shape": "list",
                    "compute": {
                        "source": "products",
                        "op": { "kind": "project", "fields": { "sku": ["id"], "name": ["name"] } },
                        "schema": {
                            "entity": "PlanProject",
                            "fields": [
                                { "name": "sku", "value_kind": "unknown", "source": ["id"] },
                                { "name": "name", "value_kind": "unknown", "source": ["name"] }
                            ]
                        }
                    }
                },
                {
                    "id": "cards",
                    "kind": "derive",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "derive_template": {
                        "kind": "map",
                        "source": "summary",
                        "item_binding": "product",
                        "value": {
                            "kind": "object",
                            "fields": {
                                "title": { "kind": "template", "template": "${product.name}", "input_bindings": [{ "from": "product.name", "to": "" }] }
                            }
                        }
                    },
                    "depends_on": ["summary"],
                    "uses_result": [{ "node": "summary", "as": "product" }]
                }
            ],
            "return": { "summary": "summary", "cards": "cards" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        let text = render_code_mode_plan_dry_text(
            &dry,
            Some(CodePlanDryRunTextMeta {
                plan_name: None,
                plan_handle: "p7",
                plan_uri: "plasm://session/s0/p/7",
                canonical_plan_uri: "plasm://execute/ph/s/plan/uuid",
                plan_hash: "abc123",
            }),
        );
        insta::assert_snapshot!(
            text,
            @r###"
code-plan dry-run
name: product-summary
handle: p7 (plasm://session/s0/p/7)
archive: plasm://execute/ph/s/plan/uuid
hash: abc123
nodes: 3 total, 1 read, 0 write/side-effect, 2 staged
execution: staged
roots: products
approvals: none

dag:
01. products -> query acme.Product <= Product [read; list]
02. summary <- products -> project products -> {name=name, sku=id} [artifact_read; list]
03. cards <- summary -> map summary as product => {title: template`${product.name}`} [artifact_read; artifact]
    uses: summary as product

returns:
- cards -> cards
- summary -> summary
"###
        );
        assert!(!text.contains("node_results"));
        assert!(!text.contains("\"dry_run\""));
    }

    #[test]
    fn scoped_node_symbols_evaluate_against_singleton_inputs() {
        let value = PlanValue::Object {
            fields: BTreeMap::from([
                (
                    "title".to_string(),
                    PlanValue::Template {
                        template: "${p.name} uses ${moveFacts.move}".to_string(),
                        input_bindings: vec![],
                    },
                ),
                (
                    "power".to_string(),
                    PlanValue::NodeSymbol {
                        node: "moveFacts".to_string(),
                        alias: "moveFacts".to_string(),
                        path: vec!["power".to_string()],
                    },
                ),
            ]),
        };
        let row = serde_json::json!({ "name": "pikachu" });
        let inputs = BTreeMap::from([(
            InputAlias::new("moveFacts".to_string()).expect("alias"),
            serde_json::json!({ "move": "thunderbolt", "power": 90 }),
        )]);
        let binding = BindingName::new("p".to_string()).expect("binding");
        let scope = EvalScope {
            row: &row,
            binding: Some(&binding),
        };
        let input_env = InputEnv { rows: &inputs };
        let env = PlanEvalEnv {
            scope,
            inputs: input_env,
        };
        let out = eval_plan_value(&value, &env).expect("eval");
        assert_eq!(out["title"], "pikachu uses thunderbolt");
        assert_eq!(out["power"], 90);
    }

    #[test]
    fn validation_rejects_ambiguous_auto_cross_node_input() {
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "products",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "tags",
                    "kind": "data",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "data": { "kind": "literal", "value": [{ "tag": "a" }] }
                },
                {
                    "id": "cards",
                    "kind": "derive",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "derive_template": {
                        "kind": "map",
                        "source": "tags",
                        "item_binding": "tag",
                        "inputs": [{ "node": "products", "alias": "products", "cardinality": "auto" }],
                        "value": { "kind": "node_symbol", "node": "products", "alias": "products", "path": ["name"] }
                    }
                }
            ],
            "return": "cards"
        });
        let err = crate::code_mode_plan::validate_plan_value(&plan).expect_err("ambiguous input");
        assert!(err.contains("not statically singleton"), "{err}");
    }

    #[test]
    fn evaluate_code_mode_plan_dry_reports_for_each_stage() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "label-products",
            "nodes": [
                {
                    "id": "find",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "label",
                    "kind": "for_each",
                    "effect_class": "side_effect",
                    "result_shape": "side_effect_ack",
                    "source": "find",
                    "item_binding": "product",
                    "depends_on": ["find"],
                    "uses_result": [{ "node": "find", "as": "product" }],
                    "approval": "label_products",
                    "effect_template": {
                        "kind": "action",
                        "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                        "expr_template": "Product(${product.id}).label(label=\"stale\")",
                        "effect_class": "side_effect",
                        "result_shape": "side_effect_ack"
                    }
                }
            ],
            "return": { "products": "find", "labeled": "label" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert!(!dry.can_batch_run);
        assert_eq!(dry.node_results.len(), 2);
        assert_eq!(dry.node_results[1]["simulation"]["kind"], "template_stage");
        assert_eq!(
            dry.node_results[1]["approval_gate"]["policy_key"],
            "acme.Product.label"
        );
    }

    #[test]
    fn mutating_for_each_infers_approval_without_agent_label() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "label-products",
            "nodes": [
                {
                    "id": "find",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "label",
                    "kind": "for_each",
                    "effect_class": "side_effect",
                    "result_shape": "side_effect_ack",
                    "source": "find",
                    "item_binding": "product",
                    "depends_on": ["find"],
                    "uses_result": [{ "node": "find", "as": "product" }],
                    "effect_template": {
                        "kind": "action",
                        "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                        "expr_template": "Product(${product.id}).label(label=\"stale\")",
                        "effect_class": "side_effect",
                        "result_shape": "side_effect_ack"
                    }
                }
            ],
            "return": { "products": "find", "labeled": "label" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(
            dry.graph_summary["approval_gates"][0]["policy_key"],
            "acme.Product.label"
        );
    }
}
