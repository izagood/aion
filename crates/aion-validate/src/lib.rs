use aion_propose::Proposal;
use serde::{Deserialize, Serialize};

/// Result of a single policy validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyResult {
    pub validator_name: String,
    pub passed: bool,
    pub reason: String,
}

/// Trait for policy validators.
pub trait PolicyValidator: Send + Sync {
    fn name(&self) -> &str;
    fn validate(&self, proposal: &Proposal) -> PolicyResult;
}

/// Chain of policy validators. All must pass for a proposal to be accepted.
pub struct PolicyChain {
    validators: Vec<Box<dyn PolicyValidator>>,
}

impl PolicyChain {
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }

    /// Create a chain with all default validators.
    pub fn default_chain() -> Self {
        let mut chain = Self::new();
        chain.add(Box::new(BlastRadiusValidator { max_radius: 100 }));
        chain.add(Box::new(ResourceBoundsValidator));
        chain.add(Box::new(NamespaceScopeValidator::default()));
        chain.add(Box::new(DenyListValidator::default()));
        chain
    }

    pub fn add(&mut self, validator: Box<dyn PolicyValidator>) {
        self.validators.push(validator);
    }

    /// Run all validators on a proposal.
    pub fn validate(&self, proposal: &Proposal) -> Vec<PolicyResult> {
        self.validators
            .iter()
            .map(|v| v.validate(proposal))
            .collect()
    }

    /// Run all validators and return true only if all pass.
    pub fn validate_all(&self, proposal: &Proposal) -> (bool, Vec<PolicyResult>) {
        let results = self.validate(proposal);
        let all_passed = results.iter().all(|r| r.passed);
        (all_passed, results)
    }
}

impl Default for PolicyChain {
    fn default() -> Self {
        Self::default_chain()
    }
}

// ── Blast Radius Validator ──

pub struct BlastRadiusValidator {
    pub max_radius: u32,
}

impl PolicyValidator for BlastRadiusValidator {
    fn name(&self) -> &str {
        "blast_radius"
    }

    fn validate(&self, proposal: &Proposal) -> PolicyResult {
        if proposal.estimated_blast_radius <= self.max_radius {
            PolicyResult {
                validator_name: self.name().to_string(),
                passed: true,
                reason: format!(
                    "Blast radius {} within limit {}",
                    proposal.estimated_blast_radius, self.max_radius
                ),
            }
        } else {
            PolicyResult {
                validator_name: self.name().to_string(),
                passed: false,
                reason: format!(
                    "Blast radius {} exceeds maximum {}",
                    proposal.estimated_blast_radius, self.max_radius
                ),
            }
        }
    }
}

// ── Resource Bounds Validator ──

pub struct ResourceBoundsValidator;

impl PolicyValidator for ResourceBoundsValidator {
    fn name(&self) -> &str {
        "resource_bounds"
    }

    fn validate(&self, proposal: &Proposal) -> PolicyResult {
        // Check resource parameters if present
        if let Some(replicas) = proposal.parameters.get("replicas") {
            if let Some(n) = replicas.as_u64() {
                if n > 100 {
                    return PolicyResult {
                        validator_name: self.name().to_string(),
                        passed: false,
                        reason: format!("Replica count {n} exceeds maximum of 100"),
                    };
                }
            }
        }

        if let Some(memory) = proposal.parameters.get("memory_limit_bytes") {
            if let Some(bytes) = memory.as_u64() {
                // 64 GiB max per container
                if bytes > 64 * 1024 * 1024 * 1024 {
                    return PolicyResult {
                        validator_name: self.name().to_string(),
                        passed: false,
                        reason: format!("Memory limit {bytes} exceeds 64 GiB maximum"),
                    };
                }
            }
        }

        PolicyResult {
            validator_name: self.name().to_string(),
            passed: true,
            reason: "Resource bounds within limits".to_string(),
        }
    }
}

// ── Namespace Scope Validator ──

pub struct NamespaceScopeValidator {
    pub protected_namespaces: Vec<String>,
}

impl Default for NamespaceScopeValidator {
    fn default() -> Self {
        Self {
            protected_namespaces: vec![
                "kube-system".to_string(),
                "kube-public".to_string(),
                "kube-node-lease".to_string(),
                "monitoring".to_string(),
                "istio-system".to_string(),
            ],
        }
    }
}

