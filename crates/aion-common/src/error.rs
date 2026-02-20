use thiserror::Error;

use crate::types::{AgentId, AnomalyId, ProposalId};

#[derive(Debug, Error)]
pub enum AionError {
    // ── Configuration ──
    #[error("configuration error: {0}")]
    Config(String),

    #[error("invalid configuration value for '{key}': {reason}")]
    ConfigValue { key: String, reason: String },

    // ── Agent ──
    #[error("agent '{0}' not found")]
    AgentNotFound(AgentId),

    #[error("agent '{agent_id}' launch failed: {reason}")]
    AgentLaunchFailed { agent_id: AgentId, reason: String },

    #[error("agent '{agent_id}' timed out after {timeout_secs}s")]
    AgentTimeout { agent_id: AgentId, timeout_secs: u64 },

    #[error("agent '{agent_id}' budget exceeded: {reason}")]
    BudgetExceeded { agent_id: AgentId, reason: String },

    // ── Proposal ──
    #[error("proposal '{0}' not found")]
    ProposalNotFound(ProposalId),

    #[error("proposal validation failed: {0}")]
    ProposalValidation(String),

    #[error("proposal schema invalid: {0}")]
    ProposalSchema(String),

    // ── Execution ──
    #[error("execution failed for anomaly '{anomaly_id}': {reason}")]
    ExecutionFailed {
        anomaly_id: AnomalyId,
        reason: String,
    },

    #[error("dry-run failed: {0}")]
    DryRunFailed(String),

    #[error("rollback failed: {0}")]
    RollbackFailed(String),

    // ── Permission ──
    #[error("permission denied: risk level {risk_level} requires approval")]
    PermissionDenied { risk_level: String },

    #[error("capability token expired or invalid")]
    InvalidCapabilityToken,

    // ── Observability ──
    #[error("eBPF error: {0}")]
    Ebpf(String),

    #[error("kubernetes client error: {0}")]
    Kubernetes(String),

    // ── MCP ──
    #[error("MCP server error: {0}")]
    McpServer(String),

    #[error("MCP tool invocation error: {tool_name}: {reason}")]
    McpTool { tool_name: String, reason: String },

    // ── Audit ──
    #[error("audit log integrity violation: {0}")]
    AuditIntegrity(String),

    // ── Generic ──
    #[error("internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type AionResult<T> = Result<T, AionError>;
