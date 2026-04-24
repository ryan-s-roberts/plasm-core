//! Point-in-time snapshots of execute runs for `GET /execute/.../artifacts/:run_id` and MCP `resources/read`.
//!
//! Storage backends:
//! - **In-memory** (default): [`RunArtifactStore::memory`].
//! - **Object store** (optional): set **`PLASM_RUN_ARTIFACTS_URL`** to an [`object_store`] URL (e.g. `s3://bucket/prefix`, `file:///path/to/dir`).
//!   Time-based GC deletes objects older than **`PLASM_RUN_ARTIFACTS_RETENTION_SECS`** using each object’s **`last_modified`** (objects without it are left in place).

use async_trait::async_trait;
use axum::body::Bytes;
use futures_util::TryStreamExt;
use object_store::{path::Path as StorePath, ObjectStore, ObjectStoreExt};
use plasm_runtime::{ExecutionResult, ExecutionSource, ExecutionStats};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

/// Handle for a stored run snapshot (HTTP path + MCP resource URI).
#[derive(Debug, Clone)]
pub struct RunArtifactHandle {
    pub run_id: Uuid,
    /// LLM-facing short URI (`plasm://r/{n}`), valid with MCP `resources/read` while the same execute session is bound.
    pub plasm_uri: String,
    /// Canonical long URI (`plasm://execute/.../run/{run_id}`) for logs and HTTP-adjacent tools.
    pub canonical_plasm_uri: String,
    pub http_path: String,
    pub payload_len: usize,
    pub request_fingerprints: Vec<String>,
}

/// Payload metadata for cache deltas / run artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPayloadMetadata {
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,
    pub schema_version: u32,
    pub producer: String,
}

impl ArtifactPayloadMetadata {
    pub fn json_default() -> Self {
        Self {
            content_type: "application/json".into(),
            content_encoding: None,
            schema_version: 1,
            producer: "plasm-agent".into(),
        }
    }
}

/// Opaque artifact bytes plus explicit metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPayload {
    pub metadata: ArtifactPayloadMetadata,
    pub bytes: Bytes,
}

/// JSON document returned by artifact GET and MCP `resources/read`.
#[derive(Debug, Serialize)]
pub struct RunArtifactDocument {
    pub run_id: String,
    pub prompt_hash: String,
    pub session_id: String,
    pub entry_id: String,
    /// Monotonic per `(prompt_hash, session_id)` execute session; drives `plasm://r/{n}` and archive index lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
    pub expressions: Vec<String>,
    pub request_fingerprints: Vec<String>,
    pub entities: Vec<serde_json::Value>,
    pub source: ExecutionSource,
    pub stats: ExecutionStats,
}

#[derive(Debug, thiserror::Error)]
pub enum RunArtifactError {
    #[error("run artifact JSON: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("run artifact decode: {0}")]
    Decode(String),
    #[error("run artifact object store: {0}")]
    ObjectStore(String),
}

#[async_trait]
pub trait RunArtifactBackend: Send + Sync {
    async fn insert_encoded(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
        encoded: Vec<u8>,
    ) -> Result<usize, RunArtifactError>;

    async fn get_encoded(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
    ) -> Option<Vec<u8>>;

    /// Persist `resource_index → run_id` under the same session prefix as blob artifacts.
    async fn put_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
        run_id: Uuid,
    ) -> Result<(), RunArtifactError>;

    async fn get_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
    ) -> Option<Uuid>;
}

/// Execute run snapshot storage (memory or object store).
#[derive(Clone)]
pub struct RunArtifactStore {
    inner: Arc<dyn RunArtifactBackend>,
}

impl RunArtifactStore {
    pub fn memory() -> Self {
        Self {
            inner: Arc::new(MemoryRunArtifactBackend::default()),
        }
    }

