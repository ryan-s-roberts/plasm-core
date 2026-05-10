//! Typed, graph-aware discovery over CGS catalogs with optional `fastembed` recall.
//!
//! See [`AgentDiscovery`] and [`TypedDiscovery`].

use async_trait::async_trait;

mod decompose;
mod embedder;
pub mod embedding_store;
mod engine;
pub mod index;
mod metrics;
mod types;

pub use embedder::{BlockingEmbedder, DEFAULT_EMBEDDING_MODEL_ID, DEFAULT_EMBEDDING_VECTOR_DIM};
pub use embedding_store::CatalogEmbeddingLineKey;
pub use engine::TypedDiscovery;
pub use types::*;

/// Stepwise discovery: single-shot [`Self::discover`] or clarification follow-ups.
#[async_trait]
pub trait AgentDiscovery: Send + Sync {
    async fn discover(&self, query: DiscoveryQuery) -> Result<DiscoveryDecision, DiscoveryError>;

    async fn answer_clarification(
        &self,
        state: ClarificationState,
        answer: ClarificationAnswer,
    ) -> Result<DiscoveryDecision, DiscoveryError>;
}
