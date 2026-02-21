use anyhow::Result;
use crate::client::AionClient;
use crate::output;
use crate::types::TriggerRequest;

pub async fn run(
    client: &AionClient,
    json: bool,
    event: &str,
    target: &Option<String>,
    namespace: &Option<String>,
    description: &Option<String>,
) -> Result<()> {
    let request = TriggerRequest {
        event_type: event.to_string(),
        namespace: namespace.clone(),
        target: target.clone(),
        description: description.clone(),
    };
    let result = client.trigger(&request).await?;
    if json {
        output::print_json(&result)?;
    } else {
        output::print_trigger(&result);
    }
    Ok(())
}
