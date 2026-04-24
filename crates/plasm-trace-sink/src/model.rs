//! Canonical ingest envelope (v1) and derived trace span row for billing/retrieval.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SCHEMA_VERSION: i32 = 1;

/// Ingest `AuditEvent.event_kind` for canonical MCP / execute trace segments (`payload` = [`plasm_trace::TraceEvent`] JSON).
pub const AUDIT_EVENT_KIND_MCP_TRACE_SEGMENT: &str = "mcp_trace_segment";

fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

/// Single client-emitted event (immutable audit log row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: Uuid,
    pub schema_version: i32,
    pub emitted_at: DateTime<Utc>,
    #[serde(skip_deserializing, default = "now_utc")]
    pub ingested_at: DateTime<Utc>,
    /// Correlates all spans from one logical invocation (MCP `plasm` call or HTTP batch).
    pub trace_id: Uuid,
    pub mcp_session_id: Option<String>,
    pub plasm_prompt_hash: Option<String>,
    pub plasm_execute_session: Option<String>,
    pub run_id: Option<Uuid>,
    pub call_index: Option<i64>,
    pub line_index: Option<i64>,
    pub tenant_id: Option<String>,
    pub principal_sub: Option<String>,
    /// Denormalized for Iceberg pruning; may also appear under `payload`.
    #[serde(default)]
    pub workspace_slug: Option<String>,
    /// Denormalized for Iceberg pruning; may also appear under `payload`.
    #[serde(default)]
    pub project_slug: Option<String>,
    pub event_kind: String,
    pub request_units: i64,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Denormalized span row for fast trace reads + billing projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpanRow {
    pub span_id: Uuid,
    pub event_id: Uuid,
    pub trace_id: Uuid,
    pub emitted_at: DateTime<Utc>,
    /// Coalesced for partitioning (`__none__` when tenant unknown).
    pub tenant_partition: String,
    pub mcp_session_id: Option<String>,
    pub plasm_prompt_hash: Option<String>,
    pub plasm_execute_session: Option<String>,
    pub run_id: Option<Uuid>,
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

#[derive(Debug, Deserialize, Serialize)]
pub struct IngestBatchRequest {
    pub events: Vec<AuditEvent>,
}

#[derive(Debug, Serialize)]
pub struct IngestBatchResponse {
    pub accepted: usize,
    pub duplicate_skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct TraceGetResponse {
    pub trace_id: Uuid,
    pub events: Vec<AuditEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceTotals {
    pub plasm_tool_calls: u64,
    pub plasm_expressions: u64,
    pub expression_lines: u64,
    pub batched_plasm_invocations: u64,
    pub domain_prompt_chars: u64,
    pub plasm_invocation_chars: u64,
    pub plasm_response_chars: u64,
    #[serde(default)]
    pub mcp_resource_read_chars: u64,
    pub total_duration_ms: u64,
    pub network_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub http_trace_entry_count: u64,
}

impl From<plasm_trace::TraceTotals> for TraceTotals {
    fn from(t: plasm_trace::TraceTotals) -> Self {
        Self {
            plasm_tool_calls: t.plasm_tool_calls,
            plasm_expressions: t.plasm_expressions,
            expression_lines: t.expression_lines,
            batched_plasm_invocations: t.batched_plasm_invocations,
            domain_prompt_chars: t.domain_prompt_chars,
            plasm_invocation_chars: t.plasm_invocation_chars,
            plasm_response_chars: t.plasm_response_chars,
            mcp_resource_read_chars: t.mcp_resource_read_chars,
            total_duration_ms: t.total_duration_ms,
            network_requests: t.network_requests,
            cache_hits: t.cache_hits,
            cache_misses: t.cache_misses,
            http_trace_entry_count: t.http_trace_entry_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    pub trace_id: Uuid,
    pub mcp_session_id: String,
    pub status: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub project_slug: String,
    pub tenant_id: String,
    pub totals: TraceTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceDetailRecord {
    pub kind: String,
    pub record: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurableTraceDetail {
    #[serde(flatten)]
    pub summary: TraceSummary,
    #[serde(default)]
    pub records: Vec<TraceDetailRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceListResponse {
    pub traces: Vec<TraceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceDetailResponse {
    pub trace_id: Uuid,
    pub detail: DurableTraceDetail,
}

#[derive(Debug, Serialize)]
pub struct BillingUsageResponse {
    pub usage: Vec<TraceSpanRow>,
}

impl AuditEvent {
    pub fn tenant_partition(&self) -> String {
        self.tenant_id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "__none__".to_string())
    }

    /// Workspace slug for lake columns and trace heads (`payload` fallback for older clients).
    pub fn audit_workspace_slug(&self) -> String {
        self.workspace_slug
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.payload
                    .get("workspace_slug")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
    }

    /// Project slug for lake columns and trace heads (`payload` fallback).
    pub fn audit_project_slug(&self) -> String {
        self.project_slug
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.payload
                    .get("project_slug")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
    }
}

/// `YYYYMM` UTC (e.g. `202604`) for Iceberg identity partition / pruning.
pub fn year_month_bucket_utc(dt: DateTime<Utc>) -> i32 {
    dt.year() * 100 + dt.month() as i32
}
