use anyhow::Result;
use crate::client::AionClient;
use crate::output;

pub async fn run(client: &AionClient, json: bool, id: &str) -> Result<()> {
    let result = client.approve_proposal(id).await?;
    if json {
        output::print_json(&result)?;
    } else {
        output::print_approve(&result);
    }
    Ok(())
}
