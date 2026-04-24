//! JSON-style unescaping for capability inputs whose schema marks structured string semantics.
//!
//! Quoted Plasm string literals only treat `\\` as an escape; sequences like `\n` in the source
//! become a backslash plus `n` in the parsed [`crate::Value::String`]. Agents often paste
//! JSON-escaped markdown into those quotes. Before compiling HTTP templates, we normalize those
//! payloads for structured string fields (see [`crate::schema::StringSemantics::is_structured_or_multiline`]).

use crate::schema::InputType;
use crate::value::{FieldType, Value};

/// Unescape common JSON-style backslash sequences (`\n`, `\t`, `\uXXXX`, etc.).
pub fn unescape_json_style_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let mut h = String::with_capacity(4);
                let mut early = None;
                for _ in 0..4 {
                    match it.next() {
                        Some(ch) if ch.is_ascii_hexdigit() => h.push(ch),
                        Some(ch) => {
                            early = Some(ch);
                            break;
                        }
                        None => break,
                    }
                }
                if h.len() == 4 {
                    if let Ok(cp) = u32::from_str_radix(&h, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        } else {
                            out.push('\\');
                            out.push('u');
                            out.push_str(&h);
                        }
                    } else {
                        out.push('\\');
                        out.push('u');
                        out.push_str(&h);
                    }
                } else {
                    out.push('\\');
                    out.push('u');
                    out.push_str(&h);
                    if let Some(ch) = early {
                        out.push(ch);
                    }
                }
            }
            Some(ch) => {
                out.push('\\');
                out.push(ch);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Apply [`unescape_json_style_escapes`] to string fields marked as structured in `input_type`.
pub fn normalize_structured_string_inputs(value: Value, input_type: &InputType) -> Value {
    match input_type {
        InputType::None | InputType::Value { .. } | InputType::Union { .. } => value,
        InputType::Object { fields, .. } => {
            let Value::Object(mut map) = value else {
                return value;
            };
            for field in fields {
                if field.field_type != FieldType::String {
                    continue;
                }
                if !field
                    .effective_string_semantics()
                    .is_structured_or_multiline()
                {
                    continue;
                }
                if let Some(Value::String(s)) = map.get_mut(&field.name) {
                    let new_s = unescape_json_style_escapes(s);
                    if new_s != *s {
                        *s = new_s;
                    }
                }
            }
            Value::Object(map)
        }
        InputType::Array { element_type, .. } => {
            let Value::Array(items) = value else {
                return value;
            };
            Value::Array(
                items
                    .into_iter()
                    .map(|v| normalize_structured_string_inputs(v, element_type.as_ref()))
                    .collect(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{InputFieldSchema, StringSemantics};

    #[test]
    fn unescape_basic_escapes() {
        assert_eq!(unescape_json_style_escapes("a\\nb"), "a\nb");
        assert_eq!(unescape_json_style_escapes("\\\\"), "\\");
        assert_eq!(unescape_json_style_escapes("\\\""), "\"");
    }

    #[test]
    fn unescape_unicode() {
        assert_eq!(unescape_json_style_escapes("\\u0041"), "A");
    }

    #[test]
    fn normalize_object_markdown_field() {
        let input_type = InputType::Object {
            fields: vec![InputFieldSchema {
                name: "p2".to_string(),
                field_type: FieldType::String,
                value_format: None,
                required: false,
                allowed_values: None,
                array_items: None,
                string_semantics: Some(StringSemantics::Markdown),
                description: None,
                default: None,
                role: None,
            }],
            additional_fields: false,
        };
        let v = Value::Object(
            vec![
                (
                    "p2".to_string(),
                    Value::String("line1\\n\\nline2".to_string()),
                ),
                ("id".to_string(), Value::String("x".to_string())),
            ]
            .into_iter()
            .collect(),
        );
        let out = normalize_structured_string_inputs(v, &input_type);
        let obj = out.as_object().unwrap();
        assert_eq!(obj.get("p2").unwrap().as_str().unwrap(), "line1\n\nline2");
        assert_eq!(obj.get("id").unwrap().as_str().unwrap(), "x");
    }

    #[test]
    fn short_semantics_unchanged() {
        let input_type = InputType::Object {
            fields: vec![InputFieldSchema {
                name: "p2".to_string(),
                field_type: FieldType::String,
                value_format: None,
                required: false,
                allowed_values: None,
                array_items: None,
                string_semantics: Some(StringSemantics::Short),
                description: None,
                default: None,
                role: None,
            }],
            additional_fields: false,
        };
        let v = Value::Object(
            vec![("p2".to_string(), Value::String("a\\nb".to_string()))]
                .into_iter()
                .collect(),
        );
        let out = normalize_structured_string_inputs(v, &input_type);
        assert_eq!(
            out.as_object()
                .unwrap()
                .get("p2")
                .unwrap()
                .as_str()
                .unwrap(),
            "a\\nb"
        );
    }
}
