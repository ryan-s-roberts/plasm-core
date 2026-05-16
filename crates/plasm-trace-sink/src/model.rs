//! Sink-specific projection row types; wire DTOs live in [`plasm_observability_contracts`].

pub use plasm_observability_contracts::{
    AuditEvent, DurableTraceDetail, IngestBatchRequest, IngestBatchResponse, TraceDetailRecord,
    TraceDetailResponse, TraceGetResponse, TraceListResponse, TraceSummary, TraceTotals,
    AUDIT_EVENT_KIND_MCP_TRACE_SEGMENT, SCHEMA_VERSION,
};

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `YYYYMM` UTC (e.g. `202604`) for Iceberg identity partition / pruning.
pub fn year_month_bucket_utc(dt: DateTime<Utc>) -> i32 {
    dt.year() * 100 + dt.month() as i32
}

/// Denormalized span row for fast trace reads + billing projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpanRow {
    pub span_id: Uuid,
    pub event_id: Uuid,
    pub trace_id: Uuid,
    pub emitted_at: chrono::DateTime<chrono::Utc>,
    /// Coalesced for partitioning (`__none__` when tenant unknown).
    pub tenant_partition: String,
    pub mcp_session_id: Option<String>,
    pub plasm_prompt_hash: Option<String>,
    pub plasm_execute_session: Option<String>,
    pub run_id: Option<String>,
    pub call_index: Option<i64>,
    pub line_index: Option<i64>,
    pub span_name: String,
    pub is_billing_event: bool,
    pub billing_event_type: Option<String>,
    pub request_units: i64,
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub api_entry_id: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub attributes_json: serde_json::Value,
}

/// Append-only durable projection row for latest trace summary snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceHeadRow {
    pub trace_id: Uuid,
    pub tenant_partition: String,
    pub tenant_id: String,
    pub project_slug: String,
    pub mcp_session_id: Option<String>,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub updated_at_ms: i64,
    pub expression_lines: i64,
    pub max_call_index: Option<i64>,
    /// JSON [`plasm_trace::SessionTraceCountersSnapshot`] for incremental list totals.
    #[serde(default)]
    pub totals_json: String,
    #[serde(default)]
    pub workspace_slug: String,
}

#[derive(Debug, Serialize)]
pub struct BillingUsageResponse {
    pub usage: Vec<TraceSpanRow>,
}
