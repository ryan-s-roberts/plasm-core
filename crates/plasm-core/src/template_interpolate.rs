//! `${binding.path}` interpolation in program string literals (heredocs, quoted strings).
//!
//! Distinct from plan-layer `PlanValue::Template` and from Minijinja row templates (`{{ }}`).

use std::collections::BTreeMap;

use thiserror::Error;

use crate::value::Value;

/// Binding name → value for `${alias.path}` resolution.
pub type BindingScope<'a> = BTreeMap<&'a str, &'a Value>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InterpolateError {
    #[error("unresolved template reference `${path}` (in-scope bindings: {available})")]
    UnresolvedReference { path: String, available: String },
    #[error("interpolated string exceeds maximum length ({max} bytes)")]
    MaxLengthExceeded { max: usize },
}

pub const DEFAULT_MAX_INTERPOLATED_LEN: usize = 512 * 1024;

pub use crate::template_ref::{
    contains_dollar_interpolation, for_each_interpolation_path, interpolation_paths,
    interpolation_roots, validate_interpolation_syntax, RefKind, TemplateRefContext,
};

/// Root binding names referenced by `${name}` or `${name.path}` in `s`.
#[inline]
pub fn dollar_interpolation_roots(s: &str) -> Vec<String> {
    interpolation_roots(s)
}

/// Expand `${ident}` and `${ident.path}` using `scope` (binding roots only).
pub fn interpolate_string(
    input: &str,
    scope: &BindingScope<'_>,
) -> Result<String, InterpolateError> {
    interpolate_string_with_max(input, scope, DEFAULT_MAX_INTERPOLATED_LEN)
}

/// Like [`interpolate_string`] with an owned binding map.
pub fn interpolate_string_map(
    input: &str,
    scope: &BTreeMap<String, Value>,
) -> Result<String, InterpolateError> {
    let refs: BindingScope<'_> = scope.iter().map(|(k, v)| (k.as_str(), v)).collect();
    interpolate_string(input, &refs)
}

pub fn interpolate_string_with_max(
    input: &str,
    scope: &BindingScope<'_>,
    max_len: usize,
) -> Result<String, InterpolateError> {
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'{' {
                let start = i + 2;
                let Some(end_rel) = input[start..].find('}') else {
                    out.push('$');
                    i += 1;
                    continue;
                };
                let path = &input[start..start + end_rel];
                let value = resolve_path(path, scope).map_err(|available| {
                    InterpolateError::UnresolvedReference {
                        path: path.to_string(),
                        available,
                    }
                })?;
                out.push_str(&value);
                i = start + end_rel + 1;
                if out.len() > max_len {
                    return Err(InterpolateError::MaxLengthExceeded { max: max_len });
                }
                continue;
            }
        }
        out.push(char::from(bytes[i]));
        i += 1;
        if out.len() > max_len {
            return Err(InterpolateError::MaxLengthExceeded { max: max_len });
        }
    }
    Ok(out)
}

fn resolve_path(path: &str, scope: &BindingScope<'_>) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(list_bindings(scope));
    }
    let mut parts = path.split('.');
    let root = parts.next().unwrap();
    let Some(v) = scope.get(root) else {
        return Err(list_bindings(scope));
    };
    let mut cur = (*v).clone();
    for seg in parts {
        cur = match cur {
            Value::Object(map) => map.get(seg).cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        };
    }
    scalar_to_string(&cur).ok_or_else(|| list_bindings(scope))
}

fn scalar_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

fn list_bindings(scope: &BindingScope<'_>) -> String {
    let mut names: Vec<_> = scope.keys().copied().collect();
    names.sort();
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_binding_path() {
        let report = Value::Object(indexmap::IndexMap::from([(
            "content".to_string(),
            Value::String("hello".into()),
        )]));
        let scope = BindingScope::from([("spec_md", &report)]);
        let out = interpolate_string("prefix ${spec_md.content} suffix", &scope).unwrap();
        assert_eq!(out, "prefix hello suffix");
    }

    #[test]
    fn escape_dollar_dollar() {
        let scope = BindingScope::new();
        let out = interpolate_string("cost $$50", &scope).unwrap();
        assert_eq!(out, "cost $50");
    }

    #[test]
    fn unresolved_lists_bindings() {
        let binding = Value::String("x".into());
        let scope = BindingScope::from([("a", &binding)]);
        let err = interpolate_string("${missing}", &scope).unwrap_err();
        assert!(matches!(err, InterpolateError::UnresolvedReference { .. }));
    }

    #[test]
    fn template_ref_context_classifies_row_binding() {
        use crate::template_ref::{RefKind, TemplateRefContext};
        let ctx = TemplateRefContext::for_row_scope("_");
        assert_eq!(ctx.classify_root("_"), RefKind::RowBinding);
        assert!(ctx.plan_node_roots_from_string("${_.id}").is_empty());
    }
}