    pub async fn insert(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
        doc: &RunArtifactDocument,
    ) -> Result<usize, RunArtifactError> {
        let bytes = serde_json::to_vec(doc)?;
        self.insert_payload(
            prompt_hash,
            session_id,
            run_id,
            doc.resource_index,
            &ArtifactPayload {
                metadata: ArtifactPayloadMetadata::json_default(),
                bytes: bytes.into(),
            },
        )
        .await
    }

    pub async fn insert_payload(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
        resource_index: Option<u64>,
        payload: &ArtifactPayload,
    ) -> Result<usize, RunArtifactError> {
        let encoded = encode_payload(payload)?;
        let n = self
            .inner
            .insert_encoded(prompt_hash, session_id, run_id, encoded)
            .await?;
        if let Some(idx) = resource_index {
            self.inner
                .put_run_id_for_resource_index(prompt_hash, session_id, idx, run_id)
                .await?;
        }
        Ok(n)
    }

    pub async fn get_payload(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
    ) -> Option<ArtifactPayload> {
        self.get_payload_result(prompt_hash, session_id, run_id)
            .await
            .ok()
            .flatten()
    }

    pub async fn get_payload_result(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
    ) -> Result<Option<ArtifactPayload>, RunArtifactError> {
        let encoded = self
            .inner
            .get_encoded(prompt_hash, session_id, run_id)
            .await;
        match encoded {
            Some(bytes) => decode_payload(&bytes).map(Some),
            None => Ok(None),
        }
    }

    pub async fn get(&self, prompt_hash: &str, session_id: &str, run_id: Uuid) -> Option<Vec<u8>> {
        let payload = self.get_payload(prompt_hash, session_id, run_id).await?;
        Some(payload.bytes.to_vec())
    }

    pub async fn get_payload_result_by_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
    ) -> Result<Option<ArtifactPayload>, RunArtifactError> {
        let Some(run_id) = self
            .inner
            .get_run_id_for_resource_index(prompt_hash, session_id, resource_index)
            .await
        else {
            return Ok(None);
        };
        self.get_payload_result(prompt_hash, session_id, run_id)
            .await
    }

    /// Resolve canonical `run_id` for a short-URI resource index (archive / object-store mapping).
    pub async fn resolve_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
    ) -> Option<Uuid> {
        self.inner
            .get_run_id_for_resource_index(prompt_hash, session_id, resource_index)
            .await
    }
}

impl Default for RunArtifactStore {
    fn default() -> Self {
        Self::memory()
    }
}

#[derive(Debug, Default)]
struct MemoryRunArtifactState {
    blobs: HashMap<(String, String, Uuid), Vec<u8>>,
    by_resource_index: HashMap<(String, String, u64), Uuid>,
}

#[derive(Debug, Default)]
struct MemoryRunArtifactBackend {
    inner: std::sync::RwLock<MemoryRunArtifactState>,
}

#[async_trait]
impl RunArtifactBackend for MemoryRunArtifactBackend {
    async fn insert_encoded(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
        encoded: Vec<u8>,
    ) -> Result<usize, RunArtifactError> {
        let n = encoded.len();
        let mut g = self.inner.write().expect("run artifact mutex poisoned");
        g.blobs.insert(
            (prompt_hash.to_string(), session_id.to_string(), run_id),
            encoded,
        );
        Ok(n)
    }

    async fn get_encoded(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
    ) -> Option<Vec<u8>> {
        let g = self.inner.read().ok()?;
        g.blobs
            .get(&(prompt_hash.to_string(), session_id.to_string(), run_id))
            .cloned()
    }

    async fn put_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
        run_id: Uuid,
    ) -> Result<(), RunArtifactError> {
        let mut g = self.inner.write().expect("run artifact mutex poisoned");
        g.by_resource_index.insert(
            (
                prompt_hash.to_string(),
                session_id.to_string(),
                resource_index,
            ),
            run_id,
        );
        Ok(())
    }

    async fn get_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
    ) -> Option<Uuid> {
        let g = self.inner.read().ok()?;
        g.by_resource_index
            .get(&(
                prompt_hash.to_string(),
                session_id.to_string(),
                resource_index,
            ))
            .copied()
    }
}

