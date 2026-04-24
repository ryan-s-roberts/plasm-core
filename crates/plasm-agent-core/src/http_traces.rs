//! `GET /v1/traces` (list), `GET /v1/traces/:trace_id` (detail), `GET /v1/traces/:trace_id/stream` (SSE patches).
//!
//! **Durable reads** (tenant-scoped list/detail) go to [`plasm_trace_sink`] via `PLASM_TRACE_SINK_READ_URL` /
//! `PLASM_TRACE_SINK_URL`. There is **no** fallback to the in-memory [`crate::trace_hub`] when that HTTP
//! call fails — the handler returns **503** (`application/problem+json`) so clients do not misread outages
//! as an empty history.
//!
//! **SSE** (`/stream`) remains hub-backed for real-time patches on active sessions only.

use axum::extract::{Extension, Path, Query};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt;
use http_problem::prelude::{StatusCode as ProblemStatus, Uri};
use http_problem::Problem;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::convert::Infallible;
use uuid::Uuid;

use crate::http_problem_util::{problem_response, problem_types};
use crate::incoming_auth::IncomingPrincipal;
use crate::server_state::PlasmHostState;
use crate::trace_hub::{TraceListStatus, TraceSummaryDto};
use plasm_trace_sink::model::{
    TraceDetailResponse as SinkTraceDetailResponse, TraceListResponse as SinkTraceListResponse,
};
use tracing::Instrument;

#[derive(Debug, Deserialize)]
pub struct TraceListQuery {
    #[serde(default)]
    pub project_slug: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub status: Option<String>,
}

fn default_limit() -> usize {
    50
}

#[derive(Serialize)]
struct TraceListResponse {
    traces: Vec<TraceSummaryDto>,
}

fn viewer_tenant(principal: &IncomingPrincipal) -> Option<&str> {
    principal.0.as_ref().map(|p| p.tenant_id.as_str())
}

fn problem_trace_sink_not_configured() -> Problem {
    Problem::custom(
        ProblemStatus::SERVICE_UNAVAILABLE,
        Uri::from_static(problem_types::TRACE_SINK_NOT_CONFIGURED),
    )
    .with_title("Trace sink not configured")
    .with_detail(
        "Set PLASM_TRACE_SINK_READ_URL or PLASM_TRACE_SINK_URL for durable trace reads (no in-memory fallback).",
    )
}

fn problem_trace_sink_unavailable(detail: impl Into<String>) -> Problem {
    Problem::custom(
        ProblemStatus::SERVICE_UNAVAILABLE,
        Uri::from_static(problem_types::TRACE_SINK_UNAVAILABLE),
    )
    .with_title("Trace sink unavailable")
    .with_detail(detail.into())
}

async fn list_traces(
    Extension(st): Extension<PlasmHostState>,
    Extension(principal): Extension<IncomingPrincipal>,
    Query(q): Query<TraceListQuery>,
) -> Result<Json<TraceListResponse>, Response> {
    if let Some(tenant) = viewer_tenant(&principal) {
        let Some(base) = st.trace_sink_read_base_url.as_deref() else {
            tracing::warn!(
                target: "plasm_agent::http_traces",
                tenant,
                "GET /v1/traces: trace sink read URL not configured",
            );
            return Err(problem_response(problem_trace_sink_not_configured()));
        };
        let merged_limit = q.offset.saturating_add(q.limit);
        let list_span = crate::spans::billing_trace_list(tenant, q.limit, q.offset);
        match list_traces_from_sink(
            base,
            tenant,
            q.project_slug.as_deref(),
            q.status.as_deref(),
            0,
            merged_limit,
        )
        .instrument(list_span)
        .await
        {
            Ok(traces) => return Ok(Json(TraceListResponse { traces })),
            Err(detail) => {
                tracing::warn!(
                    target: "plasm_agent::http_traces",
                    tenant,
                    error = %detail,
                    "GET /v1/traces: trace sink read failed",
                );
                return Err(problem_response(problem_trace_sink_unavailable(detail)));
            }
        }
    }

    let traces = st
        .trace_hub
        .list_for_tenant(
            viewer_tenant(&principal),
            q.project_slug.as_deref(),
            q.offset,
            q.limit,
            TraceListStatus::parse(q.status.as_deref()),
        )
        .await;
    Ok(Json(TraceListResponse { traces }))
}

async fn get_trace_detail(
    Extension(st): Extension<PlasmHostState>,
    Extension(principal): Extension<IncomingPrincipal>,
    Path(trace_id): Path<Uuid>,
) -> Result<Json<crate::trace_hub::TraceDetailDto>, Response> {
    let viewer = viewer_tenant(&principal);
    if let Some(live) = st.trace_hub.get_detail(trace_id, viewer).await {
        if live.summary.status == "live" {
            return Ok(Json(live));
        }
    }

    let Some(tenant) = viewer else {
        return match st.trace_hub.get_detail(trace_id, None).await {
            Some(d) => Ok(Json(d)),
            None => Err(StatusCode::NOT_FOUND.into_response()),
        };
    };

    let Some(base) = st.trace_sink_read_base_url.as_deref() else {
        tracing::warn!(
            target: "plasm_agent::http_traces",
            tenant,
            %trace_id,
            "GET /v1/traces/:id: trace sink read URL not configured",
        );
        return Err(problem_response(problem_trace_sink_not_configured()));
    };

    let detail_span = crate::spans::billing_trace_detail(tenant, &trace_id);
    match fetch_trace_detail_from_sink(base, tenant, trace_id)
        .instrument(detail_span)
        .await
    {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(detail) => {
            tracing::warn!(
                target: "plasm_agent::http_traces",
                tenant,
                %trace_id,
                error = %detail,
                "GET /v1/traces/:id: trace sink read failed",
            );
            Err(problem_response(problem_trace_sink_unavailable(detail)))
        }
    }
}

