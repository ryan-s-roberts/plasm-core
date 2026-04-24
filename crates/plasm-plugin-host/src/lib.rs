//! Load compile-only Plasm plugins (`cdylib`) and build host-native compile closures for [`plasm_runtime::ExecuteOptions`]
//! (via [`plasm_compile::CompileOperationHook`] / [`plasm_compile::CompileQueryHook`]).
//!
//! See [`plasm_plugin_abi`] for the C symbol contract.

mod error;
mod ffi;
mod plugin_metadata;

pub use error::PluginLoadError;
pub use plasm_plugin_abi::PluginCatalogMetadata;
pub use plugin_metadata::load_catalog_metadata;

use plasm_compile::{compile_operation, compile_query, CompileOperationHook, CompileQueryHook};
use plasm_plugin_abi::PLASM_PLUGIN_ABI_VERSION;
use std::path::Path;
use std::sync::Arc;

/// One loaded plugin artifact; clone [`Arc`] into new execute sessions to pin compile behavior.
pub struct LoadedPluginGeneration {
    pub id: u64,
    /// Keeps dylib mapped; [`None`] for [`LoadedPluginGeneration::host_native`] (tests).
    _library: Option<Arc<libloading::Library>>,
    pub compile_operation_fn: Arc<CompileOperationHook>,
    pub compile_query_fn: Arc<CompileQueryHook>,
}

/// Tracks the current compile-plugin generation; on each [`Self::reload`], drops **manager** references
/// to prior generations so dylibs can unload once no execute session still holds a pinned [`Arc`].
pub struct PluginManager {
    inner: std::sync::Mutex<PluginManagerInner>,
}

struct PluginManagerInner {
    next_id: u64,
    current_id: u64,
    by_id: std::collections::HashMap<u64, Arc<LoadedPluginGeneration>>,
}

impl PluginManager {
    /// Load a plugin from `path` and make it the current generation.
    pub fn load(path: &Path) -> Result<Self, PluginLoadError> {
        let s = Self {
            inner: std::sync::Mutex::new(PluginManagerInner {
                next_id: 1,
                current_id: 0,
                by_id: std::collections::HashMap::new(),
            }),
        };
        s.reload(path)?;
        Ok(s)
    }

    /// Load another dylib; bumps generation. Prior generations are removed from the manager map so
    /// the shared library can unload when the last [`Arc<LoadedPluginGeneration>`] from an old session
    /// is dropped; [`Self::generation`] only resolves the **current** id after reload.
    pub fn reload(&self, path: &Path) -> Result<u64, PluginLoadError> {
        let mut st = self.inner.lock().expect("plugin manager mutex poisoned");
        let id = st.next_id;
        st.next_id = st.next_id.saturating_add(1);
        let gen = LoadedPluginGeneration::open(id, path)?;
        st.current_id = id;
        let arc = Arc::new(gen);
        st.by_id.clear();
        st.by_id.insert(id, arc);
        tracing::info!(generation = id, path = %path.display(), "loaded compile plugin");
        Ok(id)
    }

    pub fn current_generation(&self) -> Option<Arc<LoadedPluginGeneration>> {
        let st = self.inner.lock().expect("plugin manager mutex poisoned");
        st.by_id.get(&st.current_id).cloned()
    }

    /// Resolve a generation by id. After [`Self::reload`], only the **current** id remains in the
    /// manager map (older ids are dropped here even if sessions still hold a pinned [`Arc`]).
    pub fn generation(&self, id: u64) -> Option<Arc<LoadedPluginGeneration>> {
        let st = self.inner.lock().expect("plugin manager mutex poisoned");
        st.by_id.get(&id).cloned()
    }

    pub fn current_generation_id(&self) -> u64 {
        self.inner
            .lock()
            .expect("plugin manager mutex poisoned")
            .current_id
    }
}

impl LoadedPluginGeneration {
    /// Load a dylib; `id` is assigned by [`PluginManager`].
    pub fn open(id: u64, path: &Path) -> Result<Self, PluginLoadError> {
        let library = Arc::new(unsafe { libloading::Library::new(path)? });

        let ver: libloading::Symbol<unsafe extern "C" fn() -> u32> =
            unsafe { library.get(b"plasm_plugin_abi_version\0")? };
        let abi = unsafe { ver() };
        if abi != PLASM_PLUGIN_ABI_VERSION {
            return Err(PluginLoadError::AbiMismatch {
                expected: PLASM_PLUGIN_ABI_VERSION,
                got: abi,
            });
        }

        let compile_operation_fn = ffi::make_compile_operation_fn(library.clone())?;
        let compile_query_fn = ffi::make_compile_query_fn(library.clone())?;

        Ok(Self {
            id,
            _library: Some(library),
            compile_operation_fn,
            compile_query_fn,
        })
    }

