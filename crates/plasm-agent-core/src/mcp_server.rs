//! MCP Streamable HTTP server (rust-mcp-sdk) over Plasm discovery + execute ([`crate::server_state::PlasmHostState`]).
//! Tool results use Markdown [`TextContent`]; `plasm` sets `CallToolResult._meta.plasm` (request
//! fingerprints, artifact URIs, optional `lossy_summary_fields` per truncated step).
//! Run snapshot URIs in Markdown use logical-session short form `plasm://session/{logical_session_ref}/r/{n}`
//! (`s0`, `s1`, … per MCP transport; see [`crate::run_artifacts::plasm_session_short_resource_uri`]);
//! canonical `plasm://execute/.../run/{uuid}` remains accepted on read.
//! Tool results may include run snapshot URIs and inline hints when full data requires MCP `resources/read`;
//! the server repeats that obligation in the reply when it applies.
//!
//! Execute bindings (`add_capabilities` → `plasm`) are stored **per agent logical session**
//! ([`PlasmExecBinding`]), keyed by canonical logical session UUID from `plasm_session_init` (client uses per-transport **`logical_session_ref`** slots: `s0`, `s1`, …).
//! One MCP transport may host **many** logical sessions; `MCP-Session-Id` is transport correlation only.
//! If the server-side execute session expires while the MCP transport stays open, the next
//! `add_capabilities` opens a **new** `(prompt_hash, session_id)` and refreshes the binding.
//! `add_capabilities` replaces that binding when opening a new catalog entry or session. Tenant MCP policy
//! is enforced from `Authorization: Bearer <api_key>` (opaque key from control-plane provision) when tenant configs exist.
//! Tool text includes
//! the full Plasm instructions body only when the session is newly created server-side (`reused: false`); repeated
//! opens with the same entry + seeds omit the instruction body to avoid token churn.
//! **Symbols:** for a fixed binding (`prompt_hash` + `session`), `e#` / `m#` / `p#` are append-only across
//! incremental `add_capabilities` waves; they do not reshuffle. A new primary catalog open or logical session
//! starts a new symbol space—always read tokens from the current session `prompt` / Plasm language text.
//! A soft cap evicts one arbitrary older binding when the map grows past [`MAX_MCP_EXEC_BINDINGS`].
//!
//! Plasm language / instructions body (first wave on `add_capabilities` open plus append-only delta waves from
//! `add_capabilities` `seeds`) is counted in Unicode scalar values per MCP transport session.
//! Each `plasm` call also accumulates invocation text (`expressions` plus optional `reasoning` and
//! optional TSV `tsv_static_frontmatter`) and,
//! on success, returned Markdown. Server logs use a rough **token estimate** ≈ `ceil(chars / 4)` per
//! bucket (`prompt` / `invocation` / `tool_response`). When the session leaves the SDK session store,
//! an `INFO` line logs cumulative character totals and token estimates (`plasm_agent::mcp`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use crate::trace_hub::{AddCapabilitiesTrace, McpPlasmTraceSink};
use std::time::Duration;
use tracing::Instrument;

use async_trait::async_trait;
use base64::Engine as _;
use plasm_core::CgsDiscovery;
use plasm_core::discovery::{CapabilityQuery, DiscoveryError};
#[cfg(feature = "code_mode")]
use plasm_facade_gen::{FacadeGenRequest, build_code_facade, quickjs_runtime_from_facade_delta};
use rust_mcp_sdk::McpServer;
use rust_mcp_sdk::error::SdkResult;
use rust_mcp_sdk::event_store::InMemoryEventStore;
use rust_mcp_sdk::mcp_server::hyper_server;
use rust_mcp_sdk::mcp_server::{
    HyperServer, HyperServerOptions, ServerHandler, ToMcpServerHandler,
};
use rust_mcp_sdk::schema::{
    BlobResourceContents, CallToolRequestParams, CallToolResult, ContentBlock, Implementation,
    InitializeResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ReadResourceContent, ReadResourceRequestParams,
    ReadResourceResult, ResourceTemplate, RpcError, ServerCapabilities,
    ServerCapabilitiesResources, ServerCapabilitiesTools, TextContent, TextResourceContents, Tool,
    ToolAnnotations, ToolInputSchema,
};
use rust_mcp_sdk::schema::{ToolExecution, ToolExecutionTaskSupport, schema_utils::CallToolError};
#[cfg(feature = "code_mode")]
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

#[cfg(feature = "code_mode")]
use crate::http_execute::mcp_add_code_capabilities_markdown;
use crate::http_execute::{
    ApplyCapabilitySeedsOutcome, CapabilitySeed, apply_capability_seeds,
    execute_session_run_markdown, normalize_capability_seeds,
};
use crate::incoming_auth::{IncomingAuthMethod, IncomingAuthMode, TenantPrincipal, tenant_scope};
#[cfg(feature = "code_mode")]
use crate::mcp_plasm_code::{
    CodeModePlasmRunHooks, CodePlanDryRunTextMeta, code_mode_plan_dag_json,
    evaluate_code_mode_plan_dry, render_code_mode_plan_dry_text, run_code_mode_plan,
};
use crate::mcp_plasm_meta::PlasmMetaIndex;
use crate::mcp_policy;
use crate::mcp_runtime_config::McpRuntimeConfig;
use crate::mcp_stream_auth::{config_id_from_auth_info, is_anonymous_mcp_auth};
use crate::run_artifacts::{
    ArtifactPayload, LogicalSessionUriSegment, parse_plasm_execute_plan_uri,
    parse_plasm_execute_run_uri, parse_plasm_session_short_plan_uri,
    parse_plasm_session_short_resource_uri,
};
#[cfg(feature = "code_mode")]
use crate::run_artifacts::{
    CodePlanArchiveDocument, code_plan_http_path, parse_code_plan_handle,
    plasm_session_short_plan_uri,
};
use crate::server_state::PlasmHostState;
use crate::session_identity::{ClientSessionKey, LogicalSessionId};
use crate::trace_sink_emit::PlasmTraceContext;
use plasm_trace::{CodePlanRunArtifactRef, RunArtifactArchiveRef};
use serde_json::json;
use uuid::Uuid;

/// Best-effort bound on concurrent MCP transport sessions holding an execute binding (see module doc).
const MAX_MCP_EXEC_BINDINGS: usize = 512;

/// Max Unicode scalars allowed for `plasm` `tsv_static_frontmatter` (the Plasm language contract block).
const MAX_TSV_STATIC_FRONTMATTER_SCALARS: usize = 262_144;

/// Model-facing `plasm` tool description: run expressions (session setup is in [`MCP_SERVER_INITIALIZE_INSTRUCTIONS`]).
pub(crate) const MCP_PLASM_TOOL_DESCRIPTION: &str = "**Run** Plasm lines (`expressions` required; **`logical_session_ref`** from **`plasm_session_init`**). Full setup, paging, and output shape: MCP **`initialize` `instructions`**. \
     Optional **`tsv_static_frontmatter`**: the `#`-comment Plasm language contract (cache from **`add_capabilities`** `_meta.plasm.tsv_static_frontmatter` on first TSV open); not executed, counts toward session invocation tokens. \
     **Steady state:** same **`logical_session_ref`**, **`plasm` only** for follow-ups -- do **not** re-run **`plasm_session_init`** or **`add_capabilities`** every turn once capabilities are loaded.";

/// MCP `initialize` `instructions` field: tool flow (LLM-facing; transport auth is host-owned).
pub(crate) const MCP_SERVER_INITIALIZE_INSTRUCTIONS: &str = "**Call `plasm_session_init` first** on each MCP connection (before `discover_capabilities`, `add_capabilities`, or `plasm`): pass **`client_session_key`** -- **the host’s stable agent-context id** (same value for the same window, subagent, or other isolation boundary the host defines); use **one key per context you want to share one Plasm logical session**, not a new random id every message. The response gives **`logical_session_ref`** (`s0`, `s1`, ...). **Idempotent:** same transport + same `client_session_key` + tenant => **reuse** the same logical session and ref. \
     **Session reuse (required default):** After the first successful **`add_capabilities`** open for that ref, **most subsequent user requests should be `plasm` only** with the **same** **`logical_session_ref`**. **Do not** re-invoke **`plasm_session_init`** or repeat **`add_capabilities`** with the same catalog just to \"re-initialize\" -- that wastes tokens and breaks continuity. Call **`add_capabilities`** again **only** when you must **append** new **`api` / `entity`** seeds (another API, more entities) you have **not** already added. \
     Optional **`discover_capabilities`** with `query` — **one string** (plain-language or keywords) **or** a string array (**search**; **TSV rows** are entities with descriptions). Use columns **`api`** + **`entity`** for each `add_capabilities` seed. \
     **`add_capabilities`**: **`logical_session_ref`** + **`seeds`**, a JSON array of objects with keys **`api`** (catalog id) and **`entity`** (legacy key **`entry_id`** still accepted per object). Multiple distinct **`api`** values **federate** into **one Plasm language** for that session—`plasm` lines may reference entities from every included catalog. Re-call with more seeds on the **same** **`logical_session_ref`** to extend the session; responses may include **`reused: true`** when the server matches a prior open (less prompt churn). On a **new** TSV open, the Plasm language **contract** is in **`_meta.plasm.tsv_static_frontmatter`**; the body is the teaching table only. **Cache** the contract and pass it as **`plasm` `tsv_static_frontmatter`**; do not paste it into the system or user message. \
     **`plasm`**: **`logical_session_ref`** + **`expressions`**, optional **`tsv_static_frontmatter`**, optional **`reasoning`**. **Paging:** follow **`page(s0_pgN)`** / `_meta.plasm.paging` for more rows in the **same** logical session. \
    **Code Mode:** prefer **`plasm`** for one simple expression, one-shot reads/writes, and simple follow-ups. Use Code Mode only when the user intent is best satisfied by synthesizing a **program** with multiple operations needing coordination, transformation, compute, fan-out/fan-in, or reusable logic; it is **not a query interface**. Flow: **`add_code_capabilities`** -> write a complete TypeScript program -> **`evaluate_code_plan(name, code)`** -> inspect the dry-run execution plan -> **`execute_code_plan(plan_handle)`** once the plan satisfies the user's intent and risk. If the dry-run reveals a defect, missing capability, excessive output, or unacceptable risk, revise and re-evaluate instead of executing. Reuse the **`plan_handle`**; resend TypeScript only when changing the program or symbol space. Start uncertain plans small with **`Plan.limit(...)`** before widening. Minimize output: select/project only needed source fields, use **`Plan.project`** / **`.select(...)`**, and make **`Plan.return(...)`** publish only final answer nodes, never intermediates.";

fn parse_tool_seeds(
    tool: &str,
    v: &serde_json::Value,
) -> Result<Vec<CapabilitySeed>, CallToolError> {
    if v.get("seeds").is_none() && (v.get("entry_id").is_some() || v.get("entities").is_some()) {
        return Err(CallToolError::invalid_arguments(
            tool,
            Some(
                "missing seeds: add_capabilities requires a `seeds` array of `{api, entity}` objects (legacy `entry_id` key per object still accepted); legacy top-level `{entry_id, entities}` is not supported"
                    .into(),
            ),
        ));
    }
    let seeds: Vec<CapabilitySeed> = serde_json::from_value(
        v.get("seeds")
            .cloned()
            .ok_or_else(|| {
                CallToolError::invalid_arguments(
                    tool,
                    Some(
                        "missing seeds: expected `seeds` as non-empty array of `{api, entity}` objects (legacy `entry_id` key accepted)".into(),
                    ),
                )
            })?,
    )
    .map_err(|e| CallToolError::invalid_arguments(tool, Some(e.to_string())))?;
    let seeds = normalize_capability_seeds(seeds);
    if seeds.is_empty() {
        return Err(CallToolError::invalid_arguments(
            tool,
            Some("`seeds` must be a non-empty array of {api, entity} objects (legacy `entry_id` key accepted)".into()),
        ));
    }
    Ok(seeds)
}

fn parse_optional_principal(v: &serde_json::Value) -> Option<String> {
    v.get("principal")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

#[cfg(feature = "code_mode")]
fn parse_required_string_arg(
    tool: &str,
    v: &serde_json::Value,
    key: &str,
) -> Result<String, CallToolError> {
    let s = v
        .get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            CallToolError::invalid_arguments(tool, Some(format!("missing `{key}` string")))
        })?;
    Ok(s.to_string())
}

#[cfg(feature = "code_mode")]
fn code_plan_session_mismatch(
    doc: &CodePlanArchiveDocument,
    prompt_hash: &str,
    session_id: &str,
    catalog_cgs_hash: &str,
    domain_revision: u32,
) -> Option<&'static str> {
    if doc.prompt_hash != prompt_hash || doc.session_id != session_id {
        return Some("execute_session");
    }
    if doc.catalog_cgs_hash != catalog_cgs_hash {
        return Some("catalog_cgs_hash");
    }
    if doc.domain_revision > domain_revision {
        return Some("domain_revision");
    }
    None
}

#[cfg(feature = "code_mode")]
fn code_plan_run_artifacts_from_meta(
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> (Vec<String>, Vec<CodePlanRunArtifactRef>) {
    fn dict_string<'a>(
        index_delta: Option<&'a serde_json::Value>,
        dict_name: &str,
        id: Option<u64>,
    ) -> Option<String> {
        let key = id?.to_string();
        index_delta?
            .get(dict_name)?
            .get(key)?
            .as_str()
            .map(str::to_string)
    }

    fn dict_string_vec(
        index_delta: Option<&serde_json::Value>,
        dict_name: &str,
        id: Option<u64>,
    ) -> Vec<String> {
        let Some(key) = id.map(|n| n.to_string()) else {
            return Vec::new();
        };
        index_delta
            .and_then(|v| v.get(dict_name))
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    let plasm = meta.and_then(|m| m.get("plasm"));
    let index_delta = plasm.and_then(|v| v.get("index_delta"));
    let Some(steps) = meta
        .and_then(|_| plasm)
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
    else {
        return (Vec::new(), Vec::new());
    };

    let mut run_ids = Vec::new();
    let mut refs = Vec::new();
    for step in steps {
        let Some(run_id) = step.get("run_id").and_then(|v| v.as_str()) else {
            continue;
        };
        run_ids.push(run_id.to_string());
        let request_fingerprints = step
            .get("request_fingerprints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .or_else(|| {
                let fp_id = step
                    .get("dict_ref")
                    .and_then(|v| v.get("fp"))
                    .and_then(|v| v.as_u64());
                Some(dict_string_vec(index_delta, "fp", fp_id))
            })
            .unwrap_or_default();
        let artifact_path = step
            .get("artifact_path")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                let path_id = step
                    .get("dict_ref")
                    .and_then(|v| v.get("artifact_path"))
                    .and_then(|v| v.as_u64());
                dict_string(index_delta, "artifact_path", path_id)
            });
        refs.push(CodePlanRunArtifactRef {
            run_id: run_id.to_string(),
            artifact_uri: step
                .get("artifact_uri")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            canonical_artifact_uri: step
                .get("canonical_artifact_uri")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            artifact_path,
            batch_step: step
                .get("batch_step")
                .and_then(|v| v.as_u64())
                .and_then(|n| usize::try_from(n).ok()),
            node_id: step
                .get("node_id")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            display: step
                .get("display")
                .or_else(|| step.get("expr_preview"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            request_fingerprints,
        });
    }
    (run_ids, refs)
}

