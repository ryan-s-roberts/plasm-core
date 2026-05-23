//! Background materialization of discovery embeddings per [`CatalogEntryMeta::catalog_cgs_hash`](plasm_core::discovery::CatalogEntryMeta).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use plasm_core::discovery::CgsCatalog;
use plasm_discovery::{BlockingEmbedder, CatalogIndexCache, DEFAULT_EMBEDDING_MODEL_ID};
use tokio::time::MissedTickBehavior;

use crate::discovery_embedding_chunks::RECONCILE_EMBED_BATCH;
use crate::discovery_embedding_repository::DiscoveryEmbeddingRepository;
use crate::server_state::PlasmHostState;

/// Runs once immediately, then every `PLASM_DISCOVERY_EMBEDDINGS_RECONCILE_SECS` (default **600**; **0** = one-shot only).
pub fn spawn_discovery_embedding_reconcile_background(
    host: PlasmHostState,
    repo: std::sync::Arc<DiscoveryEmbeddingRepository>,
) {
    let secs = std::env::var("PLASM_DISCOVERY_EMBEDDINGS_RECONCILE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(600);

    tokio::spawn(async move {
        reconcile_catalog_hashes(&host, repo.as_ref()).await;
        if secs == 0 {
            return;
        }
        let mut ticker = tokio::time::interval(Duration::from_secs(secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            reconcile_catalog_hashes(&host, repo.as_ref()).await;
        }
    });
}

pub async fn reconcile_catalog_hashes(host: &PlasmHostState, repo: &DiscoveryEmbeddingRepository) {
    let reg = host.catalog.snapshot();
    let entries = reg.list_entries();
    let model_id = DEFAULT_EMBEDDING_MODEL_ID;
    let embedder = host.discovery_embedder();
    let index_cache = host.discovery_index_cache.clone();

    let mut tasks = FuturesUnordered::new();
    for meta in entries {
        let reg = reg.clone();
        let repo = repo.clone();
        let embedder = embedder.clone();
        let index_cache = index_cache.clone();
        let meta = meta.clone();
        tasks.push(async move {
            reconcile_one_catalog(&reg, &repo, &embedder, &index_cache, &meta, model_id).await;
        });
    }
    while tasks.next().await.is_some() {}
}

async fn reconcile_one_catalog(
    reg: &plasm_core::discovery::InMemoryCgsRegistry,
    repo: &DiscoveryEmbeddingRepository,
    embedder: &Arc<BlockingEmbedder>,
    index_cache: &CatalogIndexCache,
    meta: &plasm_core::discovery::CatalogEntryMeta,
    model_id: &str,
) {
    let Ok(ctx) = reg.load_context(meta.entry_id.as_str()) else {
        return;
    };
    let idx = index_cache.get_or_build(meta.entry_id.clone(), ctx.cgs);
    let lines = idx.distinct_discovery_embed_lines();
    if lines.is_empty() {
        return;
    }

    let expected: HashSet<String> = lines.into_iter().collect();
    let existing = match repo
        .list_line_texts_for_hash(&meta.catalog_cgs_hash, model_id)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                catalog_cgs_hash = %meta.catalog_cgs_hash,
                "discovery embeddings: list lines failed"
            );
            return;
        }
    };

    let missing: Vec<String> = expected.difference(&existing).cloned().collect();
    let stale: Vec<String> = existing.difference(&expected).cloned().collect();

    if missing.is_empty() && stale.is_empty() {
        return;
    }

    tracing::info!(
        entry_id = %meta.entry_id,
        catalog_cgs_hash = %meta.catalog_cgs_hash,
        missing = missing.len(),
        stale = stale.len(),
        "discovery embeddings: incremental materialize"
    );

    if !stale.is_empty() {
        if let Err(e) = repo
            .delete_lines_for_hash(&meta.catalog_cgs_hash, model_id, &stale)
            .await
        {
            tracing::warn!(
                error = %e,
                catalog_cgs_hash = %meta.catalog_cgs_hash,
                "discovery embeddings: delete stale lines failed"
            );
            return;
        }
    }

    for chunk in missing.chunks(RECONCILE_EMBED_BATCH) {
        let texts = chunk.to_vec();
        let vecs = match embedder.embed_batch(texts).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    catalog_cgs_hash = %meta.catalog_cgs_hash,
                    "discovery embeddings: embed batch failed"
                );
                break;
            }
        };
        if vecs.len() != chunk.len() {
            tracing::warn!(
                catalog_cgs_hash = %meta.catalog_cgs_hash,
                "discovery embeddings: embed batch length mismatch"
            );
            break;
        }
        let pairs: Vec<(String, Vec<f32>)> = chunk.iter().cloned().zip(vecs).collect();
        if let Err(e) = repo
            .upsert_lines_batch(&meta.catalog_cgs_hash, model_id, pairs)
            .await
        {
            tracing::warn!(
                error = %e,
                catalog_cgs_hash = %meta.catalog_cgs_hash,
                "discovery embeddings: batch upsert failed"
            );
        }
    }
}
