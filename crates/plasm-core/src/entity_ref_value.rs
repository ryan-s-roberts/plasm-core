//! Strongly typed **normalized** shape for compound (and atomic) `entity_ref` constructor values.
//!
//! Wire / JSON / MCP still use [`crate::value::Value`]; this module is the type-system boundary
//! for values that are *intended* as `FieldType::EntityRef` payloads after parse/coercion — see
//! [`EntityRefPayload::try_from_value`] and [`EntityRefPayload::to_value`].

use crate::value::Value;
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

#[cfg(test)]
mod tests {
    use super::*;

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
