//! Integration timing for trace list (SQL projection) vs detail (Iceberg scan).
//!
//! Prints timings to stdout; run with:
//!   `cargo test -p plasm-trace-sink --test trace_read_timing -- --nocapture --ignored`
//!
//! Requires Postgres (testcontainers or `PLASM_TEST_POSTGRES_URL`).
//!
//! For multi-month warehouse comparison: ingest traces across UTC month boundaries on a busy
//! tenant, then compare cold `trace_detail` latency before/after head-guided
//! `year_month_bucket` pruning (`year_month_buckets_for_trace_ms` from `trace_heads`).

mod common;

use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{iceberg_test_state, read_json_body, sample_mcp_trace_line_event};
use plasm_trace_sink::append_port::{TenantId, TraceListFilter, TraceListStatusFilter};
use plasm_trace_sink::http::router;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: &str = "tenant-timing";
const LINE_COUNTS: &[usize] = &[50, 200, 500];

#[tokio::test]
#[ignore = "manual perf probe; run with --ignored --nocapture"]
async fn trace_read_list_vs_detail_timing() {
    let Some(ctx) = iceberg_test_state().await else {
        eprintln!("skip: no Postgres for trace sink integration");
        return;
    };
    let store = ctx.state.clone();
    let app = router(ctx.state);

    eprintln!("\n=== trace read timing (Iceberg + SQL projection) ===\n");
    eprintln!(
        "{:>6} | {:>10} | {:>10} | {:>10} | {:>10} | {:>12}",
        "lines", "ingest_ms", "proj_ms", "list_ms", "detail_ms", "detail_bytes"
    );
    eprintln!("{}", "-".repeat(72));

    for &n in LINE_COUNTS {
        let trace_id = Uuid::new_v4();
        let mut events = Vec::with_capacity(n);
        for i in 0..n {
            events.push(sample_mcp_trace_line_event(
                Uuid::new_v4(),
                trace_id,
                &format!("2026-04-07T10:00:{i:02}Z"),
                Some(TENANT),
                i as i64,
            ));
        }

        let t_ingest = Instant::now();
        let post = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({ "events": events })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(post.status(), StatusCode::OK);
        let ingest_ms = t_ingest.elapsed().as_millis();

        let tenant = TenantId::parse(TENANT).unwrap();

        let t_proj = Instant::now();
        let projected = store
            .trace_detail(&tenant, trace_id)
            .await
            .expect("trace_detail projection");
        let proj_ms = t_proj.elapsed().as_millis();
        assert!(projected.is_some());

        let t_list = Instant::now();
        let summaries = store
            .list_traces(TraceListFilter {
                tenant: &tenant,
                project_slug: None,
                status: TraceListStatusFilter::All,
                offset: 0,
                limit: 80,
            })
            .await
            .expect("list_traces");
        let list_ms = t_list.elapsed().as_millis();
        assert!(summaries.iter().any(|s| s.trace_id == trace_id));

        let t_detail = Instant::now();
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/traces/{trace_id}?tenant_id={TENANT}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get.status(), StatusCode::OK);
        let doc = read_json_body(get.into_body()).await;
        let detail_ms = t_detail.elapsed().as_millis();
        let detail_bytes = serde_json::to_vec(&doc).map(|v| v.len()).unwrap_or(0);

        eprintln!(
            "{n:>6} | {ingest_ms:>10} | {proj_ms:>10} | {list_ms:>10} | {detail_ms:>10} | {detail_bytes:>12}"
        );
    }
}
