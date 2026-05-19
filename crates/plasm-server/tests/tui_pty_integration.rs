#![cfg(unix)]

//! PTY keystroke integration tests for the real `plasm-server` binary (Ratatui + Crossterm +
//! embedded Postgres + auth KV). See `tests/tui_feature_inventory.md` and
//! `docs/appliance-surface-inventory.md`.
//!
//! Bootstrap progress is polled via **`PLASM_APPLIANCE_DIAG_LOG` + TCP only** (no PTY drain) so
//! `update_state` cannot block the test thread. See `appliance_boot_support` and
//! `appliance_headless_boot` for the shared smoke path.
//!
//! Requires `PLASM_TUI_PTY_TESTS=1` and `--features tui_pty_tests`.

mod appliance_boot_support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use portable_pty::CommandBuilder;
use ratatui_testlib::{KeyCode, TuiTestHarness};

use appliance_boot_support::{
    apply_appliance_test_env_pty, bin_path, diag_boot_milestone_report, diag_has_fatal,
    make_appliance_data_root, push_appliance_cli_args_pty, read_tail, repo_root, schema_path,
    wait_bootstrap_ready, BootstrapMode, BOOTSTRAP_PROGRESS_INTERVAL, DIAG_TAIL_MAX,
};

/// Max `update_state` rounds per PTY drain (ratatui-testlib reads until dry each call).
const PTY_DRAIN_MAX_ROUNDS: u32 = 4;

fn require_pty_env() {
    assert_eq!(
        std::env::var("PLASM_TUI_PTY_TESTS").as_deref(),
        Ok("1"),
        "PTY integration tests require PLASM_TUI_PTY_TESTS=1 (see docs/appliance-surface-inventory.md)"
    );
}

fn build_harness() -> TuiTestHarness {
    TuiTestHarness::builder()
        .with_size(120, 40)
        .with_timeout(Duration::from_secs(420))
        .with_poll_interval(Duration::from_millis(150))
        .build()
        .expect("TuiTestHarness::builder")
}

fn spawn_appliance(harness: &mut TuiTestHarness) -> (u16, tempfile::TempDir, PathBuf) {
    let listen_port = appliance_boot_support::pick_free_tcp_port();
    let schema = schema_path();
    let (data_root, diag_log) = make_appliance_data_root("plasm-server-pty-");

    let mut cmd = CommandBuilder::new(bin_path());
    cmd.cwd(repo_root());
    apply_appliance_test_env_pty(&mut cmd, &diag_log);
    push_appliance_cli_args_pty(
        &mut cmd,
        data_root.path().as_os_str(),
        schema.as_os_str(),
        listen_port,
    );

    eprintln!(
        "appliance-pty: spawn plasm-server listen_port={listen_port} embedded_pg_port=ephemeral data_dir={} diag_log={}",
        data_root.path().display(),
        diag_log.display(),
    );

    harness.spawn(cmd).expect("spawn plasm-server in PTY");
    (listen_port, data_root, diag_log)
}

