//! Shared helpers for HTTP integration tests.
//!
//! SqlCatalog metadata uses **Postgres** only. Tests use [`PLASM_TEST_POSTGRES_URL`](../../plasm-agent-core/tests/support/postgres.rs)
//! when set and reachable, else **testcontainers** when Docker is available. If neither works, tests **return early** (skip).

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use plasm_trace_sink::append_port::AuditSpanStore;
use plasm_trace_sink::config::{CatalogConnectionString, IcebergConnectParams, WarehouseLocation};
use plasm_trace_sink::iceberg_writer::IcebergSink;
use plasm_trace_sink::model::AUDIT_EVENT_KIND_MCP_TRACE_SEGMENT;
use plasm_trace_sink::persisted::PersistedTraceSink;
use plasm_trace_sink::state::AppState;
use serde_json::{json, Value};
use uuid::Uuid;

#[path = "../../../plasm-agent-core/tests/support/postgres.rs"]
mod integration_postgres;

use integration_postgres::{integration_postgres_url, PostgresKeepAlive};

/// Keeps the Postgres container alive until the end of the test.
#[allow(dead_code)]
pub struct ContainerDrop(pub PostgresKeepAlive);

/// Postgres JDBC/catalog URL for SqlCatalog: shared integration helper.
pub async fn trace_sink_test_catalog_url() -> Option<(Option<ContainerDrop>, String)> {
    const START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
    let (keep, url) = integration_postgres_url(START_TIMEOUT).await?;
    Some((Some(ContainerDrop(keep)), url))
}

/// Temp warehouse dir + Postgres catalog + [`PersistedTraceSink`] wiring.
pub struct IcebergTestCtx {
    pub _dir: tempfile::TempDir,
    pub _pg: Option<ContainerDrop>,
    pub connect: IcebergConnectParams,
    pub state: Arc<AppState>,
    /// Exposed for `http_iceberg_integration` row scans; other suites only use `state`.
    #[allow(dead_code)]
    pub sink: Arc<IcebergSink>,
}

/// SqlCatalog (Postgres) + local warehouse under a temp dir; shared by HTTP integration tests.
pub async fn iceberg_test_state() -> Option<IcebergTestCtx> {
    let (keep, jdbc) = trace_sink_test_catalog_url().await?;
    let dir = tempfile::tempdir().expect("tempdir");
    let warehouse = dir.path().join("iceberg_warehouse");
    std::fs::create_dir_all(&warehouse).expect("warehouse dir");
    let catalog =
        CatalogConnectionString::resolve(dir.path(), Some(jdbc.as_str())).expect("catalog");
    let connect = IcebergConnectParams {
        catalog,
        warehouse: WarehouseLocation::Filesystem(warehouse),
    };
    let sink = Arc::new(
        IcebergSink::connect(&connect)
            .await
            .expect("IcebergSink::connect"),
    );
    let store: Arc<dyn AuditSpanStore> = PersistedTraceSink::connect(
        &connect,
        sink.clone(),
        0,
        300,
    )
    .await
    .expect("PersistedTraceSink::connect");
    let state = AppState::new(store);
    Some(IcebergTestCtx {
        _dir: dir,
        _pg: keep,
        connect,
        state,
        sink,
    })
}

/// Canonical `mcp_trace_segment` audit row with a `plasm_line` trace-event payload.
pub fn sample_mcp_trace_line_event(
    event_id: Uuid,
    trace_id: Uuid,
    emitted_at: &str,
    tenant_id: Option<&str>,
    line_index: i64,
) -> Value {
    let li = line_index.max(0);
    let payload = json!({
        "emitted_at_ms": li as u64,
        "kind": "plasm_line",
        "call_index": 0,
        "line_index": li,
        "source_expression": "e1()",
        "repl_pre": "",
        "repl_post": "",
        "capability": null,
        "operation": "query",
        "api_entry_id": null,
        "duration_ms": 10,
        "stats": {
            "duration_ms": 10,
            "network_requests": 1,
            "cache_hits": 0,
            "cache_misses": 0
        },
        "source": "live",
        "request_fingerprints": [],
        "http_calls": []
    });

    let mut ev = json!({
        "event_id": event_id,
        "schema_version": 1,
        "emitted_at": emitted_at,
        "trace_id": trace_id,
        "event_kind": AUDIT_EVENT_KIND_MCP_TRACE_SEGMENT,
        "request_units": 1_i64,
        "call_index": 0,
        "line_index": line_index,
        "payload": payload,
    });
    if let Some(t) = tenant_id {
        ev["tenant_id"] = json!(t);
    }
    ev
}

/// Minimal audit row for tests that assert the projector skips unknown `event_kind`s.
pub fn sample_misc_audit_event(
    event_id: Uuid,
    trace_id: Uuid,
    emitted_at: &str,
    tenant_id: Option<&str>,
    event_kind: &str,
) -> Value {
    let mut ev = json!({
        "event_id": event_id,
        "schema_version": 1,
        "emitted_at": emitted_at,
        "trace_id": trace_id,
        "event_kind": event_kind,
        "request_units": 0_i64,
        "payload": { "note": "non-billing fixture" }
    });
    if let Some(t) = tenant_id {
        ev["tenant_id"] = json!(t);
    }
    ev
}

pub async fn read_json_body(body: Body) -> Value {
    let bytes = to_bytes(body, usize::MAX).await.expect("body");
    serde_json::from_slice(&bytes).expect("json")
}
