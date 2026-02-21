use anyhow::Result;
use crate::client::AionClient;
use crate::output;

pub async fn run(client: &AionClient, json: bool) -> Result<()> {
    let proposals = client.get_proposals().await?;
    if json {
        output::print_json(&proposals)?;
    } else {
        output::print_proposals(&proposals);
    }
    Ok(())
}