fn parse_logical_session_ref_arg(
    tool: &str,
    v: &serde_json::Value,
) -> Result<String, CallToolError> {
    let s = v
        .get("logical_session_ref")
        .and_then(|x| x.as_str())
        .ok_or_else(|| {
            CallToolError::invalid_arguments(
                tool,
                Some("missing `logical_session_ref`: call `plasm_session_init` first".into()),
            )
        })?;
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('s') && t[1..].chars().all(|c| c.is_ascii_digit()) {
        Ok(t.to_string())
    } else {
        Err(CallToolError::invalid_arguments(
            tool,
            Some(
                "invalid `logical_session_ref`: expected a slot id like `s0` or `s1` from `plasm_session_init`"
                    .into(),
            ),
        ))
    }
}

/// Per MCP transport session: Plasm execute `prompt_hash` + `session` ids (same as HTTP paths).
#[derive(Clone, Default)]
struct PlasmExecBinding {
    prompt_hash: String,
    session_id: String,
}

/// Cumulative MCP-side text volume for token-ish telemetry (Unicode scalar counts).
#[derive(Clone, Default, Debug)]
pub(crate) struct McpSessionPlasmStats {
    /// Plasm instructions body from `add_capabilities` tool results.
    domain_prompt_chars: u64,
    /// `plasm` tool payloads: expression lines plus optional `reasoning`.
    plasm_invocation_chars: u64,
    /// Successful `plasm` tool Markdown bodies.
    plasm_response_chars: u64,
    plasm_call_count: u64,
}

/// Incremental state for `add_code_capabilities` TypeScript / facade clients.
#[cfg(feature = "code_mode")]
#[derive(Default, Clone)]
struct CodeModeMcpState {
    /// `(entry_id, entity)` already taught to the client in prior waves.
    emitted: plasm_facade_gen::ExposedSet,
    /// Whether the shared `Plasm` prelude from [`plasm_facade_gen::build_code_facade`] was sent.
    prelude_issued: bool,
    /// Last execute `(prompt_hash, session_id)` used for `facade_delta` generation.
    last_binding: Option<(String, String)>,
}

#[derive(Default)]
struct McpLogicalSessionState {
    binding: Option<PlasmExecBinding>,
    stats: McpSessionPlasmStats,
    meta_index: PlasmMetaIndex,
    #[cfg(feature = "code_mode")]
    code_mode: CodeModeMcpState,
}

#[derive(Default)]
struct McpTransportState {
    /// Logical session UUID string → per-agent state (execute binding, stats, `_meta.plasm` index).
    logical_by_id: HashMap<String, Arc<Mutex<McpLogicalSessionState>>>,
    /// Client-facing slot ids on this MCP transport (`s0`, …) → canonical logical session UUID.
    ref_to_uuid: HashMap<String, Uuid>,
    uuid_to_ref: HashMap<Uuid, String>,
    next_session_slot: u64,
}

impl McpTransportState {
    /// Assign a stable per-transport slot (`s{n}`) for this canonical logical id (idempotent).
    fn ensure_session_ref(&mut self, uuid: Uuid) -> String {
        if let Some(r) = self.uuid_to_ref.get(&uuid) {
            return r.clone();
        }
        let r = format!("s{}", self.next_session_slot);
        self.next_session_slot = self.next_session_slot.saturating_add(1);
        self.ref_to_uuid.insert(r.clone(), uuid);
        self.uuid_to_ref.insert(uuid, r.clone());
        r
    }
}

/// Rough token estimate for logging (Latin-heavy text; not a billing tokenizer).
#[inline]
fn mcp_chars_to_token_est(chars: u64) -> u64 {
    chars.saturating_add(3) / 4
}

/// Per `plasm` call: count expression + reasoning + optional TSV static frontmatter for invocation telemetry.
fn plasm_invocation_char_count(
    expressions: &[String],
    reasoning: Option<&str>,
    tsv_static_frontmatter: Option<&str>,
) -> u64 {
    let mut n: u64 = 0;
    for (i, line) in expressions.iter().enumerate() {
        if i > 0 {
            n = n.saturating_add(1);
        }
        n = n.saturating_add(line.chars().count() as u64);
    }
    if let Some(r) = reasoning {
        n = n.saturating_add(r.chars().count() as u64);
    }
    if let Some(f) = tsv_static_frontmatter {
        n = n.saturating_add(f.chars().count() as u64);
    }
    n
}

pub(crate) struct PlasmMcpHandler {
    plasm: Arc<PlasmHostState>,
    /// MCP transport session key -> per-session mutable state.
    session_states: Arc<RwLock<HashMap<String, Arc<Mutex<McpTransportState>>>>>,
}

impl PlasmMcpHandler {
    pub(crate) fn new(plasm: Arc<PlasmHostState>) -> Self {
        Self {
            plasm,
            session_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn session_state(&self, key: &str) -> Arc<Mutex<McpTransportState>> {
        {
            let g = self.session_states.read().await;
            if let Some(state) = g.get(key) {
                return Arc::clone(state);
            }
        }
        let mut g = self.session_states.write().await;
        Arc::clone(
            g.entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(McpTransportState::default()))),
        )
    }

    async fn logical_mutex(
        &self,
        transport_key: &str,
        logical_id: &str,
    ) -> Arc<Mutex<McpLogicalSessionState>> {
        let transport = self.session_state(transport_key).await;
        {
            let g = transport.lock().await;
            if let Some(entry) = g.logical_by_id.get(logical_id) {
                return Arc::clone(entry);
            }
        }
        let mut g = transport.lock().await;
        Arc::clone(
            g.logical_by_id
                .entry(logical_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(McpLogicalSessionState::default()))),
        )
    }

    async fn resolve_logical_session_ref_to_uuid(
        &self,
        tool: &str,
        transport_key: &str,
        ref_str: &str,
    ) -> Result<Uuid, CallToolError> {
        let transport = self.session_state(transport_key).await;
        let g = transport.lock().await;
        g.ref_to_uuid.get(ref_str).copied().ok_or_else(|| {
            CallToolError::invalid_arguments(
                tool,
                Some(
                    "unknown `logical_session_ref`: call `plasm_session_init` on this MCP connection first"
                        .into(),
                ),
            )
        })
    }

    /// Resolve execute binding: in-memory per-logical row first, then shared `logical_execute_bindings`.
    ///
    /// **Locking:** drop the per-logical mutex before reading `logical_execute_bindings` so we never
    /// nest that mutex with the host `RwLock` (consistent lock order vs writers elsewhere).
    async fn resolve_binding_for_logical(
        &self,
        transport_key: &str,
        logical_uuid: Uuid,
    ) -> Option<PlasmExecBinding> {
        let lid = logical_uuid.to_string();
        let ls = self.logical_mutex(transport_key, &lid).await;
        let g = ls.lock().await;
        if let Some(b) = &g.binding {
            return Some(b.clone());
        }
        drop(g);
        let map = self.plasm.logical_execute_bindings.read().await;
        map.get(&logical_uuid).map(|(ph, sid)| PlasmExecBinding {
            prompt_hash: ph.clone(),
            session_id: sid.clone(),
        })
    }

    async fn mcp_plasm_token_snapshot_logical(
        &self,
        transport_key: &str,
        logical_id: &str,
    ) -> (u64, u64, u64, u64) {
        let ls = self.logical_mutex(transport_key, logical_id).await;
        let g = ls.lock().await;
        let tp = mcp_chars_to_token_est(g.stats.domain_prompt_chars);
        let ti = mcp_chars_to_token_est(g.stats.plasm_invocation_chars);
        let tr = mcp_chars_to_token_est(g.stats.plasm_response_chars);
        (tp, ti, tr, tp.saturating_add(ti).saturating_add(tr))
    }

    /// Latest tenant MCP policy for this transport session (from HTTP `Authorization` + control-plane store).
    async fn tenant_mcp_cfg(
        &self,
        runtime: &Arc<dyn McpServer>,
    ) -> Result<Option<Arc<McpRuntimeConfig>>, CallToolError> {
        let has_tenant_configs = match self.plasm.mcp_config_repository() {
            Some(r) => r.has_tenant_configs().await.unwrap_or(false),
            None => false,
        };
        let auth = runtime.auth_info_cloned().await;
        let Some(info) = auth else {
            if has_tenant_configs {
                return Err(CallToolError::from_message(
                    "MCP Authorization required: send `Authorization: Bearer <api_key>` (tenant MCP API key from control plane).",
                ));
            }
            return Ok(None);
        };

        if is_anonymous_mcp_auth(&info) {
            return Ok(None);
        }

        let Some(id) = config_id_from_auth_info(&info) else {
            if has_tenant_configs {
                return Err(CallToolError::from_message(
                    "MCP Authorization missing tenant binding (expected Bearer API key).",
                ));
            }
            return Ok(None);
        };

        let Some(repo) = self.plasm.mcp_config_repository() else {
            return Ok(None);
        };

        let Some(cfg) = repo.get_runtime_config(&id).await.map_err(|_| {
            CallToolError::from_message(
                "Tenant MCP configuration store failed while loading policy.",
            )
        })?
        else {
            return Err(CallToolError::from_message(
                "Tenant MCP configuration is no longer available (disabled or revoked on the agent).",
            ));
        };
        if cfg.space_type == "personal" && cfg.owner_subject.is_none() {
            return Err(CallToolError::from_message(
                "Personal MCP configuration is missing owner binding metadata. Re-provision from control plane.",
            ));
        }

        Ok(Some(Arc::new(cfg)))
    }

    async fn mcp_principal_from_transport_auth(
        &self,
        runtime: &Arc<dyn McpServer>,
    ) -> Option<TenantPrincipal> {
        let info = runtime.auth_info_cloned().await?;
        let tenant_id = info.client_id?;
        let subject = info.user_id?;
        if tenant_id.trim().is_empty() || subject.trim().is_empty() {
            return None;
        }
        let method = if info
            .extra
            .as_ref()
            .and_then(|m| m.get("plasm_mcp_oauth"))
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            IncomingAuthMethod::Jwt
        } else {
            IncomingAuthMethod::ApiKey
        };
        Some(TenantPrincipal {
            tenant_id,
            subject,
            method,
        })
    }

    fn incoming_mode(&self) -> IncomingAuthMode {
        self.plasm
            .incoming_auth
            .as_ref()
            .map(|v| v.mode())
            .unwrap_or(IncomingAuthMode::Off)
    }

    /// Ensures MCP tool calls satisfy `PLASM_INCOMING_AUTH_MODE` (principal from MCP transport auth: API key / OAuth).
    async fn ensure_mcp_principal(
        &self,
        _mcp_key: &str,
        runtime: &Arc<dyn McpServer>,
    ) -> Result<Option<TenantPrincipal>, CallToolError> {
        let mode = self.incoming_mode();
        let p = self.mcp_principal_from_transport_auth(runtime).await;
        if mode == IncomingAuthMode::Required && p.is_none() {
            return Err(CallToolError::from_message(
                "incoming auth required: authenticate the MCP transport with a valid bearer credential",
            ));
        }
        Ok(p)
    }

    async fn trace_session_meta(
        &self,
        _mcp_key: &str,
        runtime: &Arc<dyn McpServer>,
    ) -> crate::trace_hub::TraceSessionMeta {
        use crate::trace_hub::{McpConfigRef, TraceSessionMeta};
        let tenant_incoming = self
            .mcp_principal_from_transport_auth(runtime)
            .await
            .map(|p| p.tenant_id);
        let (tenant_mcp, mcp_config) = match self.tenant_mcp_cfg(runtime).await {
            Ok(Some(cfg)) => (
                Some(cfg.tenant_id.clone()),
                Some(McpConfigRef {
                    config_id: cfg.id.to_string(),
                    tenant_id: cfg.tenant_id.clone(),
                }),
            ),
            _ => (None, None),
        };
        let tenant_id = tenant_incoming
            .or(tenant_mcp)
            .unwrap_or_else(|| "anonymous".to_string());
        TraceSessionMeta {
            tenant_id,
            project_slug: "main".into(),
            mcp_config,
        }
    }

    fn plasm_tools() -> Vec<Tool> {
        let mut init_props = BTreeMap::new();
        init_props.insert(
            "client_session_key".into(),
            json_schema_string_type(
                "Stable handle for one ongoing workspace/task (e.g. one id for the whole chat). Same key + tenant reuses the same logical session—do not rotate a new key every user message.",
            ),
        );
        let mut discover_props = BTreeMap::new();
        discover_props.insert(
            "query".into(),
            json_schema_string_or_string_array(
                "What to find: one string (plain-language or keyword intent) is tokenized; or a non-empty array of strings. Scored against capabilities and entity text; the reply is a TSV of `api` / `entity` / `description` rows.",
            ),
        );
        let mut add_props = BTreeMap::new();
        add_props.insert(
            "logical_session_ref".into(),
            json_schema_string_type(
                "Session slot from `plasm_session_init` (e.g. `s0`). Reuse for every `add_capabilities` / `plasm` in this workspace—do not request a new slot unless you intentionally start a new logical session.",
            ),
        );
        add_props.insert(
            "seeds".into(),
            json_schema_non_empty_object_array(
                "Non-empty JSON array of seed objects: each must include `api` (registry catalog id) and `entity` (CGS entity name). Example: [{\"api\":\"pokeapi\",\"entity\":\"Pokemon\"}]. Legacy per-object key `entry_id` is accepted as an alias for `api`.",
                vec!["api", "entity"],
            ),
        );
        let mut run_props = BTreeMap::new();
        run_props.insert(
            "logical_session_ref".into(),
            json_schema_string_type(
                "Same `logical_session_ref` you already use for this task—call `plasm` repeatedly with it; do not pair every `plasm` with a fresh `add_capabilities` unless appending new seeds.",
            ),
        );
        run_props.insert(
            "expressions".into(),
            json_schema_non_empty_string_array(
                "Non-empty array of executable lines—one string per line, using shapes from the TSV teaching table in `add_capabilities` (and optional `tsv_static_frontmatter` for the `#` contract, same as `_meta.plasm.tsv_static_frontmatter` on first open).",
            ),
        );
        run_props.insert(
            "reasoning".into(),
            json_schema_string_type("Optional note for the caller only; not executed."),
        );
        run_props.insert(
            "tsv_static_frontmatter".into(),
            json_schema_string_type(
                "Optional. The `#`-comment Plasm language contract (TSV symbolic mode) — use the value from the first `add_capabilities` result `_meta.plasm.tsv_static_frontmatter`; not executed; not duplicated in the teaching table body. Repeat on `plasm` when the model needs the contract in tool context.",
            ),
        );

        let mut tools = vec![
            Tool {
                name: "plasm_session_init".into(),
                title: Some("Open Plasm logical session".into()),
                description: Some(
                    "**Call once per stable workspace** before other Plasm tools (same MCP connection). Pass **one** ongoing **`client_session_key`** for the whole task—not a new id each message. Server **reuses** the logical session for that key + tenant; response **`logical_session_ref`** (`s0`, …) should stay your default for **`add_capabilities`** / **`plasm`** until you deliberately start a new workspace.".into(),
                ),
                input_schema: ToolInputSchema::new(
                    vec!["client_session_key".into()],
                    Some(init_props),
                    None,
                ),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                execution: Some(ToolExecution {
                    task_support: Some(ToolExecutionTaskSupport::Forbidden),
                }),
                icons: vec![],
                meta: None,
                output_schema: None,
            },
            Tool {
                name: "discover_capabilities".into(),
                title: None,
                description: Some(
                    "**Requires `plasm_session_init` first** (same connection). **Search the catalog** with `query` — a **single string** (natural language or keywords) or an **array of strings**; each is tokenized and scored against capabilities and entity domains; **rows are entities**. **Skip** if you already know catalog **`api`** ids and entity names. \
                     Reply: **TSV in a fenced block** — `api`, `entity`, `description` (entity blurb). Use **`api`** + **`entity`** for each `add_capabilities` seed (`entry_id` is accepted as a legacy alias in JSON).".into(),
                ),
                input_schema: ToolInputSchema::new(vec![], Some(discover_props), None),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                execution: Some(ToolExecution {
                    task_support: Some(ToolExecutionTaskSupport::Forbidden),
                }),
                icons: vec![],
                meta: None,
                output_schema: None,
            },
            Tool {
                name: "add_capabilities".into(),
                title: None,
                description: Some(
                    "**Append** catalog surface to an **existing** session: reuse **`logical_session_ref`** from **`plasm_session_init`**. Call when you need **new** **`api`/`entity`** pairs; **do not** repeat this with identical seeds before every **`plasm`** call. \
                     Set `seeds` as JSON objects (`{\"api\":\"...\",\"entity\":\"...\"}`), one object per entity (`entry_id` accepted instead of `api`). \
                     Example: `[{\"api\":\"pokeapi\",\"entity\":\"Pokemon\"},{\"api\":\"pokeapi\",\"entity\":\"Move\"}]`. \
                     First distinct **`api`** is the primary open; additional **`api`** values federate into the **same** Plasm language session. \
                     Legacy top-level `{entry_id, entities}` input is invalid; always send one seed object per entity. \
                     Unknown or disallowed catalog ids fail the whole call. \
                     On a **new** TSV open, the fenced result is the **teaching table only**; the Plasm language **contract** (leading `#` comments) is in **`_meta.plasm.tsv_static_frontmatter`**. **Cache** it and pass with each **`plasm`** as **`tsv_static_frontmatter`**; execute expressions with **`plasm`**. Symbol `eN` exists only up to **N** exposed entities. One execute binding per **logical** session.".into(),
                ),
                input_schema: ToolInputSchema::new(
                    vec!["logical_session_ref".into(), "seeds".into()],
                    Some(add_props.clone()),
                    None,
                ),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                execution: Some(ToolExecution {
                    task_support: Some(ToolExecutionTaskSupport::Forbidden),
                }),
                icons: vec![],
                meta: None,
                output_schema: None,
            },
        ];
        #[cfg(feature = "code_mode")]
        {
            let mut eval_props = BTreeMap::new();
            eval_props.insert(
                "logical_session_ref".into(),
                json_schema_string_type(
                    "From `plasm_session_init` (e.g. `s0`); the execute session must be open via `add_code_capabilities` for the entities the TypeScript facade uses.",
                ),
            );
            eval_props.insert(
                "name".into(),
                json_schema_string_type("Stable human-readable name for the archived program; use a short task-oriented name so the `pN` handle is auditable."),
            );
            eval_props.insert(
                "code".into(),
                json_schema_string_type(
                    "Complete TypeScript Code Mode program that would satisfy the user's intent if executed. Use it for multiple coordinated operations, transformations, compute, fan-out/fan-in, or reuse -- not as a query interface. Keep output minimal: use `.select(...)` / `Plan.project` for required fields and `Plan.return(...)` only for final answer nodes.",
                ),
            );
            let mut execute_plan_props = BTreeMap::new();
            execute_plan_props.insert(
                "logical_session_ref".into(),
                json_schema_string_type("Same logical session used to evaluate the plan; stale symbol spaces must re-evaluate before execution."),
            );
            execute_plan_props.insert(
                "plan_handle".into(),
                json_schema_string_type("Monotonic handle from `evaluate_code_plan`, e.g. `p1`; execute this reviewed dry-run plan by handle instead of resending TypeScript."),
            );
            tools.push(Tool {
                name: "add_code_capabilities".into(),
                title: None,
                description: Some(
                    "Open capabilities for Code Mode program authoring. Same seeds as **`add_capabilities`**, plus **`_meta.plasm.facade_delta`** and prompt-facing **typescript** (`.d.ts`-style fragments; prelude on first or new symbol space). Use only when synthesizing a program with multiple coordinated operations, transformations, compute, fan-out/fan-in, or reuse; prefer **`plasm`** for one simple expression. Code Mode is **not a query interface**. Keep output minimal with **`.select(...)`**, **`Plan.project`**, and a narrow **`Plan.return(...)`** containing final answer nodes only.".into(),
                ),
                input_schema: ToolInputSchema::new(
                    vec!["logical_session_ref".into(), "seeds".into()],
                    Some(add_props.clone()),
                    None,
                ),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                execution: Some(ToolExecution {
                    task_support: Some(ToolExecutionTaskSupport::Forbidden),
                }),
                icons: vec![],
                meta: None,
                output_schema: None,
            });
            tools.push(Tool {
                name: "evaluate_code_plan".into(),
                title: None,
                description: Some(
                    "Evaluate a complete named TypeScript Code Mode program, archive the validated Plan permanently, and return a small **`plan_handle`** plus compact dry-run execution DAG. This is plan validation and review, not a query endpoint and usually not the final answer. If the dry-run program satisfies the user's intent with acceptable risk and minimal output, follow with **`execute_code_plan(plan_handle)`**. Revise and re-evaluate only when the dry run shows a defect, missing capability, excessive output, or unacceptable risk. Do not send TypeScript again once a handle exists unless changing the program or symbol space. Start uncertain list/feed plans with **`Plan.limit(...)`** before widening. Author for minimal output: project/select source fields and **`Plan.return(...)`** only the final nodes the user needs.".into(),
                ),
                input_schema: ToolInputSchema::new(
                    vec!["logical_session_ref".into(), "name".into(), "code".into()],
                    Some(eval_props),
                    None,
                ),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                execution: Some(ToolExecution {
                    task_support: Some(ToolExecutionTaskSupport::Forbidden),
                }),
                icons: vec![],
                meta: None,
                output_schema: None,
            });
            tools.push(Tool {
                name: "execute_code_plan".into(),
                title: None,
                description: Some(
                    "Execute a previously evaluated and reviewed Code Mode program by **`plan_handle`** (for example `p1`). This is the expected follow-up after a satisfactory dry-run from **`evaluate_code_plan`**; use the handle instead of resending code. The response publishes only nodes named by **`Plan.return(...)`** and uses the same Markdown, **`_meta.plasm.steps`**, resource links, and paging conventions as **`plasm`**. Because only returned nodes are published, programs should return final answer data only and use artifact/resource links for full snapshots instead of returning wide intermediates.".into(),
                ),
                input_schema: ToolInputSchema::new(
                    vec!["logical_session_ref".into(), "plan_handle".into()],
                    Some(execute_plan_props),
                    None,
                ),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                execution: Some(ToolExecution {
                    task_support: Some(ToolExecutionTaskSupport::Forbidden),
                }),
                icons: vec![],
                meta: None,
                output_schema: None,
            });
        }
        tools.push(Tool {
            name: "plasm".into(),
            title: None,
            description: Some(MCP_PLASM_TOOL_DESCRIPTION.into()),
            input_schema: ToolInputSchema::new(
                vec!["logical_session_ref".into(), "expressions".into()],
                Some(run_props),
                None,
            ),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(false),
                open_world_hint: Some(true),
                ..Default::default()
            }),
            execution: Some(ToolExecution {
                task_support: Some(ToolExecutionTaskSupport::Forbidden),
            }),
            icons: vec![],
            meta: None,
            output_schema: None,
        });
        tools
    }
}

