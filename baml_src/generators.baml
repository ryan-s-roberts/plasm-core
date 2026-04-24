generator target {
    output_type "rust"
    output_dir "../crates/plasm-extract"
    version "0.220.0"
    default_client_mode sync
}

generator eval {
    output_type "rust"
    output_dir "../crates/plasm-eval"
    version "0.220.0"
    default_client_mode sync
}