//! Shared bootstrap polling for appliance integration tests (diag log + TCP; no PTY during boot).

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const TEST_AUTH_STORAGE_ENCRYPTION_KEY: &str =
    "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";

pub const BOOTSTRAP_WAIT: Duration = Duration::from_secs(600);
pub const BOOTSTRAP_PROGRESS_INTERVAL: Duration = Duration::from_secs(15);
pub const DIAG_TAIL_MAX: usize = 16 * 1024;
pub const EMBEDDED_PG_TIMEOUT_SECS: &str = "300";
pub const APPLIANCE_TEST_RUST_LOG: &str =
    "warn,plasm_appliance_boot=info,plasm_agent=info,plasm_agent_core=warn,pg_embed=warn,sqlx=warn";

/// Keep in sync with `scripts/appliance-tui-pty-tests.sh` unset list.
pub const EXTERNAL_POSTGRES_ENV_KEYS: &[&str] = &[
    "DATABASE_URL",
    "PLASM_MCP_CONFIG_DATABASE_URL",
    "PLASM_AUTH_STORAGE_URL",
    "PGDATA",
    "PGHOST",
    "PGPORT",
    "PGUSER",
    "PGPASSWORD",
    "PGDATABASE",
    "POSTGRES_URL",
    "POSTGRES_HOST",
    "POSTGRES_PORT",
    "PLASM_EMBEDDED_POSTGRES_DATA_DIR",
    "PLASM_LOCAL_STATE_DIR",
];

const OTEL_EXPORT_ENV_KEYS: &[&str] = &[
    "OTEL_SDK_DISABLED",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
    "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    "OTEL_TRACES_EXPORTER",
    "OTEL_METRICS_EXPORTER",
    "OTEL_LOGS_EXPORTER",
];

/// Must match `tracing::info!(target: "plasm_appliance_boot", …)` in `plasm-server` `main.rs`.
pub const LOG_PHASE_START_UNIFIED_HTTP_MCP: &str = "phase: start unified HTTP+MCP listener";
pub const LOG_PHASE_CONTROL_STATION_READY: &str = "phase: control station ready";
pub const LOG_PHASE_HEADLESS_LISTENER: &str = "phase: headless listener running";
pub const LOG_EMBEDDED_PG_READY: &str = "embedded postgres: server ready";
pub const LOG_EMBEDDED_PG_PORT: &str = "embedded postgres: listener port selected";
pub const LOG_RUN_UI_HANDOFF: &str = "phase: run UI handoff";
pub const LOG_RUN_HANDSHAKE_TIMEOUT: &str = "RUN UI RunEntered not observed within 120s";

pub const BOOT_SUCCESS_MILESTONES: &[(&str, &str)] = &[
    (LOG_EMBEDDED_PG_PORT, "embedded PG port chosen"),
    (LOG_EMBEDDED_PG_READY, "embedded Postgres up"),
    (LOG_PHASE_START_UNIFIED_HTTP_MCP, "HTTP+MCP listener started (diag)"),
    (LOG_RUN_UI_HANDOFF, "supervisor sent RUN handoff"),
    (LOG_PHASE_CONTROL_STATION_READY, "control station ready (TUI diag)"),
    (LOG_PHASE_HEADLESS_LISTENER, "headless listener running (diag)"),
];

/// Plain stderr milestones (PTY mode only).
pub const PTY_BOOT_MILESTONES: &[(&str, &str)] = &[
    (
        "[plasm-server] bootstrap: sent RUN handoff to UI thread",
        "supervisor sent RUN handoff (stderr)",
    ),
    (
        "[plasm-server] bootstrap: UI received RUN handoff",
        "UI received RUN handoff (stderr)",
    ),
    (
        "[plasm-server] bootstrap: emitted RunEntered",
        "UI emitted RunEntered (stderr)",
    ),
    (
        "[plasm-server] bootstrap: RUN UI RunEntered received",
        "supervisor got RunEntered (stderr)",
    ),
    (
        "[plasm-server] bootstrap: control station ready",
        "control station ready (stderr)",
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapMode {
    /// TUI: unified listener + RUN handoff in diag.
    Tui,
    /// `--no-tui`: headless listener milestone in diag.
    Headless,
}

pub fn read_tail(path: &Path, max: usize) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let s = String::from_utf8_lossy(&bytes).into_owned();
            if s.len() <= max {
                s
            } else {
                format!("…[truncated]\n{}", &s[s.len().saturating_sub(max)..])
            }
        }
        Err(e) => format!("(could not read {:?}: {e})", path),
    }
}

