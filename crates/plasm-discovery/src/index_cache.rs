//! Cross-request cache for [`CatalogIndex`](crate::index::CatalogIndex) keyed by `(entry_id, catalog_cgs_hash)`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use plasm_core::schema::CGS;

use crate::index::CatalogIndex;
use crate::metrics;

/// Memoizes built discovery indexes per catalog snapshot.
#[derive(Debug, Default)]
pub struct CatalogIndexCache {
    inner: RwLock<HashMap<(String, String), Arc<CatalogIndex>>>,
}

impl CatalogIndexCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a cached index or build and store one for `(entry_id, catalog_cgs_hash)`.
    pub fn get_or_build(&self, entry_id: String, cgs: Arc<CGS>) -> Arc<CatalogIndex> {
        let hash = cgs.catalog_cgs_hash_hex();
        let key = (entry_id.clone(), hash);
        if let Some(idx) = self.inner.read().expect("index cache lock").get(&key) {
            metrics::record_index_cache("hit");
            return idx.clone();
        }
        let idx = Arc::new(CatalogIndex::build(entry_id, cgs));
        metrics::record_index_cache("miss");
        self.inner
            .write()
            .expect("index cache lock")
            .insert(key, idx.clone());
        idx
    }

    /// Clear all cached indexes (e.g. after catalog reload).
    pub fn clear(&self) {
        self.inner.write().expect("index cache lock").clear();
    }
}

#[cfg(test)]
mod cache_tests {
    use std::sync::Arc;

    use plasm_core::loader::load_schema_dir;

    use super::CatalogIndexCache;

    #[test]
    fn index_cache_returns_same_arc_on_second_build() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join("fixtures/schemas/pokeapi_mini");
        let cgs = Arc::new(load_schema_dir(&root).expect("pokeapi_mini"));
        let cache = CatalogIndexCache::new();
        let a = cache.get_or_build("pokeapi".into(), cgs.clone());
        let b = cache.get_or_build("pokeapi".into(), cgs);
        assert!(Arc::ptr_eq(&a, &b));
    }
}
