//! FFI bridge: call `cdylib` exports and free buffers with the plugin’s [`plasm_plugin_free_buffer`].

use crate::PluginLoadError;
use plasm_compile::{CapabilityTemplate, CmlEnv, CmlError, CompiledOperation};
use plasm_compile::{CompileOperationHook, CompileQueryHook};
use plasm_core::{QueryExpr, CGS};
use plasm_plugin_abi::{
    decode_compile_operation_response, decode_compile_query_response,
    encode_compile_operation_request, encode_compile_query_request, PluginCompileOperationRequest,
    PluginCompileQueryRequest, PluginWireError,
};
use std::sync::Arc;

type CompileOp = unsafe extern "C" fn(
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

fn wire_to_cml(e: PluginWireError) -> CmlError {
    CmlError::InvalidTemplate {
        message: e.to_string(),
    }
}

fn wire_to_compile(e: PluginWireError) -> plasm_compile::CompileError {
    plasm_compile::CompileError::CompilationFailed {
        message: e.to_string(),
    }
}

pub fn make_compile_operation_fn(
    lib: Arc<libloading::Library>,
) -> Result<Arc<CompileOperationHook>, PluginLoadError> {
    let f: libloading::Symbol<CompileOp> = unsafe { lib.get(b"plasm_plugin_compile_operation\0")? };
    let free: libloading::Symbol<FreeBuf> = unsafe { lib.get(b"plasm_plugin_free_buffer\0")? };
    let f = *f;
    let free = *free;
    Ok(Arc::new(
        move |template: &CapabilityTemplate, env: &CmlEnv| -> Result<CompiledOperation, CmlError> {
            let req = PluginCompileOperationRequest {
                template: template.clone(),
                cml_env: env.clone(),
            };
            let req_bytes = encode_compile_operation_request(&req).map_err(wire_to_cml)?;
            unsafe {
                let mut out_ptr: *mut u8 = std::ptr::null_mut();
                let mut out_len = 0usize;
                let mut err_ptr: *mut u8 = std::ptr::null_mut();
                let mut err_len = 0usize;
                let code = f(
                    req_bytes.as_ptr(),
                    req_bytes.len(),
                    &mut out_ptr,
                    &mut out_len,
                    &mut err_ptr,
                    &mut err_len,
                );
                if code != 0 {
                    let msg = read_err_msg(free, err_ptr, err_len);
                    return Err(CmlError::InvalidTemplate {
                        message: if msg.is_empty() {
                            format!("plasm_plugin_compile_operation failed (code {code})")
                        } else {
                            msg
                        },
                    });
                }
                if out_ptr.is_null() {
                    return Err(CmlError::InvalidTemplate {
                        message: "plugin returned null output buffer".into(),
                    });
                }
                let slice = std::slice::from_raw_parts(out_ptr, out_len);
                let parsed = decode_compile_operation_response(slice).map_err(|e| {
                    free_plugin_buffer(free, out_ptr, out_len);
                    wire_to_cml(e)
                })?;
                free_plugin_buffer(free, out_ptr, out_len);
                Ok(parsed.compiled)
            }
        },
    ))
}

pub fn make_compile_query_fn(
    lib: Arc<libloading::Library>,
) -> Result<Arc<CompileQueryHook>, PluginLoadError> {
    let f: libloading::Symbol<CompileOp> = unsafe { lib.get(b"plasm_plugin_compile_query\0")? };
    let free: libloading::Symbol<FreeBuf> = unsafe { lib.get(b"plasm_plugin_free_buffer\0")? };
    let f = *f;
    let free = *free;
    Ok(Arc::new(move |query: &QueryExpr, cgs: &CGS| {
        let req = PluginCompileQueryRequest {
            query: query.clone(),
            cgs: cgs.clone(),
        };
        let req_bytes = encode_compile_query_request(&req).map_err(wire_to_compile)?;
        unsafe {
            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len = 0usize;
            let mut err_ptr: *mut u8 = std::ptr::null_mut();
            let mut err_len = 0usize;
            let code = f(
                req_bytes.as_ptr(),
                req_bytes.len(),
                &mut out_ptr,
                &mut out_len,
                &mut err_ptr,
                &mut err_len,
            );
            if code != 0 {
                let msg = read_err_msg(free, err_ptr, err_len);
                return Err(plasm_compile::CompileError::CompilationFailed {
                    message: if msg.is_empty() {
                        format!("plasm_plugin_compile_query failed (code {code})")
                    } else {
                        msg
                    },
                });
            }
            if out_ptr.is_null() {
                return Err(plasm_compile::CompileError::CompilationFailed {
                    message: "plugin returned null output buffer".into(),
                });
            }
            let slice = std::slice::from_raw_parts(out_ptr, out_len);
            let parsed = decode_compile_query_response(slice).map_err(|e| {
                free_plugin_buffer(free, out_ptr, out_len);
                wire_to_compile(e)
            })?;
            free_plugin_buffer(free, out_ptr, out_len);
            Ok(parsed.filter)
        }
    }))
}
