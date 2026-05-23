//! Insta-backed integration: `plasm` CLI subprocess against an in-process discovery/execute router.
//!
//! Contract snapshots for client-owned symbol sessions, discovery merge, mirror layout, and
//! resolved-plan `plasm run` (`POST …/plan` with locally compiled plan IR).
//! Docker-free; uses `fixtures/schemas/overshow_tools`.

use axum::extract::Extension;
use plasm_agent::http::{build_plasm_host_state, discovery_execute_router, PlasmHostBootstrap};
use plasm_agent::incoming_auth::IncomingPrincipal;
use plasm_agent::server_state::CatalogBootstrap;
use plasm_core::discovery::InMemoryCgsRegistry;
use plasm_core::loader::load_schema_dir;
use plasm_runtime::{ExecutionConfig, ExecutionEngine, ExecutionMode};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle as ThreadJoinHandle};
use tempfile::TempDir;
use tokio::net::TcpListener;

fn server_slug(server: &str) -> String {
    let h = Sha256::digest(server.as_bytes());
    hex::encode(h)[..12].to_string()
}

fn plasm_exe() -> PathBuf {
    if let Some(p) = std::env::var_os("CARGO_BIN_EXE_plasm") {
        return PathBuf::from(p);
    }
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target")
        .join(&profile)
        .join("plasm")
}

fn overshow_state() -> plasm_agent::server_state::PlasmHostState {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/schemas/overshow_tools");
    let cgs = Arc::new(load_schema_dir(&dir).expect("overshow_tools"));
    let reg = InMemoryCgsRegistry::from_pairs(vec![(
        "overshow".into(),
        "Overshow".into(),
        vec!["demo".into()],
        cgs.clone(),
    )]);
    let engine = ExecutionEngine::new(ExecutionConfig::default()).expect("engine");
    build_plasm_host_state(PlasmHostBootstrap {
        engine,
        mode: ExecutionMode::Live,
        registry: Arc::new(reg),
        catalog_bootstrap: CatalogBootstrap::Fixed,
        plugin_manager: None,
        incoming_auth: None,
        run_artifacts: Arc::new(plasm_agent::run_artifacts::RunArtifactStore::memory()),
        session_graph_persistence: None,
        oss_local_filesystem_defaults: false,
    })
}

/// Dedicated OS thread + runtime so blocking `Command::output` in tests cannot starve Axum.
fn spawn_test_server() -> (String, ThreadJoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("server runtime");
        rt.block_on(async {
            let st = overshow_state();
            let app = discovery_execute_router(st).layer(Extension(IncomingPrincipal(None)));
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let addr = listener.local_addr().expect("addr");
            tx.send(format!("http://{addr}")).expect("send url");
            axum::serve(listener, app).await.expect("serve");
        });
    });
    let url = rx.recv().expect("server url");
    (url, handle)
}

