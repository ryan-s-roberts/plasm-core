//! Build an [`plasm_core::discovery::InMemoryCgsRegistry`] from self-describing `cdylib` plugins (ABI v4).

use plasm_core::CgsCatalog;
use plasm_core::discovery::InMemoryCgsRegistry;
use plasm_core::schema::CGS;
use plasm_plugin_host::load_catalog_metadata;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

pub fn validate_registry_templates(reg: &InMemoryCgsRegistry) -> Result<(), String> {
    for meta in reg.list_entries() {
        let ctx = reg
            .load_context(&meta.entry_id)
            .map_err(|e| e.to_string())?;
        plasm_compile::validate_cgs_capability_templates(ctx.cgs.as_ref())
            .map_err(|e| format!("{}: {e}", meta.entry_id))?;
    }
    Ok(())
}

fn is_plugin_artifact(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("so" | "dylib" | "dll")
    )
}

/// Scan `dir` for plugin dylibs, select the highest `CGS.version` per `entry_id`, and build a catalog.
pub fn load_registry_from_plugin_dir(dir: &Path) -> Result<InMemoryCgsRegistry, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for ent in read {
        let ent = ent.map_err(|e| format!("read_dir: {e}"))?;
        let p = ent.path();
        if p.is_file() && is_plugin_artifact(&p) {
            paths.push(p);
        }
    }

    let target = env!("PLASM_HOST_TARGET_TRIPLE");
    let mut by_key: HashMap<(String, u64), (plasm_plugin_host::PluginCatalogMetadata, PathBuf)> =
        HashMap::new();

    for path in paths {
        let meta = load_catalog_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.target_triple != target {
            tracing::warn!(
                path = %path.display(),
                want = %target,
                got = %meta.target_triple,
                "skipping plugin: target triple mismatch"
            );
            continue;
        }

        let key = (meta.entry_id.clone(), meta.version);
        match by_key.entry(key) {
            Entry::Vacant(v) => {
                v.insert((meta, path));
            }
            Entry::Occupied(mut o) => {
                let (prev_meta, prev_path) = o.get();
                if prev_meta.cgs_hash != meta.cgs_hash {
                    return Err(format!(
                        "conflicting plugins for entry `{}` v{}: cgs_hash {} vs {}",
                        meta.entry_id, meta.version, prev_meta.cgs_hash, meta.cgs_hash
                    ));
                }
                if path.to_string_lossy() < prev_path.to_string_lossy() {
                    o.insert((meta, path));
                }
            }
        }
    }

    if by_key.is_empty() {
        return Err(format!(
            "no loadable plugins in `{}` for target `{}`",
            dir.display(),
            target
        ));
    }

    let mut winner: HashMap<String, (u64, plasm_plugin_host::PluginCatalogMetadata, PathBuf)> =
        HashMap::new();
    for ((eid, ver), (meta, path)) in by_key {
        match winner.entry(eid.clone()) {
            Entry::Vacant(v) => {
                v.insert((ver, meta, path));
            }
            Entry::Occupied(mut o) => {
                let (best_ver, _, _) = o.get();
                if ver > *best_ver {
                    o.insert((ver, meta, path));
                } else if ver == *best_ver {
                    return Err(format!(
                        "duplicate plugin version {ver} for entry `{eid}` after dedupe"
                    ));
                }
            }
        }
    }

    let mut ids: Vec<String> = winner.keys().cloned().collect();
    ids.sort();

    let mut pairs = Vec::new();
    for id in ids {
        let (_ver, meta, _path) = winner.remove(&id).expect("key exists");
        let cgs: CGS = serde_yaml::from_slice(&meta.cgs_yaml)
            .map_err(|e| format!("{}: parse embedded CGS: {e}", meta.entry_id))?;
        cgs.validate()
            .map_err(|e| format!("{}: {e}", meta.entry_id))?;
        let hash = cgs.catalog_cgs_hash_hex();
        if hash != meta.cgs_hash {
            return Err(format!(
                "cgs_hash mismatch for entry `{}`: metadata claims {}, parsed CGS is {}",
                meta.entry_id, meta.cgs_hash, hash
            ));
        }
        if cgs.version != meta.version {
            return Err(format!(
                "version mismatch for entry `{}`: metadata {}, CGS {}",
                meta.entry_id, meta.version, cgs.version
            ));
        }
        if cgs.entry_id.as_deref() != Some(meta.entry_id.as_str()) {
            return Err(format!(
                "entry_id mismatch for `{}`: metadata vs CGS {:?}",
                meta.entry_id, cgs.entry_id
            ));
        }

        let label = if meta.label.is_empty() {
            meta.entry_id.clone()
        } else {
            meta.label.clone()
        };

        pairs.push((meta.entry_id, label, meta.tags, std::sync::Arc::new(cgs)));
    }

    Ok(InMemoryCgsRegistry::from_pairs(pairs))
}
