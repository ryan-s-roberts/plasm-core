//! Strongly typed **normalized** shape for compound (and atomic) `entity_ref` constructor values.
//!
//! Wire / JSON / MCP still use [`crate::value::Value`]; this module is the type-system boundary
//! for values that are *intended* as `FieldType::EntityRef` payloads after parse/coercion — see
//! [`EntityRefPayload::try_from_value`] and [`EntityRefPayload::to_value`].

use crate::value::Value;
use crate::EntityDef;
use indexmap::IndexMap;
use thiserror::Error;

/// Scalar accepted inside a compound `entity_ref` map (and as a unary ref value).
#[derive(Debug, Clone, PartialEq)]
pub enum EntityRefAtom {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
}

/// Recursive compound `entity_ref` constructor: leaves are [`EntityRefAtom`]; nesting is
/// `Compound` maps (e.g. nested compound-key targets in parser compensation).
#[derive(Debug, Clone, PartialEq)]
pub enum EntityRefPayload {
    Atom(EntityRefAtom),
    Compound(IndexMap<String, EntityRefPayload>),
}

/// [`Value`] is not a legal normalized `entity_ref` constructor shape.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EntityRefValueError {
    #[error("entity_ref value cannot be null")]
    Null,
    #[error("entity_ref value cannot be an array")]
    Array,
    #[error("entity_ref compound map must be non-empty")]
    EmptyCompound,
    #[error("unsupported value variant for entity_ref")]
    Unsupported,
}

impl EntityRefPayload {
    /// Recognize the subset of [`Value`] used for `FieldType::EntityRef` after normalization.
    pub fn try_from_value(v: &Value) -> Result<Self, EntityRefValueError> {
        match v {
            Value::PlasmInputRef(_) => Err(EntityRefValueError::Unsupported),
            Value::Null => Err(EntityRefValueError::Null),
            Value::Bool(b) => Ok(Self::Atom(EntityRefAtom::Bool(*b))),
            Value::Integer(i) => Ok(Self::Atom(EntityRefAtom::Integer(*i))),
            Value::Float(f) => Ok(Self::Atom(EntityRefAtom::Float(*f))),
            Value::String(s) => Ok(Self::Atom(EntityRefAtom::String(s.clone()))),
            Value::Array(_) => Err(EntityRefValueError::Array),
            Value::Object(m) => {
                if m.is_empty() {
                    return Err(EntityRefValueError::EmptyCompound);
                }
                let mut out = IndexMap::with_capacity(m.len());
                for (k, child) in m.iter() {
                    out.insert(k.clone(), Self::try_from_value(child)?);
                }
                Ok(Self::Compound(out))
            }
        }
    }

    /// Serialize back to [`Value`] for env maps, HTTP, and serde-stable roundtrip.
    pub fn to_value(&self) -> Value {
        match self {
            Self::Atom(EntityRefAtom::String(s)) => Value::String(s.clone()),
            Self::Atom(EntityRefAtom::Integer(i)) => Value::Integer(*i),
            Self::Atom(EntityRefAtom::Float(f)) => Value::Float(*f),
            Self::Atom(EntityRefAtom::Bool(b)) => Value::Bool(*b),
            Self::Compound(m) => {
                Value::Object(m.iter().map(|(k, v)| (k.clone(), v.to_value())).collect())
            }
        }
    }

    /// True when `v` parses as a legal `entity_ref` payload (atomic or compound tree).
    #[inline]
    pub fn value_is_legal_shape(v: &Value) -> bool {
        Self::try_from_value(v).is_ok()
    }
}

/// If `value` is a row-shaped object whose **top-level** fields include scalar identity slots for
/// `target` (per [`EntityDef::key_vars`] or [`EntityDef::id_field`]), return an equivalent
/// [`Value`] that satisfies [`EntityRefPayload::value_is_legal_shape`] for `EntityRef(target)`.
///
/// Used when a predicate RHS carries a bound **entity row** (get/query result) but the slot
/// expects an **entity reference**: narrow to the canonical key shape without nested embeds.
#[must_use]
pub fn try_narrow_entity_row_to_entity_ref_value(
    value: &Value,
    target: &EntityDef,
) -> Option<Value> {
    let Value::Object(map) = value else {
        return None;
    };

    fn scalar_atom(v: &Value) -> Option<Value> {
        match v {
            Value::String(_) | Value::Integer(_) | Value::Float(_) | Value::Bool(_) => {
                Some(v.clone())
            }
            _ => None,
        }
    }

    if target.key_vars.len() >= 2 {
        let mut out = IndexMap::new();
        for k in &target.key_vars {
            let v = map.get(k.as_str())?;
            out.insert(k.to_string(), scalar_atom(v)?);
        }
        let candidate = Value::Object(out);
        return EntityRefPayload::value_is_legal_shape(&candidate).then_some(candidate);
    }

    if target.key_vars.len() == 1 {
        let v = map.get(target.key_vars[0].as_str())?;
        return scalar_atom(v);
    }

    let v = map.get(target.id_field.as_str())?;
    scalar_atom(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityDef;
    use crate::EntityFieldName;

    #[test]
    fn narrow_row_to_entity_ref_compound_keys() {
        let target = EntityDef {
            name: "Repo".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: IndexMap::new(),
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![
                EntityFieldName::from("owner"),
                EntityFieldName::from("name"),
            ],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let row = Value::Object(
            vec![
                ("owner".into(), Value::String("a".into())),
                ("name".into(), Value::String("b".into())),
                ("extra".into(), Value::String("noise".into())),
            ]
            .into_iter()
            .collect(),
        );
        let narrow = try_narrow_entity_row_to_entity_ref_value(&row, &target).unwrap();
        assert!(EntityRefPayload::value_is_legal_shape(&narrow));
    }

    #[test]
    fn narrow_row_fails_when_keys_nested_not_scalar() {
        let target = EntityDef {
            name: "Repo".into(),
            description: String::new(),
            id_field: "id".into(),
            id_format: None,
            id_from: None,
            fields: IndexMap::new(),
            relations: IndexMap::new(),
            expression_aliases: vec![],
            implicit_request_identity: false,
            key_vars: vec![
                EntityFieldName::from("owner"),
                EntityFieldName::from("name"),
            ],
            abstract_entity: false,
            domain_projection_examples: false,
            primary_read: None,
        };
        let row = Value::Object(
            vec![(
                "owner".into(),
                Value::Object(
                    vec![("login".into(), Value::String("x".into()))]
                        .into_iter()
                        .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        );
        assert!(try_narrow_entity_row_to_entity_ref_value(&row, &target).is_none());
    }

    #[test]
    fn roundtrip_compound() {
        let v = Value::Object(
            vec![
                ("owner".into(), Value::String("a".into())),
                ("name".into(), Value::String("b".into())),
            ]
            .into_iter()
            .collect::<IndexMap<_, _>>(),
        );
        let p = EntityRefPayload::try_from_value(&v).unwrap();
        assert_eq!(p.to_value(), v);
    }

    #[test]
    fn rejects_null_and_array() {
        assert!(matches!(
            EntityRefPayload::try_from_value(&Value::Null),
            Err(EntityRefValueError::Null)
        ));
        assert!(matches!(
            EntityRefPayload::try_from_value(&Value::Array(vec![])),
            Err(EntityRefValueError::Array)
        ));
    }

    #[test]
    fn rejects_empty_object() {
        assert!(matches!(
            EntityRefPayload::try_from_value(&Value::Object(IndexMap::new())),
            Err(EntityRefValueError::EmptyCompound)
        ));
    }
}
