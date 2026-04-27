//! Plasm-native DAG composition compiler.
//!
//! This is deliberately a thin envelope around existing Plasm path expressions. Plain Plasm remains
//! the executable leaf language; this module only recognizes local labels, a small set of
//! collection transforms, and final response roots.

use crate::execute_session::ExecuteSession;
use crate::plasm_plan::{
    AggregateFunction, ComputeOp, EffectClass, FieldPath, OutputName, PlanNodeKind, PlanValue,
    QualifiedEntityKey, SyntheticFieldSchema, SyntheticResultSchema, SyntheticValueKind,
};
use crate::plasm_plan_run::parse_plasm_surface_line;
use plasm_core::Expr;
use plasm_core::PromptPipelineConfig;
use plasm_core::SymbolMapCrossRequestCache;
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
            return Err(format!("duplicate Plasm-DAG node label {:?}", node.id));
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
        || src.contains(".limit(")
        || src.contains(".sort(")
        || src.contains(".aggregate(")
        || src.contains(".group_by(")
        || src.contains(".page_size(")
}

pub fn split_bare_plasm_roots(src: &str) -> Option<Vec<String>> {
    let src = src.trim();
    if src.is_empty() || src.contains('\n') || split_assignment(src).is_some() {
        return None;
    }
    let parts = split_top_level(src, ',').ok()?;
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
    let mut state = CompileState::new(pipeline, symbol_map_cross_cache);
    let statements = parse_statements(source)?;
    if statements.is_empty() {
        return Err("Plasm-DAG program is empty".to_string());
    }
    let mut final_roots: Option<Vec<String>> = None;
    for stmt in statements {
        if let Some((id, rhs)) = split_assignment(&stmt) {
            validate_label(id)?;
            let node = compile_node_expr(session, &state, id, rhs.trim())?;
            state.insert(node)?;
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
        "Plasm-DAG program needs a final line of bare roots (comma-separated expressions or node labels)"
            .to_string()
    })?;
    if roots.is_empty() {
        return Err("Plasm-DAG final roots list is empty".to_string());
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

fn compile_node_expr(
    session: &ExecuteSession,
    state: &CompileState<'_>,
    id: &str,
    rhs: &str,
) -> Result<DagNode, String> {
    if let Some(inner) = rhs.strip_suffix(".singleton()") {
        let base = compile_node_expr(session, state, id, inner.trim())?;
        return Ok(DagNode {
            singleton: true,
            ..base
        });
    }
    if let Some((inner, n)) = parse_unary_call(rhs, "page_size")? {
        let base = compile_node_expr(session, state, id, inner.trim())?;
        return Ok(DagNode {
            page_size: Some(n),
            ..base
        });
    }
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
            .map_err(|e| format!("Plasm-DAG `{id}` template parse: {e}"))?;
            let (kind, qualified, _effect, _shape) = infer_surface_contract(session, &parsed.expr)?;
            if !matches!(
                kind,
                PlanNodeKind::Create
                    | PlanNodeKind::Update
                    | PlanNodeKind::Delete
                    | PlanNodeKind::Action
            ) {
                return Err(format!(
                    "Plasm-DAG `{id}` for_each right side must be a write/side-effect expression"
                ));
            }
            return Ok(DagNode {
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
            });
        }
        let (value, inputs) = parse_plan_value_expr(right.trim(), state, Some("_"))?;
        return Ok(DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: false,
            page_size: None,
            source: DagNodeSource::Derive {
                source: source.to_string(),
                value,
                inputs,
            },
        });
    }
    if let Some((source, fields, template)) = parse_render(rhs)? {
        require_node(state, source)?;
        let columns = parse_field_list(fields)?
            .into_iter()
            .map(OutputName::new)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(DagNode {
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
        });
    }
    if let Some((source, fields)) = parse_projection(rhs)? {
        require_node(state, source)?;
        let mut map = BTreeMap::new();
        for field in parse_field_list(fields)? {
            map.insert(
                OutputName::new(field.clone())?,
                FieldPath::from_dotted(&field)?,
            );
        }
        let schema =
            schema_from_output_fields("PlanProject", map.keys(), SyntheticValueKind::Unknown);
        return Ok(DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: false,
            page_size: None,
            source: DagNodeSource::Compute {
                source: source.to_string(),
                op: ComputeOp::Project { fields: map },
                schema,
            },
        });
    }
    if let Some((source, n)) = parse_unary_call(rhs, "limit")? {
        require_node(state, source)?;
        return Ok(DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: n <= 1,
            page_size: None,
            source: DagNodeSource::Compute {
                source: source.to_string(),
                op: ComputeOp::Limit { count: n },
                schema: single_unknown_schema("PlanLimit"),
            },
        });
    }
    if let Some((source, args)) = parse_call(rhs, "sort")? {
        require_node(state, source)?;
        let args = split_top_level(args, ',')?;
        let key = args
            .first()
            .ok_or_else(|| "sort(...) requires a field".to_string())?
            .trim();
        let descending = args
            .get(1)
            .map(|s| s.trim().eq_ignore_ascii_case("desc"))
            .unwrap_or(false);
        return Ok(DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: false,
            page_size: None,
            source: DagNodeSource::Compute {
                source: source.to_string(),
                op: ComputeOp::Sort {
                    key: FieldPath::from_dotted(key)?,
                    descending,
                },
                schema: single_unknown_schema("PlanSort"),
            },
        });
    }
    if let Some((source, args)) = parse_call(rhs, "aggregate")? {
        require_node(state, source)?;
        let aggregates = parse_aggregates(args)?;
        let schema = schema_from_aggregates("PlanAggregate", &aggregates);
        return Ok(DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: true,
            page_size: None,
            source: DagNodeSource::Compute {
                source: source.to_string(),
                op: ComputeOp::Aggregate { aggregates },
                schema,
            },
        });
    }
    if let Some((source, args)) = parse_call(rhs, "group_by")? {
        require_node(state, source)?;
        let parts = split_top_level(args, ',')?;
        let key = parts
            .first()
            .ok_or_else(|| "group_by(...) requires a key field".to_string())?
            .trim();
        let rest = args
            .split_once(',')
            .map(|(_, r)| r.trim())
            .ok_or_else(|| "group_by(...) requires aggregate specs".to_string())?;
        let aggregates = parse_aggregates(rest)?;
        let schema = schema_from_aggregates("PlanGroup", &aggregates);
        return Ok(DagNode {
            id: id.to_string(),
            expr: rhs.to_string(),
            singleton: false,
            page_size: None,
            source: DagNodeSource::Compute {
                source: source.to_string(),
                op: ComputeOp::GroupBy {
                    key: FieldPath::from_dotted(key)?,
                    aggregates,
                },
                schema,
            },
        });
    }
    if let Ok(value) = parse_plan_value_expr(rhs, state, None) {
        if looks_like_data_literal(rhs) {
            return Ok(DagNode {
                id: id.to_string(),
                expr: rhs.to_string(),
                singleton: true,
                page_size: None,
                source: DagNodeSource::Data(value.0),
            });
        }
    }
    compile_surface_node(session, state, id, rhs)
}

