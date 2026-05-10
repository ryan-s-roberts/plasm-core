//! JSON discovery API (`/v1/*`): catalog, capability search, and operator [`tool-model`](crate::tool_model). Use discovery results to build `POST /execute`
//! (`entry_id` + deduped `entity` values from [`RankedCandidate`](plasm_core::discovery::RankedCandidate)).

use axum::extract::{Extension, Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use http_problem::prelude::{StatusCode as ProblemStatus, Uri};
use http_problem::Problem;
use plasm_core::discovery::{
    CapabilityQuery, CatalogEntryMeta, CgsCatalog, CgsDiscovery, DiscoveryError, DiscoveryResult,
    RankedCandidate,
};
use plasm_core::schema::CGS;
use plasm_discovery::{DiscoveryDecision, DiscoveryQuery};
use serde::{Deserialize, Serialize};

use crate::http_problem_util::problem_response;
use crate::http_problem_util::problem_types;
use crate::server_state::PlasmHostState;
use crate::tool_model::{build_tool_model, ToolModelBuildError, ToolModelQuery};
use crate::typed_discovery_host::run_typed_catalog_discovery;

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

fn typed_discovery_problem(e: plasm_discovery::DiscoveryError) -> Problem {
    match e {
        plasm_discovery::DiscoveryError::EmptyUtterance
        | plasm_discovery::DiscoveryError::InvalidClarificationAnswer => Problem::custom(
            ProblemStatus::BAD_REQUEST,
            Uri::from_static(problem_types::DISCOVERY_TYPED_BAD_REQUEST),
        )
        .with_title("Bad Request")
        .with_detail(e.to_string()),
        plasm_discovery::DiscoveryError::UnknownEntry(_) => Problem::custom(
            ProblemStatus::NOT_FOUND,
            Uri::from_static(problem_types::DISCOVERY_UNKNOWN_ENTRY),
        )
        .with_title("Not Found")
        .with_detail(e.to_string()),
        _ => Problem::custom(
            ProblemStatus::INTERNAL_SERVER_ERROR,
            Uri::from_static(problem_types::DISCOVERY_TYPED_ERROR),
        )
        .with_title("Discovery Error")
        .with_detail(e.to_string()),
    }
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

async fn post_discover_typed(
    Extension(st): Extension<PlasmHostState>,
    Json(query): Json<DiscoveryQuery>,
) -> Response {
    tracing::debug!(
        utterance_len = query.utterance.len(),
        allowed = query.allowed_entry_ids.len(),
        max_options = query.max_options,
        enable_embeddings = query.enable_embeddings,
        "plasm typed discovery request"
    );
    let reg = st.catalog.snapshot();
    let emb = st.discovery_embedding_store();
    match run_typed_catalog_discovery(&reg, query, emb).await {
        Ok(out) => Json(out).into_response(),
        Err(e) => {
            tracing::debug!(error = %e, "typed discovery failed");
            problem_response(typed_discovery_problem(e))
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TerminalDiscoverBody {
    pub intent: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub allowed_entry_ids: Vec<String>,
    #[serde(default = "terminal_discover_default_embeddings")]
    pub enable_embeddings: bool,
}

fn terminal_discover_default_embeddings() -> bool {
    false
}

#[derive(Debug, Serialize)]
pub struct TerminalDiscoverSeedRow {
    pub entry_id: String,
    pub entity: String,
    pub capability_name: String,
    pub score: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub capability_description: String,
    /// `{ "api": entry_id, "entity" }` — drop-in for `POST /execute` seeding and `POST .../context`.
    pub seed: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct TerminalDiscoverResponse {
    pub intent: String,
    pub rows: Vec<TerminalDiscoverSeedRow>,
    pub candidates: Vec<RankedCandidate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub schema_neighborhoods: Vec<plasm_core::discovery::DiscoverySchemaNeighborhood>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entity_summaries: Vec<plasm_core::discovery::EntitySummary>,
    pub typed: DiscoveryDecision,
}

async fn post_terminal_discover(
    Extension(st): Extension<PlasmHostState>,
    Json(body): Json<TerminalDiscoverBody>,
) -> Response {
    let intent = body.intent.trim();
    if intent.is_empty() {
        return problem_response(discovery_problem(DiscoveryError::EmptyQuery));
    }
    let cq = CapabilityQuery {
        tokens: intent.split_whitespace().map(|s| s.to_string()).collect(),
        phrases: vec![intent.to_string()],
        entry_ids: if body.allowed_entry_ids.is_empty() {
            None
        } else {
            Some(body.allowed_entry_ids.clone())
        },
        ..Default::default()
    };
    let reg = st.catalog.snapshot();
    let structured = match reg.discover(&cq) {
        Ok(o) => o,
        Err(e) => {
            tracing::debug!(error = %e, "terminal structured discovery failed");
            return problem_response(discovery_problem(e));
        }
    };
    let limit = body.limit.unwrap_or(32).clamp(1, 128);
    let candidates: Vec<RankedCandidate> =
        structured.candidates.iter().take(limit).cloned().collect();
    let rows: Vec<TerminalDiscoverSeedRow> = candidates
        .iter()
        .map(|c| TerminalDiscoverSeedRow {
            entry_id: c.entry_id.clone(),
            entity: c.entity.clone(),
            capability_name: c.capability_name.clone(),
            score: c.score,
            reason_codes: c.reason_codes.clone(),
            capability_description: c.capability_description.clone(),
            seed: serde_json::json!({
                "api": c.entry_id,
                "entity": c.entity,
            }),
        })
        .collect();

    let typed_query = DiscoveryQuery {
        utterance: intent.to_string(),
        allowed_entry_ids: body.allowed_entry_ids.clone(),
        max_options: limit.max(8),
        enable_embeddings: body.enable_embeddings,
        ..Default::default()
    };
    let emb = st.discovery_embedding_store();
    let typed = match run_typed_catalog_discovery(&reg, typed_query, emb).await {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = %e, "terminal typed discovery failed");
            return problem_response(typed_discovery_problem(e));
        }
    };

    Json(TerminalDiscoverResponse {
        intent: intent.to_string(),
        rows,
        candidates,
        schema_neighborhoods: structured.schema_neighborhoods.clone(),
        entity_summaries: structured.entity_summaries.clone(),
        typed,
    })
    .into_response()
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
        .route("/v1/discover-typed", post(post_discover_typed))
        .route("/v1/terminal/discover", post(post_terminal_discover))
}
