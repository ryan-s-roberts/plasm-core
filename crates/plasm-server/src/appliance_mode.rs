//! Appliance bootstrap policy requirements (embedded vs external Postgres).

use plasm_agent::embedded_postgres::EmbeddedPostgresGuard;
use plasm_agent_core::mcp_config_repository;
use plasm_agent_core::mcp_host_bootstrap::McpPolicyAttachOutcome;
use plasm_agent_core::server_state::PlasmHostState;

use crate::ServeCli;

/// When the MCP policy store (`project_mcp_*`) must be attached before RUN handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppliancePolicyRequirement {
    /// Default appliance: embedded PostgreSQL autostart is expected.
    RequiredEmbedded,
    /// External Postgres URL after reconcile (`PLASM_EMBEDDED_POSTGRES=0` or loopback URL).
    RequiredExternal,
    /// Migrate-only or other paths that do not require a live policy store.
    Optional,
}

/// Typed bootstrap detail for Overview when the policy store did not attach.
#[derive(Debug)]
pub enum PolicyStoreBootstrapDetail {
    NoPostgresUrl,
    MigrateFailed(mcp_config_repository::McpConfigRepositoryError),
}

impl PolicyStoreBootstrapDetail {
    pub fn from_attach(outcome: McpPolicyAttachOutcome) -> Option<Self> {
        match outcome {
            McpPolicyAttachOutcome::Attached => None,
            McpPolicyAttachOutcome::NoDatabaseUrl => Some(Self::NoPostgresUrl),
            McpPolicyAttachOutcome::Failed(e) => Some(Self::MigrateFailed(e)),
        }
    }

    pub fn display_lines(&self) -> Vec<String> {
        match self {
            Self::NoPostgresUrl => vec![
                "No postgres URL is set (DATABASE_URL / PLASM_MCP_CONFIG_DATABASE_URL / PLASM_AUTH_STORAGE_URL)."
                    .into(),
            ],
            Self::MigrateFailed(e) => vec![format!("project_mcp_* connect/migrate failed: {e:#}")],
        }
    }

    pub fn fatal_message(&self) -> String {
        match self {
            Self::NoPostgresUrl => "MCP policy store requires a Postgres URL but none is configured"
                .into(),
            Self::MigrateFailed(e) => format!("project_mcp_* connect/migrate failed: {e:#}"),
        }
    }
}

#[derive(Debug)]
pub enum BootstrapGateOutcome {
    Proceed(McpPolicyAttachOutcome),
    Fatal(PolicyStoreBootstrapDetail),
}

pub fn appliance_policy_requirement(cli: &ServeCli) -> AppliancePolicyRequirement {
    if cli.migrate_mcp_config_db {
        return AppliancePolicyRequirement::Optional;
    }
    if EmbeddedPostgresGuard::will_autostart_embedded_postgres() {
        return AppliancePolicyRequirement::RequiredEmbedded;
    }
    if mcp_config_repository::mcp_config_database_url().is_some() {
        return AppliancePolicyRequirement::RequiredExternal;
    }
    AppliancePolicyRequirement::RequiredEmbedded
}

pub fn evaluate_policy_bootstrap_gate(
    requirement: AppliancePolicyRequirement,
    state: &PlasmHostState,
    attach: McpPolicyAttachOutcome,
) -> BootstrapGateOutcome {
    if matches!(requirement, AppliancePolicyRequirement::Optional) {
        return BootstrapGateOutcome::Proceed(attach);
    }
    if state.mcp_config_repository().is_some() {
        return BootstrapGateOutcome::Proceed(attach);
    }
    let Some(detail) = PolicyStoreBootstrapDetail::from_attach(attach) else {
        return BootstrapGateOutcome::Fatal(PolicyStoreBootstrapDetail::NoPostgresUrl);
    };
    BootstrapGateOutcome::Fatal(detail)
}
