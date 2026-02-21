use anyhow::Result;
use crate::client::AionClient;
use crate::output;

pub async fn run(client: &AionClient, json: bool) -> Result<()> {
    let agents = client.get_agents().await?;
    if json {
        output::print_json(&agents)?;
    } else {
        output::print_agents(&agents);
    }
    Ok(())
}
