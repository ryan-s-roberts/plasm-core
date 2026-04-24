//! Binary entry: `plasm-trace-sink`.
//!
//! Startup is structured so every failure path logs (`tracing::error`) and writes a plain line to
//! stderr before exit code 1 — silent exits are unacceptable for operators (`kubectl logs`).

use std::io::Write;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::error::ErrorKind;
use clap::Parser;
use plasm_trace_sink::append_port::AuditSpanStore;
use plasm_trace_sink::config::{Config, WarehouseLocation};
use plasm_trace_sink::http::router;
use plasm_trace_sink::iceberg_writer::IcebergSink;
use plasm_trace_sink::persisted::PersistedTraceSink;
use plasm_trace_sink::state::AppState;

#[derive(Parser, Debug)]
#[command(name = "plasm-trace-sink")]
struct Args {
    /// Override listen address (else `PLASM_TRACE_SINK_LISTEN` or default).
    #[arg(long)]
    listen: Option<String>,
}

fn install_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::io::stderr().write_all(format!("plasm-trace-sink panic: {info}\n").as_bytes());
        let _ = std::io::stderr().flush();
        default_panic(info);
    }));
}

fn init_tracing() {
    if let Err(e) = plasm_otel::init("plasm-trace-sink") {
        let _ = writeln!(
            std::io::stderr(),
            "plasm-trace-sink FATAL: tracing / OpenTelemetry init failed: {e:#}"
        );
        let _ = std::io::stderr().flush();
        std::process::exit(1);
    }
}

fn log_fatal_and_exit(err: &anyhow::Error) -> ! {
    // tracing first (structured), then a single guaranteed line for log collectors / systemd.
    tracing::error!(error = %err, error_debug = ?err, "plasm-trace-sink fatal startup or runtime error");
    let _ = writeln!(std::io::stderr(), "plasm-trace-sink FATAL: {err:#}",);
    let _ = std::io::stderr().flush();
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> ExitCode {
    install_panic_hook();
    init_tracing();

    tracing::info!("plasm-trace-sink process starting");
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log_fatal_and_exit(&e);
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let args = match Args::try_parse() {
        Ok(a) => a,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            e.exit()
        }
        Err(e) => return Err(e).context("parse CLI arguments"),
    };

    Config::ensure_iceberg_not_disabled().context("PLASM_TRACE_SINK_ICEBERG")?;

    let mut config = Config::from_env();
    if let Some(l) = args.listen {
        config.listen = l;
    }

    tracing::info!(
        data_dir = %config.data_dir.display(),
        warehouse_fs_path = %config.warehouse_fs_path.display(),
        warehouse_s3_url_set = config.warehouse_s3_url.is_some(),
        catalog_url_set = config.catalog_url.is_some(),
        "config loaded"
    );

    tokio::fs::create_dir_all(&config.data_dir)
        .await
        .with_context(|| format!("create data_dir {}", config.data_dir.display()))?;

    let connect = config
        .iceberg_connect_params()
        .context("build Iceberg catalog URL + warehouse params")?;

    if let WarehouseLocation::Filesystem(root) = &connect.warehouse {
        tokio::fs::create_dir_all(root)
            .await
            .with_context(|| format!("create warehouse dir {}", root.display()))?;
    }

    tracing::info!(
        catalog = %connect.catalog.redacted_for_logs(),
        warehouse = %connect.warehouse.summary_for_logs(),
        "connecting Iceberg SqlCatalog"
    );
    let iceberg = Arc::new(
        IcebergSink::connect(&connect)
            .await
            .context("Iceberg SqlCatalog::connect / namespace / table init")?,
    );

    tracing::info!(
        catalog = %connect.catalog.redacted_for_logs(),
        warehouse = %connect.warehouse.summary_for_logs(),
        "Iceberg SqlCatalog ready"
    );

    let store: Arc<dyn AuditSpanStore> = PersistedTraceSink::connect(&connect, iceberg.clone())
        .await
        .context("SQL trace projections (same catalog DB as Iceberg)")?;

    tracing::info!("trace projection store ready (plasm_trace_sink schema on Postgres)");
    let state = AppState::new(store);
    let app = router(state);

    let addr: SocketAddr = config
        .listen
        .parse()
        .with_context(|| format!("parse listen address {:?}", config.listen))?;

    tracing::info!(%addr, "binding HTTP listener");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("TcpListener::bind({addr})"))?;

    tracing::info!(%addr, "serving HTTP (health GET /v1/health)");
    axum::serve(listener, app)
        .await
        .context("axum::serve ended (listener closed or protocol error)")?;

    tracing::warn!("axum::serve returned Ok — this is unexpected for a long-running server");
    Ok(())
}