fn json_schema_string_type(description: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("type".into(), serde_json::json!("string"));
    m.insert(
        "description".into(),
        serde_json::Value::String(description.to_string()),
    );
    m
}

fn json_schema_string_array(description: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut items = serde_json::Map::new();
    items.insert("type".into(), serde_json::json!("string"));
    let mut m = serde_json::Map::new();
    m.insert("type".into(), serde_json::json!("array"));
    m.insert("items".into(), serde_json::Value::Object(items));
    m.insert(
        "description".into(),
        serde_json::Value::String(description.to_string()),
    );
    m
}

fn json_schema_string_or_string_array(description: &str) -> serde_json::Map<String, serde_json::Value> {
    let v = serde_json::json!({
        "description": description,
        "oneOf": [
            {
                "type": "string",
                "minLength": 1,
                "description": "One intent or keyword phrase; tokenized for search."
            },
            {
                "type": "array",
                "minItems": 1,
                "items": { "type": "string" },
                "description": "Multiple search strings (each tokenized; OR-style coverage via shared token set)."
            }
        ]
    });
    match v {
        serde_json::Value::Object(m) => m,
        _ => unreachable!(),
    }
}

fn json_schema_non_empty_string_array(
    description: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut m = json_schema_string_array(description);
    m.insert("minItems".into(), serde_json::json!(1));
    m
}

fn json_schema_non_empty_object_array(
    description: &str,
    required_fields: Vec<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut item_props = serde_json::Map::new();
    item_props.insert(
        "api".into(),
        serde_json::Value::Object(json_schema_string_type(
            "Registry catalog id for one API (discover TSV column `api`; legacy JSON key `entry_id` accepted)",
        )),
    );
    item_props.insert(
        "entity".into(),
        serde_json::Value::Object(json_schema_string_type("Entity id/name in that catalog")),
    );
    let mut item_obj = serde_json::Map::new();
    item_obj.insert("type".into(), serde_json::json!("object"));
    item_obj.insert("properties".into(), serde_json::Value::Object(item_props));
    item_obj.insert(
        "required".into(),
        serde_json::Value::Array(
            required_fields
                .into_iter()
                .map(|f| serde_json::Value::String(f.to_string()))
                .collect(),
        ),
    );
    let mut m = serde_json::Map::new();
    m.insert("type".into(), serde_json::json!("array"));
    m.insert("items".into(), serde_json::Value::Object(item_obj));
    m.insert("minItems".into(), serde_json::json!(1));
    m.insert(
        "description".into(),
        serde_json::Value::String(description.to_string()),
    );
    m
}

fn args_value(params: &CallToolRequestParams) -> serde_json::Value {
    serde_json::Value::Object(params.arguments.clone().unwrap_or_default())
}

/// MCP `discover_capabilities` accepts `query` as one string (intent / keywords) or a string array.
/// Each entry is fed into [`CapabilityQuery::tokens`] and tokenized the same as HTTP discovery.
fn mcp_discover_query_from_arguments(v: &serde_json::Value) -> Result<CapabilityQuery, String> {
    let Some(obj) = v.as_object() else {
        return Err("discover_capabilities arguments must be a JSON object".to_string());
    };
    let q = obj.get("query");
    let tokens: Vec<String> = match q {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::String(s)) => {
            if s.is_empty() {
                Vec::new()
            } else {
                vec![s.clone()]
            }
        }
        Some(serde_json::Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    serde_json::Value::String(s) if !s.is_empty() => out.push(s.clone()),
                    serde_json::Value::String(_) => {}
                    _ => {
                        return Err(
                            "discover_capabilities `query` array must contain only strings"
                                .to_string(),
                        );
                    }
                }
            }
            out
        }
        Some(_) => {
            return Err(
                "discover_capabilities `query` must be a string or an array of strings"
                    .to_string(),
            );
        }
    };
    Ok(CapabilityQuery {
        tokens,
        phrases: vec![],
        ..CapabilityQuery::default()
    })
}

fn discovery_mcp_error(e: DiscoveryError) -> CallToolError {
    match e {
        DiscoveryError::EmptyQuery => {
            CallToolError::invalid_arguments("discover_capabilities", Some(e.to_string()))
        }
        DiscoveryError::UnknownEntry(_) => CallToolError::from_message(format!("catalog: {e}")),
    }
}

/// MCP entity `description` column: max chars (Unicode scalars).
const MCP_DISCOVERY_ENTITY_SUMMARY_MAX: usize = 200;

/// Single-line TSV field: collapse whitespace, strip tabs, truncate (Unicode scalars).
fn mcp_discovery_tsv_field(s: &str, max_chars: usize) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let no_tabs = collapsed.replace('\t', " ");
    let n = no_tabs.chars().count();
    if n <= max_chars {
        no_tabs
    } else {
        let head: String = no_tabs.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

fn entity_summary_description<'a>(
    entity_summaries: &'a [plasm_core::discovery::EntitySummary],
    entity: &str,
) -> Option<&'a str> {
    entity_summaries
        .iter()
        .find(|e| e.name == entity)
        .map(|e| e.description.as_str())
}

fn format_discovery_tsv_body(
    candidates: &[plasm_core::discovery::RankedCandidate],
    entity_summaries: &[plasm_core::discovery::EntitySummary],
) -> String {
    let mut by_entry: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in candidates {
        by_entry
            .entry(c.entry_id.clone())
            .or_default()
            .insert(c.entity.clone());
    }

    let mut lines = vec!["api\tentity\tdescription".to_string()];
    for (eid, entities) in &by_entry {
        for entity in entities {
            let description = entity_summary_description(entity_summaries, entity)
                .map(|raw| mcp_discovery_tsv_field(raw, MCP_DISCOVERY_ENTITY_SUMMARY_MAX))
                .unwrap_or_default();
            lines.push(format!(
                "{}\t{}\t{}",
                mcp_discovery_tsv_field(eid, 200),
                mcp_discovery_tsv_field(entity, 200),
                description,
            ));
        }
    }
    lines.join("\n")
}

fn format_discovery_markdown(r: &plasm_core::discovery::DiscoveryResult) -> String {
    use plasm_core::discovery::Ambiguity;

    let mut s = String::new();
    if r.candidates.is_empty() {
        s.push_str("_No matching entities._\n\n");
    } else {
        let body = format_discovery_tsv_body(&r.candidates, &r.entity_summaries);
        s.push_str("```tsv\n");
        s.push_str(&body);
        s.push_str("\n```\n\n");
    }

    if !r.ambiguities.is_empty() {
        s.push_str("**Ambiguities**\n\n");
        for Ambiguity {
            dimension,
            entry_ids,
            capability_name,
            score,
        } in &r.ambiguities
        {
            s.push_str(&format!(
                "- **{dimension}** `{capability_name}` score={score} entries: {}\n",
                entry_ids.join(", ")
            ));
        }
        s.push('\n');
    }

    s
}

fn mcp_key(runtime: &Arc<dyn McpServer>) -> Result<String, CallToolError> {
    runtime.session_id().ok_or_else(|| {
        CallToolError::from_message(
            "MCP session not ready: complete the MCP initialize handshake before calling tools.",
        )
    })
}

fn mcp_call_tool_error_class(err: &CallToolError) -> &'static str {
    let msg = err.to_string();
    if msg.contains("entry_id not allowed by tenant MCP configuration") {
        return "entry_not_allowed";
    }
    if msg.contains("incoming auth required") {
        return "incoming_auth_required";
    }
    if msg.contains("MCP Authorization missing tenant binding") {
        return "missing_tenant_binding";
    }
    if msg.contains("Tenant MCP configuration is no longer available") {
        return "tenant_mcp_unavailable";
    }
    if msg.contains("Personal MCP configuration is missing owner binding") {
        return "owner_binding_missing";
    }
    if msg.contains("MCP Authorization required") {
        return "mcp_authorization_required";
    }
    "call_tool_error"
}

fn mcp_truncate_resource_uri_display(uri: &str) -> String {
    const MAX: usize = 160;
    if uri.chars().count() <= MAX {
        uri.to_string()
    } else {
        format!(
            "{}…",
            uri.chars().take(MAX.saturating_sub(1)).collect::<String>()
        )
    }
}

