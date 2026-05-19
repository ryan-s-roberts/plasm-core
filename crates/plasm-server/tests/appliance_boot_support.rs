//! Shared bootstrap polling for appliance integration tests (diag log + TCP; no PTY).

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const TEST_AUTH_STORAGE_ENCRYPTION_KEY: &str =
    "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";

pub const BOOTSTRAP_WAIT: Duration = Duration::from_secs(600);
pub const BOOTSTRAP_PROGRESS_INTERVAL: Duration = Duration::from_secs(15);
pub const DIAG_TAIL_MAX: usize = 16 * 1024;

pub const BOOT_MILESTONES: &[(&str, &str)] = &[
    ("embedded postgres: listener port selected", "embedded PG port chosen"),
    ("embedded postgres: server ready", "embedded Postgres up"),
    ("plasm HTTP+MCP unified listening", "HTTP+MCP listening"),
    ("phase: run UI handoff", "supervisor sent RUN handoff"),
    ("phase: control station ready", "control station ready (diag)"),
    ("RUN UI first frame not observed", "RUN handshake timeout (fatal)"),
    ("bootstrap failed", "bootstrap failed (fatal)"),
];

/// Plain stderr milestones (PTY mode only; headless passes empty screen).
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

pub fn diag_boot_milestone_report(diag: &Path, screen: &str) -> String {
    let full = read_tail(diag, 512 * 1024);
    let mut lines = Vec::new();
    lines.push(format!("diag log: {}", diag.display()));
    for (needle, label) in BOOT_MILESTONES {
        let mark = if full.contains(needle) { "ok" } else { "MISSING" };
        lines.push(format!("  [{mark}] {label}"));
    }
    for (needle, label) in PTY_BOOT_MILESTONES {
        let mark = if screen.contains(needle) { "ok" } else { "MISSING" };
        lines.push(format!("  [{mark}] {label} (PTY)"));
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
        || tail.contains("FATAL:")
        || tail.contains("Fatal")
        || tail.contains("fatal:")
    {
        Some(tail)
    } else {
        None
    }
}

fn bootstrap_diag_progress(diag: &Path, listen_port: u16, elapsed: Duration, diag_tail: &str) {
    let addr = SocketAddr::from(([127, 0, 0, 1], listen_port));
    let http_up = TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    eprintln!(
        "appliance-boot: waiting ({elapsed:?}) http_connect={http_up}\n{}",
        diag_boot_milestone_report(diag, "")
    );
    if !diag_tail.is_empty() {
        eprintln!(
            "appliance-boot: diag excerpt:\n{}",
            diag_tail.chars().take(1200).collect::<String>()
        );
    }
}

/// Poll `PLASM_APPLIANCE_DIAG_LOG` and HTTP until bootstrap + RUN handoff complete (no PTY I/O).
pub fn wait_bootstrap_ready(listen_port: u16, diag: &Path) {
    let started = Instant::now();
    let deadline = started + BOOTSTRAP_WAIT;
    let mut last_progress = started;
    let addr = SocketAddr::from(([127, 0, 0, 1], listen_port));

    while Instant::now() < deadline {
        if let Some(tail) = diag_has_fatal(diag) {
            panic!(
                "bootstrap fatal\n{}\n--- diag tail ---\n{tail}",
                diag_boot_milestone_report(diag, "")
            );
        }
        let tail = read_tail(diag, 64 * 1024);
        if tail.contains("RUN UI first frame not observed") {
            panic!(
                "plasm-server gave up waiting for RUN UI\n{}",
                diag_boot_milestone_report(diag, "")
            );
        }

        let http_up = TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
        let pg_up = tail.contains("embedded postgres: server ready");
        let http_logged = tail.contains("plasm HTTP+MCP unified listening");
        let run_ready = tail.contains("phase: control station ready");

        if http_up && pg_up && http_logged && run_ready {
            eprintln!(
                "appliance-boot: bootstrap ready (diag+TCP) after {:?}",
                started.elapsed()
            );
            return;
        }

        let now = Instant::now();
        if now.duration_since(last_progress) >= BOOTSTRAP_PROGRESS_INTERVAL {
            last_progress = now;
            bootstrap_diag_progress(diag, listen_port, started.elapsed(), &tail);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!(
        "timeout waiting for bootstrap ({BOOTSTRAP_WAIT:?})\n{}",
        diag_boot_milestone_report(diag, "")
    );
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
    for key in [
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
    ] {
        cmd.env_remove(key);
    }
    cmd.env("PLASM_EMBEDDED_POSTGRES", "1");
}

pub fn apply_appliance_test_env(cmd: &mut std::process::Command, diag_log: &Path) {
    cmd.env("NO_COLOR", "1");
    clear_external_postgres_env(cmd);
    cmd.env("OTEL_SDK_DISABLED", "true");
    cmd.env(
        "AUTH_STORAGE_ENCRYPTION_KEY",
        TEST_AUTH_STORAGE_ENCRYPTION_KEY,
    );
    cmd.env("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS", "300");
    cmd.env("PLASM_EMBEDDED_POSTGRES_PERSISTENT", "0");
    cmd.env("PLASM_APPLIANCE_DIAG_LOG", diag_log);
    cmd.env(
        "RUST_LOG",
        "warn,plasm_appliance_boot=info,plasm_agent=info,plasm_agent_core=warn,pg_embed=warn,sqlx=warn",
    );
}
