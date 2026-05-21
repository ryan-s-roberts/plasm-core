//! CPU-bound `fastembed` calls behind `spawn_blocking` + a small semaphore (`local-embeddings` only).

#[cfg(feature = "local-embeddings")]
use std::sync::{Arc, Mutex as StdMutex};

#[cfg(feature = "local-embeddings")]
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
#[cfg(feature = "local-embeddings")]
use tokio::sync::Semaphore;

use crate::embedding_store::DEFAULT_EMBEDDING_MODEL_ID;
use crate::types::DiscoveryError;

#[cfg(feature = "local-embeddings")]
pub struct BlockingEmbedder {
    model: EmbeddingModel,
    semaphore: Arc<Semaphore>,
    inner: Arc<StdMutex<Option<TextEmbedding>>>,
}

#[cfg(feature = "local-embeddings")]
impl BlockingEmbedder {
    pub fn new(model: EmbeddingModel, max_concurrent_blocking: usize) -> Self {
        let permits = max_concurrent_blocking.max(1);
        Self {
            model,
            semaphore: Arc::new(Semaphore::new(permits)),
            inner: Arc::new(StdMutex::new(None)),
        }
    }

    pub fn model_id(&self) -> &'static str {
        DEFAULT_EMBEDDING_MODEL_ID
    }

    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, DiscoveryError> {
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| DiscoveryError::Embed("embed semaphore closed".into()))?;

        let inner = self.inner.clone();
        let model = self.model.clone();

        tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| DiscoveryError::Embed("embed mutex poisoned".into()))?;
            if guard.is_none() {
                let m = TextEmbedding::try_new(
                    InitOptions::new(model).with_show_download_progress(false),
                )
                .map_err(|e| DiscoveryError::Embed(e.to_string()))?;
                *guard = Some(m);
            }
            let emb = guard
                .as_mut()
                .ok_or_else(|| DiscoveryError::Embed("embedder not initialized".into()))?;
            emb.embed(texts, None)
                .map_err(|e| DiscoveryError::Embed(e.to_string()))
        })
        .await
        .map_err(|e| DiscoveryError::Embed(format!("join error: {e}")))?
    }
}

pub fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for i in 0..a.len() {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    dot / na.sqrt() / nb.sqrt()
}
