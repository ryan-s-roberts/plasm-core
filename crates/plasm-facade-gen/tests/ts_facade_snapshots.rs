//! Snapshot tests for `build_code_facade` — stable CGS + exposure → TS + `facade_delta` JSON.
use indexmap::IndexMap;
use plasm_core::load_schema;
use plasm_core::CgsContext;
use plasm_core::DomainExposureSession;
use plasm_facade_gen::ExposedSet;
use plasm_facade_gen::FacadeGenRequest;
use plasm_facade_gen::{build_code_facade, quickjs_runtime_from_facade_delta};
use std::path::PathBuf;
use std::sync::Arc;

fn tiny_cgs() -> plasm_core::CGS {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/tiny");
    load_schema(&dir).expect("load tiny fixture")
}

fn tiny_session(cgs: &plasm_core::CGS) -> DomainExposureSession {
    DomainExposureSession::new(cgs, "acme", &["Product"])
}

fn tiny_relation_session(cgs: &plasm_core::CGS) -> DomainExposureSession {
    DomainExposureSession::new(cgs, "acme", &["Product", "Category"])
}

fn tiny_ctxs(cgs: plasm_core::CGS) -> IndexMap<String, Arc<CgsContext>> {
    let mut m = IndexMap::new();
    m.insert(
        "acme".to_string(),
        Arc::new(CgsContext::entry("acme", Arc::new(cgs))),
    );
    m
}

fn github_cgs() -> plasm_core::CGS {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    load_schema(&root.join("../../apis/github")).expect("load github fixture")
}

fn github_ctxs(cgs: plasm_core::CGS) -> IndexMap<String, Arc<CgsContext>> {
    let mut m = IndexMap::new();
    m.insert(
        "github".to_string(),
        Arc::new(CgsContext::entry("github", Arc::new(cgs))),
    );
    m
}

#[test]
fn snapshot_code_facade_prelude_emitted() {
    let cgs = tiny_cgs();
    let exp = tiny_session(&cgs);
    let ctxs = tiny_ctxs(cgs);
    let req = FacadeGenRequest {
        new_symbol_space: true,
        seed_pairs: vec![("acme".to_string(), "Product".to_string())],
        already_emitted: ExposedSet::default(),
        emit_prelude: true,
    };
    let (facade_delta, ts) = build_code_facade(&req, &exp, &ctxs);
    assert_eq!(facade_delta.version, 1);
    assert_eq!(facade_delta.catalog_entry_ids, vec!["acme".to_string()]);
    assert_eq!(facade_delta.catalog_aliases.len(), 1);
    assert_eq!(facade_delta.qualified_entities.len(), 1);
    assert_eq!(facade_delta.qualified_entities[0].entity, "Product");
    assert!(facade_delta.collision_notes.is_empty());
    assert!(!ts.agent_prelude.contains("linearTeam"));
    assert!(!ts.agent_prelude.contains("repo("));
    assert!(!ts.agent_prelude.contains("__plasmBind"));
    assert!(!ts.agent_prelude.contains("makeEntity"));
    assert!(!ts.agent_prelude.contains("Plan.named"));
    assert!(!ts.agent_prelude.contains("dependsOn"));
    assert!(!ts.agent_prelude.contains("stage("));
    assert!(!ts.agent_prelude.contains("derive("));
    assert!(ts.agent_namespace_body.contains("/**"));
    assert!(ts
        .agent_namespace_body
        .contains("Minimal fixture product for TS facade snapshot tests"));
    assert!(ts.agent_namespace_body.contains("Fetch a product by id"));
    assert!(ts
        .agent_namespace_body
        .contains("search(input: ProductSearchInput)"));
    assert!(ts.agent_namespace_body.contains("interface ProductRow"));
    assert!(ts
        .agent_namespace_body
        .contains("category_id?: Plasm.EntityRef<\"acme\", \"Category\">;"));
    assert!(ts.agent_loaded_apis.contains("Product: Acme.ProductEntity"));
    assert!(ts
        .agent_loaded_apis
        .contains("declare const plasm: LoadedApis"));
    assert!(
        !ts.agent_loaded_apis.contains("default:"),
        "{}",
        ts.agent_loaded_apis
    );
    assert!(!ts.agent_namespace_body.contains("plasm.default"));
    assert!(ts.agent_prelude.contains("PlanDataInput"));
    assert!(ts.agent_prelude.contains("PlanReturnable"));
    assert!(ts.agent_prelude.contains("ProjectionValue"));
    assert!(ts.agent_prelude.contains("EntityRefHandle"));
    assert!(!ts.agent_prelude.contains("= K | PlanValueExpr |"));
    assert!(ts.agent_prelude.contains("kind: \"entity_ref_key\""));
    assert!(ts.agent_prelude.contains("type NonEmptyArray<T>"));
    assert!(ts.agent_prelude.contains("type FieldAggregateSpec"));
    assert!(ts.agent_prelude.contains("exists(): PlanPredicate;"));
    assert!(!ts
        .agent_prelude
        .contains("export type Symbolic<T = unknown> = T &"));
    assert!(ts.agent_prelude.contains("binding_symbol"));
    assert!(ts.agent_prelude.contains("node_symbol"));
    assert!(ts.agent_prelude.contains("static singleton"));
    let runtime = quickjs_runtime_from_facade_delta(&facade_delta);
    assert!(runtime.contains("__plasmBind"));
    assert!(runtime.contains("makeEntity"));
    assert!(!ts.agent_prelude.contains("__nodeHandle"));
    assert!(!ts.declarations_unchanged);
    assert_eq!(ts.added_catalog_aliases, vec!["acme".to_string()]);
}

