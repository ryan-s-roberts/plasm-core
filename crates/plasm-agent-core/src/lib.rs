//! `plasm-agent` library: HTTP/MCP SaaS core and schema CLI — built as **`plasm-mcp`** and **`plasm-cgs`**.
//! Interactive REPL (`plasm-repl`) lives in **`plasm-repl`** (depends on `plasm-eval` / BAML). Integration tests live under `tests/*.rs`.

// Large MCP tool async stacks + `#[async_trait]` boxing can exceed the default trait solver recursion
// budget when proving nested HTTP/hyper futures are `Send` (rustc 1.87+).
#![recursion_limit = "512"]

pub mod appliance_mcp_defaults;
pub mod auth_framework_host;
mod auth_framework_postgres_schema;
pub mod backend_normalize;
pub mod bootstrap_secrets;
pub mod catalog_runtime;
pub mod cli_builder;
pub mod control_plane_http;
mod discovery_embedding_chunks;
pub mod discovery_embedding_reconcile;
pub mod discovery_embedding_repository;
pub mod dispatch;
pub mod dotenv_safe;
pub mod error;
pub mod execute_path_ids;
pub mod execute_session;
mod execute_staging;
pub mod expr_display;
pub mod http;
pub mod http_discovery;
pub mod http_execute;
pub mod http_incoming_context;
pub mod http_mcp_config;
pub mod http_oauth_link;
pub mod http_outbound_secrets;
pub mod http_problem_util;
mod http_traces;
pub mod incoming_auth;
pub mod input_field_cli;
pub mod invoke_args;
pub mod local_trace_archive;
pub mod mcp_api_key_registry;
pub mod mcp_config_repository;
pub mod mcp_plasm_meta;
pub mod mcp_policy;
mod mcp_run_markdown;
pub mod mcp_runtime_config;
pub mod mcp_server;
mod mcp_stream_auth;
pub mod mcp_transport_auth;
pub mod metrics;
pub mod oauth_link_catalog;
pub mod oauth_link_session;
mod oauth_provider_model;
pub mod oauth_provider_pull;
mod oauth_runtime_source;
pub mod oss_local_state;
pub mod outbound_secret_provider;
pub mod output;
pub mod plasm_dag;
/// Serializable effect [`Plan`](plasm_plan::Plan) contract and DAG validation (Plasm programs, archived plans).
pub mod plasm_plan;
pub mod plasm_plan_run;
pub mod plugin_catalog;
pub mod query_args;
pub mod run_artifacts;
pub mod server_state;
pub mod session_graph_persistence;
pub mod session_identity;
pub mod spans;
pub mod subcommand_util;
mod telemetry;
pub mod tenant_binding;
pub mod terminal;
mod tool_model;
pub mod trace_hub;
pub(crate) mod trace_hub_metrics;
pub mod trace_sink_emit;
pub mod typed_discovery_host;
mod web_connected_account_notify;

pub use crate::cli_builder::AgentCliSurface;

/// Dotenv + telemetry init shared with `plasm-repl` and other front binaries.
pub fn init_agent_runtime() -> Result<(), Box<dyn std::error::Error>> {
    crate::dotenv_safe::load_from_cwd_parents();
    crate::telemetry::init().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    Ok(())
}

/// Remote HTTP terminal (`plasm-cgs` binary). Local schema-driven CGS CLIs use `plasm-repl` / tests only.
pub async fn run_cgs_main() -> Result<(), Box<dyn std::error::Error>> {
    crate::terminal::run_terminal()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
}
