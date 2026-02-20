use aion_propose::{ActionType, Proposal, ValidationResult};
use tracing::{info, warn};

/// Validates agent responses for schema correctness and safety.
pub struct ResponseValidator;

impl ResponseValidator {
    /// Run all validation checks on a proposal.
    pub fn validate(proposal: &mut Proposal) -> bool {
        let mut results = Vec::new();

        results.push(Self::validate_required_fields(proposal));
        results.push(Self::validate_target(proposal));
        results.push(Self::validate_rationale(proposal));
        results.push(Self::validate_action_type(proposal));
        results.push(Self::validate_blast_radius(proposal));

        let all_passed = results.iter().all(|r| r.passed);

        if !all_passed {
            let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
            warn!(
                failures = ?failures,
                "Proposal validation failed"
            );
        } else {
            info!(proposal_id = %proposal.id, "Proposal validation passed");
        }

        proposal.validation_results = results;
        all_passed
    }

    fn validate_required_fields(proposal: &Proposal) -> ValidationResult {
        let mut missing = Vec::new();

        if proposal.target_name.is_empty() {
            missing.push("target_name");
        }
        if proposal.target_kind.is_empty() {
            missing.push("target_kind");
        }
        if proposal.rationale.is_empty() {
            missing.push("rationale");
        }

        if missing.is_empty() {
            ValidationResult {
                validator_name: "required_fields".to_string(),
                passed: true,
                reason: "All required fields present".to_string(),
            }
        } else {
            ValidationResult {
                validator_name: "required_fields".to_string(),
                passed: false,
                reason: format!("Missing fields: {}", missing.join(", ")),
            }
        }
    }

    fn validate_target(proposal: &Proposal) -> ValidationResult {
        let valid_kinds = ["Pod", "Deployment", "StatefulSet", "DaemonSet", "Node", "ReplicaSet"];

        if valid_kinds.contains(&proposal.target_kind.as_str()) {
            ValidationResult {
                validator_name: "target_kind".to_string(),
                passed: true,
                reason: format!("Valid target kind: {}", proposal.target_kind),
            }
        } else {
            ValidationResult {
                validator_name: "target_kind".to_string(),
                passed: false,
                reason: format!(
                    "Invalid target kind '{}'. Must be one of: {}",
                    proposal.target_kind,
                    valid_kinds.join(", ")
                ),
            }
        }
    }

    fn validate_rationale(proposal: &Proposal) -> ValidationResult {
        if proposal.rationale.len() >= 10 {
            ValidationResult {
                validator_name: "rationale".to_string(),
                passed: true,
                reason: "Rationale is sufficiently detailed".to_string(),
            }
        } else {
            ValidationResult {
                validator_name: "rationale".to_string(),
                passed: false,
                reason: "Rationale must be at least 10 characters".to_string(),
            }
        }
    }

    fn validate_action_type(proposal: &Proposal) -> ValidationResult {
        // Verify action_type and target_kind are compatible
        let compatible = match (&proposal.action_type, proposal.target_kind.as_str()) {
            (ActionType::RestartPod | ActionType::ReschedulePod, "Pod") => true,
            (ActionType::AdjustResources, "Pod" | "Deployment" | "StatefulSet") => true,
            (ActionType::ScaleDeployment, "Deployment" | "StatefulSet" | "ReplicaSet") => true,
            (ActionType::CordonNode | ActionType::DrainNode | ActionType::UncordonNode, "Node") => true,
            _ => false,
        };

        if compatible {
            ValidationResult {
                validator_name: "action_target_compatibility".to_string(),
                passed: true,
                reason: format!(
                    "Action {:?} is compatible with {}",
                    proposal.action_type, proposal.target_kind
                ),
            }
        } else {
            ValidationResult {
                validator_name: "action_target_compatibility".to_string(),
                passed: false,
                reason: format!(
                    "Action {:?} is not compatible with target kind '{}'",
                    proposal.action_type, proposal.target_kind
                ),
            }
        }
    }

    fn validate_blast_radius(proposal: &Proposal) -> ValidationResult {
        if proposal.estimated_blast_radius <= 100 {
            ValidationResult {
                validator_name: "blast_radius".to_string(),
                passed: true,
                reason: format!("Blast radius {} is within bounds", proposal.estimated_blast_radius),
            }
        } else {
            ValidationResult {
                validator_name: "blast_radius".to_string(),
                passed: false,
                reason: format!(
                    "Blast radius {} exceeds maximum of 100",
                    proposal.estimated_blast_radius
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::types::*;

    fn valid_proposal() -> Proposal {
        Proposal::new(
            AnomalyId::new("a-1"),
            AgentId::new("claude"),
            InvocationId::new("inv-1"),
            ActionType::ReschedulePod,
            Namespace::new("default"),
            "my-pod-abc",
            "Pod",
            "Pod needs rescheduling due to OOM kill on current node",
        )
    }

    #[test]
    fn test_valid_proposal_passes() {
        let mut proposal = valid_proposal();
        assert!(ResponseValidator::validate(&mut proposal));
        assert!(proposal.validation_results.iter().all(|r| r.passed));
    }

    #[test]
    fn test_missing_target_name() {
        let mut proposal = valid_proposal();
        proposal.target_name = String::new();
        assert!(!ResponseValidator::validate(&mut proposal));
    }

    #[test]
    fn test_invalid_target_kind() {
        let mut proposal = valid_proposal();
        proposal.target_kind = "ConfigMap".to_string();
        assert!(!ResponseValidator::validate(&mut proposal));
    }

    #[test]
    fn test_short_rationale() {
        let mut proposal = valid_proposal();
        proposal.rationale = "short".to_string();
        assert!(!ResponseValidator::validate(&mut proposal));
    }

    #[test]
    fn test_incompatible_action_target() {
        let mut proposal = Proposal::new(
            AnomalyId::new("a-1"),
            AgentId::new("claude"),
            InvocationId::new("inv-1"),
            ActionType::CordonNode,
            Namespace::new("default"),
            "my-pod",
            "Pod", // CordonNode should target Node, not Pod
            "Node needs cordoning to prevent scheduling",
        );
        assert!(!ResponseValidator::validate(&mut proposal));
    }

    #[test]
    fn test_excessive_blast_radius() {
        let mut proposal = valid_proposal();
        proposal.estimated_blast_radius = 200;
        assert!(!ResponseValidator::validate(&mut proposal));
    }
}
