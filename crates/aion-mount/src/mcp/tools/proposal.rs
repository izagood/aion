// propose_action MCP tool — allows AI agents to submit structured remediation proposals.

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

/// Input for the propose_action tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeActionInput {
    /// Type of action: reschedule_pod, restart_pod, adjust_resources, scale_deployment,
    /// cordon_node, drain_node, uncordon_node.
    pub action_type: String,
    /// Target namespace.
    pub target_namespace: String,
    /// Target resource name.
    pub target_name: String,
    /// Target resource kind (Pod, Deployment, Node, etc.).
    pub target_kind: String,
    /// Detailed rationale for the proposed action.
    pub rationale: String,
    /// Summary of the agent's analysis.
    pub analysis_summary: Option<String>,
    /// Estimated blast radius (number of affected resources).
    pub estimated_blast_radius: Option<u32>,
    /// Whether this action is reversible.
    pub is_reversible: Option<bool>,
    /// Additional parameters for the action (JSON object).
    pub parameters: Option<serde_json::Value>,
}

/// Result returned to the agent after proposal submission.
#[derive(Debug, Serialize)]
pub struct ProposeActionResult {
    pub proposal_id: String,
    pub status: String,
    pub risk_level: String,
    pub message: String,
}
