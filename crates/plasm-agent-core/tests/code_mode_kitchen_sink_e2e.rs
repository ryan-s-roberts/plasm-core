//! End-to-end **code mode** Plan: **TypeScript** → Oxc → **QuickJS Plan API** → strict Rust
//! Plan DAG → dry-run and simple execution against a tiny in-process HTTP backend.
//!
//! ```text
//! cargo test -p plasm-agent-core --features code_mode --test code_mode_kitchen_sink_e2e
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use axum::{routing::get, Json, Router};
use indexmap::IndexMap;
use plasm_agent_core::code_mode::CodeModeSandbox;
use plasm_agent_core::execute_path_ids::{ExecuteSessionId, PromptHashHex};
use plasm_agent_core::execute_session::{ExecuteSession, SessionReuseKey};
use plasm_agent_core::http::{self, PlasmHostBootstrap};
use plasm_agent_core::mcp_plasm_code::{evaluate_code_mode_plan_dry, run_code_mode_plan};
use plasm_agent_core::server_state::CatalogBootstrap;
use plasm_core::discovery::InMemoryCgsRegistry;
use plasm_core::load_schema;
use plasm_core::CgsContext;
use plasm_core::DomainExposureSession;
use plasm_facade_gen::{
    build_code_facade, quickjs_runtime_from_facade_delta, FacadeDeltaV1, FacadeGenRequest,
    QualifiedEntitySurface,
};
use plasm_runtime::{ExecutionConfig, ExecutionEngine, ExecutionMode};
use serde_json::json;

fn test_execute_session() -> ExecuteSession {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_dir = root.join("tests/fixtures/execute_tiny");
    let cgs = Arc::new(load_schema(&schema_dir).expect("load execute_tiny"));
    let mut ctxs = IndexMap::new();
    ctxs.insert(
        "acme".into(),
        Arc::new(CgsContext::entry("acme", cgs.clone())),
    );
    let exp = DomainExposureSession::new(cgs.as_ref(), "acme", &["Product"]);
    ExecuteSession::new(
        "fixture-prompt".into(),
        "fixture prompt".into(),
        cgs.clone(),
        ctxs,
        "acme".into(),
        String::new(),
        String::new(),
        None,
        vec!["Product".into()],
        Some(exp),
        None,
        None,
        cgs.catalog_cgs_hash_hex(),
    )
}

fn eval_fixture_plan(es: &ExecuteSession, fixture: &str) -> serde_json::Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ts_path = root
        .join("tests/fixtures/code_mode_kitchen_sink")
        .join(fixture);
    let ts = std::fs::read_to_string(&ts_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", ts_path.display()));
    let (facade_delta, _) = build_code_facade(
        &FacadeGenRequest {
            new_symbol_space: true,
            seed_pairs: vec![("acme".to_string(), "Product".to_string())],
            already_emitted: Default::default(),
            emit_prelude: true,
        },
        es.domain_exposure.as_ref().expect("exposure"),
        &es.contexts_by_entry,
    );
    let quickjs_runtime = quickjs_runtime_from_facade_delta(&facade_delta);
    CodeModeSandbox::new()
        .expect("QuickJS")
        .eval_typescript_to_json_value(fixture, &ts, Some(&quickjs_runtime))
        .unwrap_or_else(|e| panic!("{fixture} TS -> plan: {e}"))
}

fn eval_fixture_with_runtime(fixture: &str, quickjs_runtime: &str) -> serde_json::Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ts_path = root
        .join("tests/fixtures/code_mode_kitchen_sink")
        .join(fixture);
    let ts = std::fs::read_to_string(&ts_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", ts_path.display()));
    CodeModeSandbox::new()
        .expect("QuickJS")
        .eval_typescript_to_json_value(fixture, &ts, Some(quickjs_runtime))
        .unwrap_or_else(|e| panic!("{fixture} TS -> plan: {e}"))
}

