use anyhow::Result;
use crate::client::AionClient;
use crate::output;
use crate::types::TriggerRequest;

pub async fn run(client: &AionClient, json: bool, query: &[String]) -> Result<()> {
    let query_str = query.join(" ");
    anyhow::ensure!(!query_str.is_empty(), "Query cannot be empty");

    let request = TriggerRequest {
        event_type: "natural_query".to_string(),
        namespace: None,
        target: None,
        description: Some(query_str),
    };
    let result = client.trigger(&request).await?;
    if json {
        output::print_json(&result)?;
    } else {
        output::print_trigger(&result);
    }
    Ok(())
}
