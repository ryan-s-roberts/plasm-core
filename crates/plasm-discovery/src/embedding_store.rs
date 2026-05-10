//! Optional Postgres-backed catalog line embeddings for typed discovery.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::types::DiscoveryError;

/// Stable lookup key: pinned CGS digest + canonical discovery embed line (see [`crate::index::discovery_embed_line_text`]).
///
/// Field order matches SQL `(catalog_cgs_hash, line_text)` and unnest bind order.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CatalogEmbeddingLineKey {
    pub catalog_cgs_hash: String,
    pub line_text: String,
}

impl CatalogEmbeddingLineKey {
    pub fn new(catalog_cgs_hash: String, line_text: String) -> Self {
        Self {
            catalog_cgs_hash,
            line_text,
        }
    }
}

/// Loads pre-materialized discovery line vectors keyed by [`CatalogEmbeddingLineKey`].
#[async_trait]
pub trait CatalogEmbeddingStore: Send + Sync {
    async fn fetch_embeddings(
        &self,
        embedding_model_id: &str,
        keys: &[CatalogEmbeddingLineKey],
    ) -> Result<HashMap<CatalogEmbeddingLineKey, Vec<f32>>, DiscoveryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_embedding_line_key_orders_fields_for_sql() {
        let k = CatalogEmbeddingLineKey::new("abc".into(), "line".into());
        assert_eq!(k.catalog_cgs_hash, "abc");
        assert_eq!(k.line_text, "line");
    }
}
