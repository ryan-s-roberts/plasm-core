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

use std::net::TcpListener;
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
    // Compile-time (normal `cargo test`) and runtime (`rtk cargo`, some nextest paths).
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
    let p = repo_root().join("apis/dnd5e");
    assert!(
        p.exists(),
        "missing schema path {p:?}; ensure apis/ symlink (plasm-oss/apis) is present"
    );
    p
}

fn build_harness() -> TuiTestHarness {
    TuiTestHarness::builder()
        .with_size(120, 40)
        // Fail fast for a wedged harness; pathological hangs rely on `scripts/appliance-tui-pty-tests.sh`.
        .with_timeout(Duration::from_secs(100))
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
    let schema = schema_path();
    let data_root = tempfile::Builder::new()
        .prefix("plasm-server-pty-")
        .tempdir_in(embedded_pg_temp_parent())
        .expect("temp appliance data root");
    // `PLASM_APPLIANCE_DIAG_LOG` must stay outside `{data_dir}/postgres` (PGDATA); beside `postgres/` is fine.
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
    // Cold CI: first pg-embed binary download + initdb can exceed 120s.
    cmd.env("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS", "300");
    cmd.env("PLASM_APPLIANCE_DIAG_LOG", diag_log.as_os_str());
    cmd.env("PLASM_APPLIANCE_BOOT_TRACE_STDERR", "1");

    cmd.arg("--schema");
    cmd.arg(schema.as_os_str());
    cmd.arg("--port");
    cmd.arg(listen_port.to_string());

    harness.spawn(cmd).expect("spawn plasm-server in PTY");
    (listen_port, data_root, diag_log)
}

fn wait_run_shell(harness: &mut TuiTestHarness, diag: &Path) {
    // RUN title uses `q quit`; BOOT footer uses `q cancel` — avoids matching BOOT chrome only.
    // `wait_for_text` alone can block forever in ratatui-testlib when the PTY is idle; tickle keys.
    let deadline = Instant::now() + Duration::from_secs(360);
    loop {
        harness
            .send_key(KeyCode::Char('1'))
            .expect("tickle PTY while waiting for RUN shell");
        if harness.screen_contents().contains("q quit") {
            return;
        }
        if deadline <= Instant::now() {
            let tail = read_tail(diag, DIAG_TAIL_MAX);
            let screen = harness.screen_contents();
            panic!(
                "timeout waiting for RUN shell (q quit); last screen:\n{screen}\n--- PLASM_APPLIANCE_DIAG_LOG ({:?}) tail ---\n{tail}",
                diag
            );
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn navigate_to_status_tab(harness: &mut TuiTestHarness) {
    for _ in 0..4 {
        harness
            .send_key(KeyCode::Left)
            .expect("send Left to reach Status tab");
    }
}

fn wait_provision_outcome(harness: &mut TuiTestHarness, diag: &Path) {
    // Assert only definitive provision strings (`provisioned key_id=` / `provision error:`), not footer chrome.
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        // `ratatui-testlib` reads the PTY in blocking mode; if the app does not emit bytes between
        // draws, a bare `update_state` can stall forever. Nudge Crossterm with an inert key (`1`
        // is a no-op on the Keys tab outside the add prompt) so the RUN loop polls + redraws.
        harness
            .send_key(KeyCode::Char('1'))
            .expect("tickle PTY while waiting for provision");
        let screen = harness.screen_contents();
        if screen.contains("provisioned key_id=") || screen.contains("provision error:") {
            return;
        }
        if deadline <= Instant::now() {
            let tail = read_tail(diag, DIAG_TAIL_MAX);
            panic!(
                "timeout waiting for provision result; last screen:\n{screen}\n--- PLASM_APPLIANCE_DIAG_LOG ({:?}) tail ---\n{tail}",
                diag
            );
        }
        std::thread::sleep(Duration::from_millis(120));
    }
}

/// One PTY session: RUN banner, tabs, help overlay, Keys provision, quit — **single** embedded Postgres boot.
#[test]
fn tui_pty_full_suite() {
    require_pty_env();
    let mut harness = build_harness();
    let (listen_port, _data, diag_log) = spawn_appliance(&mut harness);
    wait_run_shell(&mut harness, &diag_log);

    harness
        .wait_for_text(&format!("listen:{listen_port}"))
        .expect("unified listen port in tab rail");

    harness
        .wait_for_text("Listeners")
        .expect("Status tab: Listeners");
    harness
        .wait_for_text(&format!(
            "HTTP+MCP   http://127.0.0.1:{listen_port}  (MCP: /mcp)"
        ))
        .expect("Status tab: unified listener URL");

    harness.send_key(KeyCode::Right).expect("tab to Clients");
    harness
        .wait_for_text("Streamable MCP URL")
        .expect("Clients tab");

    harness.send_key(KeyCode::Right).expect("tab to APIs");
    harness
        .wait_for_text("Space toggle")
        .expect("APIs tab hint");

    harness.send_key(KeyCode::Right).expect("tab to OAuth");
    harness
        .wait_for_text("Connected APIs (OAuth)")
        .expect("OAuth tab");

    harness
        .send_key(KeyCode::Char('n'))
        .expect("new provider wizard");
    harness
        .wait_for_text("OAuth — new provider")
        .expect("wizard chrome");
    harness.send_key(KeyCode::Esc).expect("cancel wizard");
    harness
        .wait_for_text("OAuth provider wizard cancelled")
        .expect("cancel status");

    harness.send_key(KeyCode::Right).expect("tab to Keys");
    harness
        .wait_for_text("a add   r rotate")
        .expect("Keys tab hint");

    navigate_to_status_tab(&mut harness);
    harness.send_key(KeyCode::Char('?')).expect("help");
    harness
        .wait_for_text("Keys: a r d c")
        .expect("help footer extension");
    harness
        .send_key(KeyCode::Right)
        .expect("dismiss help + tab to Clients");
    harness
        .wait_for_text("Streamable MCP URL")
        .expect("Clients after help");

    harness.send_key(KeyCode::Right).expect("to APIs");
    harness.send_key(KeyCode::Right).expect("to OAuth");
    harness.send_key(KeyCode::Right).expect("to Keys");
    harness
        .wait_for_text("a add   r rotate")
        .expect("Keys tab for provision");

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
