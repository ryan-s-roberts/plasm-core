-- Discovery embeddings as BYTEA (serialized f32 LE). No pgvector extension.
-- Drops cached rows; reconcile repopulates.

DROP TABLE IF EXISTS plasm_catalog_discovery_embeddings;

DROP EXTENSION IF EXISTS vector;

CREATE TABLE IF NOT EXISTS plasm_catalog_discovery_embeddings (
    catalog_cgs_hash TEXT NOT NULL,
    embedding_model_id TEXT NOT NULL,
    line_text TEXT NOT NULL,
    embedding_dim SMALLINT NOT NULL,
    embedding BYTEA NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (catalog_cgs_hash, embedding_model_id, line_text)
);

CREATE INDEX IF NOT EXISTS plasm_catalog_discovery_embeddings_model_hash
    ON plasm_catalog_discovery_embeddings (embedding_model_id, catalog_cgs_hash);
