//! HTTP + Iceberg integration: ingest persists to `audit_events` and `trace_spans` (scan + read verification).

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{
    iceberg_test_state, read_json_body, sample_mcp_trace_line_event, sample_misc_audit_event,
};
use plasm_trace_sink::append_port::AuditSpanStore;
use plasm_trace_sink::config::IcebergConnectParams;
use plasm_trace_sink::http::router;
use plasm_trace_sink::iceberg_writer::IcebergSink;
use plasm_trace_sink::persisted::PersistedTraceSink;
use plasm_trace_sink::state::AppState;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn ingest_mcp_trace_segment_writes_audit_and_trace_iceberg_rows() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state.clone());

    let trace_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let batch = json!({
        "events": [sample_mcp_trace_line_event(
            event_id,
            trace_id,
            "2026-04-07T18:00:00Z",
            Some("tenant-ice"),
            0,
        )]
    });

    let post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::OK);
    let pr = read_json_body(post.into_body()).await;
    assert_eq!(pr["accepted"], 1);
    assert_eq!(pr["duplicate_skipped"], 0);

    let audit_rows = ctx
        .sink
        .scan_audit_row_count()
        .await
        .expect("scan audit_events");
    let trace_rows = ctx
        .sink
        .scan_trace_row_count()
        .await
        .expect("scan trace_spans");
    assert_eq!(audit_rows, 1, "expected one audit row in Iceberg");
    assert_eq!(trace_rows, 1, "expected one billing trace row in Iceberg");

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/traces/{trace_id}?tenant_id=tenant-ice"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
}

#[tokio::test]
async fn duplicate_event_id_does_not_double_iceberg_rows() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state.clone());
    let ev = sample_mcp_trace_line_event(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "2026-04-07T18:30:00Z",
        None,
        0,
    );
    let body = json!({ "events": [ev.clone()] });

    for _ in 0..2 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    assert_eq!(ctx.sink.scan_audit_row_count().await.unwrap(), 1);
    assert_eq!(ctx.sink.scan_trace_row_count().await.unwrap(), 1);
}

#[tokio::test]
async fn non_billing_event_writes_audit_only_to_iceberg() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state.clone());
    let batch = json!({
        "events": [sample_misc_audit_event(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "2026-04-07T19:00:00Z",
            None,
            "telemetry_only",
        )]
    });

    let post = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::OK);

    assert_eq!(ctx.sink.scan_audit_row_count().await.unwrap(), 1);
    assert_eq!(
        ctx.sink.scan_trace_row_count().await.unwrap(),
        0,
        "projector skips non-billing kinds"
    );
}

#[tokio::test]
async fn two_distinct_events_append_two_iceberg_files() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state.clone());
    let trace_id = Uuid::new_v4();
    let batch = json!({
        "events": [
            sample_mcp_trace_line_event(Uuid::new_v4(), trace_id, "2026-04-07T20:00:00Z", None, 0),
            sample_mcp_trace_line_event(Uuid::new_v4(), trace_id, "2026-04-07T20:00:01Z", None, 1),
        ]
    });

    let post = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::OK);

    assert_eq!(ctx.sink.scan_audit_row_count().await.unwrap(), 2);
    assert_eq!(ctx.sink.scan_trace_row_count().await.unwrap(), 2);
}

/// Same event_id after reconnecting Iceberg must still be treated as duplicate (strict idempotency).
#[tokio::test]
async fn duplicate_event_id_skipped_after_reconnect_iceberg() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let connect: IcebergConnectParams = ctx.connect.clone();

    let event_id = Uuid::new_v4();
    let trace_id = Uuid::new_v4();
    let ev = sample_mcp_trace_line_event(event_id, trace_id, "2026-04-07T21:00:00Z", None, 0);
    let body = json!({ "events": [ev.clone()] });

    let app1 = router(ctx.state);
    let post1 = app1
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post1.status(), StatusCode::OK);
    let pr1 = read_json_body(post1.into_body()).await;
    assert_eq!(pr1["accepted"], 1);

    drop(app1);

    let sink2 = Arc::new(IcebergSink::connect(&connect).await.expect("reconnect"));
    let store2: Arc<dyn AuditSpanStore> = PersistedTraceSink::connect(&connect, sink2)
        .await
        .expect("reconnect projections");
    let state2 = AppState::new(store2);
    let app2 = router(state2);
    let post2 = app2
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post2.status(), StatusCode::OK);
    let pr2 = read_json_body(post2.into_body()).await;
    assert_eq!(pr2["accepted"], 0);
    assert_eq!(pr2["duplicate_skipped"], 1);
}

#[tokio::test]
async fn list_traces_applies_offset_and_limit_server_side() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state.clone());
    let tenant = "tenant-page";
    let oldest = Uuid::new_v4();
    let middle = Uuid::new_v4();
    let newest = Uuid::new_v4();

    let batch = json!({
        "events": [
            sample_mcp_trace_line_event(Uuid::new_v4(), oldest, "2026-04-07T20:00:00Z", Some(tenant), 0),
            sample_mcp_trace_line_event(Uuid::new_v4(), middle, "2026-04-07T21:00:00Z", Some(tenant), 0),
            sample_mcp_trace_line_event(Uuid::new_v4(), newest, "2026-04-07T22:00:00Z", Some(tenant), 0),
        ]
    });

    let post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/traces?tenant_id={}&status=all&offset=1&limit=1",
                    tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let doc = read_json_body(list.into_body()).await;
    let traces = doc["traces"].as_array().expect("traces array");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace_id"], middle.to_string());
}