struct ObjectStoreRunArtifactBackend {
    store: Arc<dyn ObjectStore>,
    prefix: StorePath,
}

#[async_trait]
impl RunArtifactBackend for ObjectStoreRunArtifactBackend {
    async fn insert_encoded(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
        encoded: Vec<u8>,
    ) -> Result<usize, RunArtifactError> {
        let n = encoded.len();
        let key = artifact_object_key(&self.prefix, prompt_hash, session_id, run_id);
        self.store
            .put(&key, encoded.into())
            .await
            .map_err(|e| RunArtifactError::ObjectStore(e.to_string()))?;
        Ok(n)
    }

    async fn get_encoded(
        &self,
        prompt_hash: &str,
        session_id: &str,
        run_id: Uuid,
    ) -> Option<Vec<u8>> {
        let key = artifact_object_key(&self.prefix, prompt_hash, session_id, run_id);
        let res = self.store.get(&key).await.ok()?;
        res.bytes().await.ok().map(|b| b.to_vec())
    }

    async fn put_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
        run_id: Uuid,
    ) -> Result<(), RunArtifactError> {
        let key = resource_index_pointer_key(&self.prefix, prompt_hash, session_id, resource_index);
        let body = run_id.as_hyphenated().to_string();
        self.store
            .put(&key, body.into_bytes().into())
            .await
            .map_err(|e| RunArtifactError::ObjectStore(e.to_string()))?;
        Ok(())
    }

    async fn get_run_id_for_resource_index(
        &self,
        prompt_hash: &str,
        session_id: &str,
        resource_index: u64,
    ) -> Option<Uuid> {
        let key = resource_index_pointer_key(&self.prefix, prompt_hash, session_id, resource_index);
        let res = self.store.get(&key).await.ok()?;
        let bytes = res.bytes().await.ok()?;
        let s = std::str::from_utf8(bytes.as_ref()).ok()?;
        Uuid::parse_str(s.trim()).ok()
    }
}

fn artifact_object_key(
    prefix: &StorePath,
    prompt_hash: &str,
    session_id: &str,
    run_id: Uuid,
) -> StorePath {
    prefix
        .clone()
        .join("execute")
        .join(prompt_hash)
        .join(session_id)
        .join(format!("{run_id}.artifact"))
}

fn resource_index_pointer_key(
    prefix: &StorePath,
    prompt_hash: &str,
    session_id: &str,
    resource_index: u64,
) -> StorePath {
    prefix
        .clone()
        .join("execute")
        .join(prompt_hash)
        .join(session_id)
        .join("_index")
        .join(format!("{resource_index}.run_id"))
}

const ARTIFACT_MAGIC: &[u8] = b"PLAR1\n";

fn encode_payload(payload: &ArtifactPayload) -> Result<Vec<u8>, RunArtifactError> {
    let meta = serde_json::to_vec(&payload.metadata)?;
    let mut out = Vec::with_capacity(ARTIFACT_MAGIC.len() + 4 + meta.len() + payload.bytes.len());
    out.extend_from_slice(ARTIFACT_MAGIC);
    out.extend_from_slice(&(meta.len() as u32).to_be_bytes());
    out.extend_from_slice(&meta);
    out.extend_from_slice(payload.bytes.as_ref());
    Ok(out)
}

fn decode_payload(encoded: &[u8]) -> Result<ArtifactPayload, RunArtifactError> {
    let header = ARTIFACT_MAGIC.len() + 4;
    if encoded.len() < header || &encoded[..ARTIFACT_MAGIC.len()] != ARTIFACT_MAGIC {
        return Err(RunArtifactError::Decode(
            "invalid artifact framing header".into(),
        ));
    }
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&encoded[ARTIFACT_MAGIC.len()..header]);
    let meta_len = u32::from_be_bytes(len_bytes) as usize;
    if encoded.len() < header + meta_len {
        return Err(RunArtifactError::Decode(
            "invalid artifact framing metadata length".into(),
        ));
    }
    let metadata: ArtifactPayloadMetadata =
        serde_json::from_slice(&encoded[header..header + meta_len])?;
    let bytes = Bytes::copy_from_slice(&encoded[header + meta_len..]);
    Ok(ArtifactPayload { metadata, bytes })
}