fn mcp_artifact_payload_chars(payload: &ArtifactPayload) -> (u64, bool) {
    match std::str::from_utf8(&payload.bytes) {
        Ok(s) => (s.chars().count() as u64, false),
        Err(_) => (payload.bytes.len() as u64, true),
    }
}

fn read_resource_result_for_payload(
    uri: &str,
    payload: ArtifactPayload,
) -> Result<ReadResourceResult, RpcError> {
    let maybe_utf8 = std::str::from_utf8(&payload.bytes)
        .ok()
        .map(|s| s.to_string());
    Ok(ReadResourceResult {
        contents: vec![if let Some(text) = maybe_utf8 {
            ReadResourceContent::TextResourceContents(TextResourceContents {
                meta: None,
                mime_type: Some(payload.metadata.content_type),
                text,
                uri: uri.to_string(),
            })
        } else {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&payload.bytes);
            ReadResourceContent::BlobResourceContents(
                BlobResourceContents::new(b64, uri.to_string())
                    .with_mime_type(payload.metadata.content_type),
            )
        }],
        meta: None,
    })
}

impl PlasmMcpHandler {
    #[allow(clippy::too_many_arguments)]
    async fn emit_mcp_resource_read_trace(
        &self,
        logical_session_trace_key: Option<&str>,
        archive: Option<RunArtifactArchiveRef>,
        uri: &str,
        maybe_payload: Option<&ArtifactPayload>,
        started: Instant,
        result: &str,
        error_class: Option<&str>,
    ) {
        let Some(mcp_key) = logical_session_trace_key.filter(|s| !s.is_empty()) else {
            return;
        };
        let (chars_added, is_binary) = maybe_payload
            .map(mcp_artifact_payload_chars)
            .unwrap_or((0, false));
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        self.plasm
            .trace_hub
            .trace_record_mcp_resource_read(
                mcp_key,
                archive,
                mcp_truncate_resource_uri_display(uri),
                chars_added,
                is_binary,
                duration_ms,
                result,
                error_class,
            )
            .await;
    }
}

