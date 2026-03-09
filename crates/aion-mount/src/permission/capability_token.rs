use aion_common::types::{AgentId, AnomalyId, InvocationId, RiskLevel, Severity, Timestamp};
use chrono::Duration;
use serde::{Deserialize, Serialize};

/// A capability token issued per agent invocation.
///
/// Grants the agent specific permissions based on the anomaly severity
/// and risk policies. The same agent may receive different tokens
/// for different invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    pub invocation_id: InvocationId,
    pub agent_id: AgentId,
    pub anomaly_id: AnomalyId,

    /// Maximum risk level this invocation is authorized to propose.
    pub max_risk_level: RiskLevel,

    /// Allowed action types (empty = all allowed up to max_risk_level)
    pub allowed_actions: Vec<String>,

    /// Maximum blast radius this invocation can affect.
    pub max_blast_radius: u32,

    /// Namespaces this invocation can target (empty = all)
    pub allowed_namespaces: Vec<String>,

    /// Denied namespaces (takes precedence over allowed)
    pub denied_namespaces: Vec<String>,

    /// Token validity window
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,

    /// Whether direct execution (not just proposal) is allowed
    pub allow_direct_execution: bool,
}

impl CapabilityToken {
    /// Issue a token for a given anomaly severity.
    pub fn issue(
        agent_id: AgentId,
        anomaly_id: AnomalyId,
        severity: Severity,
        timeout_secs: u64,
    ) -> Self {
        let now = aion_common::types::now();
        let invocation_id = InvocationId::new(uuid::Uuid::new_v4().to_string());

        let (max_risk, max_blast, allow_exec) = match severity {
            Severity::Info => (RiskLevel::Low, 5, true),
            Severity::Warning => (RiskLevel::Medium, 10, false),
            Severity::Critical => (RiskLevel::High, 50, false),
        };

        Self {
            invocation_id,
            agent_id,
            anomaly_id,
            max_risk_level: max_risk,
            allowed_actions: Vec::new(),
            max_blast_radius: max_blast,
            allowed_namespaces: Vec::new(),
            denied_namespaces: vec!["kube-system".to_string()],
            issued_at: now,
            expires_at: now + Duration::seconds(timeout_secs as i64),
            allow_direct_execution: allow_exec,
        }
    }

    /// Check if the token is still valid.
    pub fn is_valid(&self) -> bool {
        aion_common::types::now() < self.expires_at
    }

    /// Check if a namespace is allowed by this token.
    pub fn is_namespace_allowed(&self, namespace: &str) -> bool {
        // Denied list takes precedence
        if self.denied_namespaces.iter().any(|n| n == namespace) {
            return false;
        }
        // If allowed list is empty, all non-denied namespaces are OK
        if self.allowed_namespaces.is_empty() {
            return true;
        }
        self.allowed_namespaces.iter().any(|n| n == namespace)
    }

    /// Check if a risk level is authorized.
    pub fn is_risk_authorized(&self, risk: RiskLevel) -> bool {
        risk <= self.max_risk_level
    }

    /// Check if blast radius is within bounds.
    pub fn is_blast_radius_allowed(&self, radius: u32) -> bool {
        radius <= self.max_blast_radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_issuance_info() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Info,
            120,
        );
        assert_eq!(token.max_risk_level, RiskLevel::Low);
        assert_eq!(token.max_blast_radius, 5);
        assert!(token.allow_direct_execution);
    }

    #[test]
    fn test_token_issuance_critical() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Critical,
            120,
        );
        assert_eq!(token.max_risk_level, RiskLevel::High);
        assert_eq!(token.max_blast_radius, 50);
        assert!(!token.allow_direct_execution);
    }

    #[test]
    fn test_namespace_allowed() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Warning,
            120,
        );
        assert!(token.is_namespace_allowed("default"));
        assert!(!token.is_namespace_allowed("kube-system"));
    }

    #[test]
    fn test_risk_authorized() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Warning,
            120,
        );
        assert!(token.is_risk_authorized(RiskLevel::Low));
        assert!(token.is_risk_authorized(RiskLevel::Medium));
        assert!(!token.is_risk_authorized(RiskLevel::High));
    }

    #[test]
    fn test_token_validity() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Info,
            120,
        );
        assert!(token.is_valid());
    }

    #[test]
    fn test_denied_namespace_kube_system() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Critical,
            120,
        );
        // kube-system is in the default denied list
        assert!(!token.is_namespace_allowed("kube-system"));
        // Other namespaces should be allowed
        assert!(token.is_namespace_allowed("default"));
        assert!(token.is_namespace_allowed("production"));
    }

    #[test]
    fn test_allowed_namespaces_restrict() {
        let mut token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Warning,
            120,
        );
        // Restrict to only "prod" namespace
        token.allowed_namespaces = vec!["prod".to_string()];

        assert!(token.is_namespace_allowed("prod"));
        assert!(!token.is_namespace_allowed("staging"));
        assert!(!token.is_namespace_allowed("default"));
        // kube-system is still denied (denied takes precedence)
        assert!(!token.is_namespace_allowed("kube-system"));
    }

    #[test]
    fn test_risk_high_denied_for_warning() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Warning, // Medium risk max
            120,
        );
        assert!(token.is_risk_authorized(RiskLevel::Low));
        assert!(token.is_risk_authorized(RiskLevel::Medium));
        assert!(!token.is_risk_authorized(RiskLevel::High));
    }

    #[test]
    fn test_blast_radius_exceeded() {
        let mut token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Info,
            120,
        );
        token.max_blast_radius = 5;

        assert!(token.is_blast_radius_allowed(5));  // exactly at limit
        assert!(token.is_blast_radius_allowed(3));  // below limit
        assert!(!token.is_blast_radius_allowed(10)); // exceeds limit
    }

    #[test]
    fn test_info_severity_allows_direct_execution() {
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Info,
            120,
        );
        assert!(token.allow_direct_execution);

        // Warning and Critical should not allow direct execution
        let warning_token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-2"),
            Severity::Warning,
            120,
        );
        assert!(!warning_token.allow_direct_execution);

        let critical_token = CapabilityToken::issue(
            AgentId::new("claude"),
            AnomalyId::new("a-3"),
            Severity::Critical,
            120,
        );
        assert!(!critical_token.allow_direct_execution);
    }
}