/// Build [`RunArtifactStore`] from environment, or in-memory when **`PLASM_RUN_ARTIFACTS_URL`** is unset.
///
/// - **`PLASM_RUN_ARTIFACTS_URL`**: [`object_store::parse_url_opts`] URL (credentials via standard env for each cloud).
/// - **`PLASM_RUN_ARTIFACTS_RETENTION_SECS`**: delete objects with `last_modified` older than this (default **604800** = 7 days).
/// - **`PLASM_RUN_ARTIFACTS_GC_INTERVAL_SECS`**: background GC period (default **300**).
pub fn init_from_env() -> Result<Arc<RunArtifactStore>, String> {
    let url_raw = match std::env::var("PLASM_RUN_ARTIFACTS_URL") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => {
            tracing::warn!(
                target: "plasm_agent::run_artifacts",
                "PLASM_RUN_ARTIFACTS_URL unset: using in-process memory for execute run snapshots (not suitable for production horizontal scale; set PLASM_RUN_ARTIFACTS_URL to an object-store URL)"
            );
            return Ok(Arc::new(RunArtifactStore::memory()));
        }
    };
    let url = url::Url::parse(&url_raw)
        .map_err(|e| format!("PLASM_RUN_ARTIFACTS_URL is not a valid URL: {e}"))?;
    let (boxed, prefix) = object_store::parse_url_opts(&url, std::env::vars())
        .map_err(|e| format!("PLASM_RUN_ARTIFACTS_URL could not open object store: {e}"))?;
    let store: Arc<dyn ObjectStore> = Arc::from(boxed);
    let retention = retention_from_env();
    let interval = gc_interval_from_env();
    let backend = Arc::new(ObjectStoreRunArtifactBackend {
        store: store.clone(),
        prefix: prefix.clone(),
    });
    spawn_run_artifact_gc_task(store, prefix, retention, interval);
    tracing::info!(
        retention_secs = retention.as_secs(),
        gc_interval_secs = interval.as_secs(),
        "run artifacts: object store backend (time-based GC)"
    );
    Ok(Arc::new(RunArtifactStore { inner: backend }))
}

fn retention_from_env() -> Duration {
    let secs: u64 = std::env::var("PLASM_RUN_ARTIFACTS_RETENTION_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(604_800);
    Duration::from_secs(secs.max(60))
}

fn gc_interval_from_env() -> Duration {
    let secs: u64 = std::env::var("PLASM_RUN_ARTIFACTS_GC_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    Duration::from_secs(secs.max(60))
}

fn spawn_run_artifact_gc_task(
    store: Arc<dyn ObjectStore>,
    list_prefix: StorePath,
    retention: Duration,
    interval: Duration,
) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = run_artifact_gc_pass(store.as_ref(), &list_prefix, retention).await {
                tracing::warn!(error = %e, "run artifact GC pass failed");
            }
        }
    });
}

async fn run_artifact_gc_pass(
    store: &dyn ObjectStore,
    list_prefix: &StorePath,
    retention: Duration,
) -> Result<(), object_store::Error> {
    use chrono::Utc;
    let secs = retention.as_secs().min(i64::MAX as u64) as i64;
    let cutoff = Utc::now() - chrono::Duration::seconds(secs);
    let mut stream = store.list(Some(list_prefix));
    while let Some(meta) = stream.try_next().await? {
        if meta.last_modified < cutoff {
            store.delete(&meta.location).await?;
            tracing::debug!(path = %meta.location, "run artifact GC deleted object");
        }
    }
    Ok(())
}