#[async_trait]
impl ServerHandler for PlasmMcpHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: Self::plasm_tools(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_list_resources_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListResourcesResult, RpcError> {
        Ok(ListResourcesResult {
            resources: vec![],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_list_resource_templates_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListResourceTemplatesResult, RpcError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate {
                    annotations: None,
                    description: Some(
                        "Typed bytes for one execute run artifact. `prompt_hash` and `session_id` match `add_capabilities`; `run_id` is in `plasm` result metadata."
                            .into(),
                    ),
                    icons: vec![],
                    meta: None,
                    mime_type: Some("application/octet-stream".into()),
                    name: "plasm_execute_run".into(),
                    title: Some("Plasm execute run artifact (canonical)".into()),
                    uri_template: "plasm://execute/{prompt_hash}/{session_id}/run/{run_id}".into(),
                },
                ResourceTemplate {
                    annotations: None,
                    description: Some(
                        "Short alias for the same snapshot JSON as the canonical URI. `logical_session_ref` is the slot from `plasm_session_init` (`s0`, …); `n` is monotonic within that logical session’s execute binding."
                            .into(),
                    ),
                    icons: vec![],
                    meta: None,
                    mime_type: Some("application/octet-stream".into()),
                    name: "plasm_execute_run_short".into(),
                    title: Some("Plasm execute run artifact (short index)".into()),
                    uri_template: "plasm://session/{logical_session_ref}/r/{n}".into(),
                },
                ResourceTemplate {
                    annotations: None,
                    description: Some(
                        "Permanent archived Code Mode plan. `plan_id` is returned by `evaluate_code_plan` metadata."
                            .into(),
                    ),
                    icons: vec![],
                    meta: None,
                    mime_type: Some("application/json".into()),
                    name: "plasm_code_plan".into(),
                    title: Some("Plasm Code Mode plan archive (canonical)".into()),
                    uri_template: "plasm://execute/{prompt_hash}/{session_id}/plan/{plan_id}".into(),
                },
                ResourceTemplate {
                    annotations: None,
                    description: Some(
                        "Short alias for an archived Code Mode plan. `logical_session_ref` is the slot from `plasm_session_init`; `n` is the monotonic plan handle number."
                            .into(),
                    ),
                    icons: vec![],
                    meta: None,
                    mime_type: Some("application/json".into()),
                    name: "plasm_code_plan_short".into(),
                    title: Some("Plasm Code Mode plan archive (short index)".into()),
                    uri_template: "plasm://session/{logical_session_ref}/p/{n}".into(),
                },
            ],
            meta: None,
            next_cursor: None,
        })
    }

    #[tracing::instrument(
        skip(self, runtime),
        name = "plasm_agent.mcp.resources.read_request",
        level = "trace"
    )]
    async fn handle_read_resource_request(
        &self,
        params: ReadResourceRequestParams,
        runtime: Arc<dyn McpServer>,
    ) -> Result<ReadResourceResult, RpcError> {
        let started = Instant::now();
        let uri = params.uri.trim();
        if let Some((segment, plan_index)) = parse_plasm_session_short_plan_uri(uri) {
            let Some(transport_key) = runtime.session_id() else {
                crate::metrics::record_mcp_resource_read(
                    "code_plan_short",
                    "error",
                    "session_not_ready",
                    started.elapsed(),
                );
                return Err(RpcError::invalid_params().with_message(
                    "MCP session not ready: complete the initialize handshake before resources/read.",
                ));
            };
            let logical_uuid = match segment {
                LogicalSessionUriSegment::Uuid(u) => u,
                LogicalSessionUriSegment::Slot(s) => {
                    let transport = self.session_state(&transport_key).await;
                    let g = transport.lock().await;
                    let Some(u) = g.ref_to_uuid.get(&s).copied() else {
                        crate::metrics::record_mcp_resource_read(
                            "code_plan_short",
                            "error",
                            "unknown_session_ref",
                            started.elapsed(),
                        );
                        return Err(RpcError::invalid_params()
                            .with_message("unknown logical session slot in Code Mode plan URI"));
                    };
                    u
                }
            };
            let ls_key = logical_uuid.to_string();
            let binding = {
                let map = self.plasm.logical_execute_bindings.read().await;
                map.get(&logical_uuid).map(|(ph, sid)| PlasmExecBinding {
                    prompt_hash: ph.clone(),
                    session_id: sid.clone(),
                })
            };
            let Some(b) = binding else {
                crate::metrics::record_mcp_resource_read(
                    "code_plan_short",
                    "error",
                    "no_binding",
                    started.elapsed(),
                );
                return Err(RpcError::invalid_params().with_message(
                    "no execute session for this logical session: call add_code_capabilities with seeds first",
                ));
            };
            let payload = self
                .plasm
                .run_artifacts
                .get_code_plan_payload_result_by_index(
                    b.prompt_hash.as_str(),
                    b.session_id.as_str(),
                    plan_index,
                )
                .await
                .map_err(|e| {
                    RpcError::internal_error().with_message(format!("code plan decode failed: {e}"))
                })?;
            let Some(payload) = payload else {
                crate::metrics::record_mcp_resource_read(
                    "code_plan_short",
                    "error",
                    "unknown_plan",
                    started.elapsed(),
                );
                return Err(RpcError::invalid_params().with_message(format!(
                    "unknown Code Mode plan index {plan_index} for this session"
                )));
            };
            crate::metrics::record_mcp_resource_read(
                "code_plan_short",
                "success",
                "none",
                started.elapsed(),
            );
            self.emit_mcp_resource_read_trace(
                Some(&ls_key),
                None,
                uri,
                Some(&payload),
                started,
                "success",
                None,
            )
            .await;
            return read_resource_result_for_payload(uri, payload);
        }
        if let Some((segment, resource_index)) = parse_plasm_session_short_resource_uri(uri) {
            let Some(transport_key) = runtime.session_id() else {
                crate::metrics::record_mcp_resource_read(
                    "logical_short",
                    "error",
                    "session_not_ready",
                    started.elapsed(),
                );
                return Err(RpcError::invalid_params().with_message(
                    "MCP session not ready: complete the initialize handshake before resources/read.",
                ));
            };
            let logical_uuid = match segment {
                LogicalSessionUriSegment::Uuid(u) => u,
                LogicalSessionUriSegment::Slot(s) => {
                    let transport = self.session_state(&transport_key).await;
                    let g = transport.lock().await;
                    let Some(u) = g.ref_to_uuid.get(&s).copied() else {
                        crate::metrics::record_mcp_resource_read(
                            "logical_short",
                            "error",
                            "unknown_session_ref",
                            started.elapsed(),
                        );
                        return Err(RpcError::invalid_params().with_message(
                            "unknown logical session slot in URI: use a `plasm://session/s{n}/r/...` URI from this connection after `plasm_session_init`, or the canonical `plasm://execute/.../run/...` URI.",
                        ));
                    };
                    u
                }
            };
            let ls_key = logical_uuid.to_string();
            let binding = {
                let map = self.plasm.logical_execute_bindings.read().await;
                map.get(&logical_uuid).map(|(ph, sid)| PlasmExecBinding {
                    prompt_hash: ph.clone(),
                    session_id: sid.clone(),
                })
            };
            let Some(b) = binding else {
                crate::metrics::record_mcp_resource_read(
                    "logical_short",
                    "error",
                    "no_binding",
                    started.elapsed(),
                );
                self.emit_mcp_resource_read_trace(
                    Some(&ls_key),
                    None,
                    uri,
                    None,
                    started,
                    "error",
                    Some("no_binding"),
                )
                .await;
                return Err(RpcError::invalid_params().with_message(
                    "no execute session for this logical session: call add_capabilities with seeds first",
                ));
            };
            let live_sess = self
                .plasm
                .sessions
                .get_by_strs(b.prompt_hash.as_str(), b.session_id.as_str())
                .await;
            let live_art = if let Some(ref sess) = live_sess {
                sess.core
                    .get_run_artifact_by_resource_index(resource_index)
                    .await
            } else {
                None
            };
            let live_payload = live_art.as_ref().map(|a| a.payload.clone());
            if live_payload.is_some() {
                crate::metrics::record_execute_artifact_resolve_layer("hot");
            }
            let persisted_payload = if live_payload.is_none() {
                match self
                    .plasm
                    .run_artifacts
                    .get_payload_result_by_resource_index(
                        b.prompt_hash.as_str(),
                        b.session_id.as_str(),
                        resource_index,
                    )
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        crate::metrics::record_mcp_resource_read(
                            "logical_short",
                            "error",
                            "decode_failed",
                            started.elapsed(),
                        );
                        let rid = self
                            .plasm
                            .run_artifacts
                            .resolve_run_id_for_resource_index(
                                b.prompt_hash.as_str(),
                                b.session_id.as_str(),
                                resource_index,
                            )
                            .await;
                        let arch = rid.map(|run_id| RunArtifactArchiveRef {
                            prompt_hash: b.prompt_hash.clone(),
                            session_id: b.session_id.clone(),
                            run_id,
                            resource_index: Some(resource_index),
                        });
                        self.emit_mcp_resource_read_trace(
                            Some(&ls_key),
                            arch,
                            uri,
                            None,
                            started,
                            "error",
                            Some("decode_failed"),
                        )
                        .await;
                        return Err(RpcError::internal_error()
                            .with_message(format!("run artifact decode failed: {e}")));
                    }
                }
            } else {
                None
            };
            if live_payload.is_none() && persisted_payload.is_some() {
                crate::metrics::record_execute_artifact_resolve_layer("archive");
            }
            let Some(payload) = live_payload.or(persisted_payload) else {
                crate::metrics::record_mcp_resource_read(
                    "logical_short",
                    "error",
                    "unknown_artifact",
                    started.elapsed(),
                );
                self.emit_mcp_resource_read_trace(
                    Some(&ls_key),
                    None,
                    uri,
                    None,
                    started,
                    "error",
                    Some("unknown_artifact"),
                )
                .await;
                return Err(RpcError::invalid_params().with_message(format!(
                    "unknown run artifact index {resource_index} for this session"
                )));
            };
            let run_id = live_art.as_ref().map(|a| a.run_id).or(self
                .plasm
                .run_artifacts
                .resolve_run_id_for_resource_index(
                    b.prompt_hash.as_str(),
                    b.session_id.as_str(),
                    resource_index,
                )
                .await);
            let archive = run_id.map(|run_id| RunArtifactArchiveRef {
                prompt_hash: b.prompt_hash.clone(),
                session_id: b.session_id.clone(),
                run_id,
                resource_index: Some(resource_index),
            });
            crate::spans::mcp_resource_read().in_scope(|| {
                tracing::info!(
                    target: "plasm_agent::mcp",
                    uri = %uri,
                    logical_session_id = %logical_uuid,
                    prompt_hash = %b.prompt_hash,
                    session_id = %b.session_id,
                    resource_index,
                    bytes = payload.bytes.len(),
                    "MCP resources/read"
                );
            });
            crate::metrics::record_mcp_resource_read(
                "logical_short",
                "success",
                "none",
                started.elapsed(),
            );
            self.emit_mcp_resource_read_trace(
                Some(&ls_key),
                archive,
                uri,
                Some(&payload),
                started,
                "success",
                None,
            )
            .await;
            return read_resource_result_for_payload(uri, payload);
        }

        if let Some((prompt_hash, session_id, plan_id)) = parse_plasm_execute_plan_uri(uri) {
            let payload = self
                .plasm
                .run_artifacts
                .get_code_plan_payload_result(&prompt_hash, &session_id, plan_id)
                .await
                .map_err(|e| {
                    RpcError::internal_error().with_message(format!("code plan decode failed: {e}"))
                })?;
            let Some(payload) = payload else {
                crate::metrics::record_mcp_resource_read(
                    "code_plan_canonical",
                    "error",
                    "unknown_plan",
                    started.elapsed(),
                );
                return Err(RpcError::invalid_params().with_message(
                    "unknown Code Mode plan (wrong plan_id or not yet stored for this session)",
                ));
            };
            crate::metrics::record_mcp_resource_read(
                "code_plan_canonical",
                "success",
                "none",
                started.elapsed(),
            );
            return read_resource_result_for_payload(uri, payload);
        }

        let Some((prompt_hash, session_id, run_id)) = parse_plasm_execute_run_uri(uri) else {
            crate::metrics::record_mcp_resource_read(
                "unsupported",
                "error",
                "unsupported_uri",
                started.elapsed(),
            );
            return Err(
                RpcError::invalid_params().with_message(format!("unsupported resource URI: {uri}"))
            );
        };
        let ls_key_opt = self
            .plasm
            .logical_session_id_for_execute_binding(prompt_hash.as_str(), session_id.as_str())
            .await
            .map(|u| u.to_string());
        let canonical_archive = RunArtifactArchiveRef {
            prompt_hash: prompt_hash.clone(),
            session_id: session_id.clone(),
            run_id,
            resource_index: None,
        };
        let live_sess = self
            .plasm
            .sessions
            .get_by_strs(prompt_hash.as_str(), session_id.as_str())
            .await;
        let live_payload = if let Some(sess) = &live_sess {
            sess.core
                .get_run_artifact(run_id)
                .await
                .map(|a| a.payload.clone())
        } else {
            None
        };
        if live_payload.is_some() {
            crate::metrics::record_execute_artifact_resolve_layer("hot");
        }
        let persisted_payload = if live_payload.is_none() {
            match self
                .plasm
                .run_artifacts
                .get_payload_result(&prompt_hash, &session_id, run_id)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    crate::metrics::record_mcp_resource_read(
                        "canonical",
                        "error",
                        "decode_failed",
                        started.elapsed(),
                    );
                    self.emit_mcp_resource_read_trace(
                        ls_key_opt.as_deref(),
                        Some(canonical_archive.clone()),
                        uri,
                        None,
                        started,
                        "error",
                        Some("decode_failed"),
                    )
                    .await;
                    return Err(RpcError::internal_error()
                        .with_message(format!("run artifact decode failed: {e}")));
                }
            }
        } else {
            None
        };
        if live_payload.is_none() && persisted_payload.is_some() {
            crate::metrics::record_execute_artifact_resolve_layer("archive");
        }
        let Some(payload) = live_payload.or(persisted_payload) else {
            crate::metrics::record_mcp_resource_read(
                "canonical",
                "error",
                "unknown_artifact",
                started.elapsed(),
            );
            self.emit_mcp_resource_read_trace(
                ls_key_opt.as_deref(),
                Some(canonical_archive.clone()),
                uri,
                None,
                started,
                "error",
                Some("unknown_artifact"),
            )
            .await;
            return Err(RpcError::invalid_params().with_message(
                "unknown run artifact (wrong run_id or not yet stored for this session)",
            ));
        };
        crate::spans::mcp_resource_read().in_scope(|| {
            tracing::info!(
                target: "plasm_agent::mcp",
                uri = %uri,
                prompt_hash = %prompt_hash,
                session_id = %session_id,
                run_id = %run_id,
                bytes = payload.bytes.len(),
                "MCP resources/read"
            );
        });
        crate::metrics::record_mcp_resource_read("canonical", "success", "none", started.elapsed());
        self.emit_mcp_resource_read_trace(
            ls_key_opt.as_deref(),
            Some(canonical_archive),
            uri,
            Some(&payload),
            started,
            "success",
            None,
        )
        .await;
        read_resource_result_for_payload(uri, payload)
    }

    #[tracing::instrument(
        skip(self, runtime),
        name = "plasm_agent.mcp.call_tool",
        fields(mcp.tool = %params.name),
        level = "trace"
    )]
    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        runtime: Arc<dyn McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        let key = mcp_key(&runtime)?;
        let v = args_value(&params);

        match params.name.as_str() {
            "plasm_session_init" => {
                let started = Instant::now();
                let res: Result<CallToolResult, CallToolError> = async {
                    let principal_incoming = self.ensure_mcp_principal(&key, &runtime).await?;
                    let client_session_key = v
                        .get("client_session_key")
                        .and_then(|x| x.as_str())
                        .ok_or_else(|| {
                        CallToolError::invalid_arguments(
                            "plasm_session_init",
                            Some("missing `client_session_key`".into()),
                        )
                    })?;
                    let scope = tenant_scope(principal_incoming.as_ref());
                    let rec = self
                        .plasm
                        .logical_sessions
                        .init_session(&scope, &ClientSessionKey::new(client_session_key))
                        .await;
                    let logical_session_ref = {
                        let transport = self.session_state(&key).await;
                        let mut g = transport.lock().await;
                        g.ensure_session_ref(rec.logical_session_id.as_uuid())
                    };
                    let execute_binding = {
                        let map = self.plasm.logical_execute_bindings.read().await;
                        map.get(&rec.logical_session_id.as_uuid())
                            .map(|(ph, sid)| json!({ "prompt_hash": ph, "session_id": sid }))
                    };
                    let body = serde_json::json!({
                        "logical_session_ref": logical_session_ref,
                        "client_session_key": rec.client_session_key.as_str(),
                        "tenant_scope": rec.tenant_scope,
                        "logical_session_id": rec.logical_session_id.to_string(),
                    })
                    .to_string();
                    let mut plasm_meta = serde_json::Map::new();
                    plasm_meta.insert(
                        "logical_session_id".to_string(),
                        json!(rec.logical_session_id.to_string()),
                    );
                    plasm_meta.insert(
                        "execute_binding".to_string(),
                        execute_binding.unwrap_or(serde_json::Value::Null),
                    );
                    let mut meta = serde_json::Map::new();
                    meta.insert("plasm".to_string(), serde_json::Value::Object(plasm_meta));
                    Ok(
                        CallToolResult::text_content(vec![TextContent::new(body, None, None)])
                            .with_meta(Some(meta)),
                    )
                }
                .instrument(crate::spans::mcp_tool_plasm_session_init())
                .await;
                let elapsed = started.elapsed();
                match &res {
                    Ok(_) => crate::metrics::record_mcp_tool(
                        "plasm_session_init",
                        None,
                        "success",
                        "none",
                        elapsed,
                    ),
                    Err(e) => crate::metrics::record_mcp_tool(
                        "plasm_session_init",
                        None,
                        "error",
                        mcp_call_tool_error_class(e),
                        elapsed,
                    ),
                }
                res
            }
            "discover_capabilities" => {
                let started = Instant::now();
                let res: Result<CallToolResult, CallToolError> = async {
                    self.ensure_mcp_principal(&key, &runtime).await?;
                    let q = mcp_discover_query_from_arguments(&v).map_err(|msg| {
                        CallToolError::invalid_arguments("discover_capabilities", Some(msg))
                    })?;
                    let discover_span = crate::spans::mcp_tool_discover_capabilities();
                    let _discover_guard = discover_span.enter();
                    tracing::info!(
                        target: "plasm_agent::mcp",
                        tool = "discover_capabilities",
                        query = ?q.tokens,
                        "MCP tool: discover_capabilities (search)"
                    );
                    let reg = self.plasm.catalog.snapshot();
                    let mut r = reg.discover(&q).map_err(discovery_mcp_error)?;
                    drop(_discover_guard);
                    let tcfg = self.tenant_mcp_cfg(&runtime).await?;
                    if let Some(cfg) = tcfg {
                        r = mcp_policy::filter_discovery_result(r, cfg.as_ref());
                    }
                    let text = format_discovery_markdown(&r);
                    Ok(CallToolResult::text_content(vec![TextContent::new(
                        text, None, None,
                    )]))
                }
                .await;
                let elapsed = started.elapsed();
                match &res {
                    Ok(_) => crate::metrics::record_mcp_tool(
                        "discover_capabilities",
                        None,
                        "success",
                        "none",
                        elapsed,
                    ),
                    Err(e) => crate::metrics::record_mcp_tool(
                        "discover_capabilities",
                        None,
                        "error",
                        mcp_call_tool_error_class(e),
                        elapsed,
                    ),
                }
                res
            }
            "add_capabilities" | "add_code_capabilities" => {
                let started = Instant::now();
                let is_add_code = params.name == "add_code_capabilities";
                let tname: &str = if is_add_code {
                    "add_code_capabilities"
                } else {
                    "add_capabilities"
                };
                let res: Result<CallToolResult, CallToolError> = async {
                    if is_add_code {
                        #[cfg(not(feature = "code_mode"))]
                        {
                            return Err(CallToolError::from_message(
                                "this build does not include Plasm Code Mode: use `add_capabilities`, or build with the `plasm-agent-core` `code_mode` feature",
                            ));
                        }
                    }
                    let principal_incoming = self.ensure_mcp_principal(&key, &runtime).await?;
                    let session_ref = parse_logical_session_ref_arg(tname, &v)?;
                    let logical_uuid = self
                        .resolve_logical_session_ref_to_uuid(tname, &key, &session_ref)
                        .await?;
                    let scope = tenant_scope(principal_incoming.as_ref());
                    if !self
                        .plasm
                        .logical_sessions
                        .verify_tenant(LogicalSessionId(logical_uuid), &scope)
                        .await
                    {
                        return Err(CallToolError::from_message(
                            "logical_session_ref is unknown or does not belong to this tenant scope",
                        ));
                    }
                    let ls_key = logical_uuid.to_string();
                    let seeds = parse_tool_seeds(tname, &v)?;
                    #[cfg(feature = "code_mode")]
                    let seed_pairs_for_facade: Vec<(String, String)> = if is_add_code {
                        seeds
                            .iter()
                            .map(|s| (s.entry_id.clone(), s.entity.clone()))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let principal = parse_optional_principal(&v);
                    let distinct_entries: Vec<String> = {
                        let mut seen = std::collections::HashSet::new();
                        let mut out = Vec::new();
                        for s in &seeds {
                            if seen.insert(s.entry_id.clone()) {
                                out.push(s.entry_id.clone());
                            }
                        }
                        out
                    };
                    let tcfg = self.tenant_mcp_cfg(&runtime).await?;
                    if let Some(ref cfg) = tcfg {
                        for eid in &distinct_entries {
                            if !cfg.entry_allowed(eid) {
                                return Err(CallToolError::from_message(format!(
                                    "entry_id not allowed by tenant MCP configuration: {eid}"
                                )));
                            }
                        }
                    }
                    let binding = self
                        .resolve_binding_for_logical(&key, logical_uuid)
                        .await;
                    tracing::debug!(
                        target: "plasm_agent::mcp",
                        tool = tname,
                        logical_session_ref = %session_ref,
                        logical_session_id = %ls_key,
                        mcp_execute_binding_present = binding.is_some(),
                        "MCP add_capabilities: Plasm execute binding before apply_capability_seeds (false means open path; true means expand/federate against existing prompt_hash/session)"
                    );
                    let add_cap_span =
                        crate::spans::mcp_tool_add_capabilities(session_ref.as_str());
                    let out: ApplyCapabilitySeedsOutcome = apply_capability_seeds(
                        self.plasm.as_ref(),
                        principal_incoming.as_ref(),
                        binding
                            .as_ref()
                            .map(|b| (b.prompt_hash.as_str(), b.session_id.as_str())),
                        seeds,
                        principal,
                        tcfg.clone(),
                        Some(logical_uuid),
                    )
                    .instrument(add_cap_span)
                    .await
                    .map_err(|msg| CallToolError::new(std::io::Error::other(msg)))?;

                    if out.stale_execute_binding_recovered {
                        self.plasm.trace_hub.finalize_mcp_session(&ls_key).await;
                    }

                    if out.binding_updated {
                        {
                            let mut g = self.session_states.write().await;
                            if g.len() >= MAX_MCP_EXEC_BINDINGS && !g.contains_key(&key) {
                                if let Some(victim) = g.keys().next().cloned() {
                                    tracing::warn!(
                                        victim = %victim,
                                        limit = MAX_MCP_EXEC_BINDINGS,
                                        "evicting MCP transport slot to respect soft cap"
                                    );
                                    g.remove(&victim);
                                }
                            }
                        }
                        let ls = self.logical_mutex(&key, &ls_key).await;
                        let mut g = ls.lock().await;
                        g.binding = Some(PlasmExecBinding {
                            prompt_hash: out.prompt_hash.clone(),
                            session_id: out.session_id.clone(),
                        });
                        drop(g);
                        let mut map = self.plasm.logical_execute_bindings.write().await;
                        map.insert(
                            logical_uuid,
                            (out.prompt_hash.clone(), out.session_id.clone()),
                        );
                    }
                    let trace_meta = self.trace_session_meta(&key, &runtime).await;
                    self.plasm
                        .trace_hub
                        .ensure_logical_session(&ls_key, Some(&key), trace_meta)
                        .await;

                    let (mut text, tsv_from_waves) = if is_add_code {
                        #[cfg(feature = "code_mode")]
                        {
                            (String::new(), None)
                        }
                        #[cfg(not(feature = "code_mode"))]
                        {
                            (String::new(), None)
                        }
                    } else {
                        let mut t = String::new();
                        let mut tsv: Option<String> = None;
                        for wave in &out.waves {
                            if tsv.is_none() {
                                if let Some(front) = wave.tsv_static_frontmatter.as_ref() {
                                    tsv = Some(front.clone());
                                }
                            }
                            if !wave.markdown_delta.is_empty() {
                                t.push_str(&wave.markdown_delta);
                                if !t.ends_with('\n') {
                                    t.push('\n');
                                }
                            }
                        }
                        (t, tsv)
                    };
                    for wave in &out.waves {
                        if wave.domain_prompt_chars_added > 0 {
                            let ls = self.logical_mutex(&key, &ls_key).await;
                            let mut g = ls.lock().await;
                            g.stats.domain_prompt_chars = g
                                .stats
                                .domain_prompt_chars
                                .saturating_add(wave.domain_prompt_chars_added);
                        }
                        self.plasm
                            .trace_hub
                            .trace_record_add_capabilities(
                                &ls_key,
                                AddCapabilitiesTrace {
                                    domain_prompt_chars_added: wave.domain_prompt_chars_added,
                                    reused_session: wave.reused_session,
                                    mode: wave.mode.clone(),
                                    entry_id: Some(wave.entry_id.clone()),
                                    entities: wave.entities.clone(),
                                    seeds: wave
                                        .entities
                                        .iter()
                                        .map(|e| format!("{}:{e}", wave.entry_id))
                                        .collect(),
                                },
                            )
                            .await;
                    }
                    let mut plasm = serde_json::Map::new();
                    if let Some(front) = tsv_from_waves {
                        plasm.insert("tsv_static_frontmatter".to_string(), json!(front));
                    }
                    let mut continuity = serde_json::Map::new();
                    continuity.insert(
                        "stale_binding_recovered".to_string(),
                        json!(out.stale_execute_binding_recovered),
                    );
                    if out.stale_execute_binding_recovered {
                        if let Some((ref ph, ref sid)) = out.stale_binding_previous {
                            continuity.insert(
                                "previous_execute".to_string(),
                                json!({ "prompt_hash": ph, "session_id": sid }),
                            );
                        }
                    }
                    continuity.insert(
                        "new_symbol_space".to_string(),
                        json!(out.new_symbol_space),
                    );
                    if out.new_symbol_space {
                        continuity.insert(
                            "discard_cached_plasm_symbols".to_string(),
                            json!(true),
                        );
                    }
                    plasm.insert("continuity".to_string(), serde_json::Value::Object(continuity));
                    if is_add_code {
                        #[cfg(feature = "code_mode")]
                        {
                            let es = self
                            .plasm
                            .sessions
                            .get_by_strs(&out.prompt_hash, &out.session_id)
                            .await
                            .ok_or_else(|| {
                                CallToolError::from_message(
                                    "add_code_capabilities invariant failed: execute session is unavailable for generated code facade",
                                )
                            })?;
                        let de = es.domain_exposure.as_ref().ok_or_else(|| {
                            CallToolError::from_message(
                                "add_code_capabilities invariant failed: execute session has no domain exposure for generated code facade",
                            )
                        })?;
                        let ls = self.logical_mutex(&key, &ls_key).await;
                        {
                            let mut g = ls.lock().await;
                            if out.new_symbol_space || out.stale_execute_binding_recovered {
                                g.code_mode = CodeModeMcpState::default();
                            }
                            g.code_mode.last_binding =
                                Some((out.prompt_hash.clone(), out.session_id.clone()));
                        }
                        let (already, emit_prelude) = {
                            let g = ls.lock().await;
                            let already = g.code_mode.emitted.clone();
                            let emit = !g.code_mode.prelude_issued || out.new_symbol_space;
                            (already, emit)
                        };
                        let gen_req = FacadeGenRequest {
                            new_symbol_space: out.new_symbol_space,
                            seed_pairs: seed_pairs_for_facade.clone(),
                            already_emitted: already,
                            emit_prelude,
                        };
                        let (fac, ts) = build_code_facade(&gen_req, de, &es.contexts_by_entry);
                        text = mcp_add_code_capabilities_markdown(&out, &ts);
                        {
                            let mut g = ls.lock().await;
                            for (entry_id, entity) in &seed_pairs_for_facade {
                                g.code_mode
                                    .emitted
                                    .insert((entry_id.clone(), entity.clone()));
                            }
                            if !ts.agent_prelude.is_empty() {
                                g.code_mode.prelude_issued = true;
                            }
                        }
                        plasm.insert(
                            "facade_delta".to_string(),
                            serde_json::to_value(&fac).unwrap_or_else(|_| json!({})),
                        );
                        plasm.insert(
                            "typescript".to_string(),
                            json!({
                                "prelude_ref": "code-mode-agent-prelude-v2",
                                "runtime_bootstrap_ref": ts.runtime_bootstrap_ref,
                                "prelude": ts.agent_prelude,
                                "namespace_delta": ts.agent_namespace_body,
                                "loaded_apis_delta": ts.agent_loaded_apis,
                                "declarations_unchanged": ts.declarations_unchanged,
                                "added_catalog_aliases": ts.added_catalog_aliases
                            }),
                        );
                        }
                    }
                    let mut res = CallToolResult::text_content(vec![TextContent::new(
                        text, None, None,
                    )]);
                    if !plasm.is_empty() {
                        let mut meta = serde_json::Map::new();
                        meta.insert("plasm".to_string(), serde_json::Value::Object(plasm));
                        res = res.with_meta(Some(meta));
                    }
                    Ok(res)
                }
                .await;
                let elapsed = started.elapsed();
                match &res {
                    Ok(_) => {
                        crate::metrics::record_mcp_tool(tname, None, "success", "none", elapsed)
                    }
                    Err(e) => crate::metrics::record_mcp_tool(
                        tname,
                        None,
                        "error",
                        mcp_call_tool_error_class(e),
                        elapsed,
                    ),
                }
                res
            }
            #[cfg(feature = "code_mode")]
            "evaluate_code_plan" => {
                let started = Instant::now();
                let res: Result<CallToolResult, CallToolError> = async {
                    let principal_incoming = self.ensure_mcp_principal(&key, &runtime).await?;
                    let session_ref = parse_logical_session_ref_arg("evaluate_code_plan", &v)?;
                    let logical_uuid = self
                        .resolve_logical_session_ref_to_uuid(
                            "evaluate_code_plan",
                            &key,
                            &session_ref,
                        )
                        .await?;
                    let scope = tenant_scope(principal_incoming.as_ref());
                    if !self
                        .plasm
                        .logical_sessions
                        .verify_tenant(LogicalSessionId(logical_uuid), &scope)
                        .await
                    {
                        return Err(CallToolError::from_message(
                            "logical_session_ref is unknown or does not belong to this tenant scope",
                        ));
                    }
                    let name = parse_required_string_arg("evaluate_code_plan", &v, "name")?;
                    let code = parse_required_string_arg("evaluate_code_plan", &v, "code")?;
                    let ls_key = logical_uuid.to_string();
                    let state = self.logical_mutex(&key, &ls_key).await;
                    let needs_binding_hydrate = {
                        let g = state.lock().await;
                        g.binding.is_none()
                    };
                    if needs_binding_hydrate {
                        if let Some(b) = self.resolve_binding_for_logical(&key, logical_uuid).await
                        {
                            let mut g = state.lock().await;
                            g.binding = Some(b);
                        }
                    }
                    let binding = {
                        let g = state.lock().await;
                        g.binding.clone()
                    };
                    let Some(b) = binding else {
                        return Err(CallToolError::from_message(
                            "no execute session: call add_code_capabilities with seeds first",
                        ));
                    };
                    let es = self
                        .plasm
                        .sessions
                        .get_by_strs(b.prompt_hash.as_str(), b.session_id.as_str())
                        .await
                        .ok_or_else(|| {
                            CallToolError::from_message(
                                "execute session is missing; call add_code_capabilities to refresh",
                            )
                        })?;
                    let de = es.domain_exposure.as_ref().ok_or_else(|| {
                        CallToolError::from_message(
                            "execute session has no domain exposure; call add_code_capabilities to refresh",
                        )
                    })?;
                    let seed_pairs: Vec<(String, String)> = de
                        .entity_catalog_entry_ids
                        .iter()
                        .cloned()
                        .zip(de.entities.iter().cloned())
                        .collect();
                    let gen_req = FacadeGenRequest {
                        new_symbol_space: true,
                        seed_pairs,
                        already_emitted: Default::default(),
                        emit_prelude: true,
                    };
                    let (facade_delta, _) = build_code_facade(&gen_req, de, &es.contexts_by_entry);
                    let quickjs_runtime = quickjs_runtime_from_facade_delta(&facade_delta);
                    let plan_value = crate::code_mode::CodeModeSandbox::new()
                        .and_then(|s| {
                            s.eval_typescript_to_json_value(
                                &format!("{}.ts", name.replace('/', "_")),
                                &code,
                                Some(&quickjs_runtime),
                            )
                        })
                        .map_err(CallToolError::from_message)?;
                    let dry = evaluate_code_mode_plan_dry(&es, &plan_value)
                        .map_err(CallToolError::from_message)?;
                    let dag = code_mode_plan_dag_json(&dry);
                    let plan_bytes = serde_json::to_vec(&plan_value)
                        .map_err(|e| CallToolError::from_message(e.to_string()))?;
                    let mut hasher = Sha256::new();
                    hasher.update(name.as_bytes());
                    hasher.update(b"\n");
                    hasher.update(code.as_bytes());
                    hasher.update(b"\n");
                    hasher.update(&plan_bytes);
                    let plan_hash = hex::encode(hasher.finalize());
                    let plan_index = es.mint_code_plan_index();
                    let plan_handle = crate::run_artifacts::code_plan_handle(plan_index);
                    let plan_id = Uuid::new_v4();
                    let doc = CodePlanArchiveDocument {
                        kind: "code_plan".into(),
                        plan_id: plan_id.to_string(),
                        prompt_hash: b.prompt_hash.clone(),
                        session_id: b.session_id.clone(),
                        entry_id: es.entry_id.clone(),
                        plan_index,
                        plan_handle: plan_handle.clone(),
                        name: name.clone(),
                        code,
                        plan_hash: plan_hash.clone(),
                        plan: plan_value,
                        catalog_cgs_hash: es.catalog_cgs_hash.clone(),
                        domain_revision: es.domain_revision,
                        entities: es.entities.clone(),
                        principal: es.principal.clone(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };
                    let stored = self
                        .plasm
                        .run_artifacts
                        .insert_code_plan(
                            b.prompt_hash.as_str(),
                            b.session_id.as_str(),
                            plan_id,
                            plan_index,
                            &doc,
                        )
                        .await
                        .map_err(|e| CallToolError::from_message(e.to_string()))?;
                    let short_uri = plasm_session_short_plan_uri(&session_ref, plan_index);
                    let trace_meta = self.trace_session_meta(&key, &runtime).await;
                    self.plasm
                        .trace_hub
                        .ensure_logical_session(&ls_key, Some(&key), trace_meta)
                        .await;
                    self.plasm
                        .trace_hub
                        .trace_record_code_plan_evaluate(
                            &ls_key,
                            crate::trace_hub::CodePlanTrace {
                                plan_handle: plan_handle.clone(),
                                plan_id: plan_id.to_string(),
                                plan_name: name.clone(),
                                plan_hash: plan_hash.clone(),
                                plan_uri: short_uri.clone(),
                                canonical_plan_uri: stored.canonical_plasm_uri.clone(),
                                plan_http_path: stored.http_path.clone(),
                                prompt_hash: b.prompt_hash.clone(),
                                session_id: b.session_id.clone(),
                                node_count: dry.node_results.len(),
                                code_chars: doc.code.chars().count() as u64,
                                dag: dag.clone(),
                                plasm_call_index: None,
                                run_ids: Vec::new(),
                                run_artifacts: Vec::new(),
                            },
                        )
                        .await;
                    let body = render_code_mode_plan_dry_text(
                        &dry,
                        Some(CodePlanDryRunTextMeta {
                            plan_name: Some(name.as_str()),
                            plan_handle: plan_handle.as_str(),
                            plan_uri: short_uri.as_str(),
                            canonical_plan_uri: stored.canonical_plasm_uri.as_str(),
                            plan_hash: plan_hash.as_str(),
                        }),
                    );
                    let root_value = serde_json::json!({
                        "ok": true,
                        "plan_handle": plan_handle,
                        "plan_uri": short_uri,
                        "canonical_plan_uri": stored.canonical_plasm_uri,
                        "plan_http_path": stored.http_path,
                        "plan_id": plan_id.to_string(),
                        "plan_name": name,
                        "plan_hash": plan_hash,
                        "dry_run": {
                            "version": dry.version,
                            "name": dry.name,
                            "node_results": dry.node_results,
                            "graph_summary": dry.graph_summary,
                            "can_batch_run": dry.can_batch_run,
                            "execution_unsupported": dry.execution_unsupported
                        }
                    });
                    let mut res = CallToolResult::text_content(vec![TextContent::new(
                        format!("```text\n{body}```\n"),
                        None,
                        None,
                    )]);
                    let mut meta = serde_json::Map::new();
                    meta.insert(
                        "plasm".into(),
                        json!({
                            "code_plan": {
                                "plan_handle": root_value["plan_handle"],
                                "plan_uri": root_value["plan_uri"],
                                "canonical_plan_uri": root_value["canonical_plan_uri"],
                                "plan_http_path": root_value["plan_http_path"],
                                "plan_id": root_value["plan_id"],
                                "plan_hash": root_value["plan_hash"],
                                "dag": dag,
                                "dry_run": {
                                    "graph_summary": root_value["dry_run"]["graph_summary"],
                                    "can_batch_run": root_value["dry_run"]["can_batch_run"],
                                    "execution_unsupported": root_value["dry_run"]["execution_unsupported"],
                                }
                            }
                        }),
                    );
                    res = res.with_meta(Some(meta));
                    Ok(res)
                }
                .await;
                let elapsed = started.elapsed();
                match &res {
                    Ok(_) => {
                        crate::metrics::record_mcp_tool(
                            "evaluate_code_plan",
                            None,
                            "success",
                            "none",
                            elapsed,
                        );
                    }
                    Err(e) => {
                        crate::metrics::record_mcp_tool(
                            "evaluate_code_plan",
                            None,
                            "error",
                            mcp_call_tool_error_class(e),
                            elapsed,
                        );
                    }
                }
                res
            }
            #[cfg(feature = "code_mode")]
            "execute_code_plan" => {
                let started = Instant::now();
                let res: Result<CallToolResult, CallToolError> = async {
                    let principal_incoming = self.ensure_mcp_principal(&key, &runtime).await?;
                    let session_ref = parse_logical_session_ref_arg("execute_code_plan", &v)?;
                    let logical_uuid = self
                        .resolve_logical_session_ref_to_uuid("execute_code_plan", &key, &session_ref)
                        .await?;
                    let scope = tenant_scope(principal_incoming.as_ref());
                    if !self
                        .plasm
                        .logical_sessions
                        .verify_tenant(LogicalSessionId(logical_uuid), &scope)
                        .await
                    {
                        return Err(CallToolError::from_message(
                            "logical_session_ref is unknown or does not belong to this tenant scope",
                        ));
                    }
                    let plan_handle =
                        parse_required_string_arg("execute_code_plan", &v, "plan_handle")?;
                    let Some(plan_index) = parse_code_plan_handle(&plan_handle) else {
                        return Err(CallToolError::invalid_arguments(
                            "execute_code_plan",
                            Some("plan_handle must look like `p1`".into()),
                        ));
                    };
                    let ls_key = logical_uuid.to_string();
                    let state = self.logical_mutex(&key, &ls_key).await;
                    if state.lock().await.binding.is_none() {
                        if let Some(b) = self.resolve_binding_for_logical(&key, logical_uuid).await
                        {
                            state.lock().await.binding = Some(b);
                        }
                    }
                    let Some(b) = state.lock().await.binding.clone() else {
                        return Err(CallToolError::from_message(
                            "no execute session: call add_code_capabilities with seeds first",
                        ));
                    };
                    let es = self
                        .plasm
                        .sessions
                        .get_by_strs(b.prompt_hash.as_str(), b.session_id.as_str())
                        .await
                        .ok_or_else(|| {
                            CallToolError::from_message(
                                "execute session is missing; call add_code_capabilities to refresh",
                            )
                        })?;
                    let payload = self
                        .plasm
                        .run_artifacts
                        .get_code_plan_payload_result_by_index(
                            b.prompt_hash.as_str(),
                            b.session_id.as_str(),
                            plan_index,
                        )
                        .await
                        .map_err(|e| CallToolError::from_message(e.to_string()))?
                        .ok_or_else(|| {
                            CallToolError::from_message(format!(
                                "unknown Code Mode plan handle `{plan_handle}`"
                            ))
                        })?;
                    let doc: CodePlanArchiveDocument =
                        serde_json::from_slice(payload.bytes.as_ref())
                            .map_err(|e| CallToolError::from_message(e.to_string()))?;
                    if code_plan_session_mismatch(
                        &doc,
                        b.prompt_hash.as_str(),
                        b.session_id.as_str(),
                        es.catalog_cgs_hash.as_str(),
                        es.domain_revision,
                    )
                    .is_some()
                    {
                        return Err(CallToolError::from_message(
                            "archived Code Mode plan does not match the current execute session; re-evaluate it in this symbol space",
                        ));
                    }
                    let dry = evaluate_code_mode_plan_dry(&es, &doc.plan)
                        .map_err(CallToolError::from_message)?;
                    let dag = code_mode_plan_dag_json(&dry);
                    let batch_count = dry.node_results.len();
                    let state2 = self.logical_mutex(&key, &ls_key).await;
                    let (this_invocation_chars, mut idx, call_count) = {
                        let mut g = state2.lock().await;
                        let this_invocation_chars = plan_handle.chars().count() as u64;
                        g.stats.plasm_invocation_chars = g
                            .stats
                            .plasm_invocation_chars
                            .saturating_add(this_invocation_chars);
                        g.stats.plasm_call_count = g.stats.plasm_call_count.saturating_add(1);
                        let call_count = g.stats.plasm_call_count;
                        let idx = std::mem::take(&mut g.meta_index);
                        (this_invocation_chars, idx, call_count)
                    };
                    let trace_meta = self.trace_session_meta(&key, &runtime).await;
                    let trace_id = self
                        .plasm
                        .trace_hub
                        .ensure_logical_session(&ls_key, Some(&key), trace_meta)
                        .await;
                    let call_index = self
                        .plasm
                        .trace_hub
                        .trace_record_plasm_invocation(
                            &ls_key,
                            batch_count > 1,
                            batch_count,
                            None,
                            this_invocation_chars,
                            Some(format!("execute_code_plan {plan_handle}")),
                        )
                        .await;
                    let mcp_trace = PlasmTraceContext {
                        trace_id,
                        call_index: Some(call_count as i64),
                        mcp_session_id: Some(key.clone()),
                        logical_session_id: Some(ls_key.clone()),
                        logical_session_ref: Some(session_ref.clone()),
                    };
                    let sink = McpPlasmTraceSink {
                        hub: Arc::clone(&self.plasm.trace_hub),
                        mcp_key: ls_key.clone(),
                        call_index,
                    };
                    let hooks = CodeModePlasmRunHooks {
                        meta_index: &mut idx,
                        trace: mcp_trace,
                        sink,
                    };
                    let run_result = run_code_mode_plan(
                        &es,
                        self.plasm.as_ref(),
                        principal_incoming.as_ref(),
                        b.prompt_hash.as_str(),
                        b.session_id.as_str(),
                        &doc.plan,
                        true,
                        Some(hooks),
                    )
                    .instrument(crate::spans::mcp_tool_plasm(
                        batch_count > 1,
                        batch_count as u64,
                        session_ref.as_str(),
                    ))
                    .await
                    .map_err(CallToolError::from_message)?;
                    {
                        let mut g = state2.lock().await;
                        g.meta_index = idx;
                    }
                    let markdown = run_result.run_markdown.unwrap_or_else(|| {
                        "Code Mode plan executed, but no result Markdown was produced.".to_string()
                    });
                    let response_chars = markdown.chars().count() as u64;
                    if response_chars > 0 {
                        {
                            let mut g = state2.lock().await;
                            g.stats.plasm_response_chars =
                                g.stats.plasm_response_chars.saturating_add(response_chars);
                        }
                        self.plasm
                            .trace_hub
                            .trace_note_plasm_response_chars(
                                &ls_key,
                                response_chars,
                                "execute_code_plan",
                                call_index,
                                batch_count > 1,
                                batch_count,
                            )
                            .await;
                    }
                    let (run_ids, run_artifacts) =
                        code_plan_run_artifacts_from_meta(run_result.run_plasm_meta.as_ref());
                    let plan_uuid = Uuid::parse_str(&doc.plan_id).map_err(|_| {
                        CallToolError::from_message(
                            "archived Code Mode plan has invalid plan_id; re-evaluate it",
                        )
                    })?;
                    let plan_uri = plasm_session_short_plan_uri(&session_ref, doc.plan_index);
                    let canonical_plan_uri =
                        crate::run_artifacts::plasm_code_plan_resource_uri(
                            &doc.prompt_hash,
                            &doc.session_id,
                            &plan_uuid,
                        );
                    let plan_http_path =
                        code_plan_http_path(&doc.prompt_hash, &doc.session_id, &plan_uuid);
                    self.plasm
                        .trace_hub
                        .trace_record_code_plan_execute(
                            &ls_key,
                            crate::trace_hub::CodePlanTrace {
                                plan_handle: doc.plan_handle.clone(),
                                plan_id: doc.plan_id.clone(),
                                plan_name: doc.name.clone(),
                                plan_hash: doc.plan_hash.clone(),
                                plan_uri: plan_uri.clone(),
                                canonical_plan_uri: canonical_plan_uri.clone(),
                                plan_http_path: plan_http_path.clone(),
                                prompt_hash: doc.prompt_hash.clone(),
                                session_id: doc.session_id.clone(),
                                node_count: dry.node_results.len(),
                                code_chars: doc.code.chars().count() as u64,
                                dag: dag.clone(),
                                plasm_call_index: Some(call_index),
                                run_ids,
                                run_artifacts,
                            },
                        )
                        .await;
                    let blocks = vec![ContentBlock::TextContent(TextContent::new(
                        markdown, None, None,
                    ))];
                    let mut res = CallToolResult::from_content(blocks);
                    let mut meta = run_result.run_plasm_meta.unwrap_or_default();
                    let plasm = meta
                        .entry("plasm".to_string())
                        .or_insert_with(|| json!({}));
                    if let Some(obj) = plasm.as_object_mut() {
                        obj.insert(
                            "code_plan".into(),
                            json!({
                                "plan_handle": doc.plan_handle,
                                "plan_id": doc.plan_id,
                                "plan_name": doc.name,
                                "plan_hash": doc.plan_hash,
                                "plan_uri": plan_uri,
                                "canonical_plan_uri": canonical_plan_uri,
                                "plan_http_path": plan_http_path,
                                "plasm_call_index": call_index,
                                "dag": dag,
                            }),
                        );
                    }
                    res = res.with_meta(Some(meta));
                    Ok(res)
                }
                .await;
                let elapsed = started.elapsed();
                match &res {
                    Ok(_) => crate::metrics::record_mcp_tool(
                        "execute_code_plan",
                        None,
                        "success",
                        "none",
                        elapsed,
                    ),
                    Err(e) => crate::metrics::record_mcp_tool(
                        "execute_code_plan",
                        None,
                        "error",
                        mcp_call_tool_error_class(e),
                        elapsed,
                    ),
                }
                res
            }
            "plasm" => {
                let started = Instant::now();
                let principal_incoming = self.ensure_mcp_principal(&key, &runtime).await?;
                let session_ref = parse_logical_session_ref_arg("plasm", &v)?;
                let logical_uuid = self
                    .resolve_logical_session_ref_to_uuid("plasm", &key, &session_ref)
                    .await?;
                let scope = tenant_scope(principal_incoming.as_ref());
                if !self
                    .plasm
                    .logical_sessions
                    .verify_tenant(LogicalSessionId(logical_uuid), &scope)
                    .await
                {
                    return Ok(CallToolResult::with_error(CallToolError::from_message(
                        "logical_session_ref is unknown or does not belong to this tenant scope",
                    )));
                }
                let ls_key = logical_uuid.to_string();
                let state = self.logical_mutex(&key, &ls_key).await;
                let needs_binding_hydrate = {
                    let g = state.lock().await;
                    g.binding.is_none()
                };
                if needs_binding_hydrate {
                    if let Some(b) = self.resolve_binding_for_logical(&key, logical_uuid).await {
                        let mut g = state.lock().await;
                        g.binding = Some(b);
                    }
                }
                let Some(arr) = v.get("expressions").and_then(|x| x.as_array()) else {
                    crate::metrics::record_mcp_tool(
                        "plasm",
                        Some(false),
                        "error",
                        "invalid_arguments",
                        started.elapsed(),
                    );
                    return Ok(CallToolResult::with_error(
                        CallToolError::invalid_arguments(
                            "plasm",
                            Some(
                                "missing or invalid `expressions`: non-empty JSON array of strings"
                                    .into(),
                            ),
                        ),
                    ));
                };
                if arr.is_empty() {
                    crate::metrics::record_mcp_tool(
                        "plasm",
                        Some(false),
                        "error",
                        "invalid_arguments",
                        started.elapsed(),
                    );
                    return Ok(CallToolResult::with_error(
                        CallToolError::invalid_arguments(
                            "plasm",
                            Some("`expressions` must be non-empty".into()),
                        ),
                    ));
                }
                let expressions: Vec<String> = arr
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| "expressions[] elements must be strings".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|msg| {
                        crate::metrics::record_mcp_tool(
                            "plasm",
                            Some(false),
                            "error",
                            "invalid_arguments",
                            started.elapsed(),
                        );
                        CallToolError::invalid_arguments("plasm", Some(msg))
                    })?;

                let reasoning = v
                    .get("reasoning")
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty());
                let tsv_static_frontmatter = v
                    .get("tsv_static_frontmatter")
                    .and_then(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(f) = tsv_static_frontmatter {
                    let n = f.chars().count();
                    if n > MAX_TSV_STATIC_FRONTMATTER_SCALARS {
                        return Ok(CallToolResult::with_error(
                            CallToolError::invalid_arguments(
                                "plasm",
                                Some(format!(
                                    "`tsv_static_frontmatter` exceeds max length ({} Unicode scalars, max {})",
                                    n, MAX_TSV_STATIC_FRONTMATTER_SCALARS
                                )),
                            ),
                        ));
                    }
                }
                let batch_count = expressions.len();
                let plasm_tool_span = crate::spans::mcp_tool_plasm(
                    batch_count > 1,
                    batch_count as u64,
                    session_ref.as_str(),
                );
                let (binding, this_invocation_chars, mut idx, call_count) = {
                    let mut g = state.lock().await;
                    let binding = g.binding.clone();
                    let this_invocation_chars = plasm_invocation_char_count(
                        &expressions,
                        reasoning,
                        tsv_static_frontmatter,
                    );
                    g.stats.plasm_invocation_chars = g
                        .stats
                        .plasm_invocation_chars
                        .saturating_add(this_invocation_chars);
                    g.stats.plasm_call_count = g.stats.plasm_call_count.saturating_add(1);
                    let call_count = g.stats.plasm_call_count;
                    let idx = std::mem::take(&mut g.meta_index);
                    (binding, this_invocation_chars, idx, call_count)
                };
                let Some(b) = binding else {
                    crate::metrics::record_mcp_tool(
                        "plasm",
                        Some(batch_count > 1),
                        "error",
                        "no_session",
                        started.elapsed(),
                    );
                    return Ok(CallToolResult::with_error(CallToolError::from_message(
                        "No session: call `add_capabilities` with `seeds` first.",
                    )));
                };

                if self
                    .plasm
                    .sessions
                    .get_by_strs(&b.prompt_hash, &b.session_id)
                    .await
                    .is_none()
                {
                    {
                        let mut g = state.lock().await;
                        g.binding = None;
                    }
                    {
                        let mut map = self.plasm.logical_execute_bindings.write().await;
                        map.remove(&logical_uuid);
                    }
                    crate::metrics::record_mcp_tool(
                        "plasm",
                        Some(batch_count > 1),
                        "error",
                        "session_expired",
                        started.elapsed(),
                    );
                    return Ok(CallToolResult::with_error(CallToolError::from_message(
                        "Execute session expired: call `add_capabilities` again with your `seeds` to open a new session.",
                    )));
                }

                let trace_meta = self.trace_session_meta(&key, &runtime).await;
                let trace_id = self
                    .plasm
                    .trace_hub
                    .ensure_logical_session(&ls_key, Some(&key), trace_meta)
                    .await;
                let mcp_trace = PlasmTraceContext {
                    trace_id,
                    call_index: Some(call_count as i64),
                    mcp_session_id: Some(key.clone()),
                    logical_session_id: Some(ls_key.clone()),
                    logical_session_ref: Some(session_ref.clone()),
                };
                let reasoning_chars = reasoning.map(|r| r.chars().count() as u64);
                let call_index = self
                    .plasm
                    .trace_hub
                    .trace_record_plasm_invocation(
                        &ls_key,
                        batch_count > 1,
                        batch_count,
                        reasoning_chars,
                        this_invocation_chars,
                        reasoning.map(str::to_string),
                    )
                    .await;

                let sink = McpPlasmTraceSink {
                    hub: Arc::clone(&self.plasm.trace_hub),
                    mcp_key: ls_key.clone(),
                    call_index,
                };

                let run_result = execute_session_run_markdown(
                    self.plasm.as_ref(),
                    principal_incoming.as_ref(),
                    &b.prompt_hash,
                    &b.session_id,
                    expressions,
                    Some(&mut idx),
                    Some(mcp_trace),
                    Some(sink),
                )
                .instrument(plasm_tool_span)
                .await;
                {
                    let mut g = state.lock().await;
                    g.meta_index = idx;
                }
                match run_result {
                    Ok(out) => {
                        let response_chars = out.markdown.chars().count() as u64;
                        if response_chars > 0 {
                            let mut g = state.lock().await;
                            g.stats.plasm_response_chars =
                                g.stats.plasm_response_chars.saturating_add(response_chars);
                            self.plasm
                                .trace_hub
                                .trace_note_plasm_response_chars(
                                    &ls_key,
                                    response_chars,
                                    "plasm",
                                    call_index,
                                    batch_count > 1,
                                    batch_count,
                                )
                                .await;
                        }
                        let (tok_prompt, tok_inv, tok_resp, tok_total) =
                            self.mcp_plasm_token_snapshot_logical(&key, &ls_key).await;
                        tracing::info!(
                            target: "plasm_agent::mcp",
                            tool = "plasm",
                            batch_count,
                            batch = batch_count > 1,
                            ok = true,
                            tokens_est_prompt = tok_prompt,
                            tokens_est_invocation = tok_inv,
                            tokens_est_tool_response = tok_resp,
                            tokens_est_session_total = tok_total,
                            "MCP tool: plasm (expression detail: plasm_agent::http_execute)"
                        );
                        crate::metrics::record_mcp_tool(
                            "plasm",
                            Some(batch_count > 1),
                            "success",
                            "none",
                            started.elapsed(),
                        );
                        let blocks = vec![ContentBlock::TextContent(TextContent::new(
                            out.markdown,
                            None,
                            None,
                        ))];
                        let mut res = CallToolResult::from_content(blocks);
                        if let Some(m) = out.tool_meta {
                            res = res.with_meta(Some(m));
                        }
                        Ok(res)
                    }
                    Err(msg) => {
                        self.plasm
                            .trace_hub
                            .trace_add_plasm_error(&ls_key, call_index, None, msg.clone())
                            .await;
                        let (tok_prompt, tok_inv, tok_resp, tok_total) =
                            self.mcp_plasm_token_snapshot_logical(&key, &ls_key).await;
                        tracing::error!(
                            target: "plasm_agent::mcp",
                            tool = "plasm",
                            batch_count,
                            batch = batch_count > 1,
                            tokens_est_prompt = tok_prompt,
                            tokens_est_invocation = tok_inv,
                            tokens_est_tool_response = tok_resp,
                            tokens_est_session_total = tok_total,
                            message = %msg,
                            "MCP tool: plasm failed"
                        );
                        crate::metrics::record_mcp_tool(
                            "plasm",
                            Some(batch_count > 1),
                            "error",
                            "execute_failed",
                            started.elapsed(),
                        );
                        Ok(CallToolResult::with_error(CallToolError::from_message(msg)))
                    }
                }
            }
            _ => {
                crate::metrics::record_mcp_tool(
                    "unknown_tool",
                    None,
                    "error",
                    "unknown_tool",
                    Duration::from_secs(0),
                );
                Err(CallToolError::unknown_tool(params.name))
            }
        }
    }
}

