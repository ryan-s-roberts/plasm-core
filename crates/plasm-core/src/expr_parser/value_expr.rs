//! Contract types for **typed** Plasm program values (documentation + future lowering helpers).
//!
//! The executable carrier during parse and typecheck is [`crate::value::Value::PlasmInputRef`];
//! these types describe the same surface intent at the language-design level.

use crate::Value;
use indexmap::IndexMap;

/// Dynamic value positions in programs (method args, predicate RHS in program mode, array
/// elements) — literals plus references to bindings / node outputs.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    Literal(Value),
    Binding(String),
    Field {
        base: String,
        path: Vec<String>,
    },
    Array(Vec<ValueExpr>),
    Object(IndexMap<String, ValueExpr>),
}

/// `source[field,…] <<TAG` … `TAG` render surface (see repository `docs/plasm-language-unification.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderExpr {
    pub source: String,
    pub fields: Vec<String>,
    pub template: String,
}