/// Canonical MCP / logical URI for a run artifact.
pub fn plasm_run_resource_uri(prompt_hash: &str, session_id: &str, run_id: &Uuid) -> String {
    format!("plasm://execute/{prompt_hash}/{session_id}/run/{run_id}")
}

/// Short LLM-facing URI; resolve via MCP `resources/read` using the bound execute session (HTTP / legacy).
pub fn plasm_short_resource_uri(resource_index: u64) -> String {
    format!("plasm://r/{resource_index}")
}

/// Short URI scoped to an MCP **logical session** (agent identity), not transport.
/// `session_segment` is the client-facing slot id (`s0`, `s1`, …) or a canonical UUID string.
pub fn plasm_session_short_resource_uri(session_segment: &str, resource_index: u64) -> String {
    format!("plasm://session/{session_segment}/r/{resource_index}")
}

/// Legacy helper: embed canonical logical session UUID in the short resource URI.
pub fn plasm_short_resource_uri_logical(logical_session_id: &Uuid, resource_index: u64) -> String {
    plasm_session_short_resource_uri(&logical_session_id.to_string(), resource_index)
}

/// First path segment after `plasm://session/` for short run resources: UUID **or** slot `s` + digits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogicalSessionUriSegment {
    Uuid(Uuid),
    Slot(String),
}

