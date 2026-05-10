-- Precomputed vectors for typed discovery (`plasm-discovery` / fastembed), keyed by pinned CGS digest.
-- Requires Postgres with the pgvector extension (e.g. `pgvector/pgvector` images or `CREATE EXTENSION vector`).
-- Invalidation: new rows for each `catalog_cgs_hash`; optional GC deletes rows for hashes no longer in any registry.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS plasm_catalog_discovery_embeddings (
    catalog_cgs_hash TEXT NOT NULL,
    embedding_model_id TEXT NOT NULL,
    line_text TEXT NOT NULL,
    embedding_dim SMALLINT NOT NULL,
    embedding vector(384) NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (catalog_cgs_hash, embedding_model_id, line_text)
);

CREATE INDEX IF NOT EXISTS plasm_catalog_discovery_embeddings_model_hash
    ON plasm_catalog_discovery_embeddings (embedding_model_id, catalog_cgs_hash);
