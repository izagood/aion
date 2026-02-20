// execute_action MCP tool — direct execution for Low risk actions only.
// Requires a valid CapabilityToken that permits direct execution.

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

/// Input for the execute_action tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteActionInput {
    /// The proposal ID to execute (must already be approved).
    pub proposal_id: String,
    /// Whether to run in dry-run mode only.
    pub dry_run: Option<bool>,
}

/// Result of direct execution.
#[derive(Debug, Serialize)]
pub struct ExecuteActionResult {
    pub proposal_id: String,
    pub executed: bool,
    pub stage: String,
    pub message: String,
}
