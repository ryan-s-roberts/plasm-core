//! Typed, graph-aware discovery over CGS catalogs with optional `fastembed` recall.
//!
//! See [`AgentDiscovery`] and [`TypedDiscovery`].

use async_trait::async_trait;

mod decompose;
mod embedder;
mod engine;
mod index;
mod metrics;
mod types;

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
