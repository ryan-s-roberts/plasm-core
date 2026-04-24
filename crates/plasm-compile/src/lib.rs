//! Compilation layer for Plasm: CML, predicate compiler, and decoder DSL.
//!
//! This crate transforms typed predicates into backend-specific requests
//! and provides declarative response decoding.
//!
//! CML template types and transport live in [`plasm_cml`].

pub mod backend_filter;
pub mod decoder;
pub mod error;
pub mod json_path;
pub mod predicate_compiler;

pub use plasm_cml::{
    compile_operation, compile_request, eval_cml, eval_cond, parse_capability_template,
    path_var_names_from_request, template_pagination, template_var_names, CapabilityTemplate,
    CmlCond, CmlEnv, CmlExpr, CmlRequest, CmlType, CompiledMultipartBody, CompiledMultipartPart,
    CompiledOperation, CompiledRequest, HttpBodyFormat, HttpMethod, HttpResponseDecode,
    MultipartBodySpec, MultipartPartSpec, PaginationConfig, PaginationLocation, PaginationParam,
    PaginationStop, PathSegment as CmlPathSegment, ResponsePreprocess,
};

#[cfg(feature = "evm")]
pub use plasm_cml::evm_transport::*;

pub use backend_filter::*;
pub use decoder::*;
pub use error::{CompileError, DecodeError};
pub use json_path::path_expr_from_json_segments;
pub use plasm_cml::CmlError;
pub use predicate_compiler::*;

use plasm_core::{CapabilitySchema, QueryExpr, CGS};

/// Canonical compile-plugin hook trait objects (shared by `plasm-runtime` and `plasm-plugin-host`).
pub type CompileOperationHook =
    dyn Fn(&CapabilityTemplate, &CmlEnv) -> Result<CompiledOperation, CmlError> + Send + Sync;
pub type CompileQueryHook =
    dyn Fn(&QueryExpr, &CGS) -> Result<Option<BackendFilter>, CompileError> + Send + Sync;

/// Ensure every capability's CML mapping template parses (HTTP or EVM transport).
///
/// Call after loading a [`plasm_core::CGS`] so invalid templates fail at validation time
/// instead of first execution.
pub fn validate_cgs_capability_templates(cgs: &plasm_core::CGS) -> Result<(), CmlError> {
    for (name, cap) in &cgs.capabilities {
        parse_capability_template(&cap.mapping.template).map_err(|e| {
            CmlError::InvalidTemplate {
                message: format!("capability `{name}`: {e}"),
            }
        })?;
    }
    Ok(())
}

/// Parse one capability template and return its composable pagination stanza, when present.
///
/// Shared by CLI generation and tool-model projection so both surfaces interpret pagination
/// from the exact same CML parsing path.
pub fn pagination_config_for_capability(cap: &CapabilitySchema) -> Option<PaginationConfig> {
    parse_capability_template(&cap.mapping.template)
        .ok()
        .and_then(|template| template_pagination(&template).cloned())
}
