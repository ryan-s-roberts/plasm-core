//! Integration coverage for [`plasm_agent_core::mcp_config_admin`] against real Postgres when Docker is available.
//!
//! This is the **headless** contract for MCP API key provisioning (`McpConfigAdminService::provision_api_key`),
//! the same backend call the appliance TUI issues via `AdminJob::ProvisionApiKey` — it does **not**
//! exercise Ratatui/Crossterm or PTY redraw behavior.
//!
//! Override with `PLASM_MCP_ADMIN_TEST_DATABASE_URL=postgres://...` to use an existing server.

use std::sync::Arc;

use plasm_agent_core::auth_framework_host;
use plasm_agent_core::mcp_config_admin::{McpConfigAdminService, McpConfigScope};
use plasm_agent_core::mcp_config_repository::McpConfigRepository;
use testcontainers_modules::testcontainers::{
    core::{wait::LogWaitStrategy, IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};

async fn postgres_url() -> Option<String> {
    if let Ok(url) = std::env::var("PLASM_MCP_ADMIN_TEST_DATABASE_URL") {
        let t = url.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let Ok(container) = GenericImage::new("postgres", "16")
        .with_wait_for(WaitFor::log(
            LogWaitStrategy::stderr("database system is ready to accept connections").with_times(2),
        ))
        .with_exposed_port(5432.tcp())
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_USER", "postgres")
        .start()
        .await
    else {
        eprintln!("mcp_config_admin integration: skipping (no Docker / Postgres container)");
        return None;
    };
    let port = container.get_host_port_ipv4(5432).await.ok()?;
    // Match HTTP integration tests: mapped port on loopback avoids flaky `get_host()` names.
    Some(format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
}

#[tokio::test]
async fn singleton_allowlist_and_keys_roundtrip() {
    let Some(db_url) = postgres_url().await else {
        return;
    };
    let repo = Arc::new(
        McpConfigRepository::connect_and_migrate(&db_url)
            .await
            .expect("migrate"),
    );
    let keys = auth_framework_host::mcp_api_key_registry_memory_only();
    let svc = McpConfigAdminService::new(repo, keys);

    let scope = McpConfigScope::organization_workspace_project(
        format!(
            "admintest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ),
        "default".into(),
        "default".into(),
    );

    let id = svc
        .ensure_singleton_config(&scope, None, "Your MCP")
        .await
        .expect("ensure");

    svc.set_allowed_apis_exact(id, ["alpha".into(), "beta".into()].into())
        .await
        .expect("set apis");

    let rt = svc
        .load_runtime_snapshot(id)
        .await
        .expect("snap")
        .expect("runtime");
    assert!(rt.allowed_entry_ids.contains("alpha"));
    assert!(rt.allowed_entry_ids.contains("beta"));

    let prov = svc
        .provision_api_key(id, "cli-test".into())
        .await
        .expect("provision");
    assert!(!prov.api_key.is_empty());

    let listed = svc.list_api_key_rows(id).await.expect("keys");
    assert!(
        listed
            .iter()
            .any(|r| r.label.as_deref() == Some("cli-test")),
        "{listed:?}"
    );

    let revealed = svc.reveal_api_key(id, prov.key_id).await.expect("reveal");
    assert_eq!(revealed, prov.api_key);
}
