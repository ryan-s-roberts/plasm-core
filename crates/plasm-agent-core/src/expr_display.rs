//! Short human-readable [`Expr`] line (REPL `→ …` hint, MCP tool text).

use plasm_core::Expr;

pub fn expr_display(expr: &Expr) -> String {
    match expr {
        Expr::Get(g) => format!("Get({})", g.reference),
        Expr::Query(q) => {
            let pred = if q.predicate.is_some() {
                "filtered"
            } else {
                "all"
            };
            let cap = q
                .capability_name
                .as_deref()
                .map(|c| format!(" cap={c}"))
                .unwrap_or_default();
            format!("Query({} {}){cap}", q.entity, pred)
        }
        Expr::Chain(c) => {
            format!("Chain({} .{})", expr_display(&c.source), c.selector)
        }
        Expr::Create(c) => format!("Create({})", c.entity),
        Expr::Delete(d) => format!("Delete({})", d.target),
        Expr::Invoke(i) => format!("Invoke({} {})", i.capability, i.target.entity_type),
        Expr::Page(p) => match p.limit {
            Some(l) => format!("page({} limit={l})", p.handle),
            None => format!("page({})", p.handle),
        },
    }
}
