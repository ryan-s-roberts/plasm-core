//! Code Mode MCP helpers: parse Plasm effect plans and optional QuickJS bootstrap string.

use plasm_core::cgs_federation::FederationDispatch;
use plasm_core::expr_parser::{parse, parse_with_cgs_layers, ParseError, ParsedExpr};
use plasm_core::expr_simulation_bindings;
use plasm_core::render_intent_with_projection;
use plasm_core::render_intent_with_projection_federated;
use plasm_core::type_check_expr;
use plasm_core::type_check_expr_federated;
use plasm_core::DomainExposureSession;
use plasm_core::TypeError;
use plasm_core::CGS;

use crate::code_mode_plan::{
    parse_plan_value, validate_plan_artifact, AggregateFunction, BindingName, ComputeOp,
    ComputeTemplate, EffectClass, FieldPath, InputAlias, Plan, PlanExprTemplate, PlanNodeId,
    PlanNodeKind, PlanResultUse, PlanValue, QualifiedEntityKey, ValidatedDeriveNode,
    ValidatedForEachNode, ValidatedPlan, ValidatedPlanDataInput, ValidatedPlanExprTemplate,
    ValidatedPlanNode, ValidatedPlanReturn, ValidatedPlanState, ValidatedSurfaceNode,
};
use crate::execute_session::ExecuteSession;
use crate::expr_display::expr_display_resolved;
use crate::expr_display::expr_display_resolved_federated;
use crate::http_execute::{
    archive_code_mode_result_snapshot, execute_code_mode_parsed_expr,
    publish_code_mode_result_steps, trace_record_code_mode_plasm_line, PublishedResultStep,
};
use crate::incoming_auth::TenantPrincipal;
use crate::mcp_plasm_meta::PlasmMetaIndex;
use crate::server_state::PlasmHostState;
use crate::trace_hub::McpPlasmTraceSink;
use crate::trace_sink_emit::PlasmTraceContext;
use indexmap::IndexMap;
use plasm_core::{CapabilityKind, EntityName, Expr, Ref, Value};
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

