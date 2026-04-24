//! Shared helpers for HTTP integration tests.
//!
//! SqlCatalog metadata uses **Postgres** only. Tests use `PLASM_TRACE_SINK_TEST_CATALOG_URL` if set,
//! else start **Postgres** via [testcontainers](https://crates.io/crates/testcontainers-modules)
//! when Docker is available. If neither works, individual tests **return early** (skip).

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use plasm_trace_sink::append_port::AuditSpanStore;
use plasm_trace_sink::config::{CatalogConnectionString, IcebergConnectParams, WarehouseLocation};
use plasm_trace_sink::iceberg_writer::IcebergSink;
use plasm_trace_sink::model::AUDIT_EVENT_KIND_MCP_TRACE_SEGMENT;
use plasm_trace_sink::persisted::PersistedTraceSink;
use plasm_trace_sink::state::AppState;
use serde_json::{json, Value};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{runners::AsyncRunner, ContainerAsync},
};
use uuid::Uuid;

pub const TRACE_SINK_TEST_CATALOG_URL_ENV: &str = "PLASM_TRACE_SINK_TEST_CATALOG_URL";

/// Keeps the Postgres container alive until the end of the test.
#[allow(dead_code)]
pub struct ContainerDrop(pub ContainerAsync<Postgres>);

/// Postgres JDBC URL for SqlCatalog: env override, else a throwaway Docker container.
/// Returns [`None`] when testcontainers cannot start Postgres (no Docker, timeout, etc.).
pub async fn trace_sink_test_catalog_url() -> Option<(Option<ContainerDrop>, String)> {
    if let Ok(url) = std::env::var(TRACE_SINK_TEST_CATALOG_URL_ENV) {
        let url = url.trim().to_string();
        if !url.is_empty() {
            return Some((None, url));
        }
    }

    const START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
    let node = match tokio::time::timeout(START_TIMEOUT, Postgres::default().start()).await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            eprintln!(
                "skip plasm-trace-sink integration tests: Postgres testcontainer failed ({e}). \
                 Set {TRACE_SINK_TEST_CATALOG_URL_ENV} or ensure Docker is running."
            );
            return None;
        }
        Err(_) => {
            eprintln!(
                "skip plasm-trace-sink integration tests: Postgres testcontainer start timed out after {START_TIMEOUT:?}. \
                 Set {TRACE_SINK_TEST_CATALOG_URL_ENV} or fix Docker."
            );
            return None;
        }
    };
    let port = match node.get_host_port_ipv4(5432).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip plasm-trace-sink integration tests: postgres port mapping failed: {e}");
            return None;
        }
    };
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    Some((Some(ContainerDrop(node)), url))
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
    let store: Arc<dyn AuditSpanStore> = PersistedTraceSink::connect(&connect, sink.clone())
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