#[test]
fn snapshot_search_and_relation_surface() {
    let cgs = tiny_cgs();
    let exp = tiny_relation_session(&cgs);
    let ctxs = tiny_ctxs(cgs);
    let req = FacadeGenRequest {
        new_symbol_space: true,
        seed_pairs: vec![
            ("acme".to_string(), "Product".to_string()),
            ("acme".to_string(), "Category".to_string()),
        ],
        already_emitted: ExposedSet::default(),
        emit_prelude: true,
    };
    let (facade_delta, ts) = build_code_facade(&req, &exp, &ctxs);
    assert!(ts
        .agent_namespace_body
        .contains("type ProductSearchInput = string | {"));
    assert!(ts.agent_namespace_body.contains("q: string;"));
    assert!(ts.agent_namespace_body.contains("active?: boolean;"));
    assert!(ts
        .agent_namespace_body
        .contains("category(this: ProductReadSource<\"single\"> | ProductReadSource<\"runtime_checked_singleton\">): CategoryReadSource<\"single\"> & Plasm.PlanBuilder;"));
    assert!(ts
        .agent_namespace_body
        .contains("interface ProductReadSource<C extends Plasm.SourceCardinality"));
    assert!(ts
        .agent_namespace_body
        .contains("get(id: string): ProductReadSource<\"single\"> & Plasm.PlanEffect & Plasm.EntityRefHandle<\"acme\", \"Product\", string>;"));
    assert!(ts.agent_prelude.contains(
        "static singleton<T extends Plasm.PlanSource>(source: T): Plasm.RuntimeSingleton<T>;"
    ));
    let category_entity = ts
        .agent_namespace_body
        .split("interface CategoryEntity {")
        .nth(1)
        .and_then(|s| s.split("\n  }\n").next())
        .expect("CategoryEntity section");
    assert!(!category_entity.contains("create("), "{category_entity}");
    assert!(!category_entity.contains("ref("), "{category_entity}");

    let runtime = quickjs_runtime_from_facade_delta(&facade_delta);
    assert!(runtime.contains("search(input)"));
    assert!(runtime.contains("\"name\":\"category\""));
}

#[test]
fn compound_key_get_types_match_runtime_shorthand_rules() {
    let cgs = github_cgs();
    let exp = DomainExposureSession::new(&cgs, "github", &["Repository", "Issue"]);
    let ctxs = github_ctxs(cgs);
    let req = FacadeGenRequest {
        new_symbol_space: true,
        seed_pairs: vec![
            ("github".to_string(), "Repository".to_string()),
            ("github".to_string(), "Issue".to_string()),
        ],
        already_emitted: ExposedSet::default(),
        emit_prelude: true,
    };
    let (_facade_delta, ts) = build_code_facade(&req, &exp, &ctxs);

    assert!(
        ts.agent_namespace_body
            .contains("get(id: `${string}/${string}` | { \"owner\": string; \"repo\": string })"),
        "{}",
        ts.agent_namespace_body
    );
    assert!(
        ts.agent_namespace_body
            .contains("get(id: { \"owner\": string; \"repo\": string; \"number\": string })"),
        "{}",
        ts.agent_namespace_body
    );
}

#[test]
fn relation_methods_resolve_targets_from_prior_waves() {
    let cgs = tiny_cgs();
    let exp = tiny_relation_session(&cgs);
    let ctxs = tiny_ctxs(cgs);
    let req = FacadeGenRequest {
        new_symbol_space: false,
        seed_pairs: vec![("acme".to_string(), "Product".to_string())],
        already_emitted: ExposedSet::from_iter([("acme".to_string(), "Category".to_string())]),
        emit_prelude: false,
    };
    let (_facade_delta, ts) = build_code_facade(&req, &exp, &ctxs);
    assert!(ts
        .agent_namespace_body
        .contains("category(this: ProductReadSource<\"single\"> | ProductReadSource<\"runtime_checked_singleton\">): CategoryReadSource<\"single\"> & Plasm.PlanBuilder;"));
}

#[test]
fn snapshot_incremental_wave_no_prelude() {
    let cgs = tiny_cgs();
    let exp = tiny_session(&cgs);
    let ctxs = tiny_ctxs(cgs);
    // Second wave: already emitted, no new pairs → empty TS + empty delta
    let req = FacadeGenRequest {
        new_symbol_space: false,
        seed_pairs: vec![("acme".to_string(), "Product".to_string())],
        already_emitted: ExposedSet::from_iter([("acme".to_string(), "Product".to_string())]),
        emit_prelude: false,
    };
    let (facade_delta, ts) = build_code_facade(&req, &exp, &ctxs);
    assert_eq!(facade_delta.version, 1);
    assert!(facade_delta.catalog_entry_ids.is_empty());
    assert!(facade_delta.catalog_aliases.is_empty());
    assert!(facade_delta.qualified_entities.is_empty());
    assert!(facade_delta.collision_notes.is_empty());
    assert!(ts.agent_prelude.is_empty());
    assert!(ts.agent_namespace_body.is_empty());
    assert!(ts.agent_loaded_apis.is_empty());
    assert!(ts.declarations_unchanged);
}
