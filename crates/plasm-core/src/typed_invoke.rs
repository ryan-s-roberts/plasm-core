//! Structured invoke/create inputs aligned with [`crate::schema::InputType`] (typed IR migration).

use crate::schema::{InputFieldSchema, InputType};
use crate::typed_literal::TypedLiteral;
use crate::value::{PlasmInputRef, Value};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Structured invoke body after lowering from [`Value`] using an [`InputType`] description.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedInvokeInput {
    Leaf(TypedLiteral),
    PlasmInputRef(PlasmInputRef),
    Array(Vec<TypedInvokeInput>),
    Object {
        fields: IndexMap<String, TypedInvokeInput>,
        /// Extra keys when `additional_fields` is true on the object schema.
        #[allow(clippy::option_option)]
        extra: Option<IndexMap<String, Value>>,
    },
    /// Preserves arbitrary JSON subtrees (`FieldType::Json`, unstructured blobs).
    Json(Value),
    Union {
        variant_index: usize,
        value: Box<TypedInvokeInput>,
    },
}

impl TypedInvokeInput {
    /// Materialize back to wire [`Value`] for CML / HTTP layers.
    pub fn to_value(&self) -> Value {
        match self {
            TypedInvokeInput::Leaf(t) => t.to_value(),
            TypedInvokeInput::PlasmInputRef(r) => Value::PlasmInputRef(r.clone()),
            TypedInvokeInput::Array(items) => {
                Value::Array(items.iter().map(Self::to_value).collect())
            }
            TypedInvokeInput::Object { fields, extra } => {
                let mut m: IndexMap<String, Value> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_value()))
                    .collect();
                if let Some(ex) = extra {
                    for (k, v) in ex {
                        m.insert(k.clone(), v.clone());
                    }
                }
                Value::Object(m)
            }
            TypedInvokeInput::Json(v) => v.clone(),
            TypedInvokeInput::Union { value, .. } => value.to_value(),
        }
    }
}

/// Invoke/create capability input: lowered structured form when possible, else raw [`Value`].
#[derive(Debug, Clone, PartialEq)]
pub enum InvokeInputPayload {
    Typed(TypedInvokeInput),
    Raw(Value),
}

impl InvokeInputPayload {
    #[must_use]
    pub fn raw(v: Value) -> Self {
        Self::Raw(v)
    }

    #[must_use]
    pub fn typed(t: TypedInvokeInput) -> Self {
        Self::Typed(t)
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            InvokeInputPayload::Typed(t) => t.to_value(),
            InvokeInputPayload::Raw(v) => v.clone(),
        }
    }

    /// Lift from a validated [`Value`] using the capability input schema root [`InputType`].
    pub fn lift(value: &Value, input_type: &InputType) -> Self {
        match lift_inner(value, input_type) {
            Ok(t) => Self::Typed(t),
            Err(_) => Self::Raw(value.clone()),
        }
    }
}

impl From<Value> for InvokeInputPayload {
    fn from(value: Value) -> Self {
        Self::Raw(value)
    }
}

impl Serialize for InvokeInputPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for InvokeInputPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = Value::deserialize(deserializer)?;
        Ok(InvokeInputPayload::Raw(v))
    }
}

fn lift_inner(value: &Value, input_type: &InputType) -> Result<TypedInvokeInput, ()> {
    if matches!(value, Value::PlasmInputRef(_)) {
        let r = match value {
            Value::PlasmInputRef(r) => r.clone(),
            _ => unreachable!(),
        };
        return Ok(TypedInvokeInput::PlasmInputRef(r));
    }

    match input_type {
        InputType::None => {
            if matches!(value, Value::Null) {
                Ok(TypedInvokeInput::Leaf(TypedLiteral::Null))
            } else {
                Err(())
            }
        }
        InputType::Value {
            field_type,
            allowed_values: _,
        } => {
            use crate::FieldType;
            if matches!(field_type, FieldType::Json) {
                return Ok(TypedInvokeInput::Json(value.clone()));
            }
            let lit = TypedLiteral::try_from_value(value).map_err(|_| ())?;
            Ok(TypedInvokeInput::Leaf(lit))
        }
        InputType::Object {
            fields,
            additional_fields,
        } => {
            let obj = value.as_object().ok_or(())?;
            let mut out: IndexMap<String, TypedInvokeInput> = IndexMap::new();
            for f in fields {
                match obj.get(&f.name) {
                    Some(fv) => {
                        let nested_ty = field_input_schema_to_input_type(f)?;
                        out.insert(f.name.clone(), lift_inner(fv, &nested_ty)?);
                    }
                    None => {
                        if f.required {
                            return Err(());
                        }
                    }
                }
            }
            let extra = if *additional_fields {
                let defined: std::collections::HashSet<_> =
                    fields.iter().map(|x| x.name.as_str()).collect();
                let mut rest = IndexMap::new();
                for (k, v) in obj.iter() {
                    if !defined.contains(k.as_str()) {
                        rest.insert(k.clone(), v.clone());
                    }
                }
                if rest.is_empty() {
                    None
                } else {
                    Some(rest)
                }
            } else {
                None
            };
            Ok(TypedInvokeInput::Object { fields: out, extra })
        }
        InputType::Array {
            element_type,
            min_length: _,
            max_length: _,
        } => {
            let arr = value.as_array().ok_or(())?;
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(lift_inner(item, element_type)?);
            }
            Ok(TypedInvokeInput::Array(out))
        }
        InputType::Union { variants } => {
            for (i, variant) in variants.iter().enumerate() {
                if let Ok(inner) = lift_inner(value, variant) {
                    return Ok(TypedInvokeInput::Union {
                        variant_index: i,
                        value: Box::new(inner),
                    });
                }
            }
            Err(())
        }
    }
}

fn field_input_schema_to_input_type(f: &InputFieldSchema) -> Result<InputType, ()> {
    use crate::FieldType;
    match &f.field_type {
        FieldType::Array => {
            let spec = f.array_items.as_ref().ok_or(())?;
            Ok(InputType::Array {
                element_type: Box::new(InputType::Value {
                    field_type: spec.field_type.clone(),
                    allowed_values: spec.allowed_values.clone(),
                }),
                min_length: None,
                max_length: None,
            })
        }
        FieldType::MultiSelect => Ok(InputType::Value {
            field_type: FieldType::MultiSelect,
            allowed_values: f.allowed_values.clone(),
        }),
        _ => Ok(InputType::Value {
            field_type: f.field_type.clone(),
            allowed_values: f.allowed_values.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldType;

    #[test]
    fn lifts_simple_object() {
        let input_type = InputType::Object {
            fields: vec![InputFieldSchema {
                name: "title".into(),
                field_type: FieldType::String,
                value_format: None,
                required: true,
                allowed_values: None,
                array_items: None,
                string_semantics: None,
                description: None,
                default: None,
                role: None,
            }],
            additional_fields: false,
        };
        let v = Value::Object({
            let mut m = IndexMap::new();
            m.insert("title".into(), Value::String("hi".into()));
            m
        });
        let p = InvokeInputPayload::lift(&v, &input_type);
        match p {
            InvokeInputPayload::Typed(TypedInvokeInput::Object { fields, .. }) => {
                assert!(fields.contains_key("title"));
            }
            other => panic!("expected typed object: {other:?}"),
        }
    }
}