fn eval_fixture_error(es: &ExecuteSession, fixture: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ts_path = root
        .join("tests/fixtures/code_mode_kitchen_sink")
        .join(fixture);
    let ts = std::fs::read_to_string(&ts_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", ts_path.display()));
    let (facade_delta, _) = build_code_facade(
        &FacadeGenRequest {
            new_symbol_space: true,
            seed_pairs: vec![("acme".to_string(), "Product".to_string())],
            already_emitted: Default::default(),
            emit_prelude: true,
        },
        es.domain_exposure.as_ref().expect("exposure"),
        &es.contexts_by_entry,
    );
    let quickjs_runtime = quickjs_runtime_from_facade_delta(&facade_delta);
    CodeModeSandbox::new()
        .expect("QuickJS")
        .eval_typescript_to_json_value(fixture, &ts, Some(&quickjs_runtime))
        .expect_err("fixture should fail")
        .to_string()
}

fn assert_compute_lineage_reaches_api_read(plan: &serde_json::Value, fixture: &str) {
    let nodes = plan["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("{fixture} nodes must be an array"));
    let node_by_id = nodes
        .iter()
        .filter_map(|node| Some((node["id"].as_str()?, node)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let has_compute_from_read = nodes
        .iter()
        .filter(|node| node["kind"] == json!("compute"))
        .any(|node| {
            let mut current = node["compute"]["source"].as_str();
            while let Some(source_id) = current {
                let Some(source_node) = node_by_id.get(source_id) else {
                    return false;
                };
                if source_node["kind"] == json!("query") && source_node["expr"].as_str().is_some() {
                    return true;
                }
                current = source_node["source"]
                    .as_str()
                    .or_else(|| source_node["derive_template"]["source"].as_str())
                    .or_else(|| source_node["compute"]["source"].as_str());
            }
            false
        });
    assert!(
        has_compute_from_read,
        "{fixture} must compute from API read/projection lineage, not only Plan.data: {plan:#}"
    );
}

#[tokio::test]
async fn typescript_to_plan_to_run_code_mode_executes_http() {
    // --- Mock API: GET /products (maps to `product_list` in execute_tiny) ---
    let app = Router::new().route(
        "/products",
        get(|| async { Json(json!([{ "id": "p1", "name": "KitchenSink" }])) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let backend = format!("http://{}", addr);

    // --- Host + registry (execute_tiny) ---
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_dir = root.join("tests/fixtures/execute_tiny");
    let cgs = Arc::new(load_schema(&schema_dir).expect("load execute_tiny"));
    let reg = InMemoryCgsRegistry::from_pairs(vec![(
        "acme".into(),
        "Acme".into(),
        vec!["e2e".into()],
        cgs.clone(),
    )]);
    let engine = ExecutionEngine::new(ExecutionConfig::default()).expect("engine");
    let st = http::build_plasm_host_state(PlasmHostBootstrap {
        engine,
        mode: ExecutionMode::Live,
        registry: Arc::new(reg),
        catalog_bootstrap: CatalogBootstrap::Fixed,
        plugin_manager: None,
        incoming_auth: None,
        run_artifacts: Arc::new(plasm_agent_core::run_artifacts::RunArtifactStore::memory()),
        session_graph_persistence: None,
    });

    // --- Execute session (aligned with HTTP create: same entry, entities, hash) ---
    let prompt_text = "plasm code-mode kitchen sink e2e";
    let prompt_hash = PromptHashHex::from_prompt_sha256(prompt_text).to_string();
    let session_id = ExecuteSessionId::new_random().to_string();

    let mut ctxs = IndexMap::new();
    ctxs.insert(
        "acme".into(),
        Arc::new(CgsContext::entry("acme", cgs.clone())),
    );
    let exp = DomainExposureSession::new(cgs.as_ref(), "acme", &["Product"]);
    let catalog_hash = cgs.catalog_cgs_hash_hex();
    let session = ExecuteSession::new(
        prompt_hash.clone(),
        prompt_text.to_string(),
        cgs.clone(),
        ctxs,
        "acme".into(),
        String::new(),
        String::new(),
        Some(backend),
        vec!["Product".into()],
        Some(exp),
        None,
        None,
        catalog_hash.clone(),
    );

    let reuse = SessionReuseKey {
        tenant_scope: String::new(),
        entry_id: "acme".into(),
        catalog_cgs_hash: catalog_hash,
        entities: vec!["Product".into()],
        principal: None,
        plugin_generation_id: None,
        logical_session_id: None,
    };
    st.sessions
        .insert(reuse, prompt_hash.clone(), session_id.clone(), session)
        .await;

    let es = st
        .sessions
        .get_by_strs(&prompt_hash, &session_id)
        .await
        .expect("session inserted");

    // --- TypeScript → JS (Oxc) → JSON string (QuickJS) ---
    let ts_path = root.join("tests/fixtures/code_mode_kitchen_sink/plan.ts");
    let ts = std::fs::read_to_string(&ts_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", ts_path.display()));
    let (facade_delta, _) = build_code_facade(
        &FacadeGenRequest {
            new_symbol_space: true,
            seed_pairs: vec![("acme".to_string(), "Product".to_string())],
            already_emitted: Default::default(),
            emit_prelude: true,
        },
        es.domain_exposure.as_ref().expect("exposure"),
        &es.contexts_by_entry,
    );
    let quickjs_runtime = quickjs_runtime_from_facade_delta(&facade_delta);
    let plan = CodeModeSandbox::new()
        .expect("QuickJS")
        .eval_typescript_to_json_value("plan.ts", &ts, Some(&quickjs_runtime))
        .expect("TS → Oxc → QuickJS plan JSON");

    let out = run_code_mode_plan(
        es.as_ref(),
        &st,
        None,
        &prompt_hash,
        &session_id,
        &plan,
        true,
        None,
    )
    .await
    .expect("run_code_mode_plan");

    assert_eq!(out.version, json!(1));
    assert_eq!(out.node_results.len(), 1, "one Plan node");
    let md = out
        .run_markdown
        .as_deref()
        .expect("run: true must produce markdown");
    assert!(
        md.contains("KitchenSink") || md.contains("p1"),
        "expected live HTTP result in markdown, got:\n{md}"
    );
}

#[test]
fn kitchen_sink_positive_fixtures_dry_run() {
    let es = test_execute_session();
    for fixture in [
        "pure_read.ts",
        "read_then_side_effect.ts",
        "parallel_derive.ts",
        "cross_catalog_flow.ts",
        "data_map.ts",
        "linear_issue_grouping.ts",
        "slack_thread_rollup.ts",
        "github_progress_projection.ts",
        "google_sheets_table.ts",
    ] {
        let plan = eval_fixture_plan(&es, fixture);
        let dry = evaluate_code_mode_plan_dry(&es, &plan)
            .unwrap_or_else(|e| panic!("{fixture} dry run failed: {e}\n{plan:#}"));
        assert_eq!(dry.version, json!(1), "{fixture}");
        assert!(
            dry.graph_summary["node_count"].as_u64().unwrap_or_default() >= 1,
            "{fixture}"
        );
        if fixture != "pure_read.ts" {
            assert!(
                !dry.can_batch_run,
                "{fixture} should require the phased Plan runner"
            );
        }
        if matches!(
            fixture,
            "read_then_side_effect.ts" | "cross_catalog_flow.ts" | "github_progress_projection.ts"
        ) {
            let gates = dry.graph_summary["approval_gates"]
                .as_array()
                .unwrap_or_else(|| panic!("{fixture} approval_gates must be an array"));
            assert_eq!(gates.len(), 1, "{fixture} must infer one approval gate");
            assert_eq!(gates[0]["required"], json!(true), "{fixture}");
            assert!(
                gates[0]["policy_key"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("acme.Product."),
                "{fixture} policy key: {gates:#?}"
            );
        }
        if fixture == "data_map.ts" {
            assert!(
                dry.node_results
                    .iter()
                    .any(|n| n["kind"] == json!("data") && n["data"]["kind"] == json!("array")),
                "data_map.ts should include a typed static data node: {:#?}",
                dry.node_results
            );
            assert!(
                dry.node_results
                    .iter()
                    .any(|n| n["derive_template"]["kind"] == json!("map")),
                "data_map.ts should include a typed map derive node: {:#?}",
                dry.node_results
            );
        }
        if matches!(
            fixture,
            "linear_issue_grouping.ts"
                | "slack_thread_rollup.ts"
                | "github_progress_projection.ts"
                | "google_sheets_table.ts"
        ) {
            assert!(
                dry.node_results
                    .iter()
                    .any(|n| n["kind"] == json!("compute")),
                "{fixture} should include deterministic compute nodes: {:#?}",
                dry.node_results
            );
            assert_compute_lineage_reaches_api_read(&plan, fixture);
        }
    }
}

#[tokio::test]
async fn computed_plan_result_executes_as_archived_synthetic_result() {
    let app = Router::new().route(
        "/products",
        get(|| async { Json(json!([{ "id": "p1", "name": "KitchenSink" }])) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let backend = format!("http://{}", addr);

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cgs =
        Arc::new(load_schema(&root.join("tests/fixtures/execute_tiny")).expect("load schema"));
    let mut ctxs = IndexMap::new();
    ctxs.insert(
        "acme".into(),
        Arc::new(CgsContext::entry("acme", cgs.clone())),
    );
    let exp = DomainExposureSession::new(cgs.as_ref(), "acme", &["Product"]);
    let es = ExecuteSession::new(
        "fixture-prompt".into(),
        "fixture prompt".into(),
        cgs.clone(),
        ctxs,
        "acme".into(),
        String::new(),
        String::new(),
        Some(backend),
        vec!["Product".into()],
        Some(exp),
        None,
        None,
        cgs.catalog_cgs_hash_hex(),
    );
    let plan = eval_fixture_plan(&es, "slack_thread_rollup.ts");
    let reg = InMemoryCgsRegistry::from_pairs(vec![(
        "acme".into(),
        "Acme".into(),
        vec!["e2e".into()],
        cgs,
    )]);
    let st = http::build_plasm_host_state(PlasmHostBootstrap {
        engine: ExecutionEngine::new(ExecutionConfig::default()).expect("engine"),
        mode: ExecutionMode::Live,
        registry: Arc::new(reg),
        catalog_bootstrap: CatalogBootstrap::Fixed,
        plugin_manager: None,
        incoming_auth: None,
        run_artifacts: Arc::new(plasm_agent_core::run_artifacts::RunArtifactStore::memory()),
        session_graph_persistence: None,
    });

    let out = run_code_mode_plan(
        &es,
        &st,
        None,
        es.prompt_hash.as_str(),
        "synthetic-session",
        &plan,
        true,
        None,
    )
    .await
    .expect("computed plan run");
    let markdown = out.run_markdown.expect("markdown");
    assert!(markdown.contains("KitchenSink"), "{markdown}");
    assert!(markdown.contains("messages"), "{markdown}");
    assert!(
        out.run_plasm_meta.is_some(),
        "synthetic computed result should publish MCP/run metadata"
    );
}

#[test]
fn kitchen_sink_fixtures_do_not_use_manual_plan_aliases() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/code_mode_kitchen_sink");
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("fixture entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ts") {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for forbidden in [
            ".as(",
            "Plan.named(",
            "new Plan(",
            ".stage(",
            "dependsOn(",
            "derive(",
            "parallel(",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} must not contain manual Plan API {forbidden:?}",
                path.display()
            );
        }
    }
}

#[test]
fn federated_name_clash_fixture_synthesizes_qualified_nodes() {
    let delta = FacadeDeltaV1 {
        version: 1,
        catalog_entry_ids: vec!["acme".to_string(), "other".to_string()],
        catalog_aliases: vec![],
        qualified_entities: vec![
            QualifiedEntitySurface {
                entry_id: "acme".to_string(),
                catalog_alias: "acme".to_string(),
                entity: "Product".to_string(),
                description: None,
                e_index: Some(1),
                fields: vec![],
                relations: vec![],
                capabilities: vec![],
            },
            QualifiedEntitySurface {
                entry_id: "other".to_string(),
                catalog_alias: "other".to_string(),
                entity: "Product".to_string(),
                description: None,
                e_index: Some(2),
                fields: vec![],
                relations: vec![],
                capabilities: vec![],
            },
        ],
        collision_notes: vec!["Product is intentionally federated".to_string()],
    };
    let runtime = quickjs_runtime_from_facade_delta(&delta);
    let plan = eval_fixture_with_runtime("federated_name_clash.ts", &runtime);
    let nodes = plan["nodes"].as_array().expect("nodes");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["qualified_entity"]["entry_id"], "acme");
    assert_eq!(nodes[1]["qualified_entity"]["entry_id"], "other");
}

#[test]
fn kitchen_sink_negative_fixtures_reject_invalid_plans() {
    let es = test_execute_session();
    for fixture in ["negative_unknown_return.ts"] {
        let plan = eval_fixture_plan(&es, fixture);
        assert!(
            evaluate_code_mode_plan_dry(&es, &plan).is_err(),
            "{fixture} should be rejected, got {plan:#}"
        );
    }
}

#[test]
fn arbitrary_compute_callbacks_are_rejected_before_plan_json() {
    let es = test_execute_session();
    let err = eval_fixture_error(&es, "negative_arbitrary_compute_callback.ts");
    assert!(
        err.contains("QuickJS") || err.contains("symbolic field access"),
        "unexpected error: {err}"
    );
}