pub fn diag_boot_milestone_report(diag: &Path, screen: &str, include_pty_stderr: bool) -> String {
    let full = read_tail(diag, 512 * 1024);
    let mut lines = Vec::new();
    lines.push(format!("diag log: {}", diag.display()));
    for (needle, label) in BOOT_SUCCESS_MILESTONES {
        let mark = if full.contains(needle) { "ok" } else { "MISSING" };
        lines.push(format!("  [{mark}] {label}"));
    }
    if include_pty_stderr {
        for (needle, label) in PTY_BOOT_MILESTONES {
            let mark = if screen.contains(needle) { "ok" } else { "MISSING" };
            lines.push(format!("  [{mark}] {label} (PTY)"));
        }
    }
    lines.push("--- last plasm_appliance_boot / plasm-server lines (diag) ---".into());
    let mut boot_lines = 0usize;
    for line in full.lines().rev() {
        if line.contains("plasm_appliance_boot")
            || line.contains("[plasm-server]")
            || line.contains("plasm_agent::embedded_postgres")
        {
            lines.push(format!("  {line}"));
            boot_lines += 1;
            if boot_lines >= 8 {
                break;
            }
        }
    }
    lines.join("\n")
}

pub fn pick_free_tcp_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind 127.0.0.1:0")
        .local_addr()
        .expect("local_addr")
        .port()
}

pub fn diag_has_fatal(diag: &Path) -> Option<String> {
    let tail = read_tail(diag, 8 * 1024);
    if tail.contains("bootstrap failed")
        || tail.contains("[plasm-server] fatal:")
        || tail.contains(LOG_RUN_HANDSHAKE_TIMEOUT)
    {
        Some(tail)
    } else {
        None
    }
}

fn bootstrap_complete(mode: BootstrapMode, tail: &str, http_up: bool) -> bool {
    if !http_up || !tail.contains(LOG_EMBEDDED_PG_READY) {
        return false;
    }
    match mode {
        BootstrapMode::Tui => {
            tail.contains(LOG_PHASE_START_UNIFIED_HTTP_MCP)
                && tail.contains(LOG_PHASE_CONTROL_STATION_READY)
        }
        BootstrapMode::Headless => tail.contains(LOG_PHASE_HEADLESS_LISTENER),
    }
}

fn bootstrap_diag_progress(
    diag: &Path,
    listen_port: u16,
    mode: BootstrapMode,
    elapsed: Duration,
    diag_tail: &str,
    include_pty_stderr: bool,
) {
    let addr = SocketAddr::from(([127, 0, 0, 1], listen_port));
    let http_up = TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    eprintln!(
        "appliance-boot: waiting ({elapsed:?}) mode={mode:?} http_connect={http_up}\n{}",
        diag_boot_milestone_report(diag, "", include_pty_stderr)
    );
    if !diag_tail.is_empty() {
        eprintln!(
            "appliance-boot: diag excerpt:\n{}",
            diag_tail.chars().take(1200).collect::<String>()
        );
    }
}

/// Poll `PLASM_APPLIANCE_DIAG_LOG` and HTTP until bootstrap is complete (no PTY I/O).
pub fn wait_bootstrap_ready(listen_port: u16, diag: &Path, mode: BootstrapMode) {
    let include_pty = mode == BootstrapMode::Tui;
    let started = Instant::now();
    let deadline = started + BOOTSTRAP_WAIT;
    let mut last_progress = started;
    let addr = SocketAddr::from(([127, 0, 0, 1], listen_port));

    while Instant::now() < deadline {
        if let Some(tail) = diag_has_fatal(diag) {
            panic!(
                "bootstrap fatal\n{}\n--- diag tail ---\n{tail}",
                diag_boot_milestone_report(diag, "", include_pty)
            );
        }
        let tail = read_tail(diag, 64 * 1024);
        if tail.contains(LOG_RUN_HANDSHAKE_TIMEOUT) {
            panic!(
                "plasm-server gave up waiting for RUN UI RunEntered\n{}",
                diag_boot_milestone_report(diag, "", include_pty)
            );
        }

        let http_up = TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
        if bootstrap_complete(mode, &tail, http_up) {
            eprintln!(
                "appliance-boot: bootstrap ready ({mode:?}) after {:?}",
                started.elapsed()
            );
            return;
        }

        let now = Instant::now();
        if now.duration_since(last_progress) >= BOOTSTRAP_PROGRESS_INTERVAL {
            last_progress = now;
            bootstrap_diag_progress(diag, listen_port, mode, started.elapsed(), &tail, include_pty);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!(
        "timeout waiting for bootstrap ({BOOTSTRAP_WAIT:?}, mode={mode:?})\n{}",
        diag_boot_milestone_report(diag, "", include_pty)
    );
}

pub fn make_appliance_data_root(prefix: &str) -> (tempfile::TempDir, PathBuf) {
    let data_root = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(embedded_pg_temp_parent())
        .expect("temp appliance data root");
    let diag_log = data_root.path().join("appliance-diag.log");
    (data_root, diag_log)
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize repo root from CARGO_MANIFEST_DIR/../../..")
}

pub fn schema_path() -> PathBuf {
    let p = repo_root().join("fixtures/schemas/overshow_tools");
    assert!(
        p.exists(),
        "missing schema path {p:?}; run tests from the monorepo root"
    );
    p
}

pub fn bin_path() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_plasm_server") {
        return PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("CARGO_BIN_EXE_plasm_server") {
        return PathBuf::from(p);
    }
    let profile = std::env::var("CARGO_PROFILE")
        .or_else(|_| std::env::var("PROFILE"))
        .unwrap_or_else(|_| "debug".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = Vec::new();
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        candidates.push(
            PathBuf::from(target_dir)
                .join(&profile)
                .join("plasm-server"),
        );
    }
    candidates.push(
        manifest
            .join("../../target")
            .join(&profile)
            .join("plasm-server"),
    );
    candidates.push(
        manifest
            .join("../../../target")
            .join(&profile)
            .join("plasm-server"),
    );
    for p in candidates {
        if p.is_file() {
            return p;
        }
    }
    panic!(
        "plasm-server binary not found (profile={profile}); \
         run `cargo build -p plasm-server` or set CARGO_BIN_EXE_plasm_server"
    );
}

pub fn embedded_pg_temp_parent() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/tmp")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir()
    }
}

