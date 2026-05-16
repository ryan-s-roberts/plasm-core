//! SaaS core: HTTP discovery / execute + MCP Streamable HTTP.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    plasm_agent::run_mcp_main().await
}
