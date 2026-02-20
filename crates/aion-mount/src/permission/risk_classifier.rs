use aion_common::types::RiskLevel;
use aion_propose::Proposal;

/// Classifies the risk level of a proposal based on multiple factors.
///
/// Escalation rules:
/// - blast_radius > 10 → at least Medium
/// - blast_radius > 50 → High
/// - irreversible → at least Medium
/// - kube-system or monitoring namespace → High
pub struct RiskClassifier;

impl RiskClassifier {
    /// Classify the risk level of a proposal, potentially escalating from the default.
    pub fn classify(proposal: &Proposal) -> RiskLevel {
        let base = proposal.action_type.default_risk();
        let mut level = base;

        // Blast radius escalation
        if proposal.estimated_blast_radius > 50 {
            level = level.max(RiskLevel::High);
        } else if proposal.estimated_blast_radius > 10 {
            level = level.max(RiskLevel::Medium);
        }

        // Irreversible action escalation
        if !proposal.is_reversible {
            level = level.max(RiskLevel::Medium);
        }

        // Protected namespace escalation
        let ns = proposal.target_namespace.as_str();
        if Self::is_protected_namespace(ns) {
            level = RiskLevel::High;
        }

        level
    }

    fn is_protected_namespace(namespace: &str) -> bool {
        matches!(
            namespace,
            "kube-system" | "kube-public" | "kube-node-lease" | "monitoring" | "istio-system"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::types::*;
    use aion_propose::{ActionType, Proposal};

    fn make_proposal(
        action: ActionType,
        namespace: &str,
        blast_radius: u32,
        reversible: bool,
    ) -> Proposal {
        let mut p = Proposal::new(
            AnomalyId::new("a-1"),
            AgentId::new("claude"),
            InvocationId::new("inv-1"),
            action,
            Namespace::new(namespace),
            "test-pod",
            "Pod",
            "test rationale",
        );
        p.estimated_blast_radius = blast_radius;
        p.is_reversible = reversible;
        p
    }

    #[test]
    fn test_low_risk_action() {
        let p = make_proposal(ActionType::RestartPod, "default", 1, true);
        assert_eq!(RiskClassifier::classify(&p), RiskLevel::Low);
    }

    #[test]
    fn test_high_blast_radius_escalation() {
        let p = make_proposal(ActionType::RestartPod, "default", 60, true);
        assert_eq!(RiskClassifier::classify(&p), RiskLevel::High);
    }

    #[test]
    fn test_medium_blast_radius_escalation() {
        let p = make_proposal(ActionType::RestartPod, "default", 15, true);
        assert_eq!(RiskClassifier::classify(&p), RiskLevel::Medium);
    }

    #[test]
    fn test_protected_namespace() {
        let p = make_proposal(ActionType::RestartPod, "kube-system", 1, true);
        assert_eq!(RiskClassifier::classify(&p), RiskLevel::High);
    }

    #[test]
    fn test_irreversible_escalation() {
        let p = make_proposal(ActionType::RestartPod, "default", 1, false);
        assert_eq!(RiskClassifier::classify(&p), RiskLevel::Medium);
    }

    #[test]
    fn test_node_drain_always_high() {
        let p = make_proposal(ActionType::DrainNode, "default", 1, true);
        assert_eq!(RiskClassifier::classify(&p), RiskLevel::High);
    }
}
