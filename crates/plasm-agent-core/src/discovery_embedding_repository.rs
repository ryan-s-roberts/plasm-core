//! Postgres persistence for CGS-versioned typed-discovery embeddings (`plasm-discovery` / fastembed).

use std::collections::HashMap;

use async_trait::async_trait;
use plasm_discovery::embedding_store::{CatalogEmbeddingLineKey, CatalogEmbeddingStore};
use plasm_discovery::{DiscoveryError, DEFAULT_EMBEDDING_VECTOR_DIM};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, QueryBuilder};
use thiserror::Error;

use crate::discovery_embedding_chunks as emb_chunks;
use crate::mcp_config_repository::mcp_config_database_url;

#[derive(Debug, Error)]
pub enum DiscoveryEmbeddingRepositoryError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("{0}")]
    InvalidEmbedding(String),
}

#[derive(Clone)]
pub struct DiscoveryEmbeddingRepository {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct EmbeddingRow {
    catalog_cgs_hash: String,
    line_text: String,
    embedding: Vec<u8>,
}

fn f32_slice_to_le_bytes(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for &v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn le_bytes_to_f32_vec(bytes: &[u8]) -> Result<Vec<f32>, DiscoveryEmbeddingRepositoryError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(DiscoveryEmbeddingRepositoryError::InvalidEmbedding(
            format!(
                "embedding BYTEA length {} is not a multiple of 4",
                bytes.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().expect("chunks_exact(4)");
        out.push(f32::from_le_bytes(arr));
    }
    Ok(out)
}

impl DiscoveryEmbeddingRepository {
    pub async fn connect_and_migrate(
        database_url: &str,
    ) -> Result<Self, DiscoveryEmbeddingRepositoryError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    fn validate_embedding(vec: &[f32]) -> Result<(), DiscoveryEmbeddingRepositoryError> {
        if vec.len() != DEFAULT_EMBEDDING_VECTOR_DIM {
            return Err(DiscoveryEmbeddingRepositoryError::InvalidEmbedding(
                format!(
                    "expected embedding dim {}, got {}",
                    DEFAULT_EMBEDDING_VECTOR_DIM,
                    vec.len()
                ),
            ));
        }
        Ok(())
    }

    pub async fn count_lines_for_hash(
        &self,
        catalog_cgs_hash: &str,
        embedding_model_id: &str,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT COUNT(*)::bigint FROM plasm_catalog_discovery_embeddings
               WHERE catalog_cgs_hash = $1 AND embedding_model_id = $2"#,
        )
        .bind(catalog_cgs_hash)
        .bind(embedding_model_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn delete_all_for_hash(
        &self,
        catalog_cgs_hash: &str,
        embedding_model_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"DELETE FROM plasm_catalog_discovery_embeddings
               WHERE catalog_cgs_hash = $1 AND embedding_model_id = $2"#,
        )
        .bind(catalog_cgs_hash)
        .bind(embedding_model_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete specific embed lines for a catalog hash (incremental reconcile).
    pub async fn delete_lines_for_hash(
        &self,
        catalog_cgs_hash: &str,
        embedding_model_id: &str,
        lines: &[String],
    ) -> Result<(), sqlx::Error> {
        if lines.is_empty() {
            return Ok(());
        }
        for chunk in lines.chunks(emb_chunks::FETCH_KEY_PAIRS) {
            sqlx::query(
                r#"DELETE FROM plasm_catalog_discovery_embeddings e
                   WHERE e.catalog_cgs_hash = $1
                     AND e.embedding_model_id = $2
                     AND e.line_text IN (SELECT * FROM unnest($3::text[]) AS t(line))"#,
            )
            .bind(catalog_cgs_hash)
            .bind(embedding_model_id)
            .bind(chunk)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn list_line_texts_for_hash(
        &self,
        catalog_cgs_hash: &str,
        embedding_model_id: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"SELECT line_text FROM plasm_catalog_discovery_embeddings
               WHERE catalog_cgs_hash = $1 AND embedding_model_id = $2"#,
        )
        .bind(catalog_cgs_hash)
        .bind(embedding_model_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    pub async fn upsert_line(
        &self,
        catalog_cgs_hash: &str,
        embedding_model_id: &str,
        line_text: &str,
        embedding: &[f32],
    ) -> Result<(), DiscoveryEmbeddingRepositoryError> {
        Self::validate_embedding(embedding)?;
        let dim = embedding.len().min(i16::MAX as usize) as i16;
        let bytes = f32_slice_to_le_bytes(embedding);
        sqlx::query(
            r#"INSERT INTO plasm_catalog_discovery_embeddings
               (catalog_cgs_hash, embedding_model_id, line_text, embedding_dim, embedding)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (catalog_cgs_hash, embedding_model_id, line_text) DO UPDATE SET
                 embedding_dim = EXCLUDED.embedding_dim,
                 embedding = EXCLUDED.embedding,
                 inserted_at = now()"#,
        )
        .bind(catalog_cgs_hash)
        .bind(embedding_model_id)
        .bind(line_text)
        .bind(dim)
        .bind(bytes)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Batch upsert for materialization (one statement per chunk).
    pub async fn upsert_lines_batch(
        &self,
        catalog_cgs_hash: &str,
        embedding_model_id: &str,
        mut pairs: Vec<(String, Vec<f32>)>,
    ) -> Result<(), DiscoveryEmbeddingRepositoryError> {
        if pairs.is_empty() {
            return Ok(());
        }
        let dim = DEFAULT_EMBEDDING_VECTOR_DIM.min(i16::MAX as usize) as i16;
        while !pairs.is_empty() {
            let take = emb_chunks::UPSERT_ROWS.min(pairs.len());
            let chunk: Vec<(String, Vec<f32>)> = pairs.drain(..take).collect();
            for (_, vec) in &chunk {
                Self::validate_embedding(vec.as_slice())?;
            }
            let mut qb = QueryBuilder::<Postgres>::new(
                "INSERT INTO plasm_catalog_discovery_embeddings (catalog_cgs_hash, embedding_model_id, line_text, embedding_dim, embedding) ",
            );
            qb.push_values(chunk, |mut b, (line, vec)| {
                let bytes = f32_slice_to_le_bytes(vec.as_slice());
                b.push_bind(catalog_cgs_hash)
                    .push_bind(embedding_model_id)
                    .push_bind(line)
                    .push_bind(dim)
                    .push_bind(bytes);
            });
            qb.push(
                " ON CONFLICT (catalog_cgs_hash, embedding_model_id, line_text) DO UPDATE SET \
                 embedding_dim = EXCLUDED.embedding_dim, \
                 embedding = EXCLUDED.embedding, \
                 inserted_at = now()",
            );
            qb.build().execute(&self.pool).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl CatalogEmbeddingStore for DiscoveryEmbeddingRepository {
    async fn fetch_embeddings(
        &self,
        embedding_model_id: &str,
        keys: &[CatalogEmbeddingLineKey],
    ) -> Result<HashMap<CatalogEmbeddingLineKey, Vec<f32>>, DiscoveryError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let mut out = HashMap::with_capacity(keys.len());
        for chunk in keys.chunks(emb_chunks::FETCH_KEY_PAIRS) {
            let mut hashes = Vec::with_capacity(chunk.len());
            let mut lines = Vec::with_capacity(chunk.len());
            for key in chunk {
                hashes.push(key.catalog_cgs_hash.clone());
                lines.push(key.line_text.clone());
            }
            let rows: Vec<EmbeddingRow> = sqlx::query_as(
                r#"SELECT e.catalog_cgs_hash, e.line_text, e.embedding
                   FROM plasm_catalog_discovery_embeddings e
                   WHERE e.embedding_model_id = $1
                     AND (e.catalog_cgs_hash, e.line_text) IN (
                       SELECT * FROM unnest($2::text[], $3::text[]) AS keys(h, l)
                     )"#,
            )
            .bind(embedding_model_id)
            .bind(&hashes[..])
            .bind(&lines[..])
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DiscoveryError::EmbeddingStore(e.to_string()))?;
            for row in rows {
                let arr = match le_bytes_to_f32_vec(row.embedding.as_slice()) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(
                            catalog_cgs_hash = %row.catalog_cgs_hash,
                            line_text_len = row.line_text.len(),
                            error = %e,
                            "discovery embeddings: skipping row with invalid BYTEA payload"
                        );
                        continue;
                    }
                };
                if arr.len() != DEFAULT_EMBEDDING_VECTOR_DIM {
                    tracing::warn!(
                        catalog_cgs_hash = %row.catalog_cgs_hash,
                        line_text_len = row.line_text.len(),
                        dim = arr.len(),
                        expected_dim = DEFAULT_EMBEDDING_VECTOR_DIM,
                        "discovery embeddings: skipping row with unexpected vector dim"
                    );
                    continue;
                }
                out.insert(
                    CatalogEmbeddingLineKey {
                        catalog_cgs_hash: row.catalog_cgs_hash,
                        line_text: row.line_text,
                    },
                    arr,
                );
            }
        }
        Ok(out)
    }
}

/// When unset or non-`0`, connect when [`mcp_config_database_url`] resolves.
pub async fn maybe_connect_discovery_embedding_store(
) -> Option<std::sync::Arc<DiscoveryEmbeddingRepository>> {
    if std::env::var("PLASM_DISCOVERY_EMBEDDINGS_PG")
        .ok()
        .as_deref()
        == Some("0")
    {
        tracing::debug!("discovery embeddings: disabled via PLASM_DISCOVERY_EMBEDDINGS_PG=0");
        return None;
    }
    let db_url = mcp_config_database_url()?;
    match DiscoveryEmbeddingRepository::connect_and_migrate(&db_url).await {
        Ok(repo) => {
            tracing::info!(
                "discovery embeddings: postgres store enabled (CGS-versioned materialization)"
            );
            Some(std::sync::Arc::new(repo))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "discovery embeddings: postgres connect/migrate failed; typed discovery uses local embedder only"
            );
            None
        }
    }
}
