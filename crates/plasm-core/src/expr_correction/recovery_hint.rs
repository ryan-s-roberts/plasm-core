//! Structured lexicon recovery hints — render to LLM text via [`crate::error_render::format_recovery_hints`]
//! so symbolic prompts never mix `e#`/`p#` with canonical `View{…}` strings.

/// One deterministic hint when [`super::auto_correct::try_auto_correct`] cannot pick a unique rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryHint {
    /// Query entity has multiple possible scope parameters — the model must pick one.
    AmbiguousScopes {
        entity: String,
        /// `(scope_param_name, target_entity_name)` e.g. `("team_id", "Team")`
        scope_options: Vec<(String, String)>,
    },
    /// Lexicon returned multiple candidate field shapes — pick one full query expression.
    AmbiguousFieldCandidates {
        /// Canonical entity name (e.g. `View`)
        entity: String,
        /// Full canonical expressions such as `View{type}` or `View{space_id=Space(id)}`
        option_expressions: Vec<String>,
    },
}
