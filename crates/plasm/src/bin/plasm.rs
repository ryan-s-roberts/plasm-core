//! Remote HTTP terminal (discovery + execute sessions) for a Plasm server.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    plasm_agent::run_cgs_main().await
}
