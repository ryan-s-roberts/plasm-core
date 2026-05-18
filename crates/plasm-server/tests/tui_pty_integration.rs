#![cfg(unix)]

//! PTY keystroke integration tests for the real `plasm-server` binary (Ratatui + Crossterm +
//! embedded Postgres + auth KV). See `tests/tui_feature_inventory.md` and
//! `docs/appliance-surface-inventory.md`.
//!
//! **PTY pass ≠ interactive terminal proof:** the harness tickles redraws; see that doc for PTY
//! vs hang and for headless `mcp_config_admin` coverage.
//!
//! Requires `PLASM_TUI_PTY_TESTS=1` and `--features tui_pty_tests`.
//!
//! **One suite, one boot:** a single `#[test]` avoids multiple cold `pg-embed` starts (multiplied wall
//! time and watchdog pain). OAuth wizard Esc-cancel is covered inside that suite.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use portable_pty::CommandBuilder;
use ratatui_testlib::{KeyCode, TuiTestHarness};

/// Base64-encoded 32-byte key (32× `a`) for Postgres `EncryptedStorage` in tests only.
const TEST_AUTH_STORAGE_ENCRYPTION_KEY: &str = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";

/// Max bytes of `PLASM_APPLIANCE_DIAG_LOG` to include in failure messages.
const DIAG_TAIL_MAX: usize = 16 * 1024;

fn read_tail(path: &Path, max: usize) -> String {
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

fn pick_free_tcp_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind 127.0.0.1:0")
        .local_addr()
        .expect("local_addr")
        .port()
}

/// Strip CI / developer Postgres URLs so embedded pg-embed owns loopback `DATABASE_URL`.
fn clear_external_postgres_env(cmd: &mut CommandBuilder) {
    for key in [
        "DATABASE_URL",
        "PLASM_MCP_CONFIG_DATABASE_URL",
        "PLASM_AUTH_STORAGE_URL",
    ] {
        cmd.env_remove(key);
    }
}

/// Strip OTLP env inherited from the test runner (can block boot before the TUI draws).
fn clear_otel_export_env(cmd: &mut CommandBuilder) {
    for key in [
        "OTEL_SDK_DISABLED",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
        "OTEL_TRACES_EXPORTER",
        "OTEL_METRICS_EXPORTER",
        "OTEL_LOGS_EXPORTER",
    ] {
        cmd.env_remove(key);
    }
    cmd.env("OTEL_SDK_DISABLED", "true");
}

fn require_pty_env() {
    assert_eq!(
        std::env::var("PLASM_TUI_PTY_TESTS").as_deref(),
        Ok("1"),
        "PTY integration tests require PLASM_TUI_PTY_TESTS=1 (see docs/appliance-surface-inventory.md)"
    );
}

fn bin_path() -> PathBuf {
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
         run `cargo build -p plasm-server --features tui_pty_tests` or set CARGO_BIN_EXE_plasm_server"
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize repo root from CARGO_MANIFEST_DIR/../../..")
}

fn schema_path() -> PathBuf {
    let p = repo_root().join("fixtures/schemas/overshow_tools");
    assert!(
        p.exists(),
        "missing schema path {p:?}; run tests from the monorepo root"
    );
    p
}

fn build_harness() -> TuiTestHarness {
    TuiTestHarness::builder()
        .with_size(120, 40)
        // Cold pg-embed + BOOT→RUN handoff can exceed 100s on CI.
        .with_timeout(Duration::from_secs(420))
        .with_poll_interval(Duration::from_millis(150))
        .build()
        .expect("TuiTestHarness::builder")
}

fn embedded_pg_temp_parent() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/tmp")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir()
    }
}

