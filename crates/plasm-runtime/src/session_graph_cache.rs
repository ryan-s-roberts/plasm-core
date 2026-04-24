//! Session-scoped exclusive access to a [`GraphCache`] for async execute scopes.
//!
//! Callers hold one [`MutexGraphCacheSession`] per HTTP/MCP execute session (plasm-agent) instead of a
//! process-global mutex. See cache module invariants **I5** (single writer).
//!
//! Hold the [`MutexGuard`](tokio::sync::MutexGuard) from [`Self::lock`] across the full `execute` /
//! projection await chain (do not wrap `&mut GraphCache` in nested async closures).

use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

use crate::GraphCache;

/// Wraps `Arc<Mutex<GraphCache>>` so HTTP/MCP call sites do not thread `Arc<Mutex<…>>` through helpers.
///
/// Do **not** hold two concurrent locks on the same session (deadlock).
#[derive(Clone)]
pub struct MutexGraphCacheSession {
    inner: Arc<Mutex<GraphCache>>,
}

impl MutexGraphCacheSession {
    pub fn new(cache: GraphCache) -> Self {
        Self {
            inner: Arc::new(Mutex::new(cache)),
        }
    }

    /// Exclusive access; keep the guard alive across awaited `ExecutionEngine::execute` / projection work.
    pub async fn lock(&self) -> MutexGuard<'_, GraphCache> {
        self.inner.lock().await
    }

    /// Deep copy for parallel batch fork-merge (each line runs against this snapshot).
    pub async fn snapshot(&self) -> GraphCache {
        self.inner.lock().await.clone()
    }
}
