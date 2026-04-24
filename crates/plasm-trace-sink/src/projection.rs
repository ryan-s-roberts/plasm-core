//! Hot projections on the same SqlCatalog Postgres database as Iceberg (`plasm_trace_sink` schema).
//! Iceberg remains the durable lake; this layer accelerates idempotency, trace head lookups, and listing.

use std::collections::HashSet;

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::append_port::{TraceListFilter, TraceListStatusFilter};
use crate::model::{TraceHeadRow, TraceSummary, TraceTotals};
use crate::trace_totals::trace_totals_from_head_row;

/// Connection to projection tables (same Postgres as JanKaul SqlCatalog).
pub struct ProjectionStore {
    pool: PgPool,
}

impl ProjectionStore {
    /// Connect using the same JDBC URL as Iceberg SqlCatalog (`postgresql://…` / `postgres://…` only).
    pub async fn connect(catalog_url: &str) -> anyhow::Result<Self> {
        let url = catalog_url.trim();
        if !(url.starts_with("postgres://") || url.starts_with("postgresql://")) {
            anyhow::bail!(
                "trace projections require a Postgres SqlCatalog URL (postgresql:// or postgres://); sqlite metadata is not supported (got {}).",
                url.chars().take(48).collect::<String>()
            );
        }
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        migrate_postgres(&self.pool).await
    }

    pub async fn count_trace_heads(&self) -> anyhow::Result<i64> {
        let v: i64 =
            sqlx::query_scalar("SELECT COUNT(*)::bigint FROM plasm_trace_sink.trace_heads")
                .fetch_one(&self.pool)
                .await?;
        Ok(v)
    }

