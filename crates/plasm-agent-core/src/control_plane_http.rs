//! Shared control-plane HTTP auth: `x-plasm-control-plane-secret` vs `PLASM_MCP_CONTROL_PLANE_SECRET`.

use axum::http::HeaderMap;
use subtle::ConstantTimeEq;

pub(crate) const X_PLASM_CONTROL_PLANE_SECRET: &str = "x-plasm-control-plane-secret";

const DEV_PLANE_SECRET_FALLBACK: &str = "dev-plasm-mcp-control-plane-secret-32chars-min!!";

/// Secret used to authorize internal HTTP handlers (dev default when env unset).
/// Returns `None` when configured secret is too short (handlers must reject).
pub(crate) fn control_plane_secret_for_internal_http(
    too_short_log: &'static str,
) -> Option<String> {
    let expected = std::env::var("PLASM_MCP_CONTROL_PLANE_SECRET")
        .unwrap_or_else(|_| DEV_PLANE_SECRET_FALLBACK.to_string());
    if expected.len() < 16 {
        tracing::warn!(
            "PLASM_MCP_CONTROL_PLANE_SECRET too short; rejecting {}",
            too_short_log
        );
        return None;
    }
    Some(expected)
}

/// Strict secret from env only (no dev default). For outbound callbacks that must not use implicit dev keys.
pub(crate) fn control_plane_secret_from_env_strict() -> Option<String> {
    let secret = std::env::var("PLASM_MCP_CONTROL_PLANE_SECRET").ok()?;
    let secret = secret.trim().to_string();
    (secret.len() >= 16).then_some(secret)
}

pub fn control_plane_headers_authorized(headers: &HeaderMap, too_short_log: &'static str) -> bool {
    let Some(expected) = control_plane_secret_for_internal_http(too_short_log) else {
        return false;
    };
    let Some(got) = headers
        .get(X_PLASM_CONTROL_PLANE_SECRET)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    if got.len() != expected.len() {
        return false;
    }
    got.as_bytes().ct_eq(expected.as_bytes()).into()
}