    /// In-process compile (no dylib); for tests and embedding.
    pub fn host_native(id: u64) -> Self {
        Self {
            id,
            _library: None,
            compile_operation_fn: Arc::new(compile_operation),
            compile_query_fn: Arc::new(compile_query),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LoadedPluginGeneration, PluginManager};
    use plasm_compile::{parse_capability_template, CmlEnv};
    use plasm_plugin_stub as _;
    use serde_json::json;

    #[test]
    fn host_native_compiles_operation_equivalent() {
        let g = LoadedPluginGeneration::host_native(1);
        let template = json!({
            "method": "GET",
            "path": [
                {"type": "literal", "value": "v1"},
                {"type": "literal", "value": "x"},
                {"type": "var", "name": "id"}
            ]
        });
        let mut env = CmlEnv::new();
        env.insert("id".into(), plasm_core::Value::String("1".into()));
        let t = parse_capability_template(&template).expect("template");
        let a = (g.compile_operation_fn)(&t, &env).expect("plugin");
        let b = plasm_compile::compile_operation(&t, &env).expect("host");
        assert_eq!(a, b);
    }

    /// `cargo build -p plasm-plugin-stub` must be run first; skips if artifact missing.
    #[test]
    fn loads_plasm_plugin_stub_dylib_if_built() {
        use std::path::Path;
        let name = if cfg!(target_os = "macos") {
            "libplasm_plugin_stub.dylib"
        } else if cfg!(target_os = "windows") {
            "plasm_plugin_stub.dll"
        } else {
            "libplasm_plugin_stub.so"
        };
        let debug = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
        // `cargo test -p plasm-plugin-host` links the stub as a dev-dependency; the fresh cdylib
        // lands under `target/debug/deps/`. A copy in `target/debug/` only updates after
        // `cargo build -p plasm-plugin-stub`, so prefer `deps/` when present.
        let path = {
            let in_deps = debug.join("deps").join(name);
            if in_deps.exists() {
                in_deps
            } else {
                debug.join(name)
            }
        };
        if !path.exists() {
            eprintln!(
                "skip loads_plasm_plugin_stub_dylib_if_built: {} not found (cargo build -p plasm-plugin-stub)",
                path.display()
            );
            return;
        }
        let mgr = PluginManager::load(&path).expect("plugin load");
        let g = mgr.current_generation().expect("generation");
        assert_eq!(g.id, 1);
        let template = json!({
            "method": "GET",
            "path": [{"type":"literal","value":"x"}]
        });
        let env = CmlEnv::new();
        let t = parse_capability_template(&template).expect("parse");
        let _ = (g.compile_operation_fn)(&t, &env).expect("compile_op");
    }

    /// Same artifact path as [`loads_plasm_plugin_stub_dylib_if_built`]; asserts reload drops the
    /// manager's reference to the previous generation id.
    #[test]
    fn reload_drops_manager_reference_to_previous_generation() {
        use std::path::Path;
        let name = if cfg!(target_os = "macos") {
            "libplasm_plugin_stub.dylib"
        } else if cfg!(target_os = "windows") {
            "plasm_plugin_stub.dll"
        } else {
            "libplasm_plugin_stub.so"
        };
        let debug = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
        let path = {
            let in_deps = debug.join("deps").join(name);
            if in_deps.exists() {
                in_deps
            } else {
                debug.join(name)
            }
        };
        if !path.exists() {
            eprintln!(
                "skip reload_drops_manager_reference_to_previous_generation: {} not found",
                path.display()
            );
            return;
        }
        let mgr = PluginManager::load(&path).expect("plugin load");
        let first = mgr.current_generation_id();
        mgr.reload(&path).expect("reload");
        let second = mgr.current_generation_id();
        assert_ne!(first, second);
        assert!(mgr.generation(first).is_none());
        assert!(mgr.generation(second).is_some());
    }
}
