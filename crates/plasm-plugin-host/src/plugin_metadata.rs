//! Load [`plasm_plugin_abi::PluginCatalogMetadata`] from a `cdylib` without retaining the library.

use crate::PluginLoadError;
use plasm_plugin_abi::{
    decode_catalog_metadata, PluginCatalogMetadata, PluginWireError, PLASM_PLUGIN_ABI_VERSION,
};
use std::path::Path;
use std::sync::Arc;

type CatalogMetaFn = unsafe extern "C" fn(
    *const u8,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut *mut u8,
    *mut usize,
) -> i32;

type FreeBuf = unsafe extern "C" fn(*mut u8, usize);

unsafe fn free_plugin_buffer(free: FreeBuf, ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    free(ptr, len);
}

unsafe fn read_err_msg(free: FreeBuf, err_ptr: *mut u8, err_len: usize) -> String {
    if err_ptr.is_null() || err_len == 0 {
        return String::new();
    }
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(err_ptr, err_len)).into_owned();
    free_plugin_buffer(free, err_ptr, err_len);
    s
}

fn wire_to_load(e: PluginWireError) -> PluginLoadError {
    PluginLoadError::CatalogMetadata(e.to_string())
}

/// Open `path` briefly, call `plasm_plugin_catalog_metadata`, then unload.
pub fn load_catalog_metadata(path: &Path) -> Result<PluginCatalogMetadata, PluginLoadError> {
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

    let free: libloading::Symbol<FreeBuf> = unsafe { library.get(b"plasm_plugin_free_buffer\0")? };
    let free = *free;

    let f: libloading::Symbol<CatalogMetaFn> = unsafe {
        library
            .get(b"plasm_plugin_catalog_metadata\0")
            .map_err(|_| PluginLoadError::MissingExport("plasm_plugin_catalog_metadata"))?
    };
    let f = *f;

    unsafe {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len = 0usize;
        let mut err_ptr: *mut u8 = std::ptr::null_mut();
        let mut err_len = 0usize;
        let code = f(
            std::ptr::null(),
            0,
            &mut out_ptr,
            &mut out_len,
            &mut err_ptr,
            &mut err_len,
        );
        if code != 0 {
            let msg = read_err_msg(free, err_ptr, err_len);
            return Err(PluginLoadError::CatalogMetadata(if msg.is_empty() {
                format!("plasm_plugin_catalog_metadata failed (code {code})")
            } else {
                msg
            }));
        }
        if out_ptr.is_null() {
            return Err(PluginLoadError::CatalogMetadata(
                "plugin returned null metadata buffer".into(),
            ));
        }
        let slice = std::slice::from_raw_parts(out_ptr, out_len);
        let parsed = decode_catalog_metadata(slice).map_err(wire_to_load)?;
        free_plugin_buffer(free, out_ptr, out_len);
        Ok(parsed)
    }
}
