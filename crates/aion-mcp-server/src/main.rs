use anyhow::Result;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

use aion_mount::mcp::server::AionMcpServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing (logs to stderr so stdout stays clean for MCP)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Read invocation token from CLI args
    let args: Vec<String> = std::env::args().collect();
    let token = args
        .windows(2)
        .find(|w| w[0] == "--token")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "no-token".to_string());

    tracing::info!(token = %token, "Starting AION MCP Server");

    // Create the MCP server handler
    let server = AionMcpServer::new(token);

    // Serve via stdio transport (AI agent communicates over stdin/stdout)
    let service = server.serve(rmcp::transport::stdio()).await?;

    // Wait for the service to complete (agent disconnects)
    service.waiting().await?;

    tracing::info!("AION MCP Server shutting down");
    Ok(())
}