/// Detect MCP transport sessions that disappeared from the SDK session store (disconnect / DELETE),
/// finalize logical-session traces that are no longer live, and drop orphaned per-transport state.
fn spawn_mcp_domain_prompt_session_reporter(
    server: &HyperServer,
    plasm: Arc<PlasmHostState>,
    session_states: Arc<RwLock<HashMap<String, Arc<Mutex<McpTransportState>>>>>,
) {
    let store = server.state().session_store.clone();
    tokio::spawn(async move {
        type SessionStates = Arc<RwLock<HashMap<String, Arc<Mutex<McpTransportState>>>>>;
        async fn stats_for_logical_session(
            session_states: &SessionStates,
            logical_id: &str,
        ) -> McpSessionPlasmStats {
            let g = session_states.read().await;
            for (_tk, st) in g.iter() {
                let s = st.lock().await;
                if let Some(ls) = s.logical_by_id.get(logical_id) {
                    let lg = ls.lock().await;
                    return lg.stats.clone();
                }
            }
            McpSessionPlasmStats::default()
        }

        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let current: HashSet<String> = store.keys().await.into_iter().collect();
            let mut live_trace_keys: HashSet<String> = HashSet::new();
            {
                let g = session_states.read().await;
                for tk in &current {
                    if let Some(st_arc) = g.get(tk) {
                        let s = st_arc.lock().await;
                        for lid in s.logical_by_id.keys() {
                            live_trace_keys.insert(lid.clone());
                        }
                    }
                }
            }
            let trace_hub_active = plasm.trace_hub.active_mcp_session_count().await;
            tracing::trace!(
                target: "plasm_agent::mcp",
                session_store_keys = current.len(),
                live_logical_sessions = live_trace_keys.len(),
                trace_hub_active,
                "trace hub vs MCP session store"
            );
            let finalized = plasm
                .trace_hub
                .finalize_disconnected_sessions(&live_trace_keys)
                .await;
            for ended in &finalized {
                let stats = stats_for_logical_session(&session_states, ended).await;
                let tp = mcp_chars_to_token_est(stats.domain_prompt_chars);
                let ti = mcp_chars_to_token_est(stats.plasm_invocation_chars);
                let tr = mcp_chars_to_token_est(stats.plasm_response_chars);
                let tt = tp.saturating_add(ti).saturating_add(tr);
                tracing::info!(
                    target: "plasm_agent::mcp",
                    logical_session_id = %ended,
                    domain_prompt_chars_total = stats.domain_prompt_chars,
                    plasm_invocation_chars_total = stats.plasm_invocation_chars,
                    plasm_response_chars_total = stats.plasm_response_chars,
                    plasm_call_count_total = stats.plasm_call_count,
                    tokens_est_prompt = tp,
                    tokens_est_invocation = ti,
                    tokens_est_tool_response = tr,
                    tokens_est_session_total = tt,
                    "MCP logical session trace finalized (no live transport binding)"
                );
            }
            {
                let mut g = session_states.write().await;
                g.retain(|tk, _| current.contains(tk));
            }
            let idle_ms = mcp_trace_idle_finish_ms();
            if idle_ms > 0 {
                let finalized_idle = plasm
                    .trace_hub
                    .finalize_idle_traces(&live_trace_keys, idle_ms)
                    .await;
                for ended in finalized_idle {
                    tracing::info!(
                        target: "plasm_agent::mcp",
                        logical_session_id = %ended,
                        idle_ms,
                        "MCP logical session trace finalized (idle timeout); transport still connected"
                    );
                }
            }
        }
    });
}

