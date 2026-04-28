//! Shared RFC 7807 [`Problem`] → Axum [`Response`] mapping for HTTP discovery and execute routes.

use axum::Json;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use http_problem::Problem;

/// Stable `type` URI references for machine clients.
///
/// Uses an `https` scheme so [`http_problem::Problem`] JSON includes the `type` field (relative
/// references are omitted by the crate serializer).
pub mod problem_types {
    pub const DISCOVERY_EMPTY_QUERY: &str =
        "https://plasm.invalid/problems/plasm-discovery-empty-query";
    pub const DISCOVERY_UNKNOWN_ENTRY: &str =
        "https://plasm.invalid/problems/plasm-discovery-unknown-entry";
    pub const TOOL_MODEL_BAD_REQUEST: &str =
        "https://plasm.invalid/problems/plasm-tool-model-bad-request";

    pub const EXECUTE_EMPTY_ENTITIES: &str =
        "https://plasm.invalid/problems/plasm-execute-empty-entities";
    pub const EXECUTE_PRINCIPAL_REQUIRED: &str =
        "https://plasm.invalid/problems/plasm-execute-principal-required";
    pub const EXECUTE_REGISTRY_ERROR: &str =
        "https://plasm.invalid/problems/plasm-execute-registry-error";
    pub const EXECUTE_UNKNOWN_CATALOG_ENTRY: &str =
        "https://plasm.invalid/problems/plasm-execute-unknown-catalog-entry";
    pub const EXECUTE_UNKNOWN_ENTITY: &str =
        "https://plasm.invalid/problems/plasm-execute-unknown-entity";
    pub const EXECUTE_UNKNOWN_SESSION: &str =
        "https://plasm.invalid/problems/plasm-execute-unknown-session";
    pub const EXECUTE_INVALID_PATH_PARAM: &str =
        "https://plasm.invalid/problems/plasm-execute-invalid-path-param";
    pub const EXECUTE_UNSUPPORTED_ACCEPT: &str =
        "https://plasm.invalid/problems/plasm-execute-unsupported-accept";
    pub const EXECUTE_INVALID_BODY_ENCODING: &str =
        "https://plasm.invalid/problems/plasm-execute-invalid-body-encoding";
    pub const EXECUTE_EMPTY_EXPRESSION: &str =
        "https://plasm.invalid/problems/plasm-execute-empty-expression";
    pub const EXECUTE_INVALID_REQUEST_BODY: &str =
        "https://plasm.invalid/problems/plasm-execute-invalid-request-body";
    pub const EXECUTE_INVALID_EXPRESSION: &str =
        "https://plasm.invalid/problems/plasm-execute-invalid-expression";
    pub const EXECUTE_PROJECTION_ENRICHMENT_FAILED: &str =
        "https://plasm.invalid/problems/plasm-execute-projection-enrichment-failed";
    pub const EXECUTE_EXECUTION_FAILED: &str =
        "https://plasm.invalid/problems/plasm-execute-execution-failed";
    pub const EXECUTE_SERIALIZATION_FAILED: &str =
        "https://plasm.invalid/problems/plasm-execute-serialization-failed";
    pub const EXECUTE_UNKNOWN_ARTIFACT: &str =
        "https://plasm.invalid/problems/plasm-execute-unknown-artifact";

    pub const INCOMING_AUTH_UNAUTHORIZED: &str =
        "https://plasm.invalid/problems/plasm-incoming-auth-unauthorized";
    pub const INCOMING_AUTH_FORBIDDEN: &str =
        "https://plasm.invalid/problems/plasm-incoming-auth-forbidden";

    pub const TRACE_SINK_NOT_CONFIGURED: &str =
        "https://plasm.invalid/problems/plasm-trace-sink-not-configured";
    pub const TRACE_SINK_UNAVAILABLE: &str =
        "https://plasm.invalid/problems/plasm-trace-sink-unavailable";
}

fn axum_status_for_problem(p: &Problem) -> StatusCode {
    match StatusCode::from_u16(p.status().as_u16()) {
        Ok(code) => code,
        Err(_) => {
            tracing::warn!(
                reported_status = p.status().as_u16(),
                problem_title = %p.title(),
                "problem HTTP status is not a valid axum StatusCode; mapping to 500"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub(crate) fn problem_response(p: Problem) -> Response {
    let code = axum_status_for_problem(&p);
    (
        code,
        [(CONTENT_TYPE, "application/problem+json; charset=utf-8")],
        Json(&p),
    )
        .into_response()
}
