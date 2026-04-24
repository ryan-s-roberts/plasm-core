//! Reference plugin: versioned CBOR frames (`plasm-plugin-abi`), delegates to [`plasm_compile::compile_operation`] / [`plasm_compile::compile_query`].
//!
//! Build: `cargo build -p plasm-plugin-stub` → `target/debug/libplasm_plugin_stub.{dylib,so,dll}`.

#![allow(clippy::missing_safety_doc)]

use plasm_compile::{compile_operation, compile_query};
use plasm_core::schema::CGS;
use plasm_plugin_abi::{
    decode_compile_operation_request, decode_compile_query_request, encode_catalog_metadata,
    encode_compile_operation_response, encode_compile_query_response, PluginCatalogMetadata,
    PluginCompileOperationResponse, PluginCompileQueryResponse, PLASM_PLUGIN_ABI_VERSION,
};

#[no_mangle]
pub unsafe extern "C" fn plasm_plugin_abi_version() -> u32 {
    PLASM_PLUGIN_ABI_VERSION
}

/// Release buffers returned to the host on success or error paths.
#[no_mangle]
pub unsafe extern "C" fn plasm_plugin_free_buffer(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(ptr, len, len));
}

fn leak_response_bytes(mut v: Vec<u8>) -> (*mut u8, usize) {
    v.shrink_to_fit();
    debug_assert_eq!(v.len(), v.capacity());
    let len = v.len();
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    (ptr, len)
}

unsafe fn write_err(msg: String, err_ptr: *mut *mut u8, err_len: *mut usize) -> i32 {
    let mut v = msg.into_bytes();
    v.shrink_to_fit();
    debug_assert_eq!(v.len(), v.capacity());
    let len = v.len();
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    *err_ptr = ptr;
    *err_len = len;
    1
}

/// Interchange bytes copied into `OUT_DIR/catalog.cgs.yaml` by `build.rs`
/// (either `PLASM_EMBEDDED_CGS` or the default fixture).
const EMBEDDED_CGS_YAML: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/catalog.cgs.yaml"));

// Changing `catalog.cgs.yaml` must recompile this crate so `include_bytes!` is not stale
// (see `build.rs` `PLASM_PLUGIN_EMBEDDED_CGS_HASH`).
#[allow(dead_code)]
const EMBEDDED_CGS_DIGEST_AT_COMPILE: &str = env!("PLASM_PLUGIN_EMBEDDED_CGS_HASH");

fn build_catalog_metadata() -> Result<Vec<u8>, String> {
    let cgs: CGS = serde_yaml::from_slice(EMBEDDED_CGS_YAML).map_err(|e| e.to_string())?;
    let cgs_hash = cgs.catalog_cgs_hash_hex();
    let entry_id = cgs
        .entry_id
        .clone()
        .ok_or_else(|| "CGS.entry_id must be set for plugin catalog metadata".to_string())?;
    if cgs.version == 0 {
        return Err("CGS.version must be non-zero for plugin catalog metadata".into());
    }
    let label = if entry_id == "stub_test" {
        "stub_test".into()
    } else {
        entry_id.clone()
    };
    let tags = if entry_id == "stub_test" {
        vec!["plugin".into(), "stub".into()]
    } else {
        vec!["plugin".into(), "apis-pack".into()]
    };
    let meta = PluginCatalogMetadata {
        entry_id,
        version: cgs.version,
        cgs_hash,
        target_triple: env!("PLASM_PLUGIN_TARGET_TRIPLE").to_string(),
        cgs_yaml: EMBEDDED_CGS_YAML.to_vec(),
        label,
        tags,
    };
    encode_catalog_metadata(&meta).map_err(|e| e.to_string())
}

#[no_mangle]
pub unsafe extern "C" fn plasm_plugin_catalog_metadata(
    _req: *const u8,
    _req_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    err_ptr: *mut *mut u8,
    err_len: *mut usize,
) -> i32 {
    let bytes = match build_catalog_metadata() {
        Ok(b) => b,
        Err(e) => return write_err(e, err_ptr, err_len),
    };
    let (p, l) = leak_response_bytes(bytes);
    *out_ptr = p;
    *out_len = l;
    0
}

#[no_mangle]
pub unsafe extern "C" fn plasm_plugin_compile_operation(
    req: *const u8,
    req_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    err_ptr: *mut *mut u8,
    err_len: *mut usize,
) -> i32 {
    let req_slice = std::slice::from_raw_parts(req, req_len);
    let parsed = match decode_compile_operation_request(req_slice) {
        Ok(x) => x,
        Err(e) => return write_err(e.to_string(), err_ptr, err_len),
    };
    let compiled = match compile_operation(&parsed.template, &parsed.cml_env) {
        Ok(c) => c,
        Err(e) => return write_err(e.to_string(), err_ptr, err_len),
    };
    let resp = PluginCompileOperationResponse { compiled };
    let bytes = match encode_compile_operation_response(&resp) {
        Ok(b) => b,
        Err(e) => return write_err(e.to_string(), err_ptr, err_len),
    };
    let (p, l) = leak_response_bytes(bytes);
    *out_ptr = p;
    *out_len = l;
    0
}

#[no_mangle]
pub unsafe extern "C" fn plasm_plugin_compile_query(
    req: *const u8,
    req_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    err_ptr: *mut *mut u8,
    err_len: *mut usize,
) -> i32 {
    let req_slice = std::slice::from_raw_parts(req, req_len);
    let parsed = match decode_compile_query_request(req_slice) {
        Ok(x) => x,
        Err(e) => return write_err(e.to_string(), err_ptr, err_len),
    };
    let filter = match compile_query(&parsed.query, &parsed.cgs) {
        Ok(f) => f,
        Err(e) => return write_err(e.to_string(), err_ptr, err_len),
    };
    let resp = PluginCompileQueryResponse { filter };
    let bytes = match encode_compile_query_response(&resp) {
        Ok(b) => b,
        Err(e) => return write_err(e.to_string(), err_ptr, err_len),
    };
    let (p, l) = leak_response_bytes(bytes);
    *out_ptr = p;
    *out_len = l;
    0
}
