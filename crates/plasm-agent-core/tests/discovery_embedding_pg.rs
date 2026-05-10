//! Postgres integration for [`plasm_agent_core::discovery_embedding_repository`].
//!
//! Uses the [`pgvector/pgvector`](https://hub.docker.com/r/pgvector/pgvector) image so `CREATE EXTENSION vector` succeeds.
//!
//! ```text
//! cargo test -p plasm-agent-core --test discovery_embedding_pg -- --ignored --nocapture
//! ```

use std::time::Duration;

use plasm_agent_core::discovery_embedding_repository::DiscoveryEmbeddingRepository;
use plasm_discovery::embedding_store::{CatalogEmbeddingLineKey, CatalogEmbeddingStore};
use plasm_discovery::DEFAULT_EMBEDDING_VECTOR_DIM;
use testcontainers_modules::testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};

#[tokio::test]
#[ignore = "requires Docker (see module doc comment)"]
async fn discovery_embedding_fetch_roundtrip() {
    const START_TIMEOUT: Duration = Duration::from_secs(120);
    let image = GenericImage::new("pgvector/pgvector", "pg16-bookworm")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_wait_for(WaitFor::message_on_stdout(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_DB", "postgres")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres");

    let node = tokio::time::timeout(START_TIMEOUT, image.start())
        .await
        .expect("timeout starting postgres")
        .expect("docker postgres");

    let port = node.get_host_port_ipv4(5432).await.expect("host port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let repo = DiscoveryEmbeddingRepository::connect_and_migrate(&url)
        .await
        .expect("connect_and_migrate");

    let hash = "a".repeat(64);
    let model = plasm_discovery::DEFAULT_EMBEDDING_MODEL_ID;
    let line = "demo Monster Monster";
    let embedding: Vec<f32> = (0..DEFAULT_EMBEDDING_VECTOR_DIM)
        .map(|i| i as f32 * 0.0001)
        .collect();

    repo.upsert_line(&hash, model, line, &embedding)
        .await
        .expect("upsert");

    let key = CatalogEmbeddingLineKey::new(hash.clone(), line.to_string());
    let keys = vec![key.clone()];
    let map = repo.fetch_embeddings(model, &keys).await.expect("fetch");
    assert_eq!(map.len(), 1);
    assert_eq!(map[&key], embedding);
}

#[tokio::test]
#[ignore = "requires Docker (see module doc comment)"]
async fn discovery_embedding_fetch_duplicate_keys_returns_once() {
    const START_TIMEOUT: Duration = Duration::from_secs(120);
    let image = GenericImage::new("pgvector/pgvector", "pg16-bookworm")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_wait_for(WaitFor::message_on_stdout(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_DB", "postgres")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres");

    let node = tokio::time::timeout(START_TIMEOUT, image.start())
        .await
        .expect("timeout starting postgres")
        .expect("docker postgres");

    let port = node.get_host_port_ipv4(5432).await.expect("host port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let repo = DiscoveryEmbeddingRepository::connect_and_migrate(&url)
        .await
        .expect("connect_and_migrate");

    let hash = "b".repeat(64);
    let model = plasm_discovery::DEFAULT_EMBEDDING_MODEL_ID;
    let line = "dup key line";
    let embedding: Vec<f32> = (0..DEFAULT_EMBEDDING_VECTOR_DIM)
        .map(|i| (i + 1) as f32 * 0.0001)
        .collect();

    repo.upsert_line(&hash, model, line, &embedding)
        .await
        .expect("upsert");

    let key = CatalogEmbeddingLineKey::new(hash.clone(), line.to_string());
    let keys = vec![key.clone(), key.clone()];
    let map = repo.fetch_embeddings(model, &keys).await.expect("fetch");
    assert_eq!(map.len(), 1);
    assert_eq!(map[&key], embedding);
}
