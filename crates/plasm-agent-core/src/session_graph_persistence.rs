//! Persistent session graph cache substrate (append deltas + snapshot manifests).
//!
//! Backends:
//! - Inactive when `PLASM_GRAPH_CACHE_URL` is unset.
//! - Object-store backed when `PLASM_GRAPH_CACHE_URL` is set (`object_store::parse_url_opts`).

use object_store::WriteMultipart;
use object_store::{ObjectStore, ObjectStoreExt, path::Path as StorePath};
use plasm_runtime::GraphCache;
use serde::Serialize;
use std::sync::Arc;

use crate::run_artifacts::ArtifactPayload;

#[derive(Clone)]
pub struct SessionGraphPersistence {
    store: Arc<dyn ObjectStore>,
    sessions_root: StorePath,
}

#[derive(Debug, Serialize)]
pub struct SnapshotManifest {
    pub through_seq: u64,
    pub snapshot_content_type: String,
    pub snapshot_key: String,
}

impl SessionGraphPersistence {
    pub fn new(store: Arc<dyn ObjectStore>, prefix: StorePath) -> Self {
        Self {
            store,
            sessions_root: prefix.join("v1").join("sessions"),
        }
    }

    pub async fn append_delta(
        &self,
        prompt_hash: &str,
        session_id: &str,
        seq: u64,
        payload: &ArtifactPayload,
    ) -> Result<(), String> {
        let key = self
            .sessions_root
            .clone()
            .join(prompt_hash)
            .join(session_id)
            .join("delta")
            .join(format!("{seq:020}.bin"));
        let mut framed = Vec::with_capacity(256 + payload.bytes.len());
        let metadata = serde_json::to_vec(&payload.metadata).map_err(|e| e.to_string())?;
        framed.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
        framed.extend_from_slice(&metadata);
        framed.extend_from_slice(&payload.bytes);
        self.store
            .put(&key, framed.into())
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn write_snapshot(
        &self,
        prompt_hash: &str,
        session_id: &str,
        through_seq: u64,
        content_type: &str,
        cache: &GraphCache,
    ) -> Result<(), String> {
        let snapshot_key = self
            .sessions_root
            .clone()
            .join(prompt_hash)
            .join(session_id)
            .join("snapshots")
            .join(format!("{through_seq:020}.bin"));

        // Stream JSON array rows directly to object storage chunks; avoids one giant snapshot Vec.
        let upload = self
            .store
            .put_multipart(&snapshot_key)
            .await
            .map_err(|e| e.to_string())?;
        let mut writer = WriteMultipart::new(upload);
        writer.write(b"[");
        let mut first = true;
        let mut scratch = Vec::with_capacity(4096);
        for r in cache.all_references() {
            if let Ok(v) = cache.entity_to_json(r) {
                if !first {
                    writer.write(b",");
                }
                first = false;
                scratch.clear();
                serde_json::to_writer(&mut scratch, &v).map_err(|e| e.to_string())?;
                writer.write(&scratch);
            }
        }
        writer.write(b"]");
        writer.finish().await.map_err(|e| e.to_string())?;

        let manifest = SnapshotManifest {
            through_seq,
            snapshot_content_type: content_type.to_string(),
            snapshot_key: snapshot_key.to_string(),
        };
        let manifest_key = self
            .sessions_root
            .clone()
            .join(prompt_hash)
            .join(session_id)
            .join("manifest.json");
        let bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        self.store
            .put(&manifest_key, bytes.into())
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn init_from_env() -> Result<Option<Arc<SessionGraphPersistence>>, String> {
    let url_raw = match std::env::var("PLASM_GRAPH_CACHE_URL") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => return Ok(None),
    };
    let url =
        url::Url::parse(&url_raw).map_err(|e| format!("PLASM_GRAPH_CACHE_URL invalid URL: {e}"))?;
    let (boxed, prefix) = object_store::parse_url_opts(&url, std::env::vars())
        .map_err(|e| format!("PLASM_GRAPH_CACHE_URL could not open object store: {e}"))?;
    let store: Arc<dyn ObjectStore> = Arc::from(boxed);
    Ok(Some(Arc::new(SessionGraphPersistence::new(store, prefix))))
}
