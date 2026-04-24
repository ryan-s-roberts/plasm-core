//! Pre-execution intent and post-execution outcome lines shared by REPL, CLI, and plan executor.

use crate::resolve_query_capability;
use crate::schema::CGS;
use crate::step_semantics::{OutcomeContext, StepSummary};
use crate::value::CompOp;
use crate::{Expr, Predicate, Value};

/// Human-readable description of what the expression will do (before I/O).
pub fn render_intent(expr: &Expr, cgs: &CGS) -> String {
    render_intent_with_projection(expr, None, cgs)
}

/// Like [`render_intent`], includes optional field projection from the REPL/CLI surface.
pub fn render_intent_with_projection(
    expr: &Expr,
    projection: Option<&[String]>,
    cgs: &CGS,
) -> String {
    let mut s = intent_inner(expr, cgs);
    if let Some(proj) = projection {
        if !proj.is_empty() {
            s.push_str(&format!(" projecting [{}]", proj.join(", ")));
        }
    }
    s
}

fn intent_inner(expr: &Expr, cgs: &CGS) -> String {
    match expr {
        Expr::Get(g) => {
            format!(
                "Get {} by key `{}`",
                g.reference.entity_type,
                g.reference.primary_slot_str()
            )
        }
        Expr::Query(q) => {
            let cap = resolve_query_capability(q, cgs).ok();
            let cap_note = cap
                .map(|c| format!(" via `{}`", c.name))
                .unwrap_or_default();
            let pred = q
                .predicate
                .as_ref()
                .map(|p| format!(" where {}", format_predicate_short(p)))
                .unwrap_or_default();
            let search = if cap
                .map(|c| c.kind == crate::CapabilityKind::Search)
                .unwrap_or(false)
            {
                "Search"
            } else {
                "Query"
            };
            let mut s = format!("{search} {}{cap_note}{pred}", q.entity);
            if q.hydrate == Some(false) {
                s.push_str(" (summary rows, no per-row hydrate)");
            }
            s
        }
        Expr::Chain(c) => {
            let src = intent_inner(&c.source, cgs);
            format!("{src}, then follow relation `{}`", c.selector)
        }
        Expr::Create(c) => format!("Create {} using capability `{}`", c.entity, c.capability),
        Expr::Delete(d) => format!(
            "Delete {} `{}` via `{}`",
            d.target.entity_type,
            d.target.primary_slot_str(),
            d.capability
        ),
        Expr::Invoke(i) => format!(
            "Invoke `{}` on {} `{}`",
            i.capability,
            i.target.entity_type,
            i.target.primary_slot_str()
        ),
        Expr::Page(p) => format!(
            "Continue paginated list (`{}`){}",
            p.handle,
            p.limit
                .map(|l| format!(" with per-request limit {l}"))
                .unwrap_or_default()
        ),
    }
}

/// Build a [`StepSummary`] after execution using counts and timing from the runtime.
pub fn render_outcome(expr: &Expr, ctx: &OutcomeContext, cgs: &CGS) -> StepSummary {
    let entity = ctx
        .primary_entity_type
        .clone()
        .or_else(|| primary_entity(expr));
    let operation = operation_kind(expr);
    let message = outcome_line(expr, ctx, cgs);
    StepSummary {
        message,
        entity,
        operation,
        count: Some(ctx.count),
    }
}

fn primary_entity(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Get(g) => Some(g.reference.entity_type.to_string()),
        Expr::Query(q) => Some(q.entity.to_string()),
        Expr::Chain(c) => primary_entity(&c.source),
        Expr::Create(c) => Some(c.entity.to_string()),
        Expr::Delete(d) => Some(d.target.entity_type.to_string()),
        Expr::Invoke(i) => Some(i.target.entity_type.to_string()),
        Expr::Page(_) => None,
    }
}

fn operation_kind(expr: &Expr) -> Option<String> {
    Some(
        match expr {
            Expr::Query(_) => "query",
            Expr::Get(_) => "get",
            Expr::Chain(_) => "chain",
            Expr::Create(_) => "create",
            Expr::Delete(_) => "delete",
            Expr::Invoke(_) => "invoke",
            Expr::Page(_) => "page",
        }
        .to_string(),
    )
}

