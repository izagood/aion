use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod pipeline;

use pipeline::AionPipeline;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    info!("AION Agent starting");

    // Load configurations
    let config_path = std::env::var("AION_CONFIG").unwrap_or_else(|_| "config/default.toml".into());
    let agents_config_path =
        std::env::var("AION_AGENTS_CONFIG").unwrap_or_else(|_| "config/agents.toml".into());

    info!(config = %config_path, agents_config = %agents_config_path, "Loading configuration");

    let daemon_config = aion_common::config::AionConfig::from_file(&config_path)?;
    let agents_config = aion_common::config::AgentsConfig::from_file(&agents_config_path)?;

    // Initialize pipeline
    let pipeline = Arc::new(AionPipeline::new(&daemon_config, &agents_config));

    info!(
        agents = agents_config.enabled_agents().len(),
        "Pipeline initialized"
    );

    // Initialize audit logger
    let audit_dir = std::env::var("AION_AUDIT_DIR").unwrap_or_else(|_| "/var/lib/aion/audit".into());
    let audit_logger = Arc::new(aion_audit::AuditLogger::new(&audit_dir));

    audit_logger.log(
        aion_audit::AuditAction::EventDetected,
        "system",
        "aion-agent",
        "AION Agent daemon started",
    );

    // Run the pipeline (event loop)
    let pipeline_handle = {
        let pipeline = Arc::clone(&pipeline);
        let audit_logger = Arc::clone(&audit_logger);
        tokio::spawn(async move {
            pipeline.run(audit_logger).await;
        })
    };

    // Set up graceful shutdown
    let shutdown = tokio::signal::ctrl_c();

    tokio::select! {
        _ = shutdown => {
            info!("Received shutdown signal");
        }
        _ = pipeline_handle => {
            warn!("Pipeline task exited unexpectedly");
        }
    }

    // Flush audit log
    if let Err(e) = audit_logger.flush_to_disk().await {
        error!(error = %e, "Failed to flush audit log");
    }

    info!("AION Agent shutdown complete");
    Ok(())
}
