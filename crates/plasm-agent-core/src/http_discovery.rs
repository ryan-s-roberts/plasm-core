//! JSON discovery API (`/v1/*`): catalog, capability search, and operator [`tool-model`](crate::tool_model). Use discovery results to build `POST /execute`
//! (`entry_id` + deduped `entity` values from [`RankedCandidate`](plasm_core::discovery::RankedCandidate)).

use axum::extract::{Extension, Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use http_problem::Problem;
use http_problem::prelude::{StatusCode as ProblemStatus, Uri};
use plasm_core::discovery::{
    CapabilityQuery, CatalogEntryMeta, CgsCatalog, CgsDiscovery, DiscoveryError, DiscoveryResult,
};
use plasm_core::schema::CGS;
use serde::{Deserialize, Serialize};

use crate::http_problem_util::problem_response;
use crate::http_problem_util::problem_types;
use crate::server_state::PlasmHostState;
use crate::tool_model::{ToolModelBuildError, ToolModelQuery, build_tool_model};

#[derive(Debug, Deserialize)]
pub struct IncludeCgsQuery {
    #[serde(default)]
    pub include_cgs: bool,
}

#[derive(Debug, Serialize)]
pub struct RegistryListResponse {
    pub entries: Vec<CatalogEntryMeta>,
}

#[derive(Debug, Serialize)]
pub struct RegistryEntryResponse {
    pub entry_id: String,
    pub label: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgs: Option<CGS>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

/// Returned by [`get_auth_status`] (hosted: auth-framework on; OSS: data-plane only).
#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_source: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'static str>,
}

/// Public health check (no incoming auth).
pub async fn health_response() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn get_auth_status(
    Extension(st): Extension<PlasmHostState>,
) -> Result<Json<AuthStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    if st.auth_framework().is_some() {
        return Ok(Json(AuthStatusResponse {
            status: "ok",
            storage: Some("memory"),
            open_source: None,
            message: None,
        }));
    }
    // Open-source `plasm-mcp` (no `PlasmSaaSHostExtension`): 200 — auth-framework is intentionally absent.
    if st.saas.is_none() {
        return Ok(Json(AuthStatusResponse {
            status: "ok",
            storage: None,
            open_source: Some(true),
            message: Some(
                "This process does not run auth-framework. Use GET /v1/health for liveness. \
Hosted control-plane auth (JWT, API keys, tenant policy) is composed by the private plasm-saas / plasm-mcp-app stack.",
            ),
        }));
    }
    // Hosted / composed stack without a framework instance (e.g. tests) — keep legacy 503.
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "auth_framework_disabled",
            "detail": "auth-framework is not initialized in this process"
        })),
    ))
}

fn discovery_problem(e: DiscoveryError) -> Problem {
    match e {
        DiscoveryError::EmptyQuery => Problem::custom(
            ProblemStatus::BAD_REQUEST,
            Uri::from_static(problem_types::DISCOVERY_EMPTY_QUERY),
        )
        .with_title("Bad Request")
        .with_detail(e.to_string()),
        DiscoveryError::UnknownEntry(_) => Problem::custom(
            ProblemStatus::NOT_FOUND,
            Uri::from_static(problem_types::DISCOVERY_UNKNOWN_ENTRY),
        )
        .with_title("Not Found")
        .with_detail(e.to_string()),
    }
}

fn tool_model_problem(e: ToolModelBuildError) -> Problem {
    Problem::custom(
        ProblemStatus::BAD_REQUEST,
        Uri::from_static(problem_types::TOOL_MODEL_BAD_REQUEST),
    )
    .with_title("Bad Request")
    .with_detail(e.to_string())
}

async fn get_registry(Extension(st): Extension<PlasmHostState>) -> Json<RegistryListResponse> {
    let reg = st.catalog.snapshot();
    Json(RegistryListResponse {
        entries: reg.list_entries(),
    })
}

async fn get_registry_entry(
    Extension(st): Extension<PlasmHostState>,
    Path(id): Path<String>,
    Query(q): Query<IncludeCgsQuery>,
) -> Response {
    let reg = st.catalog.snapshot();
    let meta = match reg.lookup_entry_meta(&id) {
        Some(m) => m,
        None => {
            return problem_response(discovery_problem(DiscoveryError::UnknownEntry(id.clone())));
        }
    };
    let cgs = if q.include_cgs {
        match reg.load_context(&id) {
            Ok(ctx) => Some((*ctx.cgs).clone()),
            Err(e) => return problem_response(discovery_problem(e)),
        }
    } else {
        None
    };
    Json(RegistryEntryResponse {
        entry_id: meta.entry_id,
        label: meta.label,
        tags: meta.tags,
        cgs,
    })
    .into_response()
}

async fn get_tool_model(
    Extension(st): Extension<PlasmHostState>,
    Path(entry_id): Path<String>,
    Query(q): Query<ToolModelQuery>,
) -> Response {
    let reg = st.catalog.snapshot();
    let meta = match reg.lookup_entry_meta(&entry_id) {
        Some(m) => m,
        None => {
            return problem_response(discovery_problem(DiscoveryError::UnknownEntry(
                entry_id.clone(),
            )));
        }
    };
    let ctx = match reg.load_context(&entry_id) {
        Ok(ctx) => ctx,
        Err(e) => return problem_response(discovery_problem(e)),
    };
    match build_tool_model(ctx.cgs.as_ref(), &meta, &q) {
        Ok(body) => Json(body).into_response(),
        Err(e) => problem_response(tool_model_problem(e)),
    }
}

fn log_discovery_response(out: &DiscoveryResult) {
    tracing::debug!(
        candidates = out.candidates.len(),
        contexts = out.contexts.len(),
        ambiguities = out.ambiguities.len(),
        schema_neighborhoods = out.schema_neighborhoods.len(),
        entity_summaries = out.entity_summaries.len(),
        top_scores = ?out
            .candidates
            .iter()
            .take(5)
            .map(|c| (c.capability_name.as_str(), c.score))
            .collect::<Vec<_>>(),
        "plasm discovery response"
    );
}

async fn post_discover(
    Extension(st): Extension<PlasmHostState>,
    Json(query): Json<CapabilityQuery>,
) -> Response {
    tracing::debug!(
        tokens = query.tokens.len(),
        phrases = query.phrases.len(),
        entity_hints = query.entity_hints.len(),
        kinds = query.kinds.len(),
        capability_names_len = query.capability_names.as_ref().map(Vec::len),
        entry_ids_len = query.entry_ids.as_ref().map(Vec::len),
        pick_entry = query.pick_entry.as_deref(),
        pick_capabilities_len = query.pick_capabilities.as_ref().map(Vec::len),
        exclude_capabilities_len = query.exclude_capabilities.as_ref().map(Vec::len),
        expand_entities_len = query.expand_entities.as_ref().map(Vec::len),
        "plasm discovery request"
    );
    let reg = st.catalog.snapshot();
    match reg.discover(&query) {
        Ok(out) => {
            log_discovery_response(&out);
            Json(out).into_response()
        }
        Err(e) => {
            tracing::debug!(error = %e, "plasm discovery failed");
            problem_response(discovery_problem(e))
        }
    }
}

/// Registry + discover (protected by incoming-auth middleware when enabled).
pub fn discovery_routes_protected() -> Router {
    Router::new()
        .route("/v1/registry", get(get_registry))
        .route("/v1/registry/{entry_id}", get(get_registry_entry))
        .route("/v1/registry/{entry_id}/tool-model", get(get_tool_model))
        .route("/v1/discover", post(post_discover))
}
