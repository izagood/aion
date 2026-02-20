use aion_common::types::RiskLevel;
use serde::{Deserialize, Serialize};

/// What to do with a proposal at a given risk level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// Validate and execute automatically.
    ValidateAndExecute,
    /// Require human approval before execution.
    RequireApproval,
    /// Deny execution entirely.
    Deny,
}

/// Permission policy mapping risk levels to actions.
#[derive(Debug, Clone)]
pub struct PermissionPolicy {
    pub low: PermissionAction,
    pub medium: PermissionAction,
    pub high: PermissionAction,
}

impl PermissionPolicy {
    /// The default policy from the plan.
    pub fn default_policy() -> Self {
        Self {
            low: PermissionAction::ValidateAndExecute,
            medium: PermissionAction::ValidateAndExecute,
            high: PermissionAction::RequireApproval,
        }
    }

    /// Load from config strings.
    pub fn from_config(low: &str, medium: &str, high: &str) -> Self {
        Self {
            low: Self::parse_action(low),
            medium: Self::parse_action(medium),
            high: Self::parse_action(high),
        }
    }

    fn parse_action(s: &str) -> PermissionAction {
        match s {
            "validate_and_execute" => PermissionAction::ValidateAndExecute,
            "require_approval" => PermissionAction::RequireApproval,
            "deny" => PermissionAction::Deny,
            _ => PermissionAction::RequireApproval, // default to safe option
        }
    }

    /// Get the action for a given risk level.
    pub fn action_for(&self, risk: RiskLevel) -> PermissionAction {
        match risk {
            RiskLevel::Low => self.low,
            RiskLevel::Medium => self.medium,
            RiskLevel::High => self.high,
        }
    }

    /// Check if auto-execution is allowed for a risk level.
    pub fn is_auto_execute(&self, risk: RiskLevel) -> bool {
        self.action_for(risk) == PermissionAction::ValidateAndExecute
    }
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = PermissionPolicy::default();
        assert!(policy.is_auto_execute(RiskLevel::Low));
        assert!(policy.is_auto_execute(RiskLevel::Medium));
        assert!(!policy.is_auto_execute(RiskLevel::High));
    }

    #[test]
    fn test_from_config() {
        let policy = PermissionPolicy::from_config(
            "validate_and_execute",
            "require_approval",
            "deny",
        );
        assert_eq!(policy.action_for(RiskLevel::Low), PermissionAction::ValidateAndExecute);
        assert_eq!(policy.action_for(RiskLevel::Medium), PermissionAction::RequireApproval);
        assert_eq!(policy.action_for(RiskLevel::High), PermissionAction::Deny);
    }
}