/// Normalize volatile paths and ids for insta snapshots.
fn normalize_snapshot(raw: &str, home: &Path, server_url: &str) -> String {
    let mut s = raw.to_string();
    if let Ok(canon) = home.canonicalize() {
        s = s.replace(&canon.to_string_lossy().to_string(), "$HOME");
    }
    s = s.replace(&home.to_string_lossy().to_string(), "$HOME");
    s = s.replace(server_url, "$SERVER");
    // per-server mirror directory (slug from URL; port changes each test run)
    let slug = server_slug(server_url);
    s = s.replace(&format!("/servers/{slug}/"), "/servers/SERVER_SLUG/");
    s = s.replace(&format!("servers/{slug}/"), "servers/SERVER_SLUG/");
    // client session ids (`cs_` + 32 hex from `Uuid::simple()`)
    let mut scan = 0;
    while scan < s.len() {
        let Some(rel) = s[scan..].find("cs_") else {
            break;
        };
        let start = scan + rel;
        let end = s[start + 3..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        if end >= 8 {
            s.replace_range(start..start + 3 + end, "cs_SESSION");
            scan = start + "cs_SESSION".len();
        } else {
            scan = start + 3;
        }
    }
    // prompt_hash / catalog digests (64 hex)
    for prefix in ["prompt_hash ", "catalog overshow ", "execution "] {
        if let Some(i) = s.find(prefix) {
            let after = i + prefix.len();
            if s[after..].chars().take(64).all(|c| c.is_ascii_hexdigit()) {
                s.replace_range(after..after + 64, "DIGEST64");
            }
        }
    }
    // run ids pr + hex
    while let Some(i) = s.find("pr") {
        if s[i + 2..].chars().take(64).all(|c| c.is_ascii_hexdigit()) {
            s.replace_range(i..i + 2 + 64, "prRUN64");
        } else {
            break;
        }
    }
    // mirror path noise: collapse duplicate $HOME
    s = s.replace("$HOME/$HOME", "$HOME");
    s
}

struct CliHarness {
    _home_dir: TempDir,
    home: PathBuf,
    server_url: String,
    _server: ThreadJoinHandle<()>,
}

impl CliHarness {
    fn new() -> Self {
        let (server_url, server) = spawn_test_server();
        let home_dir = tempfile::tempdir().expect("tempdir");
        let home = home_dir.path().to_path_buf();
        let exe = plasm_exe();
        let init = Command::new(&exe)
            .env("HOME", &home)
            .args(["init", "--server", &server_url])
            .output()
            .expect("init");
        assert!(
            init.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        Self {
            _home_dir: home_dir,
            home,
            server_url,
            _server: server,
        }
    }

    fn plasm(&self, args: &[&str]) -> Output {
        Command::new(plasm_exe())
            .env("HOME", &self.home)
            .args(args)
            .output()
            .expect("spawn plasm")
    }

    fn plasm_stdin(&self, args: &[&str], stdin: &str) -> Output {
        Command::new(plasm_exe())
            .env("HOME", &self.home)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map(|mut child| {
                if !stdin.is_empty() {
                    if let Some(mut pipe) = child.stdin.take() {
                        let _ = pipe.write_all(stdin.as_bytes());
                    }
                }
                child.wait_with_output().expect("wait")
            })
            .expect("spawn plasm stdin")
    }

    fn norm(&self, raw: &str) -> String {
        normalize_snapshot(raw, &self.home, &self.server_url)
    }

    fn server_root(&self) -> PathBuf {
        self.home
            .join(".plasm/cgs/servers")
            .join(server_slug(&self.server_url))
    }

    fn discovery_tsv(&self) -> String {
        let path = self.server_root().join("latest_discovery.tsv");
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn current_session_id(&self) -> Option<String> {
        let raw = std::fs::read_to_string(self.server_root().join("current_session.txt")).ok()?;
        for line in raw.lines() {
            let line = line.trim();
            if let Some(id) = line.strip_prefix("client_session_id ") {
                let id = id.trim();
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
        None
    }

    fn mirror_layout_snapshot(&self) -> String {
        let root = self.server_root();
        let mut out = String::new();
        out.push_str("tree:\n");
        if let Ok(ptr) = std::fs::read_to_string(root.join("current_session.txt")) {
            out.push_str("current_session.txt:\n");
            out.push_str(&self.norm(&ptr));
        }
        let active_legacy = root.join("active_context.txt");
        out.push_str(&format!(
            "active_context.txt exists: {}\n",
            active_legacy.exists()
        ));
        if let Some(sid) = self.current_session_id() {
            let sess = root.join("sessions").join(&sid);
            out.push_str(&format!("session_dir: sessions/{sid}/\n"));
            for name in ["session_meta.txt", "symbol_state.json", "domain.tsv"] {
                let p = sess.join(name);
                if p.exists() {
                    out.push_str(&format!("{name}: present\n"));
                    if name != "symbol_state.json" {
                        let body = std::fs::read_to_string(&p).unwrap_or_default();
                        let excerpt: String = body.lines().take(12).collect::<Vec<_>>().join("\n");
                        out.push_str(&self.norm(&excerpt));
                        out.push_str("\n");
                    }
                }
            }
            let catalog = sess.join("catalogs/overshow.json");
            out.push_str(&format!(
                "catalogs/overshow.json: {}\n",
                if catalog.exists() {
                    "present"
                } else {
                    "missing"
                }
            ));
        }
        self.norm(&out)
    }
}

/// Run a closure with snapshots pinned to this crate's `tests/snapshots/`.
fn with_insta<T>(f: impl FnOnce() -> T) -> T {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(format!("{}/tests/snapshots", env!("CARGO_MANIFEST_DIR")));
    settings.bind(f)
}

fn combined_output(out: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn cli_init_and_doctor_snapshot() {
    with_insta(|| {
        let h = CliHarness::new();
        let doctor = h.plasm(&["doctor"]);
        assert!(doctor.status.success());
        insta::assert_snapshot!("cli_init_doctor", h.norm(&combined_output(&doctor)));
    });
}

#[test]
fn cli_search_and_merge_discovery_snapshot() {
    with_insta(|| {
        let h = CliHarness::new();
        let search1 = h.plasm(&["search", "profile query"]);
        assert!(search1.status.success(), "{}", combined_output(&search1));
        let tsv1 = h.norm(&h.discovery_tsv());

        let search2 = h.plasm(&["search", "recorded content"]);
        assert!(search2.status.success());
        let tsv2 = h.norm(&h.discovery_tsv());

        insta::assert_snapshot!("cli_search_first", h.norm(&combined_output(&search1)));
        insta::assert_snapshot!("cli_discovery_tsv_merged", tsv2);
        assert!(
            tsv2.contains("Profile") && tsv2.contains("RecordedContent"),
            "merged discovery should retain both profile and recorded rows"
        );
        assert!(tsv1.lines().count() <= tsv2.lines().count());
    });
}

#[test]
fn cli_context_first_exposure_mirror_snapshot() {
    with_insta(|| {
        let h = CliHarness::new();
        assert!(h.plasm(&["search", "profile query"]).status.success());
        let ctx = h.plasm(&["context", "profile query", "Profile", "RecordedContent"]);
        assert!(ctx.status.success(), "{}", combined_output(&ctx));
        insta::assert_snapshot!("cli_context_open", h.norm(&combined_output(&ctx)));
        insta::assert_snapshot!("cli_mirror_after_open", h.mirror_layout_snapshot());
        assert!(
            combined_output(&ctx).contains("overshow:Profile")
                && combined_output(&ctx).contains("overshow:RecordedContent"),
            "qualified capability summary"
        );
    });
}

#[test]
fn cli_context_expand_and_new_snapshot() {
    with_insta(|| {
        let h = CliHarness::new();
        assert!(h.plasm(&["search", "profile query"]).status.success());
        assert!(h
            .plasm(&["context", "profile query", "Profile"])
            .status
            .success());
        let ptr1 = h.current_session_id().expect("session after open");
        let expand = h.plasm(&["context", "profile query", "RecordedContent"]);
        assert!(expand.status.success(), "{}", combined_output(&expand));
        let ptr2 = h.current_session_id().expect("session after expand");
        assert_eq!(ptr1, ptr2, "expand should keep same client session");

        let new_ctx = h.plasm(&["context", "--new", "recorded only", "RecordedContent"]);
        assert!(new_ctx.status.success(), "{}", combined_output(&new_ctx));
        let ptr3 = h.current_session_id().expect("session after --new");
        assert_ne!(ptr1, ptr3, "--new should change client session pointer");

        insta::assert_snapshot!("cli_context_expand", h.norm(&combined_output(&expand)));
        insta::assert_snapshot!("cli_context_new", h.norm(&combined_output(&new_ctx)));
    });
}

#[test]
fn cli_run_plan_snapshot() {
    with_insta(|| {
        let h = CliHarness::new();
        assert!(h.plasm(&["search", "profile query"]).status.success());
        assert!(h
            .plasm(&["context", "profile query", "Profile"])
            .status
            .success());
        let plan = h.plasm_stdin(
            &["run", "--mode", "plan", "--accept", "application/json"],
            "Profile{}",
        );
        assert!(plan.status.success(), "plan: {}", combined_output(&plan));
        insta::assert_snapshot!("cli_run_plan", h.norm(&combined_output(&plan)));
    });
}

#[test]
fn cli_error_paths_snapshot() {
    with_insta(|| {
        let h = CliHarness::new();

        let run_no_ctx = h.plasm_stdin(&["run"], "Profile{}");
        assert!(
            !run_no_ctx.status.success() || {
                let c = combined_output(&run_no_ctx);
                c.contains("No active plasm context")
            }
        );

        let ctx_no_disc = h.plasm(&["context", "profile query", "Profile"]);
        assert!(!ctx_no_disc.status.success());
        let amb = write_ambiguous_discovery(&h.home, &h.server_url);
        assert!(amb);
        let ctx_amb = h.plasm(&["context", "intent", "Issue"]);
        assert!(!ctx_amb.status.success());
        let amb_out = combined_output(&ctx_amb);
        assert!(amb_out.contains("ambiguous"));

        insta::assert_snapshot!(
            "cli_error_no_context",
            h.norm(&combined_output(&run_no_ctx))
        );
        insta::assert_snapshot!(
            "cli_error_no_discovery",
            h.norm(&combined_output(&ctx_no_disc))
        );
        insta::assert_snapshot!("cli_error_ambiguous", h.norm(&amb_out));
    });
}

fn write_ambiguous_discovery(home: &Path, server_url: &str) -> bool {
    let slug = server_slug(server_url);
    let dir = home.join(".plasm/cgs/servers").join(slug);
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let tsv = "intent\ttest\nrow\tapi\tentity\tdescription\n\
               1\tapi_a\tIssue\tfirst\n\
               2\tapi_b\tIssue\tsecond\n";
    std::fs::write(dir.join("latest_discovery.tsv"), tsv).is_ok()
}
