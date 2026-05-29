//! Unified compile-time contract for program binding labels (`ident = …`).
//!
//! Single source of truth for row entity type, cardinality proof, and continuation mode —
//! consumed by [`crate::plasm_dag`] when lowering `label.relation` / postfix chains.

use crate::plasm_plan::{
    InputCardinalityProof, RelationSourceCardinality, ResultShape, QualifiedEntityKey,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProgramBindingContract {
    pub label: String,
    pub row_entity: QualifiedEntityKey,
    pub result_shape: ResultShape,
    pub row_cardinality: RowCardinalityProof,
    pub continuation: ContinuationCapability,
    /// Surface or relation-expanded Plasm for anchor re-parse (`label.<tail>` text substitution).
    pub anchor_plasm: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowCardinalityProof {
    /// Get, single data row, or one-cardinality relation from a statically singleton parent.
    StaticSingleton,
    /// Query/search, many-relation, or list-preserving compute chain.
    StaticPlural,
    /// `.limit(1)` / `.singleton()` on a row-producing binding.
    BoundedSingleton {
        kind: BoundedSingletonKind,
        /// When true, the bounded pick came from a plural/static-many source (runtime proof).
        from_plural_source: bool,
    },
    /// Derive / for_each / unknown compute — runtime must verify.
    RuntimeChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundedSingletonKind {
    LimitOne,
    ExplicitSingletonPostfix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentPolicy {
    SingleSegment,
    MultiSegment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContinuationCapability {
    /// `label.<cgs_relation>` — CGS relation chains (typed and/or anchor expansion).
    RelationDot { segments: SegmentPolicy },
    /// Postfix only: `.limit`, `[proj]`, `<<render`, `.page_size`, `.singleton`.
    PostfixOnly,
    /// `label.content` scalar for render rows (not a relation receiver).
    RenderContentScalar,
    /// Aggregate, render row, derive, data literal — no `label.` extension.
    Terminal,
}

impl RowCardinalityProof {
    pub(crate) fn to_relation_source_cardinality(self) -> RelationSourceCardinality {
        match self {
            Self::StaticSingleton => RelationSourceCardinality::Single,
            Self::StaticPlural => RelationSourceCardinality::Many,
            Self::BoundedSingleton {
                from_plural_source: true,
                ..
            }
            | Self::RuntimeChecked => RelationSourceCardinality::RuntimeCheckedSingleton,
            Self::BoundedSingleton {
                from_plural_source: false,
                ..
            } => RelationSourceCardinality::Single,
        }
    }

    /// Maps to [`InputCardinalityProof`] for plan-layer singleton checks (see `analyze_static_cardinality`).
    #[allow(dead_code)]
    pub(crate) fn to_input_cardinality_proof(self) -> InputCardinalityProof {
        match self {
            Self::StaticSingleton
            | Self::BoundedSingleton {
                from_plural_source: false,
                ..
            } => InputCardinalityProof::StaticSingleton,
            Self::StaticPlural
            | Self::BoundedSingleton {
                from_plural_source: true,
                ..
            }
            | Self::RuntimeChecked => InputCardinalityProof::RuntimeCheckedSingleton,
        }
    }

    /// After traversing a cardinality-one relation from this binding, what row proof does the result carry?
    pub(crate) fn after_one_cardinality_relation(self) -> Self {
        match self {
            Self::StaticSingleton => Self::StaticSingleton,
            Self::BoundedSingleton {
                from_plural_source: false,
                ..
            } => Self::StaticSingleton,
            Self::StaticPlural
            | Self::BoundedSingleton {
                from_plural_source: true,
                ..
            }
            | Self::RuntimeChecked => Self::RuntimeChecked,
        }
    }

    /// After traversing a cardinality-many relation from this binding.
    pub(crate) fn after_many_cardinality_relation(self) -> Self {
        Self::StaticPlural
    }
}

impl ProgramBindingContract {
    pub(crate) fn supports_relation_dot(&self) -> bool {
        matches!(
            self.continuation,
            ContinuationCapability::RelationDot { .. } | ContinuationCapability::PostfixOnly
        )
    }

    pub(crate) fn relation_source_cardinality(&self) -> RelationSourceCardinality {
        self.row_cardinality.to_relation_source_cardinality()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_singleton_maps_to_single_source() {
        assert_eq!(
            RowCardinalityProof::StaticSingleton.to_relation_source_cardinality(),
            RelationSourceCardinality::Single
        );
    }

    #[test]
    fn static_plural_maps_to_many_source() {
        assert_eq!(
            RowCardinalityProof::StaticPlural.to_relation_source_cardinality(),
            RelationSourceCardinality::Many
        );
    }

    #[test]
    fn bounded_from_plural_maps_runtime_checked() {
        assert_eq!(
            RowCardinalityProof::BoundedSingleton {
                kind: BoundedSingletonKind::LimitOne,
                from_plural_source: true,
            }
            .to_relation_source_cardinality(),
            RelationSourceCardinality::RuntimeCheckedSingleton
        );
    }

    #[test]
    fn one_rel_from_singleton_parent_yields_singleton_child() {
        let parent = RowCardinalityProof::StaticSingleton;
        assert_eq!(
            parent.after_one_cardinality_relation(),
            RowCardinalityProof::StaticSingleton
        );
    }

    #[test]
    fn bounded_from_plural_maps_runtime_checked_input() {
        assert_eq!(
            RowCardinalityProof::BoundedSingleton {
                kind: BoundedSingletonKind::LimitOne,
                from_plural_source: true,
            }
            .to_input_cardinality_proof(),
            InputCardinalityProof::RuntimeCheckedSingleton
        );
    }
}
