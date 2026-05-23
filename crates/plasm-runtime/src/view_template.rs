//! Minijinja rendering for CGS view `kind: computed` output bindings.

use indexmap::IndexMap;
use minijinja::{value::ValueKind, Environment, UndefinedBehavior};
use plasm_core::{temporal_wire_format_from_name, wire_temporal_value, Value};
use serde_json::json;

use crate::RuntimeError;

const VIEW_TEMPLATE_MAX_CHARS: usize = 32_768;

fn plasm_value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => json!(b),
        Value::Integer(i) => json!(i),
        Value::Float(f) => json!(f),
        Value::String(s) => json!(s),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(plasm_value_to_json).collect()),
        Value::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj {
                map.insert(k.clone(), plasm_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        Value::PlasmInputRef(_) | Value::UnionCtor { .. } => serde_json::Value::Null,
    }
}

fn register_view_template_filters(env: &mut Environment<'_>) {
    env.add_filter("urlencode", |s: String| -> Result<String, minijinja::Error> {
        Ok(url::form_urlencoded::byte_serialize(s.as_bytes()).collect())
    });
    env.add_filter(
        "wire_query_suffix",
        |json_text: String| -> Result<String, minijinja::Error> {
            let t = json_text.trim();
            if t.is_empty() || t == "null" {
                return Ok(String::new());
            }
            let v: serde_json::Value = serde_json::from_str(t).map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("wire_query_suffix: invalid JSON object: {e}"),
                )
            })?;
            let Some(obj) = v.as_object() else {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "wire_query_suffix: expected JSON object",
                ));
            };
            if obj.is_empty() {
                return Ok(String::new());
            }
            let mut parts = Vec::new();
            for (k, val) in obj {
                let val_s = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => continue,
                    other => other.to_string(),
                };
                parts.push(format!(
                    "{}={}",
                    url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>(),
                    url::form_urlencoded::byte_serialize(val_s.as_bytes()).collect::<String>()
                ));
            }
            if parts.is_empty() {
                Ok(String::new())
            } else {
                Ok(format!("&{}", parts.join("&")))
            }
        },
    );
    env.add_filter("strip_trailing_slash", |s: String| -> String {
        s.trim_end_matches('/').to_string()
    });
    env.add_filter(
        "json_encode",
        |v: minijinja::Value| -> Result<String, minijinja::Error> {
            let plasm = minijinja_to_plasm(v);
            let json = plasm_value_to_json(&plasm);
            serde_json::to_string(&json).map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    e.to_string(),
                )
            })
        },
    );
    env.add_filter(
        "wire_time",
        |v: minijinja::Value, format: String| -> Result<String, minijinja::Error> {
            let fmt = temporal_wire_format_from_name(&format).map_err(|e| {
                minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e)
            })?;
            let plasm = minijinja_to_plasm(v);
            let out = wire_temporal_value(plasm, fmt).map_err(|e| {
                minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e)
            })?;
            Ok(match out {
                Value::String(s) => s,
                Value::Integer(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => String::new(),
                Value::Array(_) | Value::Object(_) => {
                    serde_json::to_string(&plasm_value_to_json(&out)).unwrap_or_default()
                }
                Value::PlasmInputRef(_) | Value::UnionCtor { .. } => String::new(),
            })
        },
    );
}

fn minijinja_to_plasm(v: minijinja::Value) -> Value {
    match v.kind() {
        ValueKind::None | ValueKind::Undefined => Value::Null,
        ValueKind::Bool => Value::Bool(v.is_true()),
        ValueKind::String => Value::String(v.as_str().unwrap_or_default().to_string()),
        ValueKind::Number => {
            if let Some(i) = v.as_i64() {
                Value::Integer(i)
            } else {
                let s = v.to_string();
                Value::Float(s.parse().unwrap_or(0.0))
            }
        }
        ValueKind::Seq => {
            let mut items = Vec::new();
            if let Ok(iter) = v.try_iter() {
                for elem in iter {
                    items.push(minijinja_to_plasm(elem));
                }
            }
            Value::Array(items)
        }
        ValueKind::Map | ValueKind::Iterable | ValueKind::Plain => {
            if let Some(s) = v.as_str() {
                Value::String(s.to_string())
            } else {
                Value::String(v.to_string())
            }
        }
        ValueKind::Bytes => {
            Value::String(String::from_utf8_lossy(v.as_bytes().unwrap_or_default()).into())
        }
        ValueKind::Invalid | _ => Value::Null,
    }
}

/// Evaluate a view computed-field template against scope and materialized output fields.
pub fn render_view_computed_template(
    template: &str,
    scope: &IndexMap<String, Value>,
    fields_plain: &IndexMap<String, Value>,
) -> Result<Value, RuntimeError> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return Err(RuntimeError::ConfigurationError {
            message: "computed view template must be non-empty".into(),
        });
    }
    if trimmed.chars().count() > VIEW_TEMPLATE_MAX_CHARS {
        return Err(RuntimeError::ConfigurationError {
            message: format!(
                "computed view template exceeds {VIEW_TEMPLATE_MAX_CHARS} characters"
            ),
        });
    }

    let mut ctx = serde_json::Map::new();
    for (k, v) in scope {
        ctx.insert(k.clone(), plasm_value_to_json(v));
    }
    for (k, v) in fields_plain {
        ctx.insert(k.clone(), plasm_value_to_json(v));
    }

    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    register_view_template_filters(&mut env);

    let tmpl = env
        .template_from_str(trimmed)
        .map_err(|e| RuntimeError::ConfigurationError {
            message: format!("computed view template compile error: {e}"),
        })?;

    let rendered = tmpl
        .render(ctx)
        .map_err(|e| RuntimeError::ConfigurationError {
            message: format!("computed view template render error: {e}"),
        })?;

    Ok(Value::String(rendered))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_time_filter_in_template() {
        let mut scope = IndexMap::new();
        scope.insert("from".to_string(), Value::String("now-1h".into()));
        let out = render_view_computed_template(
            "{{ from | wire_time('unix_ms') }}",
            &scope,
            &IndexMap::new(),
        )
        .unwrap();
        assert_eq!(out, Value::String("now-1h".into()));
    }

    #[test]
    fn urlencode_filter_in_template() {
        let out = render_view_computed_template(
            "{{ 'a=b&c' | urlencode }}",
            &IndexMap::new(),
            &IndexMap::new(),
        )
        .unwrap();
        assert_eq!(out, Value::String("a%3Db%26c".into()));
    }

    #[test]
    fn wire_query_suffix_appends_params() {
        let out = render_view_computed_template(
            "{{ '{\"foo\":\"bar\"}' | wire_query_suffix }}",
            &IndexMap::new(),
            &IndexMap::new(),
        )
        .unwrap();
        assert_eq!(out, Value::String("&foo=bar".into()));
    }

    #[test]
    fn template_sees_prior_output_fields() {
        let mut fields = IndexMap::new();
        fields.insert("app_url".to_string(), Value::String("http://x/".into()));
        let out = render_view_computed_template(
            "{{ app_url | strip_trailing_slash }}/d/foo",
            &IndexMap::new(),
            &fields,
        )
        .unwrap();
        assert_eq!(out, Value::String("http://x/d/foo".into()));
    }
}
