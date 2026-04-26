//! `insta` coverage for `build_code_facade` (TypeScript + JSON `facade_delta`).
use indexmap::IndexMap;
use plasm_core::load_schema;
use plasm_core::CgsContext;
use plasm_core::DomainExposureSession;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ExposedSet;
use crate::FacadeGenRequest;
use crate::{build_code_facade, quickjs_runtime_from_facade_delta};

fn tiny_cgs() -> plasm_core::CGS {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/tiny");
    load_schema(&dir).expect("load tiny fixture")
}

fn tiny_session(cgs: &plasm_core::CGS) -> DomainExposureSession {
    DomainExposureSession::new(cgs, "acme", &["Product"])
}

fn tiny_ctxs(cgs: plasm_core::CGS) -> IndexMap<String, Arc<CgsContext>> {
    let mut m = IndexMap::new();
    m.insert(
        "acme".to_string(),
        Arc::new(CgsContext::entry("acme", Arc::new(cgs))),
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
    assert_eq!(facade_delta.catalog_aliases[0].alias, "acme");
    assert_eq!(facade_delta.qualified_entities.len(), 1);
    assert_eq!(facade_delta.qualified_entities[0].entity, "Product");
    assert_eq!(facade_delta.qualified_entities[0].capabilities.len(), 5);
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
    assert!(ts.agent_namespace_body.contains("interface ProductRow"));
    assert!(ts
        .agent_namespace_body
        .contains("type ProductQueryInput = Partial<ProductRow> & {"));
    assert!(ts.agent_namespace_body.contains("    owner: string;"));
    assert!(ts.agent_namespace_body.contains("    repo: string;"));
    assert!(ts.agent_namespace_body.contains("    active?: boolean;"));
    assert!(ts
        .agent_namespace_body
        .contains("query(filters: ProductQueryInput): ProductQueryBuilder;"));
    assert!(ts
        .agent_namespace_body
        .contains("interface ProductReadSource<C extends Plasm.SourceCardinality"));
    assert!(ts
        .agent_namespace_body
        .contains("interface ProductNodeHandle<C extends Plasm.SourceCardinality"));
    assert!(ts
        .agent_namespace_body
        .contains("select(...fields: Array<keyof ProductRow & string>): this;"));
    assert!(ts
        .agent_namespace_body
        .contains("type ProductSearchInput = string | {"));
    assert!(ts.agent_namespace_body.contains("    q: string;"));
    assert!(ts
        .agent_namespace_body
        .contains("type ProductCreateInput = {"));
    assert!(ts.agent_namespace_body.contains("    name: string;"));
    assert!(ts
        .agent_namespace_body
        .contains("    category_id?: Plasm.EntityRef<\"acme\", \"Category\">;"));
    assert!(ts
        .agent_namespace_body
        .contains("create(input: ProductCreateInput): Plasm.PlanEffect;"));
    assert!(ts
        .agent_namespace_body
        .contains("type ProductProductLabelInput = {"));
    assert!(ts.agent_namespace_body.contains("    label: string;"));
    assert!(ts.agent_namespace_body.contains("    notify?: boolean;"));
    assert!(ts.agent_namespace_body.contains(
        "action(name: \"product_label\", input: ProductProductLabelInput): Plasm.PlanEffect;"
    ));
    assert!(
        !ts.agent_namespace_body
            .contains("action(name: string, input?: Record<string, unknown>)"),
        "{}",
        ts.agent_namespace_body
    );
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
    assert!(ts.agent_prelude.contains("exists(): PlanPredicate;"));
    assert!(!ts
        .agent_prelude
        .contains("export type Symbolic<T = unknown> = T &"));
    assert!(ts.agent_prelude.contains("RuntimeSingleton"));
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
fn snapshot_incremental_wave_no_prelude() {
    let cgs = tiny_cgs();
    let exp = tiny_session(&cgs);
    let ctxs = tiny_ctxs(cgs);
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