/// Parse `plasm://r/{decimal}` (no extra path segments).
pub fn parse_plasm_short_resource_uri(uri: &str) -> Option<u64> {
    let rest = uri.strip_prefix("plasm://r/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    if !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// Parse `plasm://session/{uuid}/r/{decimal}` or `plasm://session/s{n}/r/{decimal}` (per-transport slot).
pub fn parse_plasm_session_short_resource_uri(
    uri: &str,
) -> Option<(LogicalSessionUriSegment, u64)> {
    let rest = uri.strip_prefix("plasm://session/")?;
    let mut parts = rest.split('/').filter(|s| !s.is_empty());
    let seg = parts.next()?;
    let segment = if let Ok(u) = Uuid::parse_str(seg) {
        LogicalSessionUriSegment::Uuid(u)
    } else if seg.len() >= 2 && seg.starts_with('s') && seg[1..].chars().all(|c| c.is_ascii_digit())
    {
        LogicalSessionUriSegment::Slot(seg.to_string())
    } else {
        return None;
    };
    let r = parts.next()?;
    let n = parts.next()?;
    if r != "r" {
        return None;
    }
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if parts.next().is_some() {
        return None;
    }
    let idx: u64 = n.parse().ok()?;
    Some((segment, idx))
}

pub fn artifact_http_path(prompt_hash: &str, session_id: &str, run_id: &Uuid) -> String {
    format!("/execute/{prompt_hash}/{session_id}/artifacts/{run_id}")
}

/// Parse `plasm://execute/{prompt_hash}/{session_id}/run/{run_id}`.
pub fn parse_plasm_execute_run_uri(uri: &str) -> Option<(String, String, Uuid)> {
    let rest = uri.strip_prefix("plasm://execute/")?;
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() != 4 || parts[2] != "run" {
        return None;
    }
    let run_id = Uuid::parse_str(parts[3]).ok()?;
    Some((parts[0].to_string(), parts[1].to_string(), run_id))
}

/// Arguments for [`document_from_run`].
pub struct DocumentFromRun<'a> {
    pub run_id: Uuid,
    pub prompt_hash: &'a str,
    pub session_id: &'a str,
    pub entry_id: &'a str,
    pub principal: Option<String>,
    pub expressions: Vec<String>,
    pub result: &'a ExecutionResult,
    pub resource_index: Option<u64>,
}

pub fn document_from_run(d: DocumentFromRun<'_>) -> RunArtifactDocument {
    let entities: Vec<serde_json::Value> = d
        .result
        .entities
        .iter()
        .map(|e| e.payload_to_json())
        .collect();
    RunArtifactDocument {
        run_id: d.run_id.to_string(),
        prompt_hash: d.prompt_hash.to_string(),
        session_id: d.session_id.to_string(),
        entry_id: d.entry_id.to_string(),
        resource_index: d.resource_index,
        principal: d.principal,
        expressions: d.expressions,
        request_fingerprints: d.result.request_fingerprints.clone(),
        entities,
        source: d.result.source,
        stats: d.result.stats.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plasm_run_uri_round_trip() {
        let id = Uuid::nil();
        let ph64 = "ab".repeat(32);
        let uri = plasm_run_resource_uri(&ph64, "sess01", &id);
        let (ph, sid, rid) = parse_plasm_execute_run_uri(&uri).expect("parse");
        assert_eq!(ph, "ab".repeat(32));
        assert_eq!(sid, "sess01");
        assert_eq!(rid, id);
    }

    #[tokio::test]
    async fn memory_insert_get_round_trip() {
        let store = RunArtifactStore::memory();
        let run_id = Uuid::new_v4();
        let doc = RunArtifactDocument {
            run_id: run_id.to_string(),
            prompt_hash: "p".repeat(64),
            session_id: "s1".into(),
            entry_id: "e".into(),
            resource_index: Some(1),
            principal: None,
            expressions: vec![],
            request_fingerprints: vec![],
            entities: vec![],
            source: ExecutionSource::Live,
            stats: ExecutionStats {
                duration_ms: 0,
                network_requests: 0,
                cache_hits: 0,
                cache_misses: 0,
            },
        };
        let n = store
            .insert(&"p".repeat(64), "s1", run_id, &doc)
            .await
            .expect("insert");
        assert!(n > 0);
        let bytes = store.get(&"p".repeat(64), "s1", run_id).await.expect("get");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["run_id"], run_id.to_string());
    }

    #[tokio::test]
    async fn memory_insert_get_payload_round_trip_binary() {
        let store = RunArtifactStore::memory();
        let run_id = Uuid::new_v4();
        let payload = ArtifactPayload {
            metadata: ArtifactPayloadMetadata {
                content_type: "application/x-plasm-test".into(),
                content_encoding: Some("identity".into()),
                schema_version: 7,
                producer: "unit-test".into(),
            },
            bytes: Bytes::from_static(&[0, 1, 2, 3, 254, 255]),
        };
        store
            .insert_payload(&"p".repeat(64), "s1", run_id, Some(7), &payload)
            .await
            .expect("insert");
        let got = store
            .get_payload(&"p".repeat(64), "s1", run_id)
            .await
            .expect("payload");
        assert_eq!(got, payload);
        let by_idx = store
            .get_payload_result_by_resource_index(&"p".repeat(64), "s1", 7)
            .await
            .expect("by index")
            .expect("some");
        assert_eq!(by_idx, payload);
    }

    #[test]
    fn parse_short_plasm_resource_uri() {
        assert_eq!(parse_plasm_short_resource_uri("plasm://r/42"), Some(42));
        assert_eq!(parse_plasm_short_resource_uri("plasm://r/0"), Some(0));
        assert!(parse_plasm_short_resource_uri("plasm://r/").is_none());
        assert!(parse_plasm_short_resource_uri("plasm://r/x").is_none());
        assert!(parse_plasm_short_resource_uri("plasm://execute/a/b/run/u").is_none());
    }

    #[test]
    fn parse_logical_short_plasm_resource_uri_round_trip() {
        let id = Uuid::nil();
        let u = plasm_short_resource_uri_logical(&id, 7);
        assert_eq!(
            parse_plasm_session_short_resource_uri(&u),
            Some((LogicalSessionUriSegment::Uuid(id), 7))
        );
        let u2 = plasm_session_short_resource_uri("s3", 7);
        assert_eq!(
            parse_plasm_session_short_resource_uri(&u2),
            Some((LogicalSessionUriSegment::Slot("s3".into()), 7))
        );
        assert!(parse_plasm_session_short_resource_uri("plasm://session/not-uuid/r/1").is_none());
        assert!(parse_plasm_session_short_resource_uri("plasm://session/s/r/1").is_none());
    }
}