/// When set and > 0, active traces with no hub activity for this many milliseconds are moved to
/// `completed` even if the MCP transport session is still in the SDK store (list UIs stop showing `live`).
fn mcp_trace_idle_finish_ms() -> u64 {
    std::env::var("PLASM_MCP_TRACE_IDLE_FINISH_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn mcp_initialize_result() -> InitializeResult {
    InitializeResult {
        server_info: Implementation {
            name: "plasm-agent".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Plasm agent".into()),
            description: Some(
                "Stable `client_session_key` for the whole task; `plasm_session_init` once, then mostly `plasm` with the same `logical_session_ref`. `add_capabilities` only to append new `api`/entity seeds—not every turn."
                    .into(),
            ),
            icons: vec![],
            website_url: None,
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            resources: Some(ServerCapabilitiesResources {
                list_changed: None,
                subscribe: Some(false),
            }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_11_25.into(),
        instructions: Some(MCP_SERVER_INITIALIZE_INSTRUCTIONS.into()),
        meta: None,
    }
}

/// Run Streamable HTTP MCP on `host`:`port` (default MCP path `/mcp` from the SDK).
pub async fn run_mcp_server(host: &str, port: u16, plasm: Arc<PlasmHostState>) -> SdkResult<()> {
    let handler_struct = PlasmMcpHandler::new(Arc::clone(&plasm));
    let session_states = Arc::clone(&handler_struct.session_states);
    let handler = handler_struct.to_mcp_server_handler();
    let auth_provider: Option<Arc<dyn rust_mcp_sdk::auth::AuthProvider>> =
        if plasm.mcp_config_repository().is_some() || plasm.incoming_auth.is_some() {
            Some(Arc::new(
                crate::mcp_stream_auth::PlasmMcpApiKeyAuthProvider::new(Arc::clone(&plasm)),
            ))
        } else {
            None
        };
    let server = hyper_server::create_server(
        mcp_initialize_result(),
        handler,
        HyperServerOptions {
            host: host.to_string(),
            port,
            event_store: Some(Arc::new(InMemoryEventStore::default())),
            health_endpoint: Some("/health".into()),
            sse_support: false,
            auth: auth_provider,
            ..Default::default()
        },
    );
    spawn_mcp_domain_prompt_session_reporter(&server, Arc::clone(&plasm), session_states);
    server.start().await
}

#[cfg(test)]
mod tests {
    use super::{mcp_discover_query_from_arguments, parse_tool_seeds};
    use insta::assert_snapshot;

    #[test]
    fn mcp_discover_maps_subset_to_capability_query() {
        let v = serde_json::json!({
            "query": ["electric", "type chart"],
        });
        let q = mcp_discover_query_from_arguments(&v).expect("deserialize");
        assert_eq!(q.tokens, vec!["electric", "type chart"]);
        assert!(q.phrases.is_empty());
        assert!(q.entity_hints.is_empty());
        assert!(q.pick_entry.is_none());
        assert!(q.kinds.is_empty());
    }

    #[test]
    fn mcp_discover_query_accepts_single_intent_string() {
        let v = serde_json::json!({
            "query": "github repository commits git linear issue",
        });
        let q = mcp_discover_query_from_arguments(&v).expect("deserialize");
        assert_eq!(
            q.tokens,
            vec!["github repository commits git linear issue"]
        );
    }

    #[test]
    fn plasm_invocation_char_count_includes_tsv_static_frontmatter() {
        let e = vec!["a".to_string()];
        assert_eq!(super::plasm_invocation_char_count(&e, None, None), 1);
        assert_eq!(
            super::plasm_invocation_char_count(&e, None, Some("#c")),
            1 + 2
        );
    }

    #[test]
    fn mcp_plasm_tool_and_initialize_instructions_coherent() {
        assert!(
            super::MCP_PLASM_TOOL_DESCRIPTION.contains("plasm_session_init")
                && super::MCP_PLASM_TOOL_DESCRIPTION.contains("initialize")
                && super::MCP_PLASM_TOOL_DESCRIPTION.contains("Steady state")
                && super::MCP_PLASM_TOOL_DESCRIPTION.contains("tsv_static_frontmatter"),
            "plasm tool description: {}",
            super::MCP_PLASM_TOOL_DESCRIPTION
        );
        assert!(
            super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("plasm_session_init")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("logical_session_ref")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("api")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("Plasm language")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("reused")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("_meta.plasm.paging")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("Session reuse")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS.contains("plasm` only")
                && super::MCP_SERVER_INITIALIZE_INSTRUCTIONS
                    .contains("_meta.plasm.tsv_static_frontmatter",),
            "initialize instructions: {}",
            super::MCP_SERVER_INITIALIZE_INSTRUCTIONS
        );
    }

    #[test]
    fn mcp_tool_list_hides_internal_auth_and_registry_tools() {
        let names: Vec<String> = super::PlasmMcpHandler::plasm_tools()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(!names.iter().any(|n| n == "plasm_incoming_auth"));
        assert!(!names.iter().any(|n| n == "list_registry"));
        assert!(names.iter().any(|n| n == "plasm_session_init"));
        assert!(names.iter().any(|n| n == "discover_capabilities"));
        assert!(names.iter().any(|n| n == "add_capabilities"));
        #[cfg(feature = "code_mode")]
        {
            assert!(names.iter().any(|n| n == "add_code_capabilities"));
            assert!(names.iter().any(|n| n == "evaluate_code_plan"));
            assert!(names.iter().any(|n| n == "execute_code_plan"));
            assert!(!names.iter().any(|n| n == "execute"));
        }
        #[cfg(not(feature = "code_mode"))]
        {
            assert!(!names.iter().any(|n| n == "add_code_capabilities"));
            assert!(!names.iter().any(|n| n == "evaluate_code_plan"));
            assert!(!names.iter().any(|n| n == "execute_code_plan"));
            assert!(!names.iter().any(|n| n == "execute"));
        }
        assert!(names.iter().any(|n| n == "plasm"));
    }

    /// MCP hosts (e.g. Cursor) may validate `tools/call` args against the advertised JSON Schema
    /// from `tools/list`. `query` must allow a single string so intent-style searches are not
    /// rejected as "expected a sequence" before the request reaches the server.
    #[test]
    fn discover_capabilities_input_schema_advertises_string_or_array_query() {
        use serde_json::json;
        let tools = super::PlasmMcpHandler::plasm_tools();
        let discover = tools
            .iter()
            .find(|t| t.name == "discover_capabilities")
            .expect("discover_capabilities tool");
        let v = serde_json::to_value(&discover.input_schema).expect("input_schema json");
        let q = v
            .get("properties")
            .and_then(|p| p.get("query"))
            .expect("query property in input_schema");
        let one_of = q.get("oneOf").and_then(|x| x.as_array()).expect("query.oneOf array");
        assert!(
            one_of.len() >= 2,
            "query schema should oneOf string and array, got: {}",
            json!(q)
        );
    }

    /// Code-mode plan tools document the archive handle flow.
    #[cfg(feature = "code_mode")]
    #[test]
    fn mcp_code_plan_tools_document_when_how_and_small_outputs() {
        let tools = super::PlasmMcpHandler::plasm_tools();
        let add = tools
            .iter()
            .find(|t| t.name == "add_code_capabilities")
            .expect("add_code_capabilities tool");
        let eval = tools
            .iter()
            .find(|t| t.name == "evaluate_code_plan")
            .expect("evaluate_code_plan tool");
        let run = tools
            .iter()
            .find(|t| t.name == "execute_code_plan")
            .expect("execute_code_plan tool");
        let d = format!(
            "{}\n{}\n{}",
            add.description.as_deref().unwrap(),
            eval.description.as_deref().unwrap(),
            run.description.as_deref().unwrap()
        );
        assert!(
            d.contains("plan_handle")
                && d.contains("archived")
                && d.contains("program")
                && d.contains("not a query interface")
                && d.contains("plan validation and review")
                && d.contains("usually not the final answer")
                && d.contains("execute_code_plan(plan_handle)")
                && d.contains("satisfactory dry-run")
                && d.contains("_meta.plasm.steps")
                && d.contains("prefer **`plasm`**")
                && d.contains("multiple coordinated operations")
                && d.contains("transformations")
                && d.contains("compute")
                && d.contains("fan-out/fan-in")
                && d.contains("Plan.project")
                && d.contains(".select(...)")
                && d.contains("Plan.return(...)")
                && d.contains("minimal output")
                && d.contains("final answer nodes")
                && d.contains("wide intermediates"),
            "code plan tool descriptions: {d}"
        );
        assert!(
            !d.contains("approval") && !d.contains("approved") && !d.contains("approve"),
            "code plan tool descriptions should not mention approval: {d}"
        );
    }

    #[cfg(feature = "code_mode")]
    #[test]
    fn initialize_instructions_document_code_mode_decision_and_flow() {
        let d = super::MCP_SERVER_INITIALIZE_INSTRUCTIONS;
        for expected in [
            "prefer **`plasm`** for one simple expression",
            "synthesizing a **program**",
            "not a query interface",
            "multiple operations needing coordination",
            "transformation",
            "compute",
            "fan-out/fan-in",
            "add_code_capabilities",
            "evaluate_code_plan(name, code)",
            "execute_code_plan(plan_handle)",
            "once the plan satisfies the user's intent",
            "revise and re-evaluate",
            "Reuse the **`plan_handle`**",
            "Plan.project",
            ".select(...)",
            "Plan.return(...)",
            "never intermediates",
        ] {
            assert!(
                d.contains(expected),
                "initialize instructions missing {expected:?}: {d}"
            );
        }
        assert!(
            !d.contains("approval") && !d.contains("approved") && !d.contains("approve"),
            "initialize instructions should not mention approval: {d}"
        );
    }

    #[cfg(feature = "code_mode")]
    #[test]
    fn code_plan_session_mismatch_rejects_stale_symbol_space() {
        let doc = crate::run_artifacts::CodePlanArchiveDocument {
            kind: "code_plan".into(),
            plan_id: uuid::Uuid::nil().to_string(),
            prompt_hash: "p".repeat(64),
            session_id: "s1".into(),
            entry_id: "demo".into(),
            plan_index: 1,
            plan_handle: "p1".into(),
            name: "demo".into(),
            code: "JSON.stringify({version:1,nodes:[]})".into(),
            plan_hash: "h".repeat(64),
            plan: serde_json::json!({"version": 1, "nodes": []}),
            catalog_cgs_hash: "c".repeat(64),
            domain_revision: 2,
            entities: vec!["Widget".into()],
            principal: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(
            super::code_plan_session_mismatch(&doc, &"p".repeat(64), "s1", &"c".repeat(64), 2),
            None
        );
        assert_eq!(
            super::code_plan_session_mismatch(&doc, &"x".repeat(64), "s1", &"c".repeat(64), 2),
            Some("execute_session")
        );
        assert_eq!(
            super::code_plan_session_mismatch(&doc, &"p".repeat(64), "s1", &"x".repeat(64), 2),
            Some("catalog_cgs_hash")
        );
        assert_eq!(
            super::code_plan_session_mismatch(&doc, &"p".repeat(64), "s1", &"c".repeat(64), 1),
            Some("domain_revision")
        );
    }

    #[test]
    fn mcp_discover_ignores_unknown_json_keys() {
        let v = serde_json::json!({
            "query": ["x"],
            "kinds": ["query"],
        });
        let q = mcp_discover_query_from_arguments(&v).expect("deserialize");
        assert_eq!(q.tokens, vec!["x"]);
        assert!(q.kinds.is_empty());
    }

    /// Reference output for `discover_capabilities` Markdown (TSV fence only).
    #[test]
    fn discover_markdown_emits_tsv_snapshot() {
        use plasm_core::discovery::{
            CapabilityQuery, DiscoveryResult, EntitySummary, RankedCandidate,
        };
        let r = DiscoveryResult {
            contexts: vec![],
            candidates: vec![RankedCandidate {
                entry_id: "demo".into(),
                entity: "Widget".into(),
                capability_name: "list".into(),
                score: 2,
                reason_codes: vec![],
                capability_description: "List widgets".into(),
            }],
            ambiguities: vec![],
            applied_query_echo: CapabilityQuery::default(),
            closure_stats: None,
            schema_neighborhoods: vec![],
            entity_summaries: vec![EntitySummary {
                name: "Widget".into(),
                description: " A contrived \t widget \n line ".into(),
            }],
        };
        assert_snapshot!(
            super::format_discovery_markdown(&r),
            @"
```tsv
api\tentity\tdescription
demo\tWidget\tA contrived widget line
```

"
        );
    }

    #[test]
    fn add_capabilities_requires_non_empty_seeds() {
        let err = parse_tool_seeds("add_capabilities", &serde_json::json!({ "seeds": [] }))
            .expect_err("expected invalid seeds");
        assert!(
            err.to_string().contains("non-empty array"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn add_capabilities_legacy_shape_returns_actionable_error() {
        let err = parse_tool_seeds(
            "add_capabilities",
            &serde_json::json!({ "entry_id": "pokeapi", "entities": ["Pokemon"] }),
        )
        .expect_err("expected invalid legacy shape");
        assert!(
            err.to_string()
                .contains("legacy top-level `{entry_id, entities}`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn add_capabilities_seeds_accept_api_or_entry_id_alias() {
        let api = parse_tool_seeds(
            "add_capabilities",
            &serde_json::json!({ "seeds": [{ "api": "pokeapi", "entity": "Pokemon" }] }),
        )
        .expect("api key");
        assert_eq!(api.len(), 1);
        assert_eq!(api[0].entry_id, "pokeapi");
        assert_eq!(api[0].entity, "Pokemon");

        let legacy = parse_tool_seeds(
            "add_capabilities",
            &serde_json::json!({ "seeds": [{ "entry_id": "pokeapi", "entity": "Pokemon" }] }),
        )
        .expect("entry_id alias");
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].entry_id, "pokeapi");
    }
}
