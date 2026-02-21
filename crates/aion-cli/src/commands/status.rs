use anyhow::Result;
use crate::client::AionClient;
use crate::output;

pub async fn run(client: &AionClient, json: bool) -> Result<()> {
    let status = client.get_status().await?;
    if json {
        output::print_json(&status)?;
    } else {
        output::print_status(&status);
    }
    Ok(())
}