fn compile_surface_node(
    session: &ExecuteSession,
    state: &CompileState<'_>,
    id: &str,
    expr: &str,
) -> Result<DagNode, String> {
    if let Some(inner) = expr.trim().strip_suffix(".singleton()") {
        let base = compile_surface_node(session, state, id, inner.trim())?;
        return Ok(DagNode {
            singleton: true,
            ..base
        });
    }
    if let Some((inner, n)) = parse_unary_call(expr, "page_size")? {
        let base = compile_surface_node(session, state, id, inner.trim())?;
        return Ok(DagNode {
            page_size: Some(n),
            ..base
        });
    }
    let (rewritten, uses) = rewrite_template_expr(expr, state, None)?;
    let parsed = parse_plasm_surface_line(session, state.cross_cache, state.pipeline, &rewritten)
        .map_err(|e| format!("Plasm-DAG `{id}` expression parse: {e}"))?;
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

fn parse_statements(src: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut lines = src.lines().peekable();
    while let Some(line) = lines.next() {
        let stripped = strip_comment(line).trim();
        if stripped.is_empty() {
            continue;
        }
        if let Some((prefix, tag)) = stripped.split_once("<<") {
            let tag = tag.trim();
            if tag.is_empty() {
                return Err("heredoc tag must be non-empty".to_string());
            }
            let mut stmt = format!("{}<<{}\n", prefix.trim_end(), tag);
            let mut closed = false;
            for body in lines.by_ref() {
                stmt.push_str(body);
                stmt.push('\n');
                if body.trim() == tag {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err(format!("unterminated heredoc <<{tag}"));
            }
            out.push(stmt.trim_end().to_string());
        } else {
            out.push(stripped.to_string());
        }
    }
    Ok(out)
}

fn strip_comment(line: &str) -> &str {
    line.split_once(";;").map_or(line, |(left, _)| left)
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
            let node = compile_surface_node(session, state, &id, part)?;
            state.insert(node)?;
            roots.push(id);
        }
    }
    Ok(roots)
}

fn split_top_level(s: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            _ if c == delimiter && quote.is_none() && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced delimiters in `{s}`"));
    }
    out.push(&s[start..]);
    Ok(out)
}

fn validate_label(label: &str) -> Result<(), String> {
    if !is_valid_label(label) || matches!(label, "_" | "$" | "return") {
        return Err(format!("invalid Plasm-DAG label `{label}`"));
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
        Err(format!("unknown Plasm-DAG node `{node}`"))
    }
}

fn parse_unary_call<'a>(rhs: &'a str, name: &str) -> Result<Option<(&'a str, usize)>, String> {
    let Some((source, args)) = parse_call(rhs, name)? else {
        return Ok(None);
    };
    let n = args
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{name}(...) requires a positive integer"))?;
    if n == 0 {
        return Err(format!("{name}(...) requires a positive integer"));
    }
    Ok(Some((source, n)))
}

fn parse_call<'a>(rhs: &'a str, name: &str) -> Result<Option<(&'a str, &'a str)>, String> {
    let suffix = format!(".{name}(");
    let Some(pos) = rhs.find(&suffix) else {
        return Ok(None);
    };
    if !rhs.ends_with(')') {
        return Err(format!("{name}(...) call must end with `)`"));
    }
    Ok(Some((&rhs[..pos], &rhs[pos + suffix.len()..rhs.len() - 1])))
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
    let Some((source, fields)) = parse_projection(head.trim())? else {
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
    rhs.starts_with('{') || rhs.starts_with('[') || rhs.starts_with('"')
}

fn looks_like_plasm_effect_template(rhs: &str) -> bool {
    rhs.contains(".m") || rhs.contains("=>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasm_plan_run::evaluate_plasm_plan_dry;
    use plasm_core::{load_schema, CgsContext, DomainExposureSession, PromptPipelineConfig, CGS};
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
}
