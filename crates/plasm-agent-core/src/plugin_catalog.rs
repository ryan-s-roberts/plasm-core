//! Build an [`plasm_core::discovery::InMemoryCgsRegistry`] from self-describing `cdylib` plugins (ABI v4).

use plasm_core::discovery::InMemoryCgsRegistry;
use plasm_core::schema::CGS;
use plasm_core::CgsCatalog;
use plasm_plugin_host::load_catalog_metadata;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn validate_registry_templates(reg: &InMemoryCgsRegistry) -> Result<(), String> {
    validate_registry_templates_with_progress(reg, &mut |_: &str| {})
}

/// Like [`validate_registry_templates`], but emits short progress lines (bounded; suitable for TUIs).
pub fn validate_registry_templates_with_progress<P: FnMut(&str)>(
    reg: &InMemoryCgsRegistry,
    progress: &mut P,
) -> Result<(), String> {
    let metas = reg.list_entries();
    let n = metas.len();
    progress(&format!("validating capability templates ({n} entries)…"));
    for meta in metas {
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

fn stale_embedded_cgs_hint(detail: &str) -> &'static str {
    if detail.contains("value_ref") || detail.contains("input_type") {
        " Remove stale `libplasm_plugin_*` artifacts under the plugin dir or rebuild: `cargo run -p plasm --bin plasm-pack-plugins -- --workspace . --apis-root apis --output-dir target/plasm-plugins --force`"
    } else {
        ""
    }
}

fn skipped_plugin_warning(path: &Path, reason: &str) -> String {
    format!(
        "skipping invalid plugin artifact {}: {reason}",
        path.display()
    )
}

/// Scan `dir` for plugin dylibs, select the highest `CGS.version` per `entry_id`, and build a catalog.
pub fn load_registry_from_plugin_dir(dir: &Path) -> Result<InMemoryCgsRegistry, String> {
    load_registry_from_plugin_dir_with_progress(dir, &mut |_: &str| {})
}

/// Like [`load_registry_from_plugin_dir`], with progress callbacks (short lines; safe for alternate-screen TUIs).
pub fn load_registry_from_plugin_dir_with_progress<P: FnMut(&str)>(
    dir: &Path,
    progress: &mut P,
) -> Result<InMemoryCgsRegistry, String> {
    progress(&format!("scanning plugin-dir {}", dir.display()));
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
    let dylib_paths_seen = paths.len();
    progress(&format!(
        "found {dylib_paths_seen} plugin dylib(s) under dir; scanning for target {target}…"
    ));

    let mut by_key: HashMap<(String, u64), (plasm_plugin_host::PluginCatalogMetadata, PathBuf)> =
        HashMap::new();
    let mut skipped: Vec<String> = Vec::new();

    for path in paths {
        let meta = match load_catalog_metadata(&path) {
            Ok(meta) => meta,
            Err(e) => {
                let reason = format!("catalog metadata unavailable: {e}");
                let msg = skipped_plugin_warning(&path, &reason);
                tracing::warn!(path = %path.display(), error = %e, "skipping invalid plugin artifact");
                progress(&msg);
                skipped.push(msg);
                continue;
            }
        };
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

    progress(&format!(
        "{} versioned plugin candidate(s) after target filter",
        by_key.len()
    ));

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

    progress(&format!(
        "registry: {} catalog entry id(s) after version resolution",
        winner.len()
    ));

    let mut ids: Vec<String> = winner.keys().cloned().collect();
    ids.sort();

    progress("materializing CGS entries from embedded metadata…");

    let mut pairs = Vec::new();
    for id in ids {
        let (_ver, meta, path) = winner.remove(&id).expect("key exists");
        let cgs: CGS = match serde_yaml::from_slice(&meta.cgs_yaml) {
            Ok(cgs) => cgs,
            Err(e) => {
                let detail = e.to_string();
                let reason = format!(
                    "{}: parse embedded CGS: {}.{}",
                    meta.entry_id,
                    detail,
                    stale_embedded_cgs_hint(&detail)
                );
                let msg = skipped_plugin_warning(&path, &reason);
                tracing::warn!(
                    path = %path.display(),
                    entry_id = %meta.entry_id,
                    error = %detail,
                    "skipping plugin with invalid embedded CGS"
                );
                progress(&msg);
                skipped.push(msg);
                continue;
            }
        };
        if let Err(e) = cgs.validate() {
            let reason = format!("{}: {}", meta.entry_id, e);
            let msg = skipped_plugin_warning(&path, &reason);
            tracing::warn!(
                path = %path.display(),
                entry_id = %meta.entry_id,
                error = %e,
                "skipping plugin with invalid CGS"
            );
            progress(&msg);
            skipped.push(msg);
            continue;
        }
        let hash = cgs.catalog_cgs_hash_hex();
        if hash != meta.cgs_hash {
            let reason = format!(
                "cgs_hash mismatch for entry `{}`: metadata claims {}, parsed CGS is {}",
                meta.entry_id, meta.cgs_hash, hash
            );
            let msg = skipped_plugin_warning(&path, &reason);
            tracing::warn!(
                path = %path.display(),
                entry_id = %meta.entry_id,
                metadata_hash = %meta.cgs_hash,
                parsed_hash = %hash,
                "skipping plugin with mismatched embedded CGS hash"
            );
            progress(&msg);
            skipped.push(msg);
            continue;
        }
        if cgs.version != meta.version {
            let reason = format!(
                "version mismatch for entry `{}`: metadata {}, CGS {}",
                meta.entry_id, meta.version, cgs.version
            );
            let msg = skipped_plugin_warning(&path, &reason);
            tracing::warn!(
                path = %path.display(),
                entry_id = %meta.entry_id,
                metadata_version = meta.version,
                cgs_version = cgs.version,
                "skipping plugin with mismatched embedded CGS version"
            );
            progress(&msg);
            skipped.push(msg);
            continue;
        }
        if cgs.entry_id.as_deref() != Some(meta.entry_id.as_str()) {
            let reason = format!(
                "entry_id mismatch for `{}`: metadata vs CGS {:?}",
                meta.entry_id, cgs.entry_id
            );
            let msg = skipped_plugin_warning(&path, &reason);
            tracing::warn!(
                path = %path.display(),
                entry_id = %meta.entry_id,
                cgs_entry_id = ?cgs.entry_id,
                "skipping plugin with mismatched embedded entry_id"
            );
            progress(&msg);
            skipped.push(msg);
            continue;
        }

        let label = if meta.label.is_empty() {
            meta.entry_id.clone()
        } else {
            meta.label.clone()
        };

        pairs.push((meta.entry_id, label, meta.tags, std::sync::Arc::new(cgs)));
    }

    if pairs.is_empty() {
        let first = skipped
            .into_iter()
            .next()
            .unwrap_or_else(|| "no valid plugin artifacts remained after scan".into());
        return Err(format!(
            "no valid plugin catalogs in `{}` for target `{}`. {first}",
            dir.display(),
            target
        ));
    }

    if !skipped.is_empty() {
        progress(&format!(
            "skipped {} invalid plugin artifact(s); continuing with {} catalog entr{}",
            skipped.len(),
            pairs.len(),
            if pairs.len() == 1 { "y" } else { "ies" }
        ));
    }

    Ok(InMemoryCgsRegistry::from_pairs(pairs))
}