impl PolicyValidator for NamespaceScopeValidator {
    fn name(&self) -> &str {
        "namespace_scope"
    }

    fn validate(&self, proposal: &Proposal) -> PolicyResult {
        let ns = proposal.target_namespace.as_str();
        if self.protected_namespaces.iter().any(|p| p == ns) {
            PolicyResult {
                validator_name: self.name().to_string(),
                passed: false,
                reason: format!("Namespace '{ns}' is protected and requires manual intervention"),
            }
        } else {
            PolicyResult {
                validator_name: self.name().to_string(),
                passed: true,
                reason: format!("Namespace '{ns}' is allowed"),
            }
        }
    }
}

// ── Deny List Validator ──

pub struct DenyListValidator {
    pub denied_actions: Vec<String>,
    pub denied_targets: Vec<String>,
}

impl Default for DenyListValidator {
    fn default() -> Self {
        Self {
            denied_actions: Vec::new(),
            denied_targets: Vec::new(),
        }
    }
}

impl PolicyValidator for DenyListValidator {
    fn name(&self) -> &str {
        "deny_list"
    }

    fn validate(&self, proposal: &Proposal) -> PolicyResult {
        let action_str = format!("{:?}", proposal.action_type);
        if self.denied_actions.iter().any(|a| a == &action_str) {
            return PolicyResult {
                validator_name: self.name().to_string(),
                passed: false,
                reason: format!("Action '{action_str}' is in deny list"),
            };
        }

        if self.denied_targets.iter().any(|t| t == &proposal.target_name) {
            return PolicyResult {
                validator_name: self.name().to_string(),
                passed: false,
                reason: format!("Target '{}' is in deny list", proposal.target_name),
            };
        }

        PolicyResult {
            validator_name: self.name().to_string(),
            passed: true,
            reason: "Not in deny list".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::types::*;
    use aion_propose::ActionType;

    fn test_proposal(ns: &str, blast_radius: u32) -> Proposal {
        Proposal::new(
            AnomalyId::new("a-1"),
            AgentId::new("claude"),
            InvocationId::new("inv-1"),
            ActionType::ReschedulePod,
            Namespace::new(ns),
            "test-pod",
            "Pod",
            "Pod needs rescheduling to a different node",
        )
        .with_blast_radius(blast_radius)
    }

    #[test]
    fn test_blast_radius_passes() {
        let v = BlastRadiusValidator { max_radius: 50 };
        let p = test_proposal("default", 10);
        let result = v.validate(&p);
        assert!(result.passed);
    }

    #[test]
    fn test_blast_radius_fails() {
        let v = BlastRadiusValidator { max_radius: 50 };
        let p = test_proposal("default", 100);
        let result = v.validate(&p);
        assert!(!result.passed);
    }

    #[test]
    fn test_namespace_protected() {
        let v = NamespaceScopeValidator::default();
        let p = test_proposal("kube-system", 1);
        let result = v.validate(&p);
        assert!(!result.passed);
    }

    #[test]
    fn test_namespace_allowed() {
        let v = NamespaceScopeValidator::default();
        let p = test_proposal("default", 1);
        let result = v.validate(&p);
        assert!(result.passed);
    }

    #[test]
    fn test_resource_bounds_excessive_replicas() {
        let v = ResourceBoundsValidator;
        let mut p = test_proposal("default", 1);
        p.parameters
            .insert("replicas".to_string(), serde_json::json!(200));
        let result = v.validate(&p);
        assert!(!result.passed);
    }

    #[test]
    fn test_policy_chain_all_pass() {
        let chain = PolicyChain::default_chain();
        let p = test_proposal("default", 5);
        let (passed, results) = chain.validate_all(&p);
        assert!(passed);
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn test_policy_chain_fails_on_namespace() {
        let chain = PolicyChain::default_chain();
        let p = test_proposal("kube-system", 5);
        let (passed, results) = chain.validate_all(&p);
        assert!(!passed);
        assert!(results.iter().any(|r| !r.passed && r.validator_name == "namespace_scope"));
    }

    #[test]
    fn test_deny_list() {
        let v = DenyListValidator {
            denied_actions: vec![],
            denied_targets: vec!["critical-db-pod".to_string()],
        };
        let mut p = test_proposal("default", 1);
        p.target_name = "critical-db-pod".to_string();
        let result = v.validate(&p);
        assert!(!result.passed);
    }
}
