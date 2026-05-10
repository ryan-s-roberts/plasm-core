//! Chunk sizes for discovery embedding traffic (Postgres binds + fastembed batches).
//!
//! Values are conservative for typical parameter counts / pool memory; raise only with profiling.

/// Rows per `INSERT … ON CONFLICT` batch in [`crate::discovery_embedding_repository::DiscoveryEmbeddingRepository`].
pub const UPSERT_ROWS: usize = 64;

/// `(catalog_cgs_hash, line_text)` pairs per `unnest` fetch round-trip.
pub const FETCH_KEY_PAIRS: usize = 256;

/// Lines embedded per `BlockingEmbedder::embed_batch` call during reconcile materialization.
pub const RECONCILE_EMBED_BATCH: usize = 48;
