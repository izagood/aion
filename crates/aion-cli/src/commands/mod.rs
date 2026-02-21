mod agents;
mod approve;
mod ask;
mod audit;
mod proposals;
mod status;
mod trigger;

use anyhow::Result;

use crate::cli::{Commands, Cli};
use crate::client::AionClient;

pub async fn execute(cli: &Cli, client: &AionClient) -> Result<()> {
    match &cli.command {
        Commands::Status => status::run(client, cli.json).await,
        Commands::Agents => agents::run(client, cli.json).await,
        Commands::Audit { action } => audit::run(client, cli.json, action).await,
        Commands::Proposals => proposals::run(client, cli.json).await,
        Commands::Approve { id } => approve::run(client, cli.json, id).await,
        Commands::Trigger {
            event,
            target,
            namespace,
            description,
        } => trigger::run(client, cli.json, event, target, namespace, description).await,
        Commands::Ask { query } => ask::run(client, cli.json, query).await,
    }
}
