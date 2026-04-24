//! Stable ABI for **compile-only** Plasm backend plugins (dynamic libraries).
//!
//! ## C symbols (exported by plugins)
//!
//! Plugins **must** export:
//!
//! - `plasm_plugin_abi_version() -> u32` — must equal [`PLASM_PLUGIN_ABI_VERSION`].
//! - `plasm_plugin_free_buffer(ptr: *mut u8, len: usize)` — frees buffers returned by the plugin.
//! - `plasm_plugin_catalog_metadata(req, req_len, out_ptr, out_len, err_ptr, err_len) -> i32`  
//!   `0` = success (`out_*` set), non-zero = error (`err_*` optional UTF-8 message).  
//!   Request body may be empty (`req_len == 0`). Response is framed [`PluginCatalogMetadata`].
//! - `plasm_plugin_compile_operation(req, req_len, out_ptr, out_len, err_ptr, err_len) -> i32`  
//!   `0` = success (`out_*` set), non-zero = error (`err_*` optional UTF-8 message).
//! - `plasm_plugin_compile_query(req, req_len, out_ptr, out_len, err_ptr, err_len) -> i32` — same contract.
//!
//! ## Wire format (not JSON)
//!
//! Request and response **bodies** passed to those C functions are:
//! **[`PLASM_PLUGIN_ABI_VERSION`] (`u32`, little-endian)** followed by a **CBOR**
//! ([`ciborium`](https://docs.rs/ciborium)) payload of [`PluginCompileOperationRequest`] /
//! [`PluginCompileOperationResponse`] / [`PluginCompileQueryRequest`] / [`PluginCompileQueryResponse`] /
//! [`PluginCatalogMetadata`]
//! — binary, typed Rust structs (not JSON text, not untyped `serde_json::Value`).

use plasm_compile::{BackendFilter, CapabilityTemplate, CmlEnv, CompiledOperation};
use plasm_core::{QueryExpr, CGS};
use serde::{Deserialize, Serialize};

/// Bump when breaking the CBOR payload shape, version prefix, or C calling convention.
pub const PLASM_PLUGIN_ABI_VERSION: u32 = 4;