pub fn clear_external_postgres_env(cmd: &mut std::process::Command) {
    for key in EXTERNAL_POSTGRES_ENV_KEYS {
        cmd.env_remove(key);
    }
    cmd.env("PLASM_EMBEDDED_POSTGRES", "1");
}

fn clear_otel_export_env(cmd: &mut std::process::Command) {
    for key in OTEL_EXPORT_ENV_KEYS {
        cmd.env_remove(key);
    }
    cmd.env("OTEL_SDK_DISABLED", "true");
}

pub fn apply_appliance_test_env(cmd: &mut std::process::Command, diag_log: &Path) {
    cmd.env("NO_COLOR", "1");
    clear_external_postgres_env(cmd);
    clear_otel_export_env(cmd);
    cmd.env(
        "AUTH_STORAGE_ENCRYPTION_KEY",
        TEST_AUTH_STORAGE_ENCRYPTION_KEY,
    );
    cmd.env("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS", EMBEDDED_PG_TIMEOUT_SECS);
    cmd.env("PLASM_EMBEDDED_POSTGRES_PERSISTENT", "0");
    cmd.env("PLASM_APPLIANCE_DIAG_LOG", diag_log);
    cmd.env("RUST_LOG", APPLIANCE_TEST_RUST_LOG);
}

pub fn push_appliance_cli_args(
    cmd: &mut std::process::Command,
    data_dir: &Path,
    schema: &Path,
    listen_port: u16,
) {
    cmd.arg("--data-dir").arg(data_dir);
    cmd.arg("--schema").arg(schema);
    cmd.arg("--port").arg(listen_port.to_string());
}

#[cfg(feature = "tui_pty_tests")]
pub fn clear_external_postgres_env_pty(cmd: &mut portable_pty::CommandBuilder) {
    for key in EXTERNAL_POSTGRES_ENV_KEYS {
        cmd.env_remove(key);
    }
    cmd.env("PLASM_EMBEDDED_POSTGRES", "1");
}

#[cfg(feature = "tui_pty_tests")]
fn clear_otel_export_env_pty(cmd: &mut portable_pty::CommandBuilder) {
    for key in OTEL_EXPORT_ENV_KEYS {
        cmd.env_remove(key);
    }
    cmd.env("OTEL_SDK_DISABLED", "true");
}

#[cfg(feature = "tui_pty_tests")]
pub fn apply_appliance_test_env_pty(cmd: &mut portable_pty::CommandBuilder, diag_log: &Path) {
    cmd.env("NO_COLOR", "1");
    clear_external_postgres_env_pty(cmd);
    clear_otel_export_env_pty(cmd);
    cmd.env(
        "AUTH_STORAGE_ENCRYPTION_KEY",
        TEST_AUTH_STORAGE_ENCRYPTION_KEY,
    );
    cmd.env("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS", EMBEDDED_PG_TIMEOUT_SECS);
    cmd.env("PLASM_EMBEDDED_POSTGRES_PERSISTENT", "0");
    cmd.env("PLASM_APPLIANCE_DIAG_LOG", diag_log.as_os_str());
    cmd.env("PLASM_TUI_PTY_TESTS", "1");
    cmd.env("RUST_LOG", APPLIANCE_TEST_RUST_LOG);
}

#[cfg(feature = "tui_pty_tests")]
pub fn push_appliance_cli_args_pty(
    cmd: &mut portable_pty::CommandBuilder,
    data_dir: &std::ffi::OsStr,
    schema: &std::ffi::OsStr,
    listen_port: u16,
) {
    cmd.arg("--data-dir");
    cmd.arg(data_dir);
    cmd.arg("--schema");
    cmd.arg(schema);
    cmd.arg("--port");
    cmd.arg(listen_port.to_string());
}
