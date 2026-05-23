use std::path::PathBuf;
use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use plasm_core::discovery_adversarial_intents::iter_all_cases;
use plasm_core::loader::load_schema_dir;
use plasm_core::schema::CGS;
use plasm_discovery::index::CatalogIndex;
use plasm_discovery::{AgentDiscovery, CatalogIndexCache, DiscoveryQuery, TypedDiscovery};
use tokio::runtime::Runtime;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn github_cgs() -> Arc<CGS> {
    Arc::new(load_schema_dir(&repo_root().join("apis/github")).expect("github schema"))
}

fn bench_index_build(c: &mut Criterion) {
    let cgs = github_cgs();
    c.bench_function("CatalogIndex::build/github", |b| {
        b.iter(|| black_box(CatalogIndex::build("github".into(), cgs.clone())))
    });
}

fn bench_scan_utterance(c: &mut Criterion) {
    let idx = CatalogIndex::build("github".into(), github_cgs());
    let cases: Vec<_> = iter_all_cases().map(|c| c.intent.to_string()).collect();
    c.bench_function("scan_utterance/adversarial", |b| {
        b.iter(|| {
            for intent in &cases {
                black_box(idx.scan_utterance(intent));
            }
        })
    });
}

fn bench_typed_discover(c: &mut Criterion) {
    let cgs = github_cgs();
    let rt = Runtime::new().unwrap();
    let cases: Vec<_> = iter_all_cases().collect();
    c.bench_function("typed_discover/adversarial", |b| {
        b.iter(|| {
            rt.block_on(async {
                let disc = TypedDiscovery::from_cgs_entries(
                    vec![("github".into(), cgs.clone())],
                    false,
                    None,
                    None,
                );
                for case in &cases {
                    let q = DiscoveryQuery {
                        utterance: case.intent.to_string(),
                        allowed_entry_ids: vec!["github".into()],
                        ..Default::default()
                    };
                    black_box(disc.discover(q).await.unwrap());
                }
            })
        })
    });
}

fn bench_index_cache_hit(c: &mut Criterion) {
    let cache = CatalogIndexCache::new();
    let cgs = github_cgs();
    c.bench_function("CatalogIndexCache/get_or_build_hit", |b| {
        cache.get_or_build("github".into(), cgs.clone());
        b.iter(|| black_box(cache.get_or_build("github".into(), cgs.clone())))
    });
}

criterion_group!(
    benches,
    bench_index_build,
    bench_scan_utterance,
    bench_typed_discover,
    bench_index_cache_hit
);
criterion_main!(benches);
