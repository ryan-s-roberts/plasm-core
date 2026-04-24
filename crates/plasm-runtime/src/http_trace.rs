//! Optional per-execute HTTP call records for observability (MCP session traces, debugging).

use serde::{Deserialize, Serialize};

/// One outbound HTTP round-trip observed during an [`crate::ExecutionEngine::execute`] scope.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpTraceEntry {
    /// HTTP method label (e.g. `GET`, `POST`).
    pub method: String,
    /// Resolved URL (origin + path, or absolute URL for pagination continuations).
    pub url: String,
    /// Wall time for the transport send + response read.
    pub duration_ms: u64,
    pub outcome: HttpTraceOutcome,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HttpTraceOutcome {
    /// Request completed without transport error (includes HTTP 4xx/5xx parsed as API errors).
    Ok,
    /// Transport or runtime error before a successful JSON payload.
    Error {
        /// Short error summary (no response bodies).
        message: String,
    },
}