fn drain_pty_bounded(harness: &mut TuiTestHarness) {
    for _ in 0..PTY_DRAIN_MAX_ROUNDS {
        let _ = harness.update_state();
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn nudge_pty(harness: &mut TuiTestHarness) {
    let _ = harness.send_key(KeyCode::Char('1'));
}

fn wait_for_screen(
    harness: &mut TuiTestHarness,
    needles: &[&str],
    timeout: Duration,
    diag: &Path,
    label: &str,
) {
    let started = Instant::now();
    let deadline = started + timeout;
    let mut last_progress = started;
    while Instant::now() < deadline {
        if let Some(tail) = diag_has_fatal(diag) {
            let screen = harness.screen_contents();
            panic!(
                "bootstrap fatal in PLASM_APPLIANCE_DIAG_LOG while waiting for {label};\n--- tail ---\n{tail}\n--- screen ---\n{screen}"
            );
        }
        drain_pty_bounded(harness);
        nudge_pty(harness);
        let screen = harness.screen_contents();
        if needles.iter().any(|n| screen.contains(n)) {
            eprintln!(
                "appliance-pty: screen matched {label} after {:?}",
                started.elapsed()
            );
            return;
        }
        let now = Instant::now();
        if now.duration_since(last_progress) >= BOOTSTRAP_PROGRESS_INTERVAL {
            last_progress = now;
            eprintln!(
                "appliance-pty: still waiting for screen {label} ({:?} elapsed)\n{}",
                started.elapsed(),
                diag_boot_milestone_report(diag, &screen, true)
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let tail = read_tail(diag, DIAG_TAIL_MAX);
    let screen = harness.screen_contents();
    panic!(
        "timeout waiting for {label} (needles={needles:?}); last screen:\n{screen}\n--- PLASM_APPLIANCE_DIAG_LOG ({diag:?}) tail ---\n{tail}"
    );
}

fn wait_run_shell(harness: &mut TuiTestHarness, listen_port: u16, diag: &Path) {
    wait_bootstrap_ready(listen_port, diag, BootstrapMode::Tui);
    drain_pty_bounded(harness);
    wait_for_screen(harness, &["q: quit"], Duration::from_secs(45), diag, "RUN footer");
    wait_for_screen(
        harness,
        &["policy store (project_mcp_*)"],
        Duration::from_secs(45),
        diag,
        "MCP policy store ready",
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

fn spawn_heartbeat() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            eprintln!("appliance-pty: heartbeat (test still running)");
        }
    })
}

#[test]
fn tui_pty_full_suite() {
    require_pty_env();
    let _heartbeat = spawn_heartbeat();
    eprintln!("appliance-pty: tui_pty_full_suite starting");
    let mut harness = build_harness();
    let (listen_port, _data, diag_log) = spawn_appliance(&mut harness);
    eprintln!("appliance-pty: waiting for bootstrap (diag log + TCP, not PTY)");
    wait_run_shell(&mut harness, listen_port, &diag_log);
    eprintln!("appliance-pty: RUN shell ready, exercising tabs");

    wait_for_screen(
        &mut harness,
        &[&format!("listen:{listen_port} (HTTP+MCP)")],
        Duration::from_secs(15),
        &diag_log,
        "tab rail listen port",
    );

    wait_for_screen(
        &mut harness,
        &["Listeners"],
        Duration::from_secs(15),
        &diag_log,
        "Status tab: Listeners",
    );
    wait_for_screen(
        &mut harness,
        &[&format!(
            "HTTP+MCP   http://127.0.0.1:{listen_port}  (MCP: /mcp)"
        )],
        Duration::from_secs(15),
        &diag_log,
        "Status tab: unified listener URL",
    );

    harness.send_key(KeyCode::Right).expect("tab to Clients");
    wait_for_screen(
        &mut harness,
        &["Authorization: Bearer"],
        Duration::from_secs(15),
        &diag_log,
        "Clients tab",
    );

    harness.send_key(KeyCode::Right).expect("tab to APIs");
    wait_for_screen(
        &mut harness,
        &["Filter catalogues"],
        Duration::from_secs(15),
        &diag_log,
        "APIs tab",
    );

    harness.send_key(KeyCode::Right).expect("tab to OAuth");
    wait_for_screen(
        &mut harness,
        &["Providers"],
        Duration::from_secs(15),
        &diag_log,
        "OAuth tab",
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
    );
    harness.send_key(KeyCode::Esc).expect("cancel wizard");
    wait_for_screen(
        &mut harness,
        &["OAuth wizard cancelled"],
        Duration::from_secs(15),
        &diag_log,
        "wizard cancel notice",
    );

    harness.send_key(KeyCode::Right).expect("tab to Keys");
    wait_for_screen(
        &mut harness,
        &["a: add", "Keys"],
        Duration::from_secs(15),
        &diag_log,
        "Keys tab",
    );

    navigate_to_status_tab(&mut harness);
    harness.send_key(KeyCode::Char('?')).expect("help");
    wait_for_screen(
        &mut harness,
        &["Keys: a r d c"],
        Duration::from_secs(15),
        &diag_log,
        "help footer extension",
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
