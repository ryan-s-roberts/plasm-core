//! Semantic spans for trace-sink HTTP and Iceberg paths (ingest, read, billing).
//!
//! Stable names: `plasm_trace_sink.<domain>.<operation>` — used for security/billing dashboards.

use tracing::Span;
use uuid::Uuid;

#[inline]
pub(crate) fn ingest_events_batch(incoming_count: usize) -> Span {
    tracing::info_span!(
        "plasm_trace_sink.billing.ingest_events_batch",
        incoming_count = incoming_count,
    )
}

#[inline]
pub(crate) fn read_trace_list(tenant_id: &str, limit: usize, offset: usize) -> Span {
    tracing::debug_span!(
        "plasm_trace_sink.billing.read_trace_list",
        tenant_id = tenant_id,
        limit = limit,
        offset = offset,
    )
}

#[inline]
pub(crate) fn read_trace_detail(tenant_id: &str, trace_id: &Uuid) -> Span {
    tracing::debug_span!(
        "plasm_trace_sink.billing.read_trace_detail",
        tenant_id = tenant_id,
        trace_id = %trace_id,
    )
}

#[inline]
pub(crate) fn billing_usage_query(scoped_tenant: bool) -> Span {
    tracing::info_span!(
        "plasm_trace_sink.billing.usage_query",
        scoped_tenant = scoped_tenant,
    )
}
