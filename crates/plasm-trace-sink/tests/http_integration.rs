//! HTTP integration tests (Iceberg-backed; Postgres SqlCatalog via testcontainers or env).

mod common;

use std::sync::Arc;

use axum::body::{to_bytes, Body};
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
async fn v1_health_returns_ok() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("oneshot");

    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"ok");
}

#[tokio::test]
async fn post_empty_events_accepted_zero() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);

    let body = json!({ "events": [] });
    let res = app
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
    let v = read_json_body(res.into_body()).await;
    assert_eq!(v["accepted"], 0);
    assert_eq!(v["duplicate_skipped"], 0);
}

#[tokio::test]
async fn ingest_then_get_trace_sorted_by_line_index() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state.clone());
    let trace_id = Uuid::new_v4();
    let e1 = Uuid::new_v4();
    let e2 = Uuid::new_v4();
    let t0 = "2026-04-07T10:00:00Z";
    let t1 = "2026-04-07T10:00:01Z";

    let batch = json!({
        "events": [
            sample_mcp_trace_line_event(e2, trace_id, t1, None, 1),
            sample_mcp_trace_line_event(e1, trace_id, t0, None, 0),
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
    let pr = read_json_body(post.into_body()).await;
    assert_eq!(pr["accepted"], 2);
    assert_eq!(pr["duplicate_skipped"], 0);

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/traces/{trace_id}?tenant_id=__none__"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let doc = read_json_body(get.into_body()).await;
    let rows = doc["detail"]["records"]
        .as_array()
        .expect("detail.records array");
    assert_eq!(rows.len(), 2);
    let r0 = &rows[0]["record"];
    let r1 = &rows[1]["record"];
    assert_eq!(r0["line_index"], 0);
    assert_eq!(r1["line_index"], 1);
    assert_eq!(r0["event_id"], e1.to_string());
}

#[tokio::test]
async fn get_trace_unknown_returns_404() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);

    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/traces/{}?tenant_id=__none__", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn duplicate_event_id_skipped_on_second_ingest() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);
    let event_id = Uuid::new_v4();
    let trace_id = Uuid::new_v4();
    let ev = sample_mcp_trace_line_event(event_id, trace_id, "2026-04-07T12:00:00Z", None, 0);

    for expect_dup in [0usize, 1usize] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({ "events": [ev.clone()] })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = read_json_body(res.into_body()).await;
        if expect_dup == 0 {
            assert_eq!(v["accepted"], 1);
            assert_eq!(v["duplicate_skipped"], 0);
        } else {
            assert_eq!(v["accepted"], 0);
            assert_eq!(v["duplicate_skipped"], 1);
        }
    }
}

#[tokio::test]
async fn billing_usage_filters_tenant_and_window() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);
    let trace_a = Uuid::new_v4();
    let trace_b = Uuid::new_v4();
    let inside = "2026-04-07T15:00:00Z";
    let outside = "2026-04-01T15:00:00Z";

    let batch = json!({
        "events": [
            sample_mcp_trace_line_event(Uuid::new_v4(), trace_a, inside, Some("tenant-a"), 0),
            sample_mcp_trace_line_event(Uuid::new_v4(), trace_b, inside, Some("tenant-b"), 0),
            sample_mcp_trace_line_event(Uuid::new_v4(), trace_a, outside, Some("tenant-a"), 0),
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

    let uri =
        "/v1/billing/usage?tenant_id=tenant-a&from=2026-04-07T00:00:00Z&to=2026-04-07T23:59:59Z";

    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let doc = read_json_body(res.into_body()).await;
    let usage = doc["usage"].as_array().expect("usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0]["tenant_partition"], "tenant-a");
    assert_eq!(usage[0]["is_billing_event"], true);
}

#[tokio::test]
async fn non_billing_event_kind_produces_no_billing_row() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);
    let trace_id = Uuid::new_v4();
    let batch = json!({
        "events": [sample_misc_audit_event(
            Uuid::new_v4(),
            trace_id,
            "2026-04-07T16:00:00Z",
            Some("t1"),
            "other_kind",
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

    let uri = "/v1/billing/usage?from=2026-04-07T00:00:00Z&to=2026-04-07T23:59:59Z";
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let doc = read_json_body(res.into_body()).await;
    assert!(doc["usage"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn billing_usage_invalid_from_is_400() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let app = router(ctx.state);

    let uri = "/v1/billing/usage?from=not-a-date&to=2026-04-07T23:59:59Z";
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

/// New `AppState` + `IcebergSink` against the same catalog/warehouse can read prior writes.
#[tokio::test]
async fn trace_survives_new_state_same_iceberg_files() {
    let Some(ctx) = iceberg_test_state().await else {
        return;
    };
    let connect: IcebergConnectParams = ctx.connect.clone();

    let trace_id = Uuid::new_v4();
    let app1 = router(ctx.state);
    let batch = json!({
        "events": [sample_mcp_trace_line_event(
            Uuid::new_v4(),
            trace_id,
            "2026-04-07T18:00:00Z",
            None,
            0,
        )]
    });
    let post = app1
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

    let sink2 = Arc::new(
        IcebergSink::connect(&connect)
            .await
            .expect("reconnect Iceberg"),
    );
    let store2: Arc<dyn AuditSpanStore> = PersistedTraceSink::connect(&connect, sink2)
        .await
        .expect("reconnect projections");
    let state2 = AppState::new(store2);
    let app2 = router(state2);
    let get = app2
        .oneshot(
            Request::builder()
                .uri(format!("/v1/traces/{trace_id}?tenant_id=__none__"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let doc = read_json_body(get.into_body()).await;
    assert_eq!(doc["detail"]["records"].as_array().unwrap().len(), 1);
}
