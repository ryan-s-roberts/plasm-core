//! Linear schema loads and CML templates parse (no network).

use std::path::PathBuf;

#[test]
fn linear_mappings_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("../../apis/linear");
    let cgs = plasm_core::load_schema(&dir).expect("load apis/linear");
    cgs.validate().expect("CGS validate");
    plasm_compile::validate_cgs_capability_templates(&cgs).expect("capability templates");
}
