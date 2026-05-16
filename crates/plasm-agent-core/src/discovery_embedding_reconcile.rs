//! Background materialization of discovery embeddings per [`CatalogEntryMeta::catalog_cgs_hash`](plasm_core::discovery::CatalogEntryMeta).

use std::time::Duration;

use plasm_core::discovery::CgsCatalog;
use plasm_discovery::index::CatalogIndex;
use plasm_discovery::{BlockingEmbedder, DEFAULT_EMBEDDING_MODEL_ID};
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
    let embedder = BlockingEmbedder::new(fastembed::EmbeddingModel::AllMiniLML6V2, 2);

    for meta in entries {
        let Ok(ctx) = reg.load_context(meta.entry_id.as_str()) else {
            continue;
        };
        let idx = CatalogIndex::build(meta.entry_id.clone(), ctx.cgs);
        let lines = idx.distinct_discovery_embed_lines();
        let expected = lines.len() as i64;
        if expected == 0 {
            continue;
        }

        let count = match repo
            .count_lines_for_hash(&meta.catalog_cgs_hash, model_id)
            .await
        {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    catalog_cgs_hash = %meta.catalog_cgs_hash,
                    "discovery embeddings: count query failed"
                );
                continue;
            }
        };

        if count == expected {
            continue;
        }

        tracing::info!(
            entry_id = %meta.entry_id,
            catalog_cgs_hash = %meta.catalog_cgs_hash,
            expected,
            count,
            "discovery embeddings: materializing catalog hash"
        );

        if let Err(e) = repo
            .delete_all_for_hash(&meta.catalog_cgs_hash, model_id)
            .await
        {
            tracing::warn!(
                error = %e,
                catalog_cgs_hash = %meta.catalog_cgs_hash,
                "discovery embeddings: delete before refill failed"
            );
            continue;
        }

        for chunk in lines.chunks(RECONCILE_EMBED_BATCH) {
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
}
