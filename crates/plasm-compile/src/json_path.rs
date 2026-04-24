//! Convert CGS [`plasm_core::JsonPathSegment`] paths into decode [`PathExpr`](crate::decoder::PathExpr).

use crate::decoder::{PathExpr, PathSegment};
use plasm_core::JsonPathSegment;

/// Build a [`PathExpr`] for nested GET decode (`RelationMaterialization::FromParentGet`).
pub fn path_expr_from_json_segments(segments: &[JsonPathSegment]) -> Result<PathExpr, String> {
    let mut segs = Vec::with_capacity(segments.len());
    for s in segments {
        match s {
            JsonPathSegment::Key { key } => {
                segs.push(PathSegment::Key { name: key.clone() });
            }
            JsonPathSegment::Wildcard { wildcard } => {
                if !wildcard {
                    return Err("JSON path wildcard segment must use `wildcard: true`".to_string());
                }
                segs.push(PathSegment::Wildcard);
            }
        }
    }
    Ok(PathExpr::new(segs))
}
