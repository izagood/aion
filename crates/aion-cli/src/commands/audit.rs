use anyhow::Result;
use crate::cli::AuditAction;
use crate::client::AionClient;
use crate::output;

pub async fn run(client: &AionClient, json: bool, action: &Option<AuditAction>) -> Result<()> {
    match action {
        Some(AuditAction::Verify) => {
            let result = client.verify_audit().await?;
            if json {
                output::print_json(&result)?;
            } else {
                output::print_integrity(&result);
            }
        }
        None => {
            let entries = client.get_audit_log().await?;
            if json {
                output::print_json(&entries)?;
            } else {
                output::print_audit_log(&entries);
            }
        }
    }
    Ok(())
}