/// Handshake payload (optional; host may read before full load).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHandshake {
    pub abi_version: u32,
    /// Human-readable plugin id (e.g. schema name or genco artifact id).
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCompileOperationRequest {
    pub template: CapabilityTemplate,
    pub cml_env: CmlEnv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCompileOperationResponse {
    pub compiled: CompiledOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCompileQueryRequest {
    pub query: QueryExpr,
    pub cgs: CGS,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCompileQueryResponse {
    pub filter: Option<BackendFilter>,
}

/// Tiny catalog metadata export (derived from embedded CGS) for startup selection and integrity checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCatalogMetadata {
    pub entry_id: String,
    pub version: u64,
    /// Hex SHA-256 of canonical JSON for the embedded CGS (see [`plasm_core::schema::CGS::catalog_cgs_hash_hex`]).
    pub cgs_hash: String,
    pub target_triple: String,
    /// Serialized CGS interchange YAML bytes embedded in this plugin artifact.
    pub cgs_yaml: Vec<u8>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginWireError {
    #[error("plugin wire: expected ABI version {expected}, got {got}")]
    VersionMismatch { expected: u32, got: u32 },
    #[error("plugin wire: frame too short")]
    FrameTooShort,
    #[error("plugin wire: cbor encode: {0}")]
    CborEncode(String),
    #[error("plugin wire: cbor decode: {0}")]
    CborDecode(String),
}

fn push_frame(payload: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&PLASM_PLUGIN_ABI_VERSION.to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

fn split_frame(bytes: &[u8]) -> Result<(u32, &[u8]), PluginWireError> {
    if bytes.len() < 4 {
        return Err(PluginWireError::FrameTooShort);
    }
    let ver = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    Ok((ver, &bytes[4..]))
}

fn verify_version(ver: u32) -> Result<(), PluginWireError> {
    if ver != PLASM_PLUGIN_ABI_VERSION {
        return Err(PluginWireError::VersionMismatch {
            expected: PLASM_PLUGIN_ABI_VERSION,
            got: ver,
        });
    }
    Ok(())
}

fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, PluginWireError> {
    let mut payload = Vec::new();
    ciborium::ser::into_writer(value, &mut payload)
        .map_err(|e| PluginWireError::CborEncode(e.to_string()))?;
    Ok(push_frame(payload))
}

fn decode_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, PluginWireError> {
    let (ver, rest) = split_frame(bytes)?;
    verify_version(ver)?;
    ciborium::de::from_reader(rest).map_err(|e| PluginWireError::CborDecode(e.to_string()))
}

pub fn encode_compile_operation_request(
    req: &PluginCompileOperationRequest,
) -> Result<Vec<u8>, PluginWireError> {
    encode_cbor(req)
}

pub fn decode_compile_operation_request(
    bytes: &[u8],
) -> Result<PluginCompileOperationRequest, PluginWireError> {
    decode_cbor(bytes)
}

pub fn encode_compile_operation_response(
    resp: &PluginCompileOperationResponse,
) -> Result<Vec<u8>, PluginWireError> {
    encode_cbor(resp)
}

pub fn decode_compile_operation_response(
    bytes: &[u8],
) -> Result<PluginCompileOperationResponse, PluginWireError> {
    decode_cbor(bytes)
}

pub fn encode_compile_query_request(
    req: &PluginCompileQueryRequest,
) -> Result<Vec<u8>, PluginWireError> {
    encode_cbor(req)
}

pub fn decode_compile_query_request(
    bytes: &[u8],
) -> Result<PluginCompileQueryRequest, PluginWireError> {
    decode_cbor(bytes)
}

pub fn encode_compile_query_response(
    resp: &PluginCompileQueryResponse,
) -> Result<Vec<u8>, PluginWireError> {
    encode_cbor(resp)
}

pub fn decode_compile_query_response(
    bytes: &[u8],
) -> Result<PluginCompileQueryResponse, PluginWireError> {
    decode_cbor(bytes)
}

pub fn encode_catalog_metadata(meta: &PluginCatalogMetadata) -> Result<Vec<u8>, PluginWireError> {
    encode_cbor(meta)
}

pub fn decode_catalog_metadata(bytes: &[u8]) -> Result<PluginCatalogMetadata, PluginWireError> {
    decode_cbor(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plasm_compile::parse_capability_template;

    #[test]
    fn capability_template_round_trip_cbor() {
        let v = serde_json::json!({
            "method": "GET",
            "path": [
                {"type": "literal", "value": "v1"},
                {"type": "var", "name": "id"}
            ]
        });
        let template = parse_capability_template(&v).unwrap();
        let req = PluginCompileOperationRequest {
            template,
            cml_env: Default::default(),
        };
        let bytes = encode_compile_operation_request(&req).unwrap();
        let back = decode_compile_operation_request(&bytes).unwrap();
        assert_eq!(req.template, back.template);
        assert_eq!(req.cml_env, back.cml_env);
    }

    #[test]
    fn catalog_metadata_round_trip_cbor() {
        let meta = PluginCatalogMetadata {
            entry_id: "stub_test".into(),
            version: 1,
            cgs_hash: "abc".into(),
            target_triple: "aarch64-apple-darwin".into(),
            cgs_yaml: b"entities: {}\ncapabilities: {}\n".to_vec(),
            label: "stub".into(),
            tags: vec!["plugin".into()],
        };
        let bytes = encode_catalog_metadata(&meta).unwrap();
        let back = decode_catalog_metadata(&bytes).unwrap();
        assert_eq!(meta.entry_id, back.entry_id);
        assert_eq!(meta.version, back.version);
        assert_eq!(meta.cgs_hash, back.cgs_hash);
        assert_eq!(meta.cgs_yaml, back.cgs_yaml);
    }
}