fn spawn_appliance(harness: &mut TuiTestHarness) -> (u16, tempfile::TempDir, PathBuf) {
    let listen_port = pick_free_tcp_port();
    let pg_port = pick_free_tcp_port();
    let schema = schema_path();
    let data_root = tempfile::Builder::new()
        .prefix("plasm-server-pty-")
        .tempdir_in(embedded_pg_temp_parent())
        .expect("temp appliance data root");
    let diag_log = data_root.path().join("appliance-diag.log");

    let mut cmd = CommandBuilder::new(bin_path());
    cmd.cwd(repo_root());
    cmd.env("NO_COLOR", "1");
    clear_external_postgres_env(&mut cmd);
    clear_otel_export_env(&mut cmd);
    cmd.env(
        "AUTH_STORAGE_ENCRYPTION_KEY",
        TEST_AUTH_STORAGE_ENCRYPTION_KEY,
    );
    cmd.arg("--data-dir");
    cmd.arg(data_root.path().as_os_str());
    cmd.env("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS", "300");
    cmd.env("PLASM_EMBEDDED_POSTGRES_PORT", pg_port.to_string());
    cmd.env("PLASM_APPLIANCE_DIAG_LOG", diag_log.as_os_str());

    cmd.arg("--schema");
    cmd.arg(schema.as_os_str());
    cmd.arg("--port");
    cmd.arg(listen_port.to_string());

    harness.spawn(cmd).expect("spawn plasm-server in PTY");
    (listen_port, data_root, diag_log)
}

/// Poll the PTY for any of `needles` (one non-blocking `update_state` per iteration).
fn wait_for_screen(
    harness: &mut TuiTestHarness,
    needles: &[&str],
    timeout: Duration,
    diag: &Path,
    label: &str,
    tickle: bool,
) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(tail) = diag_has_fatal(diag) {
            let screen = harness.screen_contents();
            panic!(
                "bootstrap fatal in PLASM_APPLIANCE_DIAG_LOG while waiting for {label};\n--- tail ---\n{tail}\n--- screen ---\n{screen}"
            );
        }
        if tickle {
            harness
                .send_key(KeyCode::Char('1'))
                .expect("tickle PTY while polling screen");
        } else {
            harness
                .update_state()
                .expect("update_state while polling screen");
        }
        let screen = harness.screen_contents();
        if needles.iter().any(|n| screen.contains(n)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let tail = read_tail(diag, DIAG_TAIL_MAX);
    let screen = harness.screen_contents();
    panic!(
        "timeout waiting for {label} (needles={needles:?}); last screen:\n{screen}\n--- PLASM_APPLIANCE_DIAG_LOG ({diag:?}) tail ---\n{tail}"
    );
}

fn diag_has_fatal(diag: &Path) -> Option<String> {
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

/// Wait until HTTP+MCP bind succeeds (survives BOOT redraw spam on the PTY).
fn wait_tcp_listen(port: u16, timeout: Duration, diag: &Path) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(tail) = diag_has_fatal(diag) {
            panic!("bootstrap fatal in PLASM_APPLIANCE_DIAG_LOG:\n{tail}");
        }
        if TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let tail = read_tail(diag, DIAG_TAIL_MAX);
    panic!(
        "timeout waiting for TCP listen on {addr} (timeout {timeout:?})\n--- PLASM_APPLIANCE_DIAG_LOG ({diag:?}) tail ---\n{tail}"
    );
}

fn wait_run_shell(harness: &mut TuiTestHarness, listen_port: u16, diag: &Path) {
    wait_tcp_listen(listen_port, Duration::from_secs(300), diag);
    wait_for_screen(
        harness,
        &["q: quit", "[Status]"],
        Duration::from_secs(60),
        diag,
        "RUN shell",
        false,
    );
    wait_for_screen(
        harness,
        &["policy store (project_mcp_*)"],
        Duration::from_secs(60),
        diag,
        "MCP policy store ready",
        false,
    );
}

fn navigate_to_status_tab(harness: &mut TuiTestHarness) {
    for _ in 0..4 {
        harness
            .send_key(KeyCode::Left)
            .expect("send Left to reach Status tab");
    }
}

fn wait_provision_outcome(harness: &mut TuiTestHarness, diag: &Path) {
    wait_for_screen(
        harness,
        &[
            "API key provisioned",
            "API key provision failed",
            "Wait for the appliance config refresh",
        ],
        Duration::from_secs(120),
        diag,
        "API key provision",
        false,
    );
    let screen = harness.screen_contents();
    if screen.contains("API key provision failed")
        || screen.contains("Wait for the appliance config refresh")
    {
        panic!(
            "API key provision did not succeed; screen:\n{screen}\n--- PLASM_APPLIANCE_DIAG_LOG ({diag:?}) tail ---\n{}",
            read_tail(diag, DIAG_TAIL_MAX)
        );
    }
}