fn sse_event_name_for_trace_payload(json: &str) -> &'static str {
    let Ok(v) = serde_json::from_str::<JsonValue>(json) else {
        return "patch";
    };
    match v.get("kind").and_then(|k| k.as_str()) {
        Some("terminal") => "terminal",
        Some("snapshot") => "snapshot",
        Some("durable_ingest") => "durable_ingest",
        _ => "patch",
    }
}

/// Hub-backed SSE for live session patches (not a substitute for durable `GET` detail).
async fn stream_trace_events(
    Extension(st): Extension<PlasmHostState>,
    Extension(principal): Extension<IncomingPrincipal>,
    Path(trace_id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, StatusCode> {
    let viewer = viewer_tenant(&principal);
    let snap = st
        .trace_hub
        .sse_snapshot_payload(trace_id, viewer)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let hub = st.trace_hub.clone();
    let first = stream::once(async move {
        Ok::<Event, Infallible>(Event::default().event("snapshot").data(snap))
    });

    let rx = hub.subscribe_trace_async(trace_id).await;
    let body: std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        if let Some(rx) = rx {
            let rest = stream::unfold(rx, |mut rx| async move {
                match rx.recv().await {
                    Ok(payload) => {
                        let name = sse_event_name_for_trace_payload(&payload);
                        Some((Ok(Event::default().event(name).data(payload)), rx))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        Some((Ok(Event::default().comment("lagged")), rx))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                }
            });
            Box::pin(first.chain(rest))
        } else {
            Box::pin(first)
        };

    Ok(Sse::new(body).keep_alive(KeepAlive::default()))
}

pub fn trace_routes() -> Router {
    Router::new()
        .route("/v1/traces", get(list_traces))
        .route("/v1/traces/{trace_id}", get(get_trace_detail))
        .route("/v1/traces/{trace_id}/stream", get(stream_trace_events))
}

async fn list_traces_from_sink(
    base: &str,
    tenant_id: &str,
    project_slug: Option<&str>,
    status: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Vec<TraceSummaryDto>, String> {
    let mut qp = vec![
        ("tenant_id", tenant_id.to_string()),
        ("offset", offset.to_string()),
        ("limit", limit.to_string()),
    ];
    if let Some(ps) = project_slug.filter(|s| !s.is_empty()) {
        qp.push(("project_slug", ps.to_string()));
    }
    if let Some(status) = status.filter(|s| !s.is_empty()) {
        qp.push(("status", status.to_string()));
    }
    let qs = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(qp.iter().map(|(k, v)| (*k, v.as_str())))
        .finish();
    let url = format!("{}/v1/traces?{qs}", base.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP transport to trace sink: {e}"))?;
    let status_code = resp.status();
    if !status_code.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(512)
            .collect::<String>();
        return Err(format!(
            "trace sink GET /v1/traces returned {status_code}: {body}"
        ));
    }
    let body: SinkTraceListResponse = resp
        .json()
        .await
        .map_err(|e| format!("trace sink list response JSON: {e}"))?;
    let traces = body
        .traces
        .into_iter()
        .map(|t| TraceSummaryDto {
            trace_id: t.trace_id.to_string(),
            mcp_session_id: t.mcp_session_id,
            logical_session_id: None,
            status: hub_status_from_sink(&t.status),
            started_at_ms: t.started_at_ms,
            ended_at_ms: t.ended_at_ms,
            project_slug: t.project_slug,
            tenant_id: t.tenant_id,
            mcp_config: None,
            totals: hub_totals_from_sink(&t.totals),
        })
        .collect::<Vec<_>>();
    Ok(traces)
}

async fn fetch_trace_detail_from_sink(
    base: &str,
    tenant_id: &str,
    trace_id: Uuid,
) -> Result<Option<crate::trace_hub::TraceDetailDto>, String> {
    let url = format!(
        "{}/v1/traces/{}?tenant_id={}",
        base.trim_end_matches('/'),
        trace_id,
        url::form_urlencoded::byte_serialize(tenant_id.as_bytes()).collect::<String>()
    );
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP transport to trace sink: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(512)
            .collect::<String>();
        return Err(format!(
            "trace sink GET /v1/traces/{{id}} returned {status}: {body}"
        ));
    }
    let body: SinkTraceDetailResponse = resp
        .json()
        .await
        .map_err(|e| format!("trace sink detail response JSON: {e}"))?;
    let summary = body.detail.summary;
    let records = body
        .detail
        .records
        .into_iter()
        .map(|r| r.record)
        .collect::<Vec<_>>();
    let detail = crate::trace_hub::TraceDetailDto {
        summary: TraceSummaryDto {
            trace_id: summary.trace_id.to_string(),
            mcp_session_id: summary.mcp_session_id,
            logical_session_id: None,
            status: hub_status_from_sink(&summary.status),
            started_at_ms: summary.started_at_ms,
            ended_at_ms: summary.ended_at_ms,
            project_slug: summary.project_slug,
            tenant_id: summary.tenant_id,
            mcp_config: None,
            totals: hub_totals_from_sink(&summary.totals),
        },
        records,
    };
    Ok(Some(detail))
}

fn hub_status_from_sink(status: &str) -> &'static str {
    if status == "live" {
        "live"
    } else {
        "completed"
    }
}

fn hub_totals_from_sink(t: &plasm_trace_sink::model::TraceTotals) -> crate::trace_hub::TraceTotals {
    crate::trace_hub::TraceTotals {
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
