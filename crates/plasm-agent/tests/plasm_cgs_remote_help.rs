//! Smoke test: `plasm-cgs` remote-terminal CLI is wired (no HTTP server required).

use std::path::{Path, PathBuf};

fn plasm_cgs_exe() -> PathBuf {
    if let Some(p) = std::env::var_os("CARGO_BIN_EXE_plasm_cgs") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target")
        .join(&profile)
        .join("plasm-cgs")
}

#[test]
fn plasm_cgs_help_ok() {
    let exe = plasm_cgs_exe();
    let out = std::process::Command::new(exe)
        .arg("--help")
        .output()
        .expect("spawn plasm-cgs");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("whoami") && s.contains("search"),
        "expected remote subcommands in help: {s}"
    );
}
