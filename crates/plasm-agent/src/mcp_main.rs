//! `plasm-mcp` entry wiring (HTTP + MCP) — `plasm-agent-core` data plane. HTTP is OSS
//! `serve_http_listener` (no hosted `/internal/*`); the product super-repo composes `plasm-saas` for
//! the full control-plane surface.

use clap::{Arg, ArgAction, Command};
use plasm_agent_core::bootstrap_secrets::McpBootstrapMaterializer;
use plasm_agent_core::error::AgentError;
use plasm_agent_core::server_state::CatalogBootstrap;
use plasm_agent_core::server_state::PlasmSaaSHostExtension;
use plasm_core::discovery::InMemoryCgsRegistry;
use plasm_core::{PromptPipelineConfig, PromptRenderMode};
use plasm_plugin_host::PluginManager;
use plasm_runtime::{AuthResolver, ExecutionConfig, ExecutionEngine, ExecutionMode};

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM signal handler");
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                let _ = res;
            }
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

pub async fn run_mcp_main() -> Result<(), Box<dyn std::error::Error>> {
    plasm_agent_core::init_agent_runtime()?;
    plasm_agent_core::bootstrap_secrets::DefaultMcpBootstrapSecrets
        .materialize_mcp_process_env()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(AgentError::from(e)) })?;

    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();

    // Keep this in sync with Helm `deploy/charts/plasm-mcp/values.yaml` default `args`
    // and optional initContainer `plasm-mcp --migrate-mcp-config-db`.
    // Unknown flags here make clap drop earlier flags (e.g. `--plugin-dir`) even with `ignore_errors`.
    let pre_cmd = Command::new("plasm-mcp")
        .arg(
            Arg::new("schema")
                .long("schema")
                .short('s')
                .help("Path to CGS schema file"),
        )
        .arg(
            Arg::new("plugin_dir")
                .long("plugin-dir")
                .value_name("DIR")
                .help(
                    "Load catalogs from self-describing plugin cdylibs in this directory (ABI v4)",
                ),
        )
        .arg(
            Arg::new("symbol_tuning")
                .long("symbol-tuning")
                .value_name("MODE")
                .num_args(1)
                .required(false),
        )
        .arg(Arg::new("http").long("http").action(ArgAction::SetTrue))
        .arg(
            Arg::new("port")
                .long("port")
                .value_name("PORT")
                .num_args(1)
                .required(false),
        )
        .arg(Arg::new("mcp").long("mcp").action(ArgAction::SetTrue))
        .arg(
            Arg::new("mcp_port")
                .long("mcp-port")
                .value_name("PORT")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("migrate_mcp_config_db")
                .long("migrate-mcp-config-db")
                .action(ArgAction::SetTrue)
                .help(
                    "Run embedded sqlx migrations for tenant MCP tables (project_mcp_*), then exit. \
Uses PLASM_MCP_CONFIG_DATABASE_URL, else PLASM_AUTH_STORAGE_URL, else DATABASE_URL (after bootstrap secrets).",
                ),
        )
        .ignore_errors(true);

    let pre_matches = pre_cmd.get_matches_from(&argv);

    if pre_matches.get_flag("migrate_mcp_config_db") {
        let Some(db_url) = plasm_agent_core::mcp_config_repository::mcp_config_database_url()
        else {
            eprintln!(
                "plasm-mcp: --migrate-mcp-config-db requires PLASM_MCP_CONFIG_DATABASE_URL, PLASM_AUTH_STORAGE_URL, or DATABASE_URL (mounted under PLASM_SECRETS_DIR in Kubernetes)"
            );
            std::process::exit(1);
        };
        plasm_agent_core::mcp_config_repository::McpConfigRepository::connect_and_migrate(&db_url)
            .await
            .map_err(|e| -> Box<dyn std::error::Error> {
                format!("MCP config database migrate failed: {e}").into()
            })?;
        tracing::info!("MCP configuration sqlx migrations applied successfully");
        return Ok(());
    }

    let plugin_dir = pre_matches.get_one::<String>("plugin_dir");
    let server_mode = pre_matches.get_flag("http") || pre_matches.get_flag("mcp");

    let (schema_path, cgs, preloaded_opt) = match pre_matches.get_one::<String>("schema") {
        Some(path) => {
            if plugin_dir.is_some() {
                eprintln!("plasm-mcp: do not combine --schema with --plugin-dir");
                std::process::exit(1);
            }
            let cgs = plasm_core::loader::load_schema(std::path::Path::new(path))
                .map_err(AgentError::Schema)?;
            (path.clone(), cgs, None)
        }
        None => {
            if server_mode {
                if let Some(pd) = plugin_dir {
                    let reg = plasm_agent_core::plugin_catalog::load_registry_from_plugin_dir(
                        std::path::Path::new(pd),
                    )
                    .map_err(AgentError::Schema)?;
                    let arc_cgs = reg.first_cgs().ok_or_else(|| {
                        AgentError::Schema("plugin-dir catalog has no entries".to_string())
                    })?;
                    let cgs = (*arc_cgs).clone();
                    (pd.clone(), cgs, Some(reg))
                } else {
                    eprintln!("Usage: plasm-mcp --schema <path> [--http] [--mcp] …");
                    eprintln!(
                        "   or: plasm-mcp --plugin-dir <dir> --http and/or --mcp (multi-entry plugin catalogs)"
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!("Usage: plasm-mcp --schema <path> [--http] [--mcp] …");
                std::process::exit(1);
            }
        }
    };

    if let Some(reg) = &preloaded_opt {
        plasm_agent_core::plugin_catalog::validate_registry_templates(reg)
            .map_err(AgentError::Schema)?;
    } else {
        plasm_compile::validate_cgs_capability_templates(&cgs)
            .map_err(|e| AgentError::Schema(e.to_string()))?;
    }

    let app = plasm_agent_core::cli_builder::build_app(
        &cgs,
        plasm_agent_core::AgentCliSurface::McpServer,
    );
    let matches = app.get_matches_from(&argv);

    if matches.get_one::<String>("schema").is_some()
        && matches.get_one::<String>("plugin_dir").is_some()
    {
        eprintln!("plasm-mcp: do not combine --schema with --plugin-dir");
        std::process::exit(1);
    }

    let backend_raw = matches
        .get_one::<String>("backend")
        .map(|s| s.as_str())
        .unwrap_or("http://localhost:1080");
    let backend = plasm_agent_core::backend_normalize::normalize_live_backend_url(
        schema_path.as_str(),
        backend_raw,
    );

    let mode = match matches
        .get_one::<String>("mode")
        .map(|s| s.as_str())
        .unwrap_or("live")
    {
        "replay" => ExecutionMode::Replay,
        "hybrid" => ExecutionMode::Hybrid,
        _ => ExecutionMode::Live,
    };

    let prompt_focus = matches.get_one::<String>("focus").cloned();
    let render_mode = matches
        .get_one::<String>("symbol_tuning")
        .map(|s| PromptRenderMode::parse_user_facing_or_default(s))
        .unwrap_or_default();
    let prompt_pipeline =
        PromptPipelineConfig::for_cli_focus(prompt_focus.as_deref()).with_render_mode(render_mode);

    let config = ExecutionConfig {
        base_url: Some(backend.to_string()),
        prompt_pipeline,
        ..ExecutionConfig::default()
    };

    let auth_resolver = cgs.auth.clone().map(AuthResolver::from_env);
    let engine = ExecutionEngine::new_with_auth(config, auth_resolver)?;

    let registry = std::sync::Arc::new(if let Some(reg) = preloaded_opt {
        reg
    } else if let Some(pd) = matches.get_one::<String>("plugin_dir") {
        plasm_agent_core::plugin_catalog::load_registry_from_plugin_dir(std::path::Path::new(pd))
            .map_err(AgentError::Schema)?
    } else {
        InMemoryCgsRegistry::from_pairs(vec![(
            "default".into(),
            "default".into(),
            vec![],
            std::sync::Arc::new(cgs.clone()),
        )])
    });

    let catalog_bootstrap = if matches.get_one::<String>("plugin_dir").is_some() {
        CatalogBootstrap::PluginDir {
            path: std::path::PathBuf::from(
                matches
                    .get_one::<String>("plugin_dir")
                    .expect("plugin-dir checked"),
            ),
        }
    } else {
        CatalogBootstrap::Fixed
    };

    let use_http = matches.get_flag("http");
    let use_mcp = matches.get_flag("mcp");
    let port = *matches.get_one::<u16>("port").unwrap_or(&3000);

    if !use_http && !use_mcp {
        eprintln!("plasm-mcp: pass --http and/or --mcp");
        std::process::exit(1);
    }

    let (auth_framework, mcp_api_keys, auth_storage) =
        plasm_agent_core::auth_framework_host::init_plasm_http_auth_bundle()
            .await
            .map_err(|e| std::io::Error::other(format!("auth-framework init: {e}")))?;
    let oauth_link_catalog =
        std::sync::Arc::new(plasm_agent_core::oauth_link_catalog::OauthLinkCatalog::from_env());
    if let Some(settings) =
        plasm_agent_core::oauth_provider_pull::OauthProviderPullSettings::from_env()
    {
        match plasm_agent_core::oauth_provider_pull::init_oauth_provider_pull_from_postgres(
            oauth_link_catalog.clone(),
            settings,
        )
        .await
        {
            plasm_agent_core::oauth_provider_pull::OauthProviderPullInitOutcome::ConnectFailed { error } => {
                tracing::warn!(
                    error = %error,
                    "oauth_provider_pull: could not connect; runtime catalog uses file / upsert only"
                );
            }
            plasm_agent_core::oauth_provider_pull::OauthProviderPullInitOutcome::Ran {
                periodic_spawned,
                ..
            } => {
                tracing::debug!(
                    periodic_spawned,
                    "oauth_provider_pull: startup refresh complete"
                );
            }
        }
    }
    let outbound_secret_provider = std::sync::Arc::new(
        plasm_agent_core::outbound_secret_provider::AgentOutboundSecretProvider::new(
            auth_storage.clone(),
            oauth_link_catalog.clone(),
        ),
    );
    let plugin_manager = match matches.get_one::<String>("compile_plugin") {
        Some(path) => {
            let pm = PluginManager::load(std::path::Path::new(path))
                .map_err(|e| std::io::Error::other(format!("--compile-plugin {path}: {e}")))?;
            Some(std::sync::Arc::new(pm))
        }
        None => None,
    };
    let incoming_cfg = plasm_agent_core::incoming_auth::IncomingAuthConfig::from_env();
    if let Err(e) = incoming_cfg.validate_startup() {
        eprintln!("{e}");
        std::process::exit(1);
    }
    let incoming_verifier = std::sync::Arc::new(
        plasm_agent_core::incoming_auth::IncomingAuthVerifier::new(incoming_cfg.clone())
            .map_err(std::io::Error::other)?,
    );
    plasm_agent_core::incoming_auth::log_incoming_auth_startup(&incoming_cfg, &incoming_verifier);
    let run_artifacts = plasm_agent_core::run_artifacts::init_from_env()
        .map_err(|e| std::io::Error::other(format!("run artifacts: {e}")))?;
    let session_graph_persistence = plasm_agent_core::session_graph_persistence::init_from_env()
        .map_err(|e| std::io::Error::other(format!("session graph persistence: {e}")))?;
    let mut app_state = plasm_agent_core::http::build_plasm_host_state(
        plasm_agent_core::http::PlasmHostBootstrap {
            engine,
            mode,
            registry,
            catalog_bootstrap,
            plugin_manager,
            incoming_auth: Some(incoming_verifier),
            run_artifacts,
            session_graph_persistence,
        },
    );
    let mut saas = PlasmSaaSHostExtension {
        auth_framework: Some(auth_framework),
        auth_storage: Some(auth_storage),
        oauth_link_catalog,
        outbound_secret_provider: Some(
            outbound_secret_provider as std::sync::Arc<dyn plasm_runtime::SecretProvider>,
        ),
        mcp_config_repository: None,
        mcp_transport_auth: Some(
            mcp_api_keys
                as std::sync::Arc<dyn plasm_agent_core::mcp_transport_auth::McpTransportAuth>,
        ),
        tenant_binding: None,
    };
    if let Some(db_url) = plasm_agent_core::mcp_config_repository::mcp_config_database_url() {
        match plasm_agent_core::mcp_config_repository::McpConfigRepository::connect_and_migrate(
            &db_url,
        )
        .await
        {
            Ok(repo) => {
                saas.mcp_config_repository = Some(std::sync::Arc::new(repo));
                tracing::info!("MCP configuration metadata: postgres (sqlx migrations applied)");
            }
            Err(e) => {
                return Err(Box::new(std::io::Error::other(format!(
                    "MCP config database connect/migrate failed: {e}"
                ))));
            }
        }
    } else if use_http || use_mcp {
        tracing::warn!(
            "PLASM_MCP_CONFIG_DATABASE_URL / PLASM_AUTH_STORAGE_URL / DATABASE_URL unset; tenant MCP policy and internal MCP config APIs are disabled"
        );
    }
    if let Some(url) = plasm_agent_core::tenant_binding::tenant_binding_database_url() {
        match plasm_agent_core::tenant_binding::TenantBindingStore::connect(&url).await {
            Ok(store) => {
                saas.tenant_binding = Some(std::sync::Arc::new(store));
                tracing::debug!("tenant_binding: postgres store connected");
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "tenant_binding: postgres connect failed; incoming-tenant resolve returns 503"
                );
            }
        }
    }
    app_state.saas = Some(saas);
    let mcp_port = match matches.get_one::<u16>("mcp_port").copied() {
        Some(p) => p,
        None if use_http && use_mcp => port.saturating_add(1),
        None => port,
    };
    if use_http && use_mcp && mcp_port == port {
        eprintln!("--http and --mcp cannot share the same port; set --mcp-port explicitly.");
        std::process::exit(1);
    }

    if use_http && use_mcp {
        let st = std::sync::Arc::new(app_state);
        let st_http = (*st).clone();
        let st_mcp = std::sync::Arc::clone(&st);
        tokio::select! {
            _ = shutdown_signal() => {
                eprintln!("plasm-mcp: shutting down");
            }
            res = async {
                tokio::try_join!(
                    async {
                        plasm_agent_core::http::serve_http_listener(st_http, port)
                            .await
                            .map_err(|e| {
                                std::io::Error::other(format!("plasm-mcp HTTP server: {e}"))
                            })
                    },
                    async {
                        plasm_agent_core::mcp_server::run_mcp_server("0.0.0.0", mcp_port, st_mcp)
                            .await
                            .map_err(|e| {
                                std::io::Error::other(format!("plasm-mcp MCP server: {e}"))
                            })
                    },
                )
            } => {
                res?;
            }
        }
        return Ok(());
    }
    if use_http {
        tokio::select! {
            _ = shutdown_signal() => {
                eprintln!("plasm-mcp: shutting down");
            }
            r = plasm_agent_core::http::serve_http_listener(app_state, port) => {
                r?;
            }
        }
        return Ok(());
    }
    tokio::select! {
        _ = shutdown_signal() => {
            eprintln!("plasm-mcp: shutting down");
        }
        r = plasm_agent_core::mcp_server::run_mcp_server("0.0.0.0", mcp_port, std::sync::Arc::new(app_state)) => {
            r.map_err(|e| std::io::Error::other(format!("plasm-mcp MCP server: {e}")))?;
        }
    }
    Ok(())
}
