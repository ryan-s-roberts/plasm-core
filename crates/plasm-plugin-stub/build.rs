//! When `PLASM_EMBEDDED_CGS` is set, copy that interchange YAML into `OUT_DIR/catalog.cgs.yaml`.
//! Otherwise copy [`fixtures/plugin_stub_catalog.cgs.yaml`] for default stub tests.
//!
//! When packing from `apis/<name>/`, `plasm-pack-plugins` also sets `PLASM_EMBEDDED_CGS_API_DIR` so
//! Cargo invalidates this build when `domain.yaml` / `mappings.yaml` change (not only the generated
//! interchange file).

use hex::encode as hex_encode;
use sha2::{Digest, Sha256};
use std::path::Path;

fn emit_embed_digest(dest: &Path) {
    let bytes = std::fs::read(dest).unwrap_or_else(|e| {
        panic!(
            "plasm-plugin-stub: read {} for embed digest: {e}",
            dest.display()
        );
    });
    let digest = hex_encode(Sha256::digest(&bytes));
    println!("cargo:rustc-env=PLASM_PLUGIN_EMBEDDED_CGS_HASH={digest}");
}

fn main() {
    let target = std::env::var("TARGET").expect("TARGET must be set by Cargo for build scripts");
    println!("cargo:rustc-env=PLASM_PLUGIN_TARGET_TRIPLE={target}");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set for build scripts");
    let dest = Path::new(&out_dir).join("catalog.cgs.yaml");

    if let Ok(src) = std::env::var("PLASM_EMBEDDED_CGS") {
        let src_path = Path::new(&src);
        println!("cargo:rerun-if-env-changed=PLASM_EMBEDDED_CGS");
        println!("cargo:rerun-if-changed={}", src_path.display());
        if let Ok(api_dir) = std::env::var("PLASM_EMBEDDED_CGS_API_DIR") {
            println!("cargo:rerun-if-env-changed=PLASM_EMBEDDED_CGS_API_DIR");
            let base = Path::new(&api_dir);
            for rel in ["domain.yaml", "mappings.yaml"] {
                let p = base.join(rel);
                if p.is_file() {
                    println!("cargo:rerun-if-changed={}", p.display());
                }
            }
        }
        std::fs::copy(src_path, &dest).unwrap_or_else(|e| {
            panic!(
                "plasm-plugin-stub: copy PLASM_EMBEDDED_CGS={} to {}: {e}",
                src_path.display(),
                dest.display()
            );
        });
        emit_embed_digest(&dest);
    } else {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let fixture = Path::new(&manifest_dir).join("../../fixtures/plugin_stub_catalog.cgs.yaml");
        println!("cargo:rerun-if-changed={}", fixture.display());
        std::fs::copy(&fixture, &dest).unwrap_or_else(|e| {
            panic!(
                "plasm-plugin-stub: copy default fixture {} to {}: {e}",
                fixture.display(),
                dest.display()
            );
        });
        emit_embed_digest(&dest);
    }
}