/// One PTY session: RUN banner, tabs, help overlay, Keys provision, quit — **single** embedded Postgres boot.
#[test]
fn tui_pty_full_suite() {
    require_pty_env();
    let mut harness = build_harness();
    let (listen_port, _data, diag_log) = spawn_appliance(&mut harness);
    wait_run_shell(&mut harness, listen_port, &diag_log);

    wait_for_screen(
        &mut harness,
        &[&format!("listen:{listen_port} (HTTP+MCP)")],
        Duration::from_secs(15),
        &diag_log,
        "tab rail listen port",
        false,
    );

    wait_for_screen(
        &mut harness,
        &["Listeners"],
        Duration::from_secs(15),
        &diag_log,
        "Status tab: Listeners",
        false,
    );
    wait_for_screen(
        &mut harness,
        &[&format!(
            "HTTP+MCP   http://127.0.0.1:{listen_port}  (MCP: /mcp)"
        )],
        Duration::from_secs(15),
        &diag_log,
        "Status tab: unified listener URL",
        false,
    );

    harness.send_key(KeyCode::Right).expect("tab to Clients");
    wait_for_screen(
        &mut harness,
        &["Authorization: Bearer"],
        Duration::from_secs(15),
        &diag_log,
        "Clients tab",
        false,
    );

    harness.send_key(KeyCode::Right).expect("tab to APIs");
    wait_for_screen(
        &mut harness,
        &["Filter catalogues"],
        Duration::from_secs(15),
        &diag_log,
        "APIs tab",
        false,
    );

    harness.send_key(KeyCode::Right).expect("tab to OAuth");
    wait_for_screen(
        &mut harness,
        &["Providers"],
        Duration::from_secs(15),
        &diag_log,
        "OAuth tab",
        false,
    );

    harness
        .send_key(KeyCode::Char('n'))
        .expect("new provider wizard");
    wait_for_screen(
        &mut harness,
        &["New OAuth provider"],
        Duration::from_secs(15),
        &diag_log,
        "OAuth wizard",
        false,
    );
    harness.send_key(KeyCode::Esc).expect("cancel wizard");
    wait_for_screen(
        &mut harness,
        &["OAuth wizard cancelled"],
        Duration::from_secs(15),
        &diag_log,
        "wizard cancel notice",
        false,
    );

    harness.send_key(KeyCode::Right).expect("tab to Keys");
    wait_for_screen(
        &mut harness,
        &["a: add", "Keys"],
        Duration::from_secs(15),
        &diag_log,
        "Keys tab",
        false,
    );

    navigate_to_status_tab(&mut harness);
    harness.send_key(KeyCode::Char('?')).expect("help");
    wait_for_screen(
        &mut harness,
        &["Keys: a r d c"],
        Duration::from_secs(15),
        &diag_log,
        "help footer extension",
        false,
    );
    harness
        .send_key(KeyCode::Right)
        .expect("dismiss help + tab to Clients");
    wait_for_screen(
        &mut harness,
        &["Authorization: Bearer"],
        Duration::from_secs(15),
        &diag_log,
        "Clients after help",
        false,
    );

    harness.send_key(KeyCode::Right).expect("to APIs");
    harness.send_key(KeyCode::Right).expect("to OAuth");
    harness.send_key(KeyCode::Right).expect("to Keys");
    wait_for_screen(
        &mut harness,
        &["a: add", "Keys"],
        Duration::from_secs(15),
        &diag_log,
        "Keys tab for provision",
        false,
    );

    let label = format!(
        "ptyk{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    harness.send_key(KeyCode::Char('a')).expect("add key");
    harness.send_keys(&label).expect("type key label");
    harness.send_key(KeyCode::Enter).expect("submit label");
    wait_provision_outcome(&mut harness, &diag_log);

    harness.send_key(KeyCode::Char('q')).expect("quit");
    let status = harness.wait_exit().expect("wait_exit");
    assert!(
        status.success(),
        "plasm-server should exit cleanly after q (got {status:?})"
    );
}
