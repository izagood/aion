use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub agents_registered: usize,
    pub agents_enabled: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub model: String,
    pub enabled: bool,
    pub priority: u32,
    pub specializations: Vec<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditEntryResponse {
    pub entry_id: String,
    pub sequence: u64,
    pub action: String,
    pub actor_type: String,
    pub actor_id: String,
    pub description: String,
    pub anomaly_id: Option<String>,
    pub proposal_id: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntegrityResult {
    pub is_valid: bool,
    pub entries_verified: u64,
    pub first_broken_sequence: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerRequest {
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerResponse {
    pub accepted: bool,
    pub message: String,
    pub anomaly_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApproveResponse {
    pub proposal_id: String,
    pub approved: bool,
    pub message: String,
}
