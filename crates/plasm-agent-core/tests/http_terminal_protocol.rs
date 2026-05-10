//! Integration tests for terminal HTTP extensions (`/v1/terminal/discover`, `/execute/.../context`,
//! plan mode, `/symbols`, `/status`, `/runs`). Docker-free.

use axum::body::Body;
use axum::extract::Extension;
use axum::http::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use axum::http::{Request, StatusCode};
use axum::Router;
use plasm_agent_core::http::{build_plasm_host_state, health_public_routes, PlasmHostBootstrap};
use plasm_agent_core::http_discovery::discovery_routes_protected;
use plasm_agent_core::http_execute::execute_routes;
use plasm_agent_core::incoming_auth::IncomingPrincipal;
use plasm_agent_core::server_state::PlasmHostState;
use plasm_core::discovery::InMemoryCgsRegistry;
use plasm_core::loader::load_schema_dir;
use plasm_runtime::{ExecutionConfig, ExecutionEngine, ExecutionMode};
use std::path::Path;
use std::sync::Arc;
use tower::util::ServiceExt;

fn test_state() -> PlasmHostState {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/schemas/overshow_tools");
    let cgs = Arc::new(load_schema_dir(&dir).expect("overshow_tools"));
    let reg = InMemoryCgsRegistry::from_pairs(vec![(
        "overshow".into(),
        "Overshow".into(),
        vec!["demo".into()],
        cgs.clone(),
    )]);
    let engine = ExecutionEngine::new(ExecutionConfig::default()).expect("engine");
    build_plasm_host_state(PlasmHostBootstrap {
        engine,
        mode: ExecutionMode::Live,
        registry: Arc::new(reg),
        catalog_bootstrap: plasm_agent_core::server_state::CatalogBootstrap::Fixed,
        plugin_manager: None,
        incoming_auth: None,
        run_artifacts: Arc::new(plasm_agent_core::run_artifacts::RunArtifactStore::memory()),
        session_graph_persistence: None,
        oss_local_filesystem_defaults: false,
    })
}

fn test_app(st: PlasmHostState) -> Router<()> {
    Router::new()
        .merge(health_public_routes())
        .merge(
            Router::new()
                .merge(discovery_routes_protected())
                .merge(execute_routes())
                .layer(axum::middleware::from_fn(
                    plasm_agent_core::incoming_auth::incoming_auth_http_middleware,
                )),
        )
        .layer(Extension(st))
        .layer(Extension(IncomingPrincipal(None)))
}

#[tokio::test]
async fn terminal_discover_and_execute_extensions() {
    let app = test_app(test_state());

    let disc = Request::builder()
        .method("POST")
        .uri("/v1/terminal/discover")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "intent": "profile query",
                "limit": 4,
                "allowed_entry_ids": [],
                "enable_embeddings": false,
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(disc).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        v.get("typed").is_some(),
        "expected typed discovery decision"
    );

    let create = Request::builder()
        .method("POST")
        .uri("/execute")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "entry_id": "overshow", "entities": ["Profile"] }).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(create).await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let loc = res.headers().get(LOCATION).unwrap().to_str().unwrap();
    let created: serde_json::Value = {
        let get = Request::builder()
            .method("GET")
            .uri(loc)
            .header(ACCEPT, "application/json")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(get).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let b = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&b).unwrap()
    };
    let ph = created["prompt_hash"].as_str().unwrap();
    let sid = created["session"].as_str().unwrap();

    let sym = Request::builder()
        .method("GET")
        .uri(format!("/execute/{ph}/{sid}/symbols"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(sym).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let stat = Request::builder()
        .method("GET")
        .uri(format!("/execute/{ph}/{sid}/status"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(stat).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let stbody: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stbody["alive"], true);

    let runs_uri = format!("/execute/{ph}/{sid}/runs");
    let runs = Request::builder()
        .method("GET")
        .uri(&runs_uri)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(runs).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let plan_uri = format!("/execute/{ph}/{sid}?mode=plan");
    let plan = Request::builder()
        .method("POST")
        .uri(&plan_uri)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("Profile{}"))
        .unwrap();
    let res = app.clone().oneshot(plan).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let plan_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(plan_body["plan"], true);

    let ctx_uri = format!("/execute/{ph}/{sid}/context");
    let ctx = Request::builder()
        .method("POST")
        .uri(&ctx_uri)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "seeds": [{ "api": "overshow", "entity": "RecordedContent" }]
            })
            .to_string(),
        ))
        .unwrap();
    let res = app.oneshot(ctx).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "context expand should succeed for second entity in same catalog"
    );
}