fn entry_scoped_execute_session(
    session: &ExecuteSession,
    qualified_entity: Option<&QualifiedEntityKey>,
) -> Result<ExecuteSession, String> {
    let Some(q) = qualified_entity else {
        return Ok(session.clone());
    };
    if session.contexts_by_entry.len() <= 1 && session.entry_id == q.entry_id {
        return Ok(session.clone());
    }
    let ctx = session.contexts_by_entry.get(&q.entry_id).ok_or_else(|| {
        format!(
            "Code Mode node targets catalog {:?}, but that catalog is not loaded in this execute session",
            q.entry_id
        )
    })?;
    let mut scoped = session.clone();
    scoped.cgs = ctx.cgs.clone();
    scoped.contexts_by_entry = IndexMap::from([(q.entry_id.clone(), ctx.clone())]);
    scoped.entry_id = q.entry_id.clone();
    scoped.http_backend = Some(ctx.cgs.http_backend.clone());
    scoped.entities = ctx
        .cgs
        .entities
        .keys()
        .map(|name| name.as_str().to_string())
        .collect();
    let focus = [q.entity.as_str()];
    scoped.domain_exposure = Some(DomainExposureSession::new(
        ctx.cgs.as_ref(),
        q.entry_id.as_str(),
        &focus,
    ));
    Ok(scoped)
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

/// Dry-run a code-mode plan: validate, type-check, and render simulation JSON per node.
#[derive(Debug)]
pub struct DryCodeModePlanEvaluation {
    pub version: serde_json::Value,
    pub name: Option<String>,
    plan: Plan<ValidatedPlanState>,
    pub topological_order: Vec<String>,
    pub node_results: Vec<serde_json::Value>,
    pub can_batch_run: bool,
    pub staged_nodes: Vec<String>,
    pub execution_unsupported: Vec<String>,
    pub graph_summary: serde_json::Value,
}

impl DryCodeModePlanEvaluation {
    pub fn validated_plan(&self) -> &Plan<ValidatedPlanState> {
        &self.plan
    }
}

/// Optional archive/provenance fields shown at the top of compact dry-run text.
pub struct CodePlanDryRunTextMeta<'a> {
    pub plan_name: Option<&'a str>,
    pub plan_handle: &'a str,
    pub plan_uri: &'a str,
    pub canonical_plan_uri: &'a str,
    pub plan_hash: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeModeApprovalDecision {
    Approved,
}

#[derive(Debug, Clone)]
struct CodeModeApprovalReceipt {
    decision: CodeModeApprovalDecision,
    policy: &'static str,
    gate: serde_json::Value,
}

/// Host-owned approval policy for Code Mode write/side-effect nodes.
///
/// The current product default is intentionally automatic so Code Mode can execute mutating plans
/// while the real user/tenant approval surface is built above this boundary.
#[derive(Debug, Clone)]
struct CodeModeApprovalPolicy {
    mode: CodeModeApprovalMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeModeApprovalMode {
    AutoApprove,
}

impl CodeModeApprovalPolicy {
    fn automatic() -> Self {
        Self {
            mode: CodeModeApprovalMode::AutoApprove,
        }
    }

    fn review(&self, gate: serde_json::Value) -> CodeModeApprovalReceipt {
        match self.mode {
            CodeModeApprovalMode::AutoApprove => CodeModeApprovalReceipt {
                decision: CodeModeApprovalDecision::Approved,
                policy: "host.auto_approve",
                gate,
            },
        }
    }
}

/// Parse, validate, and dry-run a typed code-mode `Plan`.
pub(crate) fn evaluate_code_mode_plan_dry(
    es: &ExecuteSession,
    plan: &serde_json::Value,
) -> Result<DryCodeModePlanEvaluation, String> {
    let plan = parse_plan_value(plan)?;
    let validated = validate_plan_artifact(&plan)?;
    evaluate_validated_code_mode_plan_dry(es, &validated)
}

pub fn evaluate_validated_code_mode_plan_dry(
    es: &ExecuteSession,
    validated: &ValidatedPlan,
) -> Result<DryCodeModePlanEvaluation, String> {
    let plan = validated.artifact();
    let version = serde_json::json!(plan.version);
    let mut out = Vec::new();
    let mut can_batch_run = true;
    let mut staged_nodes = Vec::new();
    let mut execution_unsupported = Vec::new();
    for node_id in validated.topological_order() {
        let i = validated
            .node_index(node_id)
            .ok_or_else(|| format!("validated node {:?} missing index", node_id.as_str()))?;
        let n = &plan.nodes[i];
        ensure_node_dispatchable(es, n, i)?;
        if let ValidatedPlanNode::RelationTraversal(relation) = n {
            let pe = ParsedExpr {
                expr: relation.relation.ir.expr.clone(),
                projection: relation.relation.ir.projection.clone(),
            };
            typecheck_parsed_for_session(es, &pe)
                .map_err(|e| format!("type check in plan.nodes[{i}].relation.expr: {e}"))?;
            ensure_relation_expr_matches_plan(es, relation, &pe, i)?;
        }
        let inferred_approval = inferred_node_approval(n);
        if n.depends_on().is_empty() && n.uses_result().is_empty() {
            let Some(surface) = n.as_surface() else {
                can_batch_run = false;
                staged_nodes.push(format!("{} ({:?})", n.id(), n.kind()));
                out.push(dry_stage_result(i, n));
                continue;
            };
            let ir = surface
                .ir
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{i}] requires staged IR execution"))?;
            let scoped_es = entry_scoped_execute_session(es, surface.qualified_entity.as_ref())?;
            let pe = ParsedExpr {
                expr: ir.expr.clone(),
                projection: ir.projection.clone(),
            };
            typecheck_parsed_for_session(&scoped_es, &pe)
                .map_err(|e| format!("type check in plan.nodes[{i}]: {e}"))?;
            ensure_surface_expr_matches_plan_kind(&scoped_es, surface, &pe, i)?;
            let (intent, il, bindings) = dry_run_simulation_for_session(&scoped_es, &pe);
            let expr = ir
                .display_expr
                .as_deref()
                .or(surface.display_expr.as_deref())
                .unwrap_or("<ir>");
            out.push(serde_json::json!({
                "index": i,
                "ok": true,
                "id": n.id().as_str(),
                "kind": n.kind(),
                "operation": render_node_operation(n),
                "qualified_entity": surface.qualified_entity,
                "effect_class": n.effect_class(),
                "result_shape": n.result_shape(),
                "projection": surface.projection,
                "predicates": surface.predicates,
                "approval_gate": inferred_approval,
                "ir": {
                    "expr": pe.expr,
                    "projection": pe.projection
                },
                "execution_contract": {
                    "entry_id": surface.qualified_entity.as_ref().map(|q| q.entry_id.as_str()).unwrap_or(es.entry_id.as_str()),
                    "entity": surface.qualified_entity.as_ref().map(|q| q.entity.as_str()),
                    "display_expr": expr,
                    "ir": pe.expr,
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
        staged_nodes.push(format!("{} ({:?})", n.id(), n.kind()));
        out.push(dry_stage_result(i, n));
    }
    Ok(DryCodeModePlanEvaluation {
        version,
        name: plan.name.clone(),
        plan: plan.clone(),
        topological_order: validated
            .topological_order()
            .iter()
            .map(|id| id.as_str().to_string())
            .collect(),
        node_results: out,
        can_batch_run,
        staged_nodes,
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
    let plan = dry.validated_plan();
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
    let warnings = json_string_array(summary.get("warnings"));
    let boundedness_facts = json_string_array(summary.get("boundedness_facts"));
    let staged = dry.staged_nodes.len();

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
    if !warnings.is_empty() {
        let _ = writeln!(out, "warnings:");
        for warning in warnings {
            let _ = writeln!(out, "- {warning}");
        }
        let _ = writeln!(out);
    }
    if !boundedness_facts.is_empty() {
        let _ = writeln!(out, "boundedness:");
        for fact in boundedness_facts {
            let _ = writeln!(out, "- {fact}");
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "dag:");

    for (ordinal, id) in dry.topological_order.iter().enumerate() {
        let Some(node) = plan.nodes.iter().find(|n| n.id().as_str() == id) else {
            continue;
        };
        let deps = node_dependencies(node);
        let _ = writeln!(
            out,
            "{:02}. {}{} -> {} [{}; {}]",
            ordinal + 1,
            node.id(),
            render_dependency_suffix(&deps),
            render_node_operation(node),
            render_effect_class(node.effect_class()),
            render_result_shape(node.result_shape())
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

/// Structured DAG payload for trace/UI renderers. This is the machine-readable companion to the
/// compact dry-run text, so clients do not have to parse Markdown to draw plan topology.
pub fn code_mode_plan_dag_json(dry: &DryCodeModePlanEvaluation) -> serde_json::Value {
    let plan = dry.validated_plan();
    let nodes = plan
        .nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "id": node.id().as_str(),
                "kind": node.kind(),
                "effect_class": node.effect_class(),
                "result_shape": node.result_shape(),
                "dependencies": node_dependencies(node),
                "uses_result": render_uses_result(node),
                "operation": render_node_operation(node),
            })
        })
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    for node in &plan.nodes {
        for from in node_dependencies(node) {
            edges.push(serde_json::json!({
                "from": from,
                "to": node.id().as_str(),
            }));
        }
    }
    serde_json::json!({
        "version": plan.version,
        "name": dry.name.clone(),
        "nodes": nodes,
        "edges": edges,
        "topological_order": dry.topological_order.clone(),
        "returns": render_return_lines(&plan.return_value),
        "summary": dry.graph_summary.clone(),
    })
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

fn node_dependencies(node: &ValidatedPlanNode) -> Vec<String> {
    let mut out = Vec::new();
    push_unique(
        &mut out,
        node.depends_on().iter().map(|id| id.as_str().to_string()),
    );
    push_unique(&mut out, node.uses_result().iter().map(|u| u.node.clone()));
    match node {
        ValidatedPlanNode::Derive(n) => {
            push_unique(&mut out, std::iter::once(n.source.as_str().to_string()));
            push_unique(
                &mut out,
                n.inputs.iter().map(|input| input.node.as_str().to_string()),
            );
        }
        ValidatedPlanNode::Compute(n) => {
            push_unique(&mut out, std::iter::once(n.compute.source.clone()));
        }
        ValidatedPlanNode::ForEach(n) => {
            push_unique(&mut out, std::iter::once(n.source.as_str().to_string()));
        }
        ValidatedPlanNode::RelationTraversal(n) => {
            push_unique(
                &mut out,
                std::iter::once(n.relation.source.as_str().to_string()),
            );
        }
        _ => {}
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

fn render_uses_result(node: &ValidatedPlanNode) -> Vec<String> {
    node.uses_result()
        .iter()
        .map(|u| format!("{} as {}", u.node, u.r#as))
        .collect()
}

fn render_node_operation(node: &ValidatedPlanNode) -> String {
    match node {
        ValidatedPlanNode::Surface(n) => render_surface_operation(n),
        ValidatedPlanNode::Data(n) => format!("data {}", render_plan_value(&n.data)),
        ValidatedPlanNode::Derive(n) => render_derive_template(n),
        ValidatedPlanNode::Compute(n) => render_compute_template(&n.compute),
        ValidatedPlanNode::RelationTraversal(n) => {
            let source = n.relation.source.as_str();
            let relation = n.relation.relation.as_str();
            let target = format!(
                "{}.{}",
                n.relation.target.entry_id, n.relation.target.entity
            );
            format!(
                "relation {source}.{relation} -> {target} <= {}",
                render_plan_expr_ir(&n.relation.ir)
            )
        }
        ValidatedPlanNode::ForEach(n) => {
            let source = n.source.as_str();
            let binding = n.item_binding.as_str();
            let template = render_effect_template_expr(&n.effect_template);
            format!("for_each {source} as {binding} => {template}")
        }
    }
}

fn render_surface_operation(node: &ValidatedSurfaceNode) -> String {
    let entity = node
        .qualified_entity
        .as_ref()
        .map(|q| format!("{}.{}", q.entry_id, q.entity))
        .unwrap_or_else(|| "<unqualified>".to_string());
    let expr = node
        .ir
        .as_ref()
        .map(render_plan_expr_ir)
        .or_else(|| node.ir_template.as_ref().map(render_plan_expr_template))
        .or_else(|| node.display_expr.clone())
        .unwrap_or_else(|| "<typed Plasm IR>".to_string());
    format!("{} {} <= {}", render_kind(node.kind), entity, expr)
}

fn render_plan_expr_ir(ir: &crate::code_mode_plan::ValidatedPlanExprIr) -> String {
    ir.display_expr
        .clone()
        .unwrap_or_else(|| crate::expr_display::expr_display(&ir.expr))
}

fn render_plan_expr_template(
    template: &crate::code_mode_plan::ValidatedPlanExprTemplate,
) -> String {
    template
        .display_expr
        .clone()
        .unwrap_or_else(|| "<typed Plasm IR template>".to_string())
}

fn render_effect_template_expr(template: &crate::code_mode_plan::EffectTemplate) -> String {
    if !template.expr_template.trim().is_empty() {
        template.expr_template.clone()
    } else {
        template
            .ir_template
            .display_expr
            .clone()
            .unwrap_or_else(|| "<typed Plasm IR template>".to_string())
    }
}

fn render_derive_template(template: &ValidatedDeriveNode) -> String {
    let source = template.source.as_str();
    let binding = template.item_binding.as_str();
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

fn render_data_inputs(inputs: &[ValidatedPlanDataInput]) -> Vec<String> {
    inputs
        .iter()
        .map(|input| {
            format!(
                "{} as {} {}",
                input.node.as_str(),
                input.alias.as_str(),
                render_input_cardinality(input.proof)
            )
        })
        .collect()
}

fn render_input_cardinality(proof: crate::code_mode_plan::InputCardinalityProof) -> &'static str {
    match proof {
        crate::code_mode_plan::InputCardinalityProof::StaticSingleton => "static-singleton",
        crate::code_mode_plan::InputCardinalityProof::RuntimeCheckedSingleton => {
            "runtime-checked-singleton"
        }
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
        PlanValue::EntityRefKey { key, .. } => render_plan_value(key),
        PlanValue::Array { items } => {
            if items.is_empty() {
                return "[0 items]".to_string();
            }
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
            if fields.is_empty() {
                return "{0 fields}".to_string();
            }
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
            if items.is_empty() {
                return "[0 items]".to_string();
            }
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
            if obj.is_empty() {
                return "{0 fields}".to_string();
            }
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

fn render_return_lines(ret: &ValidatedPlanReturn) -> Vec<String> {
    match ret {
        ValidatedPlanReturn::Node(id) => vec![id.as_str().to_string()],
        ValidatedPlanReturn::Parallel { parallel } => parallel
            .iter()
            .enumerate()
            .map(|(i, id)| format!("parallel[{}] -> {}", i, id.as_str()))
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
        PlanNodeKind::Relation => "relation",
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

fn graph_summary(plan: &Plan<ValidatedPlanState>) -> serde_json::Value {
    let mut read_nodes = Vec::new();
    let mut write_or_side_effect_nodes = Vec::new();
    let mut derive_nodes = Vec::new();
    let mut template_nodes = Vec::new();
    let mut approval_gates = Vec::new();
    let mut parallelizable_roots = Vec::new();
    let mut warnings = Vec::new();
    let mut boundedness_facts = Vec::new();

    for n in &plan.nodes {
        if node_dependencies(n).is_empty() {
            parallelizable_roots.push(n.id().as_str().to_string());
        }
        match n.effect_class() {
            EffectClass::Read => read_nodes.push(n.id().as_str().to_string()),
            EffectClass::Write | EffectClass::SideEffect => {
                write_or_side_effect_nodes.push(n.id().as_str().to_string())
            }
            EffectClass::ArtifactRead => derive_nodes.push(n.id().as_str().to_string()),
        }
        if matches!(n, ValidatedPlanNode::ForEach(_)) {
            template_nodes.push(n.id().as_str().to_string());
        }
        if let Some(approval) = inferred_node_approval(n) {
            approval_gates.push(approval);
        }
        if matches!(n.result_shape(), crate::code_mode_plan::ResultShape::List)
            && n.effect_class() == EffectClass::Read
            && node_dependencies(n).is_empty()
        {
            match n {
                ValidatedPlanNode::Surface(surface)
                    if surface.kind == PlanNodeKind::Search || !surface.predicates.is_empty() =>
                {
                    boundedness_facts.push(format!(
                        "{} has API-side narrowing via {}",
                        n.id().as_str(),
                        if surface.kind == PlanNodeKind::Search {
                            "search text"
                        } else {
                            "filters"
                        }
                    ));
                }
                _ => warnings.push(format!(
                    "{} is an unbounded read root; add API filters/search text or Plan.limit(...) when cost or latency is uncertain",
                    n.id().as_str()
                )),
            }
        }
        if matches!(n, ValidatedPlanNode::Compute(_)) {
            let op = render_node_operation(n);
            if op.contains("limit ") {
                boundedness_facts.push(format!(
                    "{} uses Plan.limit for explicit semantic truncation",
                    n.id().as_str()
                ));
            } else {
                warnings.push(format!(
                    "{} computes over the full logical source collection; returned result views may be paged, but aggregate/project/group/map semantics are not page-windowed",
                    n.id().as_str()
                ));
            }
        }
        if let ValidatedPlanNode::RelationTraversal(relation) = n {
            boundedness_facts.push(format!(
                "{} traverses {} relation {} from a {:?} source",
                n.id().as_str(),
                render_relation_cardinality(relation.relation.cardinality),
                relation.relation.relation.as_str(),
                relation.relation.source_cardinality
            ));
        }
        if matches!(n, ValidatedPlanNode::ForEach(_)) {
            warnings.push(format!(
                "{} may fan out over every row in its logical source; keep the source bounded when approval/cost matters",
                n.id().as_str()
            ));
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
        "warnings": warnings,
        "boundedness_facts": boundedness_facts,
    })
}

fn render_relation_cardinality(
    cardinality: crate::code_mode_plan::RelationCardinality,
) -> &'static str {
    match cardinality {
        crate::code_mode_plan::RelationCardinality::One => "one",
        crate::code_mode_plan::RelationCardinality::Many => "many",
    }
}

fn inferred_node_approval(node: &ValidatedPlanNode) -> Option<serde_json::Value> {
    match node {
        ValidatedPlanNode::ForEach(n) => inferred_template_approval(n),
        ValidatedPlanNode::Surface(n) if node_requires_approval(n.kind, n.effect_class) => {
            let q = n.qualified_entity.as_ref()?;
            Some(approval_gate_json(
                n.id.as_str(),
                q,
                n.kind,
                None,
                n.approval.as_deref(),
            ))
        }
        _ => None,
    }
}

fn inferred_template_approval(node: &ValidatedForEachNode) -> Option<serde_json::Value> {
    if !node_requires_approval(node.effect_template.kind, node.effect_template.effect_class) {
        return None;
    }
    let action_name = if node.effect_template.kind == PlanNodeKind::Action {
        action_name_from_template(node.effect_template.expr_template.as_str())
    } else {
        None
    };
    Some(approval_gate_json(
        node.id.as_str(),
        &node.effect_template.qualified_entity,
        node.effect_template.kind,
        action_name.as_deref(),
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
        PlanNodeKind::Relation => "relation",
    });
    serde_json::json!({
        "node": node_id,
        "required": true,
        "host_policy": "host.auto_approve",
        "default_decision": "approved",
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
    node: &ValidatedPlanNode,
    index: usize,
) -> Result<(), String> {
    if let ValidatedPlanNode::RelationTraversal(relation) = node {
        let Some(ctx) = es.contexts_by_entry.get(&relation.relation.target.entry_id) else {
            return Err(format!(
                "plan.nodes[{index}].relation.target.entry_id {:?} is not loaded in this session",
                relation.relation.target.entry_id
            ));
        };
        let target = relation.relation.target.entity.as_str();
        if !ctx.cgs.entities.contains_key(target) {
            return Err(format!(
                "plan.nodes[{index}].relation.target entity {:?} is not present under entry_id {:?}",
                relation.relation.target.entity, relation.relation.target.entry_id
            ));
        }
        return Ok(());
    };

    let ValidatedPlanNode::Surface(surface) = node else {
        return Ok(());
    };
    let Some(q) = surface.qualified_entity.as_ref() else {
        return if es.contexts_by_entry.len() > 1 {
            Err(format!(
                "plan.nodes[{index}] is missing qualified_entity in a federated session"
            ))
        } else {
            Ok(())
        };
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

fn ensure_surface_expr_matches_plan_kind(
    es: &ExecuteSession,
    surface: &ValidatedSurfaceNode,
    pe: &ParsedExpr,
    index: usize,
) -> Result<(), String> {
    let Expr::Query(query) = &pe.expr else {
        if surface.kind == PlanNodeKind::Search {
            return Err(format!(
                "plan.nodes[{index}] is kind search but did not parse to a search query expression"
            ));
        }
        return Ok(());
    };
    let Some(name) = query.capability_name.as_deref() else {
        if surface.kind == PlanNodeKind::Search {
            return Err(format!(
                "plan.nodes[{index}] is kind search but expression did not resolve a search capability"
            ));
        }
        return Ok(());
    };
    let cgs = es
        .contexts_by_entry
        .get(
            surface
                .qualified_entity
                .as_ref()
                .map(|q| q.entry_id.as_str())
                .unwrap_or(es.entry_id.as_str()),
        )
        .map(|ctx| ctx.cgs.as_ref())
        .unwrap_or(es.cgs.as_ref());
    let Some(cap) = cgs.get_capability(name) else {
        return Err(format!(
            "plan.nodes[{index}] references unknown capability {name:?}"
        ));
    };
    match (surface.kind, cap.kind) {
        (PlanNodeKind::Search, CapabilityKind::Search) => Ok(()),
        (PlanNodeKind::Search, other) => Err(format!(
            "plan.nodes[{index}] is kind search but expression resolved capability {name:?} with kind {other:?}"
        )),
        (PlanNodeKind::Query, CapabilityKind::Search) => Err(format!(
            "plan.nodes[{index}] is kind query but expression resolved search capability {name:?}; use the search facade"
        )),
        _ => Ok(()),
    }
}

fn ensure_relation_expr_matches_plan(
    es: &ExecuteSession,
    relation: &crate::code_mode_plan::ValidatedRelationTraversalNode,
    pe: &ParsedExpr,
    index: usize,
) -> Result<(), String> {
    let Expr::Chain(chain) = &pe.expr else {
        return Err(format!(
            "plan.nodes[{index}].relation.expr must parse to a Plasm relation chain"
        ));
    };
    if chain.selector != relation.relation.relation.as_str() {
        return Err(format!(
            "plan.nodes[{index}].relation relation {:?} does not match parsed selector {:?}",
            relation.relation.relation.as_str(),
            chain.selector
        ));
    }
    let source_entity = chain.source.primary_entity();
    let source_cgs = es
        .contexts_by_entry
        .get(&relation.relation.target.entry_id)
        .map(|ctx| ctx.cgs.as_ref())
        .unwrap_or(es.cgs.as_ref());
    let Some(source_def) = source_cgs.get_entity(source_entity) else {
        return Err(format!(
            "plan.nodes[{index}].relation source entity {source_entity:?} is not present"
        ));
    };
    let Some(schema_relation) = source_def
        .relations
        .get(relation.relation.relation.as_str())
    else {
        return Err(format!(
            "plan.nodes[{index}].relation source entity {source_entity:?} has no relation {:?}",
            relation.relation.relation.as_str()
        ));
    };
    if schema_relation.target_resource.as_str() != relation.relation.target.entity {
        return Err(format!(
            "plan.nodes[{index}].relation target {:?} does not match CGS target {:?}",
            relation.relation.target.entity,
            schema_relation.target_resource.as_str()
        ));
    }
    let expected_cardinality = match schema_relation.cardinality {
        plasm_core::Cardinality::One => crate::code_mode_plan::RelationCardinality::One,
        plasm_core::Cardinality::Many => crate::code_mode_plan::RelationCardinality::Many,
    };
    if relation.relation.cardinality != expected_cardinality {
        return Err(format!(
            "plan.nodes[{index}].relation cardinality {:?} does not match CGS cardinality {:?}",
            relation.relation.cardinality, expected_cardinality
        ));
    }
    Ok(())
}

fn dry_stage_result(index: usize, n: &ValidatedPlanNode) -> serde_json::Value {
    match n {
        ValidatedPlanNode::ForEach(for_each) => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id().as_str(),
            "kind": n.kind(),
            "operation": render_node_operation(n),
            "effect_class": n.effect_class(),
            "result_shape": n.result_shape(),
            "projection": for_each.projection,
            "predicates": for_each.predicates,
            "depends_on": node_ids_json(n.depends_on()),
            "uses_result": n.uses_result(),
            "source": for_each.source.as_str(),
            "item_binding": for_each.item_binding.as_str(),
            "approval": for_each.approval,
            "approval_gate": inferred_node_approval(n),
            "effect_template": for_each.effect_template,
            "simulation": {
                "kind": "template_stage",
                "max_write_set": {
                    "source": for_each.source.as_str(),
                    "shape": "one template invocation per source row"
                },
                "execution": "requires phased Plan runner"
            }
        }),
        ValidatedPlanNode::Data(data) => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id().as_str(),
            "kind": n.kind(),
            "operation": render_node_operation(n),
            "effect_class": n.effect_class(),
            "result_shape": n.result_shape(),
            "depends_on": node_ids_json(n.depends_on()),
            "uses_result": n.uses_result(),
            "approval_gate": inferred_node_approval(n),
            "data": data.data,
            "simulation": {
                "kind": "static_data",
                "execution": "materializes static Plan data through the phased Plan runner"
            }
        }),
        ValidatedPlanNode::Derive(derive) => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id().as_str(),
            "kind": n.kind(),
            "operation": render_node_operation(n),
            "effect_class": n.effect_class(),
            "result_shape": n.result_shape(),
            "depends_on": node_ids_json(n.depends_on()),
            "uses_result": n.uses_result(),
            "approval_gate": inferred_node_approval(n),
            "source": derive.source.as_str(),
            "item_binding": derive.item_binding.as_str(),
            "inputs": validated_inputs_json(&derive.inputs),
            "value": derive.value,
            "simulation": {
                "kind": "local_derivation",
                "execution": "runs after dependencies are materialized by the phased Plan runner"
            }
        }),
        ValidatedPlanNode::Compute(compute) => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id().as_str(),
            "kind": n.kind(),
            "operation": render_node_operation(n),
            "effect_class": n.effect_class(),
            "result_shape": n.result_shape(),
            "depends_on": node_ids_json(n.depends_on()),
            "uses_result": n.uses_result(),
            "approval_gate": inferred_node_approval(n),
            "compute": compute.compute,
            "simulation": {
                "kind": "deterministic_compute",
                "execution": "materializes a synthetic Plasm result set via the phased Plan runner"
            }
        }),
        ValidatedPlanNode::RelationTraversal(relation) => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id().as_str(),
            "kind": n.kind(),
            "operation": render_node_operation(n),
            "effect_class": n.effect_class(),
            "result_shape": n.result_shape(),
            "depends_on": node_ids_json(n.depends_on()),
            "uses_result": n.uses_result(),
            "approval_gate": inferred_node_approval(n),
            "relation": {
                "source": relation.relation.source.as_str(),
                "name": relation.relation.relation.as_str(),
                "target": relation.relation.target,
                "cardinality": relation.relation.cardinality,
                "source_cardinality": relation.relation.source_cardinality,
                "expr": relation.relation.ir.display_expr,
            },
            "execution_contract": {
                "entry_id": relation.relation.target.entry_id.as_str(),
                "entity": relation.relation.target.entity.as_str(),
                "ir": relation.relation.ir.expr,
                "projection": relation.relation.ir.projection,
                "source": relation.relation.source.as_str(),
                "relation": relation.relation.relation.as_str(),
            },
            "simulation": {
                "kind": "relation_traversal",
                "execution": "lowers through the typed Plasm chain relation path after the source node is materialized"
            }
        }),
        _ => serde_json::json!({
            "index": index,
            "ok": true,
            "id": n.id().as_str(),
            "kind": n.kind(),
            "operation": render_node_operation(n),
            "effect_class": n.effect_class(),
            "result_shape": n.result_shape(),
            "depends_on": node_ids_json(n.depends_on()),
            "uses_result": n.uses_result(),
            "approval_gate": inferred_node_approval(n),
            "simulation": {
                "kind": "staged_effect",
                "execution": "requires phased Plan runner"
            }
        }),
    }
}

fn node_ids_json(ids: &[PlanNodeId]) -> Vec<&str> {
    ids.iter().map(PlanNodeId::as_str).collect()
}

fn validated_inputs_json(inputs: &[ValidatedPlanDataInput]) -> Vec<serde_json::Value> {
    inputs
        .iter()
        .map(|input| {
            serde_json::json!({
                "node": input.node.as_str(),
                "alias": input.alias.as_str(),
                "proof": input.proof,
            })
        })
        .collect()
}

/// Raw MCP ingress wrapper. Validation happens once, then execution proceeds through the
/// proof-bearing [`ValidatedPlan`] core.
pub(crate) async fn run_code_mode_plan(
    es: &ExecuteSession,
    st: &PlasmHostState,
    _principal: Option<&TenantPrincipal>,
    prompt_hash: &str,
    session_id: &str,
    plan: &serde_json::Value,
    run: bool,
    mcp_plasm: Option<CodeModePlasmRunHooks<'_>>,
) -> Result<CodeModePlanRunResult, String> {
    let plan_typed = parse_plan_value(plan)?;
    let validated = validate_plan_artifact(&plan_typed)?;
    run_validated_code_mode_plan(es, st, prompt_hash, session_id, &validated, run, mcp_plasm).await
}

/// Code-mode / program-synthesis **plan** execution over a proof-bearing validated artifact.
pub async fn run_validated_code_mode_plan(
    es: &ExecuteSession,
    st: &PlasmHostState,
    prompt_hash: &str,
    session_id: &str,
    validated: &ValidatedPlan,
    run: bool,
    mcp_plasm: Option<CodeModePlasmRunHooks<'_>>,
) -> Result<CodeModePlanRunResult, String> {
    let dry = evaluate_validated_code_mode_plan_dry(es, validated)?;
    if !run {
        return Ok(CodeModePlanRunResult {
            version: dry.version,
            node_results: dry.node_results,
            graph_summary: dry.graph_summary,
            run_markdown: None,
            run_plasm_meta: None,
        });
    }
    run_validated_plan_phased(es, st, prompt_hash, session_id, validated, dry, mcp_plasm).await
}

#[derive(Debug, Clone)]
struct MaterializedNode {
    entry_id: String,
    entity: String,
    result: ExecutionResult,
    /// Complete logical rows for downstream DAG semantics. `result.entities` may be a paged view for
    /// display/publication, but compute/project/group/map must consume the full materialized collection.
    all_entities: Vec<CachedEntity>,
    artifact: Option<crate::run_artifacts::RunArtifactHandle>,
    display: String,
    projection: Option<Vec<String>>,
}

struct MaterializedInputRow {
    node: PlanNodeId,
    proof: crate::code_mode_plan::InputCardinalityProof,
    row: serde_json::Value,
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
    let approval_policy = CodeModeApprovalPolicy::automatic();
    let mut approval_receipts: Vec<CodeModeApprovalReceipt> = Vec::new();
    let mut trace = None;
    let mut sink = None;
    let mut meta_index = None;
    if let Some(hooks) = mcp_plasm {
        trace = Some(hooks.trace);
        sink = Some(hooks.sink);
        meta_index = Some(hooks.meta_index);
    }
    for node_id in validated.topological_order() {
        let idx = validated
            .node_index(node_id)
            .ok_or_else(|| format!("validated node {:?} missing index", node_id.as_str()))?;
        let node = &validated.nodes()[idx];
        if let Some(gate) = inferred_node_approval(node) {
            let receipt = approval_policy.review(gate);
            match receipt.decision {
                CodeModeApprovalDecision::Approved => approval_receipts.push(receipt),
            }
        }
        let mat = match node {
            ValidatedPlanNode::Surface(surface) => {
                let parsed = if let Some(ir) = &surface.ir {
                    ParsedExpr {
                        expr: ir.expr.clone(),
                        projection: ir.projection.clone(),
                    }
                } else if let Some(template) = &surface.ir_template {
                    let input_rows =
                        materialized_result_use_inputs(&materialized, &surface.uses_result)?;
                    let scope = EvalScope::Root {
                        row: &serde_json::Value::Null,
                    };
                    let inputs = InputEnv { rows: &input_rows };
                    let env = PlanEvalEnv { scope, inputs };
                    instantiate_expr_template(template, &env)?
                } else {
                    return Err(format!(
                        "plan node {} has no executable IR",
                        surface.id.as_str()
                    ));
                };
                let expr_label = surface
                    .ir
                    .as_ref()
                    .and_then(|ir| ir.display_expr.as_deref())
                    .or(surface.display_expr.as_deref())
                    .unwrap_or("<ir>");
                let scoped_es =
                    entry_scoped_execute_session(es, surface.qualified_entity.as_ref())?;
                let (parsed, result, artifact) = execute_code_mode_parsed_expr(
                    st,
                    &scoped_es,
                    session_id,
                    expr_label,
                    parsed,
                    trace.as_ref(),
                    idx as i64,
                )
                .await?;
                if let Some(sink) = sink.as_ref() {
                    trace_record_code_mode_plasm_line(
                        sink, idx, expr_label, &parsed, &result, &scoped_es,
                    )
                    .await;
                }
                MaterializedNode {
                    entry_id: surface
                        .qualified_entity
                        .as_ref()
                        .map(|q| q.entry_id.clone())
                        .unwrap_or_else(|| es.entry_id.clone()),
                    entity: surface
                        .qualified_entity
                        .as_ref()
                        .map(|q| q.entity.clone())
                        .unwrap_or_else(|| node.id().as_str().to_string()),
                    display: crate::expr_display::expr_display(&parsed.expr),
                    projection: parsed.projection,
                    all_entities: result.entities.clone(),
                    result,
                    artifact,
                }
            }
            ValidatedPlanNode::Data(data) => {
                materialize_synthetic_node(
                    st,
                    es,
                    session_id,
                    node,
                    es.entry_id.as_str(),
                    None,
                    plan_value_to_rows(&data.data)?,
                    trace.as_ref(),
                )
                .await?
            }
            ValidatedPlanNode::Derive(derive) => {
                let owner_entry_id = materialized
                    .get(&derive.source)
                    .map(|m| m.entry_id.clone())
                    .unwrap_or_else(|| es.entry_id.clone());
                let source_rows = materialized_rows(&materialized, &derive.source)?;
                let input_rows = materialized_singleton_inputs(&materialized, &derive.inputs)?;
                let mut rows = Vec::with_capacity(source_rows.len());
                for row in source_rows {
                    let scope = EvalScope::Bound {
                        row: &row,
                        binding: &derive.item_binding,
                    };
                    let inputs = InputEnv { rows: &input_rows };
                    let env = PlanEvalEnv { scope, inputs };
                    rows.push(eval_plan_value(&derive.value, &env)?);
                }
                materialize_synthetic_node(
                    st,
                    es,
                    session_id,
                    node,
                    owner_entry_id.as_str(),
                    None,
                    rows,
                    trace.as_ref(),
                )
                .await?
            }
            ValidatedPlanNode::Compute(compute) => {
                let owner_entry_id = PlanNodeId::new(compute.compute.source.clone())
                    .ok()
                    .and_then(|source| materialized.get(&source).map(|m| m.entry_id.clone()))
                    .unwrap_or_else(|| es.entry_id.clone());
                let rows = eval_compute(&compute.compute, &materialized)?;
                materialize_synthetic_node(
                    st,
                    es,
                    session_id,
                    node,
                    owner_entry_id.as_str(),
                    compute.compute.schema.entity.as_deref(),
                    rows,
                    trace.as_ref(),
                )
                .await?
            }
            ValidatedPlanNode::RelationTraversal(relation) => {
                let _ = materialized_rows(&materialized, &relation.relation.source)?;
                let parsed = ParsedExpr {
                    expr: relation.relation.ir.expr.clone(),
                    projection: relation.relation.ir.projection.clone(),
                };
                let expr_label = relation
                    .relation
                    .ir
                    .display_expr
                    .as_deref()
                    .unwrap_or("<ir>");
                let (parsed, result, artifact) = execute_code_mode_parsed_expr(
                    st,
                    es,
                    session_id,
                    expr_label,
                    parsed,
                    trace.as_ref(),
                    idx as i64,
                )
                .await?;
                if let Some(sink) = sink.as_ref() {
                    trace_record_code_mode_plasm_line(sink, idx, expr_label, &parsed, &result, es)
                        .await;
                }
                MaterializedNode {
                    entry_id: relation.relation.target.entry_id.clone(),
                    entity: relation.relation.target.entity.clone(),
                    display: crate::expr_display::expr_display(&parsed.expr),
                    projection: parsed.projection,
                    all_entities: result.entities.clone(),
                    result,
                    artifact,
                }
            }
            ValidatedPlanNode::ForEach(for_each) => {
                materialize_for_each_node(
                    st,
                    es,
                    session_id,
                    idx,
                    for_each,
                    &materialized,
                    trace.as_ref(),
                    sink.as_ref(),
                )
                .await?
            }
        };
        materialized.insert(node.id().clone(), mat);
    }

    let return_refs = validated.return_value().refs();
    let mut steps = Vec::new();
    let return_names = validated_return_names(validated.return_value());
    for (i, node_ref) in return_refs.into_iter().enumerate() {
        let mat = materialized.get(node_ref).ok_or_else(|| {
            format!(
                "plan.return materialized node {:?} missing",
                node_ref.as_str()
            )
        })?;
        steps.push(PublishedResultStep {
            name: return_names.get(i).cloned().flatten(),
            node_id: Some(node_ref.as_str().to_string()),
            entry_id: Some(mat.entry_id.clone()),
            entity: Some(mat.entity.clone()),
            cgs: es
                .contexts_by_entry
                .get(&mat.entry_id)
                .map(|ctx| ctx.cgs.clone()),
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
        graph_summary: graph_summary_with_approval_receipts(dry.graph_summary, &approval_receipts),
        run_markdown: Some(out.markdown),
        run_plasm_meta: out.tool_meta,
    })
}

fn graph_summary_with_approval_receipts(
    mut graph_summary: serde_json::Value,
    receipts: &[CodeModeApprovalReceipt],
) -> serde_json::Value {
    if receipts.is_empty() {
        return graph_summary;
    }
    let receipt_json = receipts
        .iter()
        .map(|r| {
            serde_json::json!({
                "decision": match r.decision {
                    CodeModeApprovalDecision::Approved => "approved",
                },
                "policy": r.policy,
                "gate": r.gate,
            })
        })
        .collect::<Vec<_>>();

    if let Some(obj) = graph_summary.as_object_mut() {
        obj.insert(
            "approval_receipts".to_string(),
            serde_json::Value::Array(receipt_json),
        );
    }
    graph_summary
}

fn validated_return_names(ret: &ValidatedPlanReturn) -> Vec<Option<String>> {
    match ret {
        ValidatedPlanReturn::Node(id) => vec![Some(id.as_str().to_string())],
        ValidatedPlanReturn::Parallel { parallel } => parallel
            .iter()
            .enumerate()
            .map(|(i, _)| Some(format!("parallel[{i}]")))
            .collect(),
    }
}

async fn materialize_synthetic_node(
    st: &PlasmHostState,
    es: &ExecuteSession,
    session_id: &str,
    node: &ValidatedPlanNode,
    entry_id: &str,
    entity_override: Option<&str>,
    rows: Vec<serde_json::Value>,
    trace: Option<&PlasmTraceContext>,
) -> Result<MaterializedNode, String> {
    let entity = entity_override
        .map(str::to_string)
        .unwrap_or_else(|| match node {
            ValidatedPlanNode::Compute(compute) => compute
                .compute
                .schema
                .entity
                .clone()
                .unwrap_or_else(|| format!("PlanComputed_{}", node.id().as_str())),
            _ => format!("PlanComputed_{}", node.id().as_str()),
        });
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
        Some(entry_id),
        vec![format!("plan.compute({})", node.id().as_str())],
        &full_result,
        trace,
    )
    .await?;
    let page_size = match node {
        ValidatedPlanNode::Compute(compute) => compute.compute.page_size.unwrap_or(50),
        _ => 50,
    };
    let (entities, has_more, paging_handle) = if full_entities.len() > page_size {
        let first = full_entities[..page_size].to_vec();
        let handle = es.register_synthetic_paging_continuation(
            crate::execute_session::SyntheticPageCursor {
                node_id: node.id().as_str().to_string(),
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
        entry_id: entry_id.to_string(),
        entity: entity.clone(),
        display: synthetic_node_display(node),
        projection: synthetic_projection(node),
        all_entities: full_result.entities.clone(),
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

fn synthetic_node_display(node: &ValidatedPlanNode) -> String {
    match node {
        ValidatedPlanNode::Data(_) => format!("plan.data({})", node.id().as_str()),
        ValidatedPlanNode::Derive(_) => format!("plan.derive({})", node.id().as_str()),
        ValidatedPlanNode::Compute(_) => format!("plan.compute({})", node.id().as_str()),
        ValidatedPlanNode::RelationTraversal(_) => {
            format!("plan.relation({})", node.id().as_str())
        }
        _ => format!("plan.stage({})", node.id().as_str()),
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
        .all_entities
        .iter()
        .map(CachedEntity::payload_to_json)
        .collect())
}

fn materialized_singleton_inputs(
    materialized: &BTreeMap<PlanNodeId, MaterializedNode>,
    inputs: &[ValidatedPlanDataInput],
) -> Result<BTreeMap<InputAlias, MaterializedInputRow>, String> {
    let mut out = BTreeMap::new();
    for input in inputs {
        let mat = materialized.get(&input.node).ok_or_else(|| {
            format!(
                "input node {:?} for alias {:?} has not been materialized",
                input.node.as_str(),
                input.alias.as_str()
            )
        })?;
        if mat.all_entities.len() != 1 {
            return Err(format!(
                "Plan input {:?} for alias {:?} expected one row for {:?} broadcast, got {}",
                input.node.as_str(),
                input.alias.as_str(),
                input.proof,
                mat.all_entities.len()
            ));
        }
        let row = mat
            .all_entities
            .first()
            .map(CachedEntity::payload_to_json)
            .ok_or_else(|| {
                format!(
                    "Plan input {:?} for alias {:?} expected one row but was empty",
                    input.node.as_str(),
                    input.alias.as_str()
                )
            })?;
        out.insert(
            input.alias.clone(),
            MaterializedInputRow {
                node: input.node.clone(),
                proof: input.proof,
                row,
            },
        );
    }
    Ok(out)
}

fn materialized_result_use_inputs(
    materialized: &BTreeMap<PlanNodeId, MaterializedNode>,
    uses_result: &[PlanResultUse],
) -> Result<BTreeMap<InputAlias, MaterializedInputRow>, String> {
    let mut out = BTreeMap::new();
    for use_result in uses_result {
        let node = PlanNodeId::new(use_result.node.clone())?;
        let alias = InputAlias::new(use_result.r#as.clone())?;
        let mat = materialized.get(&node).ok_or_else(|| {
            format!(
                "input node {:?} for alias {:?} has not been materialized",
                node.as_str(),
                alias.as_str()
            )
        })?;
        if mat.all_entities.len() != 1 {
            return Err(format!(
                "Plan input {:?} for alias {:?} expected one row for staged expression rendering, got {}",
                node.as_str(),
                alias.as_str(),
                mat.all_entities.len()
            ));
        }
        let row = mat
            .all_entities
            .first()
            .map(CachedEntity::payload_to_json)
            .ok_or_else(|| {
                format!(
                    "Plan input {:?} for alias {:?} expected one row but was empty",
                    node.as_str(),
                    alias.as_str()
                )
            })?;
        out.insert(
            alias,
            MaterializedInputRow {
                node,
                proof: crate::code_mode_plan::InputCardinalityProof::RuntimeCheckedSingleton,
                row,
            },
        );
    }
    Ok(out)
}

fn instantiate_expr_template(
    template: &ValidatedPlanExprTemplate,
    env: &PlanEvalEnv<'_>,
) -> Result<ParsedExpr, String> {
    let expr_json = instantiate_expr_template_value(&template.expr, env)?;
    let expr = serde_json::from_value(expr_json)
        .map_err(|e| format!("templated Plasm IR instantiation failed: {e}"))?;
    Ok(ParsedExpr {
        expr,
        projection: template.projection.clone(),
    })
}

fn instantiate_raw_expr_template(
    template: &PlanExprTemplate,
    env: &PlanEvalEnv<'_>,
) -> Result<ParsedExpr, String> {
    let expr_json = instantiate_expr_template_value(&template.expr, env)?;
    let expr = serde_json::from_value(expr_json)
        .map_err(|e| format!("templated Plasm IR instantiation failed: {e}"))?;
    Ok(ParsedExpr {
        expr,
        projection: template.projection.clone(),
    })
}

fn instantiate_expr_template_value(
    value: &serde_json::Value,
    env: &PlanEvalEnv<'_>,
) -> Result<serde_json::Value, String> {
    if let Some(hole) = value
        .as_object()
        .and_then(|obj| obj.get("__plasm_hole"))
        .and_then(|v| v.as_object())
    {
        return instantiate_ir_hole(hole, env);
    }
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| instantiate_expr_template_value(item, env))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| Ok((k.clone(), instantiate_expr_template_value(v, env)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(serde_json::Value::Object),
        other => Ok(other.clone()),
    }
}

fn instantiate_ir_hole(
    hole: &serde_json::Map<String, serde_json::Value>,
    env: &PlanEvalEnv<'_>,
) -> Result<serde_json::Value, String> {
    let kind = hole
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "IR value hole is missing kind".to_string())?;
    let path = hole
        .get("path")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match kind {
        "binding" => {
            let binding = hole
                .get("binding")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "binding IR hole is missing binding".to_string())?;
            let EvalScope::Bound {
                binding: scope_binding,
                ..
            } = &env.scope
            else {
                return Err("binding IR hole cannot be used outside a row scope".to_string());
            };
            if binding != scope_binding.as_str() {
                return Err(format!(
                    "binding IR hole references {binding:?}, but active binding is {:?}",
                    scope_binding.as_str()
                ));
            }
            Ok(value_at_segments(env.scope.row(), &path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        "node_input" => {
            let alias = hole
                .get("alias")
                .and_then(|v| v.as_str())
                .or_else(|| hole.get("node").and_then(|v| v.as_str()))
                .ok_or_else(|| "node_input IR hole is missing alias".to_string())?;
            let alias = InputAlias::new(alias.to_string())?;
            let input = env.inputs.rows.get(&alias).ok_or_else(|| {
                format!("node_input IR hole references unavailable alias {alias:?}")
            })?;
            Ok(value_at_segments(&input.row, &path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        other => Err(format!("unknown IR value hole kind {other:?}")),
    }
}

fn render_for_each_expressions(
    for_each: &ValidatedForEachNode,
    source_rows: &[serde_json::Value],
) -> Result<Vec<String>, String> {
    let input_rows = BTreeMap::new();
    source_rows
        .iter()
        .map(|row| {
            let scope = EvalScope::Bound {
                row,
                binding: &for_each.item_binding,
            };
            let inputs = InputEnv { rows: &input_rows };
            let env = PlanEvalEnv { scope, inputs };
            render_expr_template(&for_each.effect_template.expr_template, &env)
        })
        .collect()
}

async fn materialize_for_each_node(
    st: &PlasmHostState,
    es: &ExecuteSession,
    session_id: &str,
    node_index: usize,
    for_each: &ValidatedForEachNode,
    materialized: &BTreeMap<PlanNodeId, MaterializedNode>,
    trace: Option<&PlasmTraceContext>,
    sink: Option<&McpPlasmTraceSink>,
) -> Result<MaterializedNode, String> {
    let source_rows = materialized_rows(materialized, &for_each.source)?;
    let input_rows = BTreeMap::new();
    let mut parsed_steps = Vec::with_capacity(source_rows.len());
    let mut expressions = Vec::with_capacity(source_rows.len());
    for row in &source_rows {
        let scope = EvalScope::Bound {
            row,
            binding: &for_each.item_binding,
        };
        let inputs = InputEnv { rows: &input_rows };
        let env = PlanEvalEnv { scope, inputs };
        let parsed = instantiate_raw_expr_template(&for_each.effect_template.ir_template, &env)?;
        expressions.push(crate::expr_display::expr_display(&parsed.expr));
        parsed_steps.push(parsed);
    }
    let mut entities = Vec::new();
    let mut request_fingerprints = Vec::new();
    let mut stats = ExecutionStats {
        duration_ms: 0,
        network_requests: 0,
        cache_hits: 0,
        cache_misses: 0,
    };
    let mut source = ExecutionSource::Cache;
    let mut displays = Vec::new();
    let scoped_es =
        entry_scoped_execute_session(es, Some(&for_each.effect_template.qualified_entity))?;

    for (row_index, parsed_expr) in parsed_steps.into_iter().enumerate() {
        let trace_line_index = node_index
            .checked_mul(1000)
            .and_then(|base| base.checked_add(row_index))
            .unwrap_or(node_index);
        let expr_label = expressions
            .get(row_index)
            .map(String::as_str)
            .unwrap_or("<ir>");
        let (parsed, result, _artifact) = execute_code_mode_parsed_expr(
            st,
            &scoped_es,
            session_id,
            expr_label,
            parsed_expr,
            trace,
            trace_line_index as i64,
        )
        .await?;
        if let Some(sink) = sink {
            trace_record_code_mode_plasm_line(
                sink,
                trace_line_index,
                expr_label,
                &parsed,
                &result,
                &scoped_es,
            )
            .await;
        }
        source = combine_execution_source(source, result.source);
        stats.duration_ms = stats.duration_ms.saturating_add(result.stats.duration_ms);
        stats.network_requests = stats
            .network_requests
            .saturating_add(result.stats.network_requests);
        stats.cache_hits = stats.cache_hits.saturating_add(result.stats.cache_hits);
        stats.cache_misses = stats.cache_misses.saturating_add(result.stats.cache_misses);
        request_fingerprints.extend(result.request_fingerprints);
        entities.extend(result.entities);
        displays.push(crate::expr_display::expr_display(&parsed.expr));
    }

    let result = ExecutionResult {
        count: entities.len(),
        entities,
        has_more: false,
        pagination_resume: None,
        paging_handle: None,
        source,
        stats,
        request_fingerprints,
    };
    let artifact = archive_code_mode_result_snapshot(
        st,
        es,
        session_id,
        Some(for_each.effect_template.qualified_entity.entry_id.as_str()),
        expressions,
        &result,
        trace,
    )
    .await?;
    let display = if displays.len() == 1 {
        displays
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("for_each {}", for_each.id.as_str()))
    } else {
        format!(
            "for_each {} ({} calls)",
            for_each.id.as_str(),
            source_rows.len()
        )
    };
    let all_entities = result.entities.clone();
    Ok(MaterializedNode {
        entry_id: for_each.effect_template.qualified_entity.entry_id.clone(),
        entity: for_each.effect_template.qualified_entity.entity.clone(),
        result,
        all_entities,
        artifact: Some(artifact),
        display,
        projection: Some(for_each.projection.clone()).filter(|p| !p.is_empty()),
    })
}

fn combine_execution_source(current: ExecutionSource, next: ExecutionSource) -> ExecutionSource {
    match (current, next) {
        (ExecutionSource::Live, _) | (_, ExecutionSource::Live) => ExecutionSource::Live,
        (ExecutionSource::Replay, _) | (_, ExecutionSource::Replay) => ExecutionSource::Replay,
        (ExecutionSource::Cache, ExecutionSource::Cache) => ExecutionSource::Cache,
    }
}

fn plan_value_to_rows(value: &PlanValue) -> Result<Vec<serde_json::Value>, String> {
    let inputs = BTreeMap::new();
    let scope = EvalScope::Root {
        row: &serde_json::Value::Null,
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

enum EvalScope<'a> {
    Root {
        row: &'a serde_json::Value,
    },
    Bound {
        row: &'a serde_json::Value,
        binding: &'a BindingName,
    },
}

impl<'a> EvalScope<'a> {
    fn row(&self) -> &'a serde_json::Value {
        match self {
            Self::Root { row } | Self::Bound { row, .. } => row,
        }
    }
}

struct InputEnv<'a> {
    rows: &'a BTreeMap<InputAlias, MaterializedInputRow>,
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
            let path = match &env.scope {
                EvalScope::Root { .. } => path.as_str(),
                EvalScope::Bound { binding, .. } => strip_binding(path, binding),
            };
            Ok(value_at_dotted(env.scope.row(), path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        PlanValue::BindingSymbol { binding, path } => {
            let EvalScope::Bound {
                binding: scope_binding,
                ..
            } = &env.scope
            else {
                return Err(format!(
                    "binding symbol {binding:?} cannot resolve at root scope"
                ));
            };
            if scope_binding.as_str() != binding.as_str() {
                return Err(format!(
                    "binding symbol references unknown binding {binding:?}"
                ));
            }
            Ok(value_at_segments(env.scope.row(), path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        PlanValue::NodeSymbol { node, alias, path } => {
            let alias = InputAlias::new(alias.clone())?;
            let expected_node = PlanNodeId::new(node.clone())?;
            let input = env.inputs.rows.get(&alias).ok_or_else(|| {
                format!(
                    "node symbol references missing input alias {:?}",
                    alias.as_str()
                )
            })?;
            if input.node != expected_node {
                return Err(format!(
                    "node symbol alias {:?} is bound to {:?}, not {:?}",
                    alias.as_str(),
                    input.node.as_str(),
                    expected_node.as_str()
                ));
            }
            match input.proof {
                crate::code_mode_plan::InputCardinalityProof::StaticSingleton
                | crate::code_mode_plan::InputCardinalityProof::RuntimeCheckedSingleton => {}
            }
            Ok(value_at_segments(&input.row, path)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        }
        PlanValue::Template { template, .. } => {
            Ok(serde_json::Value::String(render_template(template, env)?))
        }
        PlanValue::EntityRefKey { key, .. } => eval_plan_value(key, env),
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

fn strip_binding<'a>(path: &'a str, binding: &BindingName) -> &'a str {
    let binding = binding.as_str();
    if path == binding {
        return "";
    }
    if let Some(rest) = path.strip_prefix(&format!("{binding}.")) {
        return rest;
    }
    path
}

fn render_template(template: &str, env: &PlanEvalEnv<'_>) -> Result<String, String> {
    render_template_with(template, env, json_scalar_display)
}

fn render_expr_template(template: &str, env: &PlanEvalEnv<'_>) -> Result<String, String> {
    render_template_with(template, env, json_plasm_literal_display)
}

fn render_template_with(
    template: &str,
    env: &PlanEvalEnv<'_>,
    render_value: fn(&serde_json::Value) -> String,
) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            return Err("template contains an unterminated ${...} substitution".to_string());
        };
        let raw_path = &after[..end];
        let rendered = resolve_template_path(raw_path, env)
            .map(render_value)
            .ok_or_else(|| format!("template path {raw_path:?} did not resolve"))?;
        out.push_str(&rendered);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn resolve_template_path<'a>(
    raw_path: &str,
    env: &'a PlanEvalEnv<'_>,
) -> Option<&'a serde_json::Value> {
    if let EvalScope::Bound { binding, .. } = &env.scope {
        if raw_path == binding.as_str() || raw_path.starts_with(&format!("{binding}.")) {
            return value_at_dotted(env.scope.row(), strip_binding(raw_path, binding));
        }
    }
    let (alias, rest) = raw_path
        .split_once('.')
        .map_or((raw_path, ""), |(alias, rest)| (alias, rest));
    let alias = InputAlias::new(alias.to_string()).ok()?;
    env.inputs
        .rows
        .get(&alias)
        .and_then(|input| value_at_dotted(&input.row, rest))
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
        PlanValue::EntityRefKey { key, .. } => match key.as_ref() {
            PlanValue::Literal { value } => value,
            _ => return false,
        },
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

fn json_plasm_literal_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => serde_json::to_string(s)
            .unwrap_or_else(|_| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
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

fn synthetic_projection(node: &ValidatedPlanNode) -> Option<Vec<String>> {
    match node {
        ValidatedPlanNode::Compute(compute) => Some(
            compute
                .compute
                .schema
                .fields
                .iter()
                .map(|f| f.name.as_str().to_string())
                .collect(),
        ),
        _ => None,
    }
}

fn compute_fingerprint(node: &ValidatedPlanNode, rows: &[serde_json::Value]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(node.id().as_str().as_bytes());
    if let ValidatedPlanNode::Compute(compute) = node {
        match serde_json::to_vec(&compute.compute) {
            Ok(bytes) => hasher.update(bytes),
            Err(e) => hasher.update(format!("compute-serialization-error:{e}").as_bytes()),
        }
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

    fn duplicate_product_create_session() -> ExecuteSession {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cgs = Arc::new(
            load_schema(&root.join("../plasm-facade-gen/tests/fixtures/tiny"))
                .expect("load facade tiny"),
        );
        let mut ctxs = indexmap::IndexMap::new();
        ctxs.insert(
            "acme".into(),
            Arc::new(CgsContext::entry("acme", cgs.clone())),
        );
        ctxs.insert(
            "other".into(),
            Arc::new(CgsContext::entry("other", cgs.clone())),
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

    #[test]
    fn entry_scoped_surface_parse_preserves_typed_catalog_create() {
        let s = duplicate_product_create_session();
        let err = parse_parsed_expr_for_session(&s, "Product.create(name=\"bolt\")")
            .expect_err("unscoped federated create should be ambiguous")
            .to_string();
        assert!(err.contains("ambiguous capability label `create`"), "{err}");

        let scoped = entry_scoped_execute_session(
            &s,
            Some(&QualifiedEntityKey {
                entry_id: "other".to_string(),
                entity: "Product".to_string(),
            }),
        )
        .expect("scope session");
        let parsed = parse_parsed_expr_for_session(&scoped, "Product.create(name=\"bolt\")")
            .expect("scoped create parses");
        typecheck_parsed_for_session(&scoped, &parsed).expect("scoped create typechecks");
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
                "ir": { "expr": { "op": "query", "entity": "Product" } },
                "effect_class": "read",
                "result_shape": "list"
            }],
            "return": { "kind": "node", "node": "n0" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(dry.node_results.len(), 1);
        assert!(dry.can_batch_run);
    }

    #[test]
    fn evaluate_code_mode_plan_dry_accepts_search_node() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "search-products",
            "nodes": [{
                "id": "search",
                "kind": "search",
                "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                "expr": "Product~\"bolt\"",
                "ir": { "expr": { "op": "query", "entity": "Product", "predicate": { "type": "comparison", "field": "q", "op": "=", "value": "bolt" }, "capability_name": "product_search" } },
                "effect_class": "read",
                "result_shape": "list"
            }],
            "return": { "kind": "node", "node": "search" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert!(dry.can_batch_run);
        assert_eq!(dry.node_results[0]["kind"], "search");
    }

    #[test]
    fn evaluate_code_mode_plan_dry_typechecks_ir_not_display_text() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "read-products",
            "nodes": [{
                "id": "n0",
                "kind": "query",
                "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                "expr": "WrongEntity",
                "ir": { "expr": { "op": "query", "entity": "Product" } },
                "effect_class": "read",
                "result_shape": "list"
            }],
            "return": { "kind": "node", "node": "n0" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(
            dry.node_results[0]["execution_contract"]["ir"]["entity"],
            "Product"
        );
    }

    #[test]
    fn evaluate_code_mode_plan_dry_rejects_relation_target_mismatch() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "product",
                    "kind": "get",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product(\"p1\")",
                    "ir": { "expr": { "op": "get", "ref": { "entity_type": "Product", "key": "p1" } } },
                    "effect_class": "read",
                    "result_shape": "single"
                },
                {
                    "id": "bad_relation",
                    "kind": "relation",
                    "effect_class": "read",
                    "result_shape": "single",
                    "relation": {
                        "source": "product",
                        "relation": "category",
                        "target": { "entry_id": "acme", "entity": "Product" },
                        "cardinality": "one",
                        "source_cardinality": "single",
                        "expr": "Product(\"p1\").category",
                        "ir": { "expr": { "op": "chain", "source": { "op": "get", "ref": { "entity_type": "Product", "key": "p1" } }, "selector": "category", "step": { "type": "auto_get" } } }
                    },
                    "depends_on": ["product"],
                    "uses_result": [{ "node": "product", "as": "source" }]
                }
            ],
            "return": { "kind": "node", "node": "bad_relation" }
        });
        let err =
            evaluate_code_mode_plan_dry(&s, &plan).expect_err("relation target mismatch rejected");
        assert!(err.contains("does not match CGS target"), "{err}");
    }

    #[test]
    fn evaluate_code_mode_plan_dry_typechecks_relation_node() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "product",
                    "kind": "get",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product(\"p1\")",
                    "ir": { "expr": { "op": "get", "ref": { "entity_type": "Product", "key": "p1" } } },
                    "effect_class": "read",
                    "result_shape": "single"
                },
                {
                    "id": "category",
                    "kind": "relation",
                    "effect_class": "read",
                    "result_shape": "single",
                    "relation": {
                        "source": "product",
                        "relation": "category",
                        "target": { "entry_id": "acme", "entity": "Category" },
                        "cardinality": "one",
                        "source_cardinality": "single",
                        "expr": "Product(\"p1\").category",
                        "ir": { "expr": { "op": "chain", "source": { "op": "get", "ref": { "entity_type": "Product", "key": "p1" } }, "selector": "category", "step": { "type": "auto_get" } } }
                    },
                    "depends_on": ["product"],
                    "uses_result": [{ "node": "product", "as": "source" }]
                }
            ],
            "return": { "kind": "node", "node": "category" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert!(!dry.can_batch_run);
        assert_eq!(
            dry.node_results[1]["simulation"]["kind"],
            "relation_traversal"
        );
    }

    #[test]
    fn evaluate_code_mode_plan_dry_accepts_runtime_checked_singleton_relation() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "products",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "category",
                    "kind": "relation",
                    "effect_class": "read",
                    "result_shape": "single",
                    "relation": {
                        "source": "products",
                        "relation": "category",
                        "target": { "entry_id": "acme", "entity": "Category" },
                        "cardinality": "one",
                        "source_cardinality": "runtime_checked_singleton",
                        "expr": "Product.category",
                        "ir": { "expr": { "op": "chain", "source": { "op": "query", "entity": "Product" }, "selector": "category", "step": { "type": "auto_get" } } }
                    },
                    "depends_on": ["products"],
                    "uses_result": [{ "node": "products", "as": "source" }]
                }
            ],
            "return": { "kind": "node", "node": "category" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(
            dry.node_results[1]["simulation"]["kind"],
            "relation_traversal"
        );
    }

    #[test]
    fn evaluate_code_mode_plan_dry_accepts_github_relation_limit_aggregate() {
        let s = github_repository_commit_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "github-repo-commits-aggregate",
            "nodes": [
                {
                    "id": "repo",
                    "kind": "get",
                    "qualified_entity": { "entry_id": "github", "entity": "Repository" },
                    "expr": "Repository({owner=\"ryan-s-roberts\", repo=\"plasm-core\"})",
                    "ir": { "expr": { "op": "get", "ref": { "entity_type": "Repository", "key": { "owner": "ryan-s-roberts", "repo": "plasm-core" } } } },
                    "effect_class": "read",
                    "result_shape": "single"
                },
                {
                    "id": "commits",
                    "kind": "relation",
                    "effect_class": "read",
                    "result_shape": "list",
                    "relation": {
                        "source": "repo",
                        "relation": "commits",
                        "target": { "entry_id": "github", "entity": "Commit" },
                        "cardinality": "many",
                        "source_cardinality": "single",
                        "expr": "Repository({owner=\"ryan-s-roberts\", repo=\"plasm-core\"}).commits[sha,message]",
                        "ir": { "expr": { "op": "chain", "source": { "op": "get", "ref": { "entity_type": "Repository", "key": { "owner": "ryan-s-roberts", "repo": "plasm-core" } } }, "selector": "commits", "step": { "type": "auto_get" } }, "projection": ["sha", "message"] }
                    },
                    "qualified_entity": { "entry_id": "github", "entity": "Commit" },
                    "projection": ["sha", "message"],
                    "depends_on": ["repo"],
                    "uses_result": [{ "node": "repo", "as": "source" }]
                },
                {
                    "id": "limited",
                    "kind": "compute",
                    "effect_class": "artifact_read",
                    "result_shape": "list",
                    "compute": {
                        "source": "commits",
                        "op": { "kind": "limit", "count": 2000 },
                        "schema": {
                            "entity": "PlanLimit",
                            "fields": [
                                { "name": "sha", "value_kind": "string", "source": ["sha"] },
                                { "name": "message", "value_kind": "string", "source": ["message"] }
                            ]
                        }
                    },
                    "depends_on": ["commits"]
                },
                {
                    "id": "n_commits",
                    "kind": "compute",
                    "effect_class": "artifact_read",
                    "result_shape": "list",
                    "compute": {
                        "source": "limited",
                        "op": { "kind": "aggregate", "aggregates": [{ "name": "n", "function": "count" }] },
                        "schema": { "entity": "PlanAggregate", "fields": [{ "name": "n", "value_kind": "integer" }] }
                    },
                    "depends_on": ["limited"]
                }
            ],
            "return": { "kind": "node", "node": "n_commits" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(
            dry.node_results[1]["simulation"]["kind"],
            "relation_traversal"
        );
        assert_eq!(dry.node_results[2]["kind"], "compute");
        assert_eq!(dry.node_results[3]["kind"], "compute");
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
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
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
            "return": { "kind": "parallel", "nodes": ["summary", "cards"] }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        let dag = code_mode_plan_dag_json(&dry);
        assert_eq!(dag["nodes"][0]["id"], "products");
        assert_eq!(dag["nodes"][1]["dependencies"][0], "products");
        assert_eq!(dag["edges"][0]["from"], "products");
        assert_eq!(dag["edges"][0]["to"], "summary");
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

warnings:
- products is an unbounded read root; add API filters/search text or Plan.limit(...) when cost or latency is uncertain
- summary computes over the full logical source collection; returned result views may be paged, but aggregate/project/group/map semantics are not page-windowed

dag:
01. products -> query acme.Product <= Query(Product all) [read; list]
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
            MaterializedInputRow {
                node: PlanNodeId::new("moveFacts".to_string()).expect("node id"),
                proof: crate::code_mode_plan::InputCardinalityProof::StaticSingleton,
                row: serde_json::json!({ "move": "thunderbolt", "power": 90 }),
            },
        )]);
        let binding = BindingName::new("p".to_string()).expect("binding");
        let scope = EvalScope::Bound {
            row: &row,
            binding: &binding,
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
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
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
            "return": { "kind": "node", "node": "cards" }
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
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
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
                        "ir_template": {
                            "expr": {
                                "op": "invoke",
                                "capability": "product_label",
                                "target": { "entity_type": "Product", "key": { "__plasm_hole": { "kind": "binding", "binding": "product", "path": ["id"] } } },
                                "input": { "label": "stale" }
                            },
                            "input_bindings": [{ "from": "product.id", "to": "id" }]
                        },
                        "effect_class": "side_effect",
                        "result_shape": "side_effect_ack"
                    }
                }
            ],
            "return": { "kind": "parallel", "nodes": ["find", "label"] }
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
    fn for_each_templates_render_concrete_row_bound_plasm_calls() {
        let plan = parse_plan_value(&serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "label-products",
            "nodes": [
                {
                    "id": "find",
                    "kind": "data",
                    "effect_class": "artifact_read",
                    "result_shape": "list",
                    "data": { "kind": "literal", "value": [{ "id": "p1" }] }
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
                        "ir_template": {
                            "expr": {
                                "op": "invoke",
                                "capability": "product_label",
                                "target": { "entity_type": "Product", "key": { "__plasm_hole": { "kind": "binding", "binding": "product", "path": ["id"] } } },
                                "input": { "label": "stale" }
                            },
                            "input_bindings": [{ "from": "product.id", "to": "id" }]
                        },
                        "effect_class": "side_effect",
                        "result_shape": "side_effect_ack"
                    }
                }
            ],
            "return": { "kind": "node", "node": "label" }
        }))
        .expect("parse plan");
        let validated = validate_plan_artifact(&plan).expect("validate plan");
        let for_each = validated
            .nodes()
            .iter()
            .find_map(|node| match node {
                ValidatedPlanNode::ForEach(node) => Some(node),
                _ => None,
            })
            .expect("for_each node");
        let expressions = render_for_each_expressions(
            for_each,
            &[serde_json::json!({ "id": "p1", "name": "Bolt" })],
        )
        .expect("render expressions");

        assert_eq!(
            expressions,
            vec!["Product(\"p1\").label(label=\"stale\")".to_string()]
        );
    }

    #[test]
    fn dry_run_text_renders_staged_read_map_body() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "dependent-product-read",
            "nodes": [
                {
                    "id": "products",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "details",
                    "kind": "for_each",
                    "effect_class": "read",
                    "result_shape": "single",
                    "source": "products",
                    "item_binding": "product",
                    "depends_on": ["products"],
                    "uses_result": [{ "node": "products", "as": "product" }],
                    "effect_template": {
                        "kind": "get",
                        "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                        "expr_template": "Product(${product.id})",
                        "ir_template": {
                            "expr": {
                                "op": "get",
                                "ref": {
                                    "entity_type": "Product",
                                    "key": { "__plasm_hole": { "kind": "binding", "binding": "product", "path": ["id"] } }
                                }
                            },
                            "input_bindings": [{ "from": "product.id", "to": "id" }]
                        },
                        "effect_class": "read",
                        "result_shape": "single"
                    }
                }
            ],
            "return": { "kind": "node", "node": "details" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(dry.staged_nodes, vec!["details (ForEach)"]);
        assert!(dry.execution_unsupported.is_empty());
        let text = render_code_mode_plan_dry_text(&dry, None);
        assert!(
            text.contains("for_each products as product => Product(${product.id})"),
            "{text}"
        );
        assert!(!text.contains("=> {}"), "{text}");
        assert!(text.contains("approvals: none"), "{text}");
    }

    #[test]
    fn dry_run_text_renders_empty_literals_explicitly() {
        let plan = parse_plan_value(&serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [{
                "id": "empty",
                "kind": "data",
                "effect_class": "artifact_read",
                "result_shape": "artifact",
                "data": { "kind": "object", "fields": {} }
            }],
            "return": { "kind": "node", "node": "empty" }
        }))
        .expect("parse plan");
        let validated = validate_plan_artifact(&plan).expect("validate");
        let operation = render_node_operation(&validated.nodes()[0]);
        assert_eq!(operation, "data {0 fields}");
        assert!(!operation.contains("{}"), "{operation}");
    }

    #[test]
    fn create_template_approval_uses_create_operation_not_description_text() {
        let s = test_session();
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "create-report",
            "nodes": [
                {
                    "id": "products",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                    "expr": "Product",
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "createIssue",
                    "kind": "for_each",
                    "effect_class": "write",
                    "result_shape": "mutation_result",
                    "source": "products",
                    "item_binding": "product",
                    "depends_on": ["products"],
                    "uses_result": [{ "node": "products", "as": "product" }],
                    "effect_template": {
                        "kind": "create",
                        "qualified_entity": { "entry_id": "linear", "entity": "Issue" },
                        "expr_template": "Issue.create(title=\"Report\", description=\"1.) text that looks like member syntax\")",
                        "ir_template": {
                            "expr": {
                                "op": "create",
                                "capability": "issue_create",
                                "entity": "Issue",
                                "input": {
                                    "title": "Report",
                                    "description": "1.) text that looks like member syntax"
                                }
                            },
                            "input_bindings": []
                        },
                        "effect_class": "write",
                        "result_shape": "mutation_result"
                    }
                }
            ],
            "return": { "kind": "node", "node": "createIssue" }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        let text = render_code_mode_plan_dry_text(&dry, None);
        assert!(text.contains("approval: linear.Issue.create"), "{text}");
        assert!(!text.contains("approval: linear.Issue.\n"), "{text}");
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
                    "ir": { "expr": { "op": "query", "entity": "Product" } },
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
                        "ir_template": {
                            "expr": {
                                "op": "invoke",
                                "capability": "product_label",
                                "target": { "entity_type": "Product", "key": { "__plasm_hole": { "kind": "binding", "binding": "product", "path": ["id"] } } },
                                "input": { "label": "stale" }
                            },
                            "input_bindings": [{ "from": "product.id", "to": "id" }]
                        },
                        "effect_class": "side_effect",
                        "result_shape": "side_effect_ack"
                    }
                }
            ],
            "return": { "kind": "parallel", "nodes": ["find", "label"] }
        });
        let dry = evaluate_code_mode_plan_dry(&s, &plan).expect("dry");
        assert_eq!(
            dry.graph_summary["approval_gates"][0]["policy_key"],
            "acme.Product.label"
        );
    }

    #[test]
    fn mutating_surface_gate_declares_default_auto_approval() {
        let plan = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [{
                "id": "c1",
                "kind": "create",
                "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                "expr": "Product.create(name=\"servo\")",
                "ir": { "expr": { "op": "create", "capability": "product_create", "entity": "Product", "input": { "name": "servo" } } },
                "effect_class": "write",
                "result_shape": "single"
            }],
            "return": { "kind": "node", "node": "c1" }
        });
        let typed = parse_plan_value(&plan).expect("parse plan");
        let validated = validate_plan_artifact(&typed).expect("validate");
        let gate = inferred_node_approval(&validated.nodes()[0]).expect("approval gate");

        assert_eq!(gate["policy_key"], "acme.Product.create");
        assert_eq!(gate["host_policy"], "host.auto_approve");
        assert_eq!(gate["default_decision"], "approved");
    }

    #[test]
    fn automatic_approval_policy_emits_receipt_for_gate() {
        let gate = serde_json::json!({
            "node": "c1",
            "required": true,
            "policy_key": "acme.Product.create"
        });
        let receipt = CodeModeApprovalPolicy::automatic().review(gate.clone());
        let summary = graph_summary_with_approval_receipts(serde_json::json!({}), &[receipt]);

        assert_eq!(summary["approval_receipts"][0]["decision"], "approved");
        assert_eq!(
            summary["approval_receipts"][0]["policy"],
            "host.auto_approve"
        );
        assert_eq!(summary["approval_receipts"][0]["gate"], gate);
    }
}
