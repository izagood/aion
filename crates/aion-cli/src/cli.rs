use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aion",
    about = "CLI client for AION autonomous infrastructure OS",
    version
)]
pub struct Cli {
    /// AION server address
    #[arg(long, env = "AION_SERVER", default_value = "http://localhost:8080", global = true)]
    pub server: String,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show system status
    Status,

    /// List registered agents
    Agents,

    /// Audit log operations
    Audit {
        #[command(subcommand)]
        action: Option<AuditAction>,
    },

    /// List proposals
    Proposals,

    /// Approve a proposal
    Approve {
        /// Proposal ID to approve
        id: String,
    },

    /// Manually trigger an analysis
    Trigger {
        /// Event type (e.g. oom_kill, cpu_spike)
        #[arg(long)]
        event: String,

        /// Target resource (e.g. pod name)
        #[arg(long)]
        target: Option<String>,

        /// Kubernetes namespace
        #[arg(long)]
        namespace: Option<String>,

        /// Description of the event
        #[arg(long)]
        description: Option<String>,
    },

    /// Ask a natural language question about infrastructure
    Ask {
        /// The question to ask
        query: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum AuditAction {
    /// Verify audit log integrity
    Verify,
}
