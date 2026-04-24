//! Storage ports: append vs query are separate traits; [`AuditSpanStore`] is their intersection (implemented by [`crate::iceberg_writer::IcebergSink`]).

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::model::{AuditEvent, DurableTraceDetail, TraceHeadRow, TraceSpanRow, TraceSummary};

#[derive(Clone, Debug)]
pub struct TenantId(String);

impl TenantId {
    pub fn parse(raw: &str) -> anyhow::Result<Self> {
        let v = raw.trim();
        if v.is_empty() {
            anyhow::bail!("tenant_id must be non-empty");
        }
        Ok(Self(v.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimeWindow {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl TimeWindow {
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> anyhow::Result<Self> {
        if from > to {
            anyhow::bail!("from must be <= to");
        }
        Ok(Self { from, to })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TraceListStatusFilter {
    All,
    Live,
    Completed,
}

impl TraceListStatusFilter {
    pub fn parse(raw: Option<&str>) -> anyhow::Result<Self> {
        match raw.unwrap_or("all").to_ascii_lowercase().as_str() {
            "all" | "" => Ok(Self::All),
            "live" => Ok(Self::Live),
            "completed" => Ok(Self::Completed),
            other => anyhow::bail!("invalid status filter: {other}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TraceListFilter<'a> {
    pub tenant: &'a TenantId,
    pub project_slug: Option<&'a str>,
    pub status: TraceListStatusFilter,
    pub offset: usize,
    pub limit: usize,
}

/// Append-only: Parquet/Iceberg writes for `audit_events` and `trace_spans`.
#[async_trait]
pub trait AuditSpanWriter: Send + Sync {
    async fn append_audit_events(&self, events: &[AuditEvent]) -> anyhow::Result<()>;
    async fn append_trace_spans(&self, rows: &[TraceSpanRow]) -> anyhow::Result<()>;
    /// Append `audit_events` then `trace_spans` under one storage lock so readers never see audit-only gaps.
    async fn append_audit_events_with_trace_spans(
        &self,
        events: &[AuditEvent],
        spans: &[TraceSpanRow],
    ) -> anyhow::Result<()>;
    async fn append_trace_heads(&self, rows: &[TraceHeadRow]) -> anyhow::Result<()>;
}

/// Read path: idempotency checks and HTTP GET backends.
#[async_trait]
pub trait AuditSpanReader: Send + Sync {
    /// Subset of `ids` already present in `audit_events`.
    ///
    /// When `tenant_partitions` is `Some` with one or more values (within the implementation cap),
    /// the query adds `AND tenant_partition IN (...)` so Iceberg can prune to those partitions.
    /// Pass `None` to scan all partitions (e.g. tests or callers without partition context).
    async fn existing_event_ids(
        &self,
        ids: &[Uuid],
        tenant_partitions: Option<&[String]>,
    ) -> anyhow::Result<HashSet<Uuid>>;

    /// Audit rows for `trace_id`, ordered by `emitted_at`, `call_index`, `line_index`.
    async fn load_trace_events(&self, trace_id: Uuid) -> anyhow::Result<Vec<AuditEvent>>;
    async fn load_latest_trace_heads(
        &self,
        trace_ids: &[Uuid],
    ) -> anyhow::Result<Vec<TraceHeadRow>>;

    /// Billing-eligible spans in `[from, to]` scoped to one tenant.
    async fn load_billing_usage_scoped(
        &self,
        tenant: &TenantId,
        window: TimeWindow,
    ) -> anyhow::Result<Vec<TraceSpanRow>>;

    /// Privileged global billing usage in `[from, to]` across all tenants.
    async fn load_billing_usage_global(
        &self,
        window: TimeWindow,
    ) -> anyhow::Result<Vec<TraceSpanRow>>;

    /// Durable trace summaries by tenant and optional project filter.
    async fn list_trace_summaries(
        &self,
        filter: TraceListFilter<'_>,
    ) -> anyhow::Result<Vec<TraceSummary>>;

    /// Durable trace detail for one trace in tenant scope.
    async fn load_trace_detail(
        &self,
        tenant: &TenantId,
        trace_id: Uuid,
    ) -> anyhow::Result<Option<DurableTraceDetail>>;
}

/// Full sink capability for [`crate::state::AppState`] (`Arc<dyn AuditSpanStore>`).
pub trait AuditSpanStore: AuditSpanWriter + AuditSpanReader {}

impl<T> AuditSpanStore for T where T: AuditSpanWriter + AuditSpanReader + ?Sized {}