    pub async fn bulk_upsert_trace_heads(&self, rows: &[TraceHeadRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        for chunk in rows.chunks(256) {
            self.upsert_trace_heads(chunk).await?;
        }
        Ok(())
    }

    pub async fn insert_ingested_events(
        &self,
        events: &[crate::model::AuditEvent],
    ) -> anyhow::Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for ev in events {
            sqlx::query(
                r#"INSERT INTO plasm_trace_sink.ingested_events (event_id, tenant_partition, ingested_at)
                   VALUES ($1, $2, $3)
                   ON CONFLICT (event_id) DO NOTHING"#,
            )
            .bind(ev.event_id)
            .bind(ev.tenant_partition())
            .bind(ev.ingested_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn existing_event_ids(
        &self,
        ids: &[Uuid],
        tenant_partitions: Option<&[String]>,
    ) -> anyhow::Result<HashSet<Uuid>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let mut out = HashSet::new();
        let p = &self.pool;
        for chunk in ids.chunks(512) {
            let ids_vec = chunk.to_vec();
            let rows: Vec<Uuid> = match tenant_partitions {
                Some(parts) if !parts.is_empty() => {
                    let parts_vec: Vec<String> = parts.to_vec();
                    sqlx::query_scalar(
                        r#"SELECT event_id FROM plasm_trace_sink.ingested_events
                           WHERE event_id = ANY($1) AND tenant_partition = ANY($2)"#,
                    )
                    .bind(ids_vec)
                    .bind(parts_vec)
                    .fetch_all(p)
                    .await?
                }
                _ => {
                    sqlx::query_scalar(
                        r#"SELECT event_id FROM plasm_trace_sink.ingested_events
                           WHERE event_id = ANY($1)"#,
                    )
                    .bind(ids_vec)
                    .fetch_all(p)
                    .await?
                }
            };
            out.extend(rows);
        }
        Ok(out)
    }

    pub async fn upsert_trace_heads(&self, rows: &[TraceHeadRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for h in rows {
            sqlx::query(
                r#"INSERT INTO plasm_trace_sink.trace_heads (
                    trace_id, tenant_partition, tenant_id, project_slug, mcp_session_id,
                    status, started_at_ms, ended_at_ms, updated_at_ms, expression_lines,
                    max_call_index, totals_json, workspace_slug
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                ON CONFLICT (trace_id) DO UPDATE SET
                    tenant_partition = EXCLUDED.tenant_partition,
                    tenant_id = EXCLUDED.tenant_id,
                    project_slug = EXCLUDED.project_slug,
                    mcp_session_id = EXCLUDED.mcp_session_id,
                    status = EXCLUDED.status,
                    started_at_ms = EXCLUDED.started_at_ms,
                    ended_at_ms = EXCLUDED.ended_at_ms,
                    updated_at_ms = EXCLUDED.updated_at_ms,
                    expression_lines = EXCLUDED.expression_lines,
                    max_call_index = EXCLUDED.max_call_index,
                    totals_json = EXCLUDED.totals_json,
                    workspace_slug = EXCLUDED.workspace_slug"#,
            )
            .bind(h.trace_id)
            .bind(&h.tenant_partition)
            .bind(&h.tenant_id)
            .bind(&h.project_slug)
            .bind(&h.mcp_session_id)
            .bind(&h.status)
            .bind(h.started_at_ms)
            .bind(h.ended_at_ms)
            .bind(h.updated_at_ms)
            .bind(h.expression_lines)
            .bind(h.max_call_index)
            .bind(&h.totals_json)
            .bind(&h.workspace_slug)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_latest_trace_heads(
        &self,
        trace_ids: &[Uuid],
    ) -> anyhow::Result<Vec<TraceHeadRow>> {
        if trace_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for chunk in trace_ids.chunks(256) {
            let q = "SELECT trace_id, tenant_partition, tenant_id, project_slug, mcp_session_id, \
                     status, started_at_ms, ended_at_ms, updated_at_ms, expression_lines, \
                     max_call_index, totals_json, workspace_slug \
                     FROM plasm_trace_sink.trace_heads WHERE trace_id = ANY($1)";
            let rows = sqlx::query(q)
                .bind(chunk.to_vec())
                .fetch_all(&self.pool)
                .await?;
            for r in rows {
                out.push(row_to_head_pg(&r)?);
            }
        }
        Ok(out)
    }

    pub async fn list_trace_summaries(
        &self,
        filter: TraceListFilter<'_>,
    ) -> anyhow::Result<Vec<TraceSummary>> {
        let tenant = filter.tenant.as_str();
        let limit = filter.limit.clamp(1, 500) as i64;
        let offset = filter.offset as i64;
        let project_filter = filter.project_slug.filter(|s| !s.is_empty());
        let status_filter: Option<&str> = match filter.status {
            TraceListStatusFilter::All => None,
            TraceListStatusFilter::Live => Some("live"),
            TraceListStatusFilter::Completed => Some("completed"),
        };

        let sql = r#"SELECT trace_id, tenant_partition, tenant_id, project_slug, mcp_session_id, status,
             started_at_ms, ended_at_ms, updated_at_ms, expression_lines, max_call_index, totals_json,
             workspace_slug
             FROM (
               SELECT trace_id, tenant_partition, tenant_id, project_slug, mcp_session_id, status,
                      started_at_ms, ended_at_ms, updated_at_ms, expression_lines, max_call_index, totals_json,
                      workspace_slug,
                      ROW_NUMBER() OVER (PARTITION BY trace_id ORDER BY updated_at_ms DESC) AS rn
               FROM plasm_trace_sink.trace_heads
               WHERE tenant_partition = $1
                 AND ($2::text IS NULL OR project_slug = $2)
                 AND ($3::text IS NULL OR status = $3)
             ) latest
             WHERE rn = 1
             ORDER BY started_at_ms DESC
             OFFSET $4 LIMIT $5"#;
        let rows = sqlx::query(sql)
            .bind(tenant)
            .bind(project_filter)
            .bind(status_filter)
            .bind(offset)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        let mut summaries = Vec::with_capacity(rows.len());
        for r in rows {
            let h = row_to_head_pg(&r)?;
            let totals: TraceTotals = trace_totals_from_head_row(&h);
            summaries.push(TraceSummary {
                trace_id: h.trace_id,
                mcp_session_id: h.mcp_session_id.unwrap_or_default(),
                status: h.status,
                started_at_ms: h.started_at_ms.max(0) as u64,
                ended_at_ms: h.ended_at_ms.map(|v| v.max(0) as u64),
                project_slug: h.project_slug,
                tenant_id: h.tenant_id,
                totals,
            });
        }
        Ok(summaries)
    }
}

fn row_to_head_pg(r: &sqlx::postgres::PgRow) -> anyhow::Result<TraceHeadRow> {
    Ok(TraceHeadRow {
        trace_id: r.try_get::<Uuid, _>("trace_id")?,
        tenant_partition: r.try_get::<String, _>("tenant_partition")?,
        tenant_id: r.try_get::<String, _>("tenant_id")?,
        project_slug: r.try_get::<String, _>("project_slug")?,
        mcp_session_id: r.try_get::<Option<String>, _>("mcp_session_id")?,
        status: r.try_get::<String, _>("status")?,
        started_at_ms: r.try_get::<i64, _>("started_at_ms")?,
        ended_at_ms: r.try_get::<Option<i64>, _>("ended_at_ms")?,
        updated_at_ms: r.try_get::<i64, _>("updated_at_ms")?,
        expression_lines: r.try_get::<i64, _>("expression_lines")?,
        max_call_index: r.try_get::<Option<i64>, _>("max_call_index")?,
        totals_json: r
            .try_get::<Option<String>, _>("totals_json")?
            .unwrap_or_default(),
        workspace_slug: r
            .try_get::<Option<String>, _>("workspace_slug")?
            .unwrap_or_default(),
    })
}

async fn migrate_postgres(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS plasm_trace_sink")
        .execute(pool)
        .await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS plasm_trace_sink.ingested_events (
            event_id UUID PRIMARY KEY,
            tenant_partition TEXT NOT NULL,
            ingested_at TIMESTAMPTZ NOT NULL
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS ix_plasm_tsink_ev_tenant
           ON plasm_trace_sink.ingested_events (tenant_partition)"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS plasm_trace_sink.trace_heads (
            trace_id UUID PRIMARY KEY,
            tenant_partition TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            project_slug TEXT NOT NULL,
            mcp_session_id TEXT,
            status TEXT NOT NULL,
            started_at_ms BIGINT NOT NULL,
            ended_at_ms BIGINT,
            updated_at_ms BIGINT NOT NULL,
            expression_lines BIGINT NOT NULL,
            max_call_index BIGINT,
            totals_json TEXT NOT NULL DEFAULT '',
            workspace_slug TEXT NOT NULL DEFAULT ''
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"ALTER TABLE plasm_trace_sink.trace_heads
           ADD COLUMN IF NOT EXISTS workspace_slug TEXT NOT NULL DEFAULT ''"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS ix_plasm_tsink_heads_tenant_proj
           ON plasm_trace_sink.trace_heads (tenant_partition, project_slug)"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}
