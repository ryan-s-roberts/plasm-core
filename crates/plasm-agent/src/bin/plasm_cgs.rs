//! Schema-driven CLI (generated subcommands from CGS).

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    plasm_agent::run_cgs_main().await
}
