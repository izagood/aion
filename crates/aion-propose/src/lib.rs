use aion_common::types::{AgentId, AnomalyId, InvocationId, Namespace, ProposalId, RiskLevel, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Types of remediation actions an AI agent can propose.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    ReschedulePod,
    RestartPod,
    AdjustResources,
    ScaleDeployment,
    CordonNode,
    DrainNode,
    UncordonNode,
}

impl ActionType {
    /// Default risk level for this action type (can be escalated by classifiers).
    pub fn default_risk(&self) -> RiskLevel {
        match self {
            ActionType::RestartPod | ActionType::AdjustResources => RiskLevel::Low,
            ActionType::ReschedulePod | ActionType::ScaleDeployment => RiskLevel::Medium,
            ActionType::CordonNode | ActionType::DrainNode | ActionType::UncordonNode => {
                RiskLevel::High
            }
        }
    }
}

/// Status of a proposal through its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Validating,
    Approved,
    Rejected,
    Executing,
    Completed,
    Failed,
    RolledBack,
    AwaitingApproval,
}

/// A remediation proposal submitted by an AI agent via MCP propose_action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub anomaly_id: AnomalyId,
    pub agent_id: AgentId,
    pub invocation_id: InvocationId,

    pub action_type: ActionType,
    pub risk_level: RiskLevel,
    pub status: ProposalStatus,

    // Target resource
    pub target_namespace: Namespace,
    pub target_name: String,
    pub target_kind: String,

    // Agent reasoning
    pub rationale: String,
    pub analysis_summary: String,

    // Action parameters
    pub parameters: HashMap<String, serde_json::Value>,

    // Blast radius estimation
    pub estimated_blast_radius: u32,
    pub is_reversible: bool,

    // Timestamps
    pub created_at: Timestamp,
    pub decided_at: Option<Timestamp>,
    pub executed_at: Option<Timestamp>,

    // Validation results
    pub validation_results: Vec<ValidationResult>,
}

/// Result of a single validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub validator_name: String,
    pub passed: bool,
    pub reason: String,
}

impl Proposal {
    pub fn new(
        anomaly_id: AnomalyId,
        agent_id: AgentId,
        invocation_id: InvocationId,
        action_type: ActionType,
        target_namespace: Namespace,
        target_name: impl Into<String>,
        target_kind: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            id: ProposalId::new(uuid::Uuid::new_v4().to_string()),
            anomaly_id,
            agent_id,
            invocation_id,
            risk_level: action_type.default_risk(),
            action_type,
            status: ProposalStatus::Pending,
            target_namespace,
            target_name: target_name.into(),
            target_kind: target_kind.into(),
            rationale: rationale.into(),
            analysis_summary: String::new(),
            parameters: HashMap::new(),
            estimated_blast_radius: 1,
            is_reversible: true,
            created_at: aion_common::types::now(),
            decided_at: None,
            executed_at: None,
            validation_results: Vec::new(),
        }
    }

    pub fn with_parameters(mut self, params: HashMap<String, serde_json::Value>) -> Self {
        self.parameters = params;
        self
    }

    pub fn with_blast_radius(mut self, radius: u32) -> Self {
        self.estimated_blast_radius = radius;
        self
    }

    pub fn with_analysis(mut self, summary: impl Into<String>) -> Self {
        self.analysis_summary = summary.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_type_default_risk() {
        assert_eq!(ActionType::RestartPod.default_risk(), RiskLevel::Low);
        assert_eq!(ActionType::ReschedulePod.default_risk(), RiskLevel::Medium);
        assert_eq!(ActionType::CordonNode.default_risk(), RiskLevel::High);
    }

    #[test]
    fn test_proposal_creation() {
        let proposal = Proposal::new(
            AnomalyId::new("anomaly-1"),
            AgentId::new("claude-primary"),
            InvocationId::new("inv-1"),
            ActionType::ReschedulePod,
            Namespace::new("default"),
            "my-pod-abc",
            "Pod",
            "Pod is OOM killed, needs rescheduling to node with more memory",
        );
        assert_eq!(proposal.risk_level, RiskLevel::Medium);
        assert_eq!(proposal.status, ProposalStatus::Pending);
        assert_eq!(proposal.target_name, "my-pod-abc");
    }

    #[test]
    fn test_proposal_serde() {
        let proposal = Proposal::new(
            AnomalyId::new("a-1"),
            AgentId::new("claude"),
            InvocationId::new("inv-1"),
            ActionType::RestartPod,
            Namespace::new("kube-system"),
            "coredns",
            "Pod",
            "restart needed",
        );
        let json = serde_json::to_string(&proposal).unwrap();
        let parsed: Proposal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.action_type, ActionType::RestartPod);
    }
}
