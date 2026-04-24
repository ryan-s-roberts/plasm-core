//! Unified semantic summaries and structured errors for REPL, CLI, and plan execution.

use serde::{Deserialize, Serialize};

/// Append extra lines to `correction` with blank-line separators (one unified CORRECTION block).
pub fn append_correction_lines(mut correction: String, lines: Vec<String>) -> String {
    if lines.is_empty() {
        return correction;
    }
    correction.push_str("\n\n");
    correction.push_str(&lines.join("\n"));
    correction
}

/// Compact outcome of a successfully executed Plasm step (human- and LLM-readable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepSummary {
    /// Single-line description, e.g. "Queried Issue (issue_query) → 42 rows".
    pub message: String,
    /// Primary entity touched, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// High-level operation: `query`, `get`, `chain`, `create`, `delete`, `invoke`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Result row count when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

/// Structured outcome: **correction** for the LLM, **error** for operator logs (not sent in eval JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepError {
    /// What to change next — full imperative text for the model (may contain multiple paragraphs).
    pub correction: String,
    pub category: StepErrorCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_offset: Option<usize>,
    /// Raw error text (`thiserror` / parser line) for logs only; omitted from eval `correction_context` JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StepError {
    pub fn new(
        category: StepErrorCategory,
        correction: impl Into<String>,
        span_offset: Option<usize>,
    ) -> Self {
        Self {
            correction: correction.into(),
            category,
            span_offset,
            error: None,
        }
    }

    /// Type-check failure: `correction` is the full instruction; `error` is the raw [`TypeError`] string.
    pub fn type_correction(correction: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            correction: correction.into(),
            category: StepErrorCategory::Type,
            span_offset: None,
            error: Some(error.into()),
        }
    }

    /// Parse failure: `correction` is the instruction; `error` is the raw log line.
    pub fn parse_correction(
        correction: impl Into<String>,
        error: impl Into<String>,
        span_offset: Option<usize>,
    ) -> Self {
        Self {
            correction: correction.into(),
            category: StepErrorCategory::Parse,
            span_offset,
            error: Some(error.into()),
        }
    }
}

/// Coarse error channel for tooling and retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepErrorCategory {
    Parse,
    Type,
    Runtime,
    Auth,
    Network,
    Config,
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.correction)
    }
}

/// Inputs for post-execution summary (decouples plasm-core from `ExecutionResult`).
#[derive(Debug, Clone, Default)]
pub struct OutcomeContext {
    pub count: usize,
    pub primary_entity_type: Option<String>,
    pub source_label: &'static str,
    pub duration_ms: u64,
    pub network_requests: usize,
    pub cache_hits: usize,
}