fn outcome_line(expr: &Expr, ctx: &OutcomeContext, cgs: &CGS) -> String {
    let src = ctx.source_label;
    let mut tail = Vec::new();
    if ctx.network_requests > 0 {
        tail.push(format!("{} HTTP", ctx.network_requests));
    }
    if ctx.cache_hits > 0 {
        tail.push(format!("{} cache hits", ctx.cache_hits));
    }
    tail.push(format!("{}ms", ctx.duration_ms));
    tail.push(format!("source={src}"));
    let stats = tail.join(", ");

    match expr {
        Expr::Query(q) => {
            let cap = resolve_query_capability(q, cgs).ok();
            let cap_s = cap.map(|c| format!(" ({})", c.name)).unwrap_or_default();
            let verb = if cap
                .map(|c| c.kind == crate::CapabilityKind::Search)
                .unwrap_or(false)
            {
                "Searched"
            } else {
                "Queried"
            };
            if ctx.count == 1 {
                format!("{verb} {}{cap_s} → 1 result ({stats})", q.entity)
            } else {
                format!(
                    "{verb} {}{cap_s} → {} results ({stats})",
                    q.entity, ctx.count
                )
            }
        }
        Expr::Get(g) => format!(
            "Fetched {} `{}` ({stats})",
            g.reference.entity_type,
            g.reference.primary_slot_str()
        ),
        Expr::Chain(c) => format!(
            "Chain via `{}` → {} result(s) ({stats})",
            c.selector, ctx.count
        ),
        Expr::Create(c) => format!("Created {} via `{}` ({stats})", c.entity, c.capability),
        Expr::Delete(d) => format!(
            "Deleted {} `{}` ({stats})",
            d.target.entity_type,
            d.target.primary_slot_str()
        ),
        Expr::Invoke(i) => format!(
            "Invoked `{}` on {} ({stats})",
            i.capability, i.target.entity_type
        ),
        Expr::Page(p) => {
            if ctx.count == 1 {
                format!("Paged next batch for `{}` → 1 result ({stats})", p.handle)
            } else {
                format!(
                    "Paged next batch for `{}` → {} results ({stats})",
                    p.handle, ctx.count
                )
            }
        }
    }
}

fn format_predicate_short(p: &Predicate) -> String {
    match p {
        Predicate::True => "true".to_string(),
        Predicate::False => "false".to_string(),
        Predicate::Comparison { field, op, value } => {
            format!("{} {} {}", field, comp_op_str(*op), value_short(value))
        }
        Predicate::And { args } => args
            .iter()
            .map(format_predicate_short)
            .collect::<Vec<_>>()
            .join(" AND "),
        Predicate::Or { args } => {
            let inner = args
                .iter()
                .map(format_predicate_short)
                .collect::<Vec<_>>()
                .join(" OR ");
            format!("({inner})")
        }
        Predicate::Not { predicate } => format!("NOT ({})", format_predicate_short(predicate)),
        Predicate::ExistsRelation {
            relation,
            predicate,
        } => match predicate {
            Some(pr) => format!("EXISTS {} WHERE {}", relation, format_predicate_short(pr)),
            None => format!("EXISTS {}", relation),
        },
    }
}

fn comp_op_str(op: CompOp) -> &'static str {
    match op {
        CompOp::Eq => "=",
        CompOp::Neq => "!=",
        CompOp::Gt => ">",
        CompOp::Lt => "<",
        CompOp::Gte => ">=",
        CompOp::Lte => "<=",
        CompOp::In => "in",
        CompOp::Contains => "~",
        CompOp::Exists => "exists",
    }
}

fn value_short(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{s:?}"),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Object(o) => format!("{{{} keys}}", o.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader;

    #[test]
    fn intent_query_all() {
        let dir = std::path::Path::new("../../fixtures/schemas/petstore");
        if !dir.exists() {
            return;
        }
        let cgs = loader::load_schema_dir(dir).unwrap();
        let e = Expr::Query(crate::QueryExpr::all("Pet"));
        let s = render_intent(&e, &cgs);
        assert!(s.contains("Query Pet"));
    }
}
