use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, warn};

use aion_common::config::AgentsConfig;
use aion_common::types::{AgentId, AgentKind, Severity};
use aion_propose::Proposal;

use crate::governor::BudgetGovernor;
use crate::launcher::{
    claude::ClaudeCodeLauncher, codex::CodexCliLauncher, gemini::GeminiCliLauncher,
    AgentLauncher, LaunchContext,
};
use crate::permission::{
    capability_token::CapabilityToken,
    policy::{PermissionAction, PermissionPolicy},
    risk_classifier::RiskClassifier,
};
use crate::registry::{AgentDescriptor, AgentRegistry};
use crate::selector::{AgentSelector, SelectionCriteria};
use crate::validator::ResponseValidator;

/// Result of a single mount pipeline execution.
#[derive(Debug)]
pub enum MountResult {
    /// Proposal validated and ready for execution
    Approved(Proposal),
    /// Proposal needs human approval
    AwaitingApproval(Proposal),
    /// Proposal was rejected by validation
    Rejected {
        proposal: Proposal,
        reasons: Vec<String>,
    },
    /// Agent invocation failed
    AgentFailed {
        agent_id: AgentId,
        error: String,
    },
    /// No suitable agent found
    NoAgentAvailable,
    /// Budget exceeded
    BudgetExceeded(String),
}

/// The Mount Pipeline orchestrator.
///
/// Coordinates the full lifecycle:
/// Agent selection → Token issuance → MCP Server prep → Agent launch →
/// Response validation → Risk classification → Permission check
pub struct MountPipeline {
    registry: Arc<AgentRegistry>,
    policy: PermissionPolicy,
    governor: Arc<BudgetGovernor>,
    mcp_server_binary: String,
}

impl MountPipeline {
    pub fn new(
        config: &AgentsConfig,
    ) -> Self {
        let registry = Arc::new(AgentRegistry::from_config(config));

        let policy = PermissionPolicy::from_config(
            &config.permission_policy.low_risk,
            &config.permission_policy.medium_risk,
            &config.permission_policy.high_risk,
        );

        let governor = Arc::new(BudgetGovernor::new(config.global.daily_budget_usd));

        Self {
            registry,
            policy,
            governor,
            mcp_server_binary: config.global.mcp_server_binary.clone(),
        }
    }

    /// Initialize budget limits from config.
    pub fn init_budgets(&self, config: &AgentsConfig) {
        for (kind, budget) in &config.budget {
            self.governor
                .register_kind(kind, budget.max_invocations_per_hour, budget.max_concurrent);
        }
    }

    /// Execute the mount pipeline for an anomaly.
    ///
    /// This is the core orchestration method that:
    /// 1. Selects the best agent
    /// 2. Issues a capability token
    /// 3. Prepares the launch context
    /// 4. Launches the agent subprocess
    /// 5. Parses the agent's proposal from stdout
    /// 6. Validates the proposal
    /// 7. Classifies the risk
    /// 8. Applies permission policy
    pub async fn execute(
        &self,
        anomaly_id: &str,
        severity: Severity,
        criteria: &SelectionCriteria,
        prompt: &str,
    ) -> MountResult {
        // 1. Select agent
        let chain = AgentSelector::select(&self.registry, criteria);
        if chain.is_empty() {
            warn!("No suitable agent found for criteria: {:?}", criteria);
            return MountResult::NoAgentAvailable;
        }

        // Try each agent in the fallback chain
        for selection in &chain.selections {
            let agent = match self.registry.get(&selection.agent_id) {
                Some(a) => a,
                None => continue,
            };

            info!(
                agent_id = %agent.id,
                score = selection.score,
                "Selected agent for mount"
            );

            // 2. Check budget
            let agent_kind_str = format!("{}", agent.kind);
            let _guard = match self
                .governor
                .try_acquire(&agent_kind_str, agent.max_cost_usd)
            {
                Ok(g) => g,
                Err(e) => {
                    warn!(agent_id = %agent.id, error = %e, "Budget check failed, trying fallback");
                    continue;
                }
            };

            // 3. Issue capability token
            let token = CapabilityToken::issue(
                agent.id.clone(),
                anomaly_id.into(),
                severity,
                agent.timeout_secs,
            );

            info!(
                invocation_id = %token.invocation_id,
                max_risk = %token.max_risk_level,
                "Capability token issued"
            );

            // 4. Prepare launch context
            let context = LaunchContext {
                prompt: prompt.to_string(),
                mcp_server_binary: self.mcp_server_binary.clone(),
                mcp_server_args: vec![
                    "--token".to_string(),
                    token.invocation_id.to_string(),
                ],
                env: agent.env.clone(),
                timeout: Duration::from_secs(agent.timeout_secs),
                working_dir: None,
            };

            // 5. Launch agent
            let launcher = self.get_launcher(agent.kind);
            let output = match launcher
                .launch(&agent.binary_path, &agent.model, &context)
                .await
            {
                Ok(o) => o,
                Err(e) => {
                    error!(agent_id = %agent.id, error = %e, "Agent launch failed, trying fallback");
                    continue;
                }
            };

            // 6. Parse proposal from output
            let mut proposal = match Self::parse_proposal(&output.stdout, agent, &token) {
                Some(p) => p,
                None => {
                    error!(
                        agent_id = %agent.id,
                        "Failed to parse proposal from agent output"
                    );
                    return MountResult::AgentFailed {
                        agent_id: agent.id.clone(),
                        error: "Failed to parse proposal from output".to_string(),
                    };
                }
            };

            // 7. Validate
            if !ResponseValidator::validate(&mut proposal) {
                let reasons: Vec<String> = proposal
                    .validation_results
                    .iter()
                    .filter(|r| !r.passed)
                    .map(|r| r.reason.clone())
                    .collect();
                return MountResult::Rejected { proposal, reasons };
            }

            // 8. Classify risk
            let risk = RiskClassifier::classify(&proposal);
            proposal.risk_level = risk;

            // 9. Apply permission policy
            match self.policy.action_for(risk) {
                PermissionAction::ValidateAndExecute => {
                    info!(
                        proposal_id = %proposal.id,
                        risk = %risk,
                        "Proposal approved for auto-execution"
                    );
                    return MountResult::Approved(proposal);
                }
                PermissionAction::RequireApproval => {
                    info!(
                        proposal_id = %proposal.id,
                        risk = %risk,
                        "Proposal requires human approval"
                    );
                    proposal.status = aion_propose::ProposalStatus::AwaitingApproval;
                    return MountResult::AwaitingApproval(proposal);
                }
                PermissionAction::Deny => {
                    warn!(
                        proposal_id = %proposal.id,
                        risk = %risk,
                        "Proposal denied by policy"
                    );
                    return MountResult::Rejected {
                        proposal,
                        reasons: vec![format!("Policy denies risk level: {risk}")],
                    };
                }
            }
        }

        MountResult::NoAgentAvailable
    }

    fn get_launcher(&self, kind: AgentKind) -> Box<dyn AgentLauncher> {
        match kind {
            AgentKind::ClaudeCode => Box::new(ClaudeCodeLauncher),
            AgentKind::CodexCli => Box::new(CodexCliLauncher),
            AgentKind::GeminiCli => Box::new(GeminiCliLauncher),
            AgentKind::Custom => Box::new(ClaudeCodeLauncher), // fallback
        }
    }

    /// Parse a Proposal from agent stdout.
    /// Agents are expected to call propose_action via MCP, which returns structured JSON.
    /// For now, we also support direct JSON output.
    fn parse_proposal(
        stdout: &str,
        _agent: &AgentDescriptor,
        _token: &CapabilityToken,
    ) -> Option<Proposal> {
        // Try to parse as JSON proposal
        if let Ok(proposal) = serde_json::from_str::<Proposal>(stdout) {
            return Some(proposal);
        }

        // Try to find JSON within the output (agent may emit other text)
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                if let Ok(proposal) = serde_json::from_str::<Proposal>(trimmed) {
                    return Some(proposal);
                }
            }
        }

        None
    }

    pub fn registry(&self) -> &AgentRegistry {
        &self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use aion_common::config::AgentsConfig;

    fn test_config() -> AgentsConfig {
        let toml = r#"
[global]
daily_budget_usd = 50.0
default_timeout_secs = 120
mcp_server_binary = "./target/release/aion-mcp-server"

[permission_policy]
low_risk = "validate_and_execute"
medium_risk = "validate_and_execute"
high_risk = "require_approval"

[[agents]]
id = "claude-primary"
kind = "claude_code"
display_name = "Claude Code"
binary_path = "/usr/local/bin/claude"
model = "sonnet"
enabled = true
priority = 1
max_cost_usd = 0.50
specializations = ["oom_analysis", "resource_optimization"]
capabilities = ["infra_analysis", "remediation_proposal", "oom_analysis"]

[budget.claude_code]
max_invocations_per_hour = 20
max_concurrent = 3
"#;
        AgentsConfig::from_str(toml).unwrap()
    }

    #[test]
    fn test_pipeline_creation() {
        let config = test_config();
        let pipeline = MountPipeline::new(&config);
        assert_eq!(pipeline.registry().enabled_count(), 1);
    }

    #[test]
    fn test_parse_proposal_empty_string() {
        let descriptor = AgentDescriptor {
            id: AgentId::new("claude"),
            kind: AgentKind::ClaudeCode,
            display_name: "Claude".to_string(),
            binary_path: "/usr/local/bin/claude".to_string(),
            model: "sonnet".to_string(),
            enabled: true,
            priority: 1,
            max_tokens: 100000,
            timeout_secs: 120,
            max_cost_usd: 0.50,
            specializations: vec![],
            capabilities: vec![],
            env: HashMap::new(),
        };
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            "a-1".into(),
            Severity::Critical,
            120,
        );
        let result = MountPipeline::parse_proposal("", &descriptor, &token);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_proposal_invalid_json() {
        let descriptor = AgentDescriptor {
            id: AgentId::new("claude"),
            kind: AgentKind::ClaudeCode,
            display_name: "Claude".to_string(),
            binary_path: "/usr/local/bin/claude".to_string(),
            model: "sonnet".to_string(),
            enabled: true,
            priority: 1,
            max_tokens: 100000,
            timeout_secs: 120,
            max_cost_usd: 0.50,
            specializations: vec![],
            capabilities: vec![],
            env: HashMap::new(),
        };
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            "a-1".into(),
            Severity::Critical,
            120,
        );
        let result = MountPipeline::parse_proposal("{not json", &descriptor, &token);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_proposal_partial_json() {
        let descriptor = AgentDescriptor {
            id: AgentId::new("claude"),
            kind: AgentKind::ClaudeCode,
            display_name: "Claude".to_string(),
            binary_path: "/usr/local/bin/claude".to_string(),
            model: "sonnet".to_string(),
            enabled: true,
            priority: 1,
            max_tokens: 100000,
            timeout_secs: 120,
            max_cost_usd: 0.50,
            specializations: vec![],
            capabilities: vec![],
            env: HashMap::new(),
        };
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            "a-1".into(),
            Severity::Critical,
            120,
        );
        // Valid JSON but missing required Proposal fields
        let result = MountPipeline::parse_proposal(r#"{"action": "restart"}"#, &descriptor, &token);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_proposal_json_in_mixed_output() {
        let proposal = Proposal::new(
            "a-1".into(),
            "claude".into(),
            "inv-1".into(),
            aion_propose::ActionType::RestartPod,
            "default".into(),
            "my-pod",
            "Pod",
            "Pod should be restarted for recovery",
        );
        let proposal_json = serde_json::to_string(&proposal).unwrap();
        let mixed_output = format!(
            "Analyzing infrastructure...\nChecking pod status...\n{}\nDone.",
            proposal_json
        );

        let descriptor = AgentDescriptor {
            id: AgentId::new("claude"),
            kind: AgentKind::ClaudeCode,
            display_name: "Claude".to_string(),
            binary_path: "/usr/local/bin/claude".to_string(),
            model: "sonnet".to_string(),
            enabled: true,
            priority: 1,
            max_tokens: 100000,
            timeout_secs: 120,
            max_cost_usd: 0.50,
            specializations: vec![],
            capabilities: vec![],
            env: HashMap::new(),
        };
        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            "a-1".into(),
            Severity::Critical,
            120,
        );

        let result = MountPipeline::parse_proposal(&mixed_output, &descriptor, &token);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target_name, "my-pod");
    }

    #[tokio::test]
    async fn test_execute_no_agent_available() {
        let toml = r#"
[global]
daily_budget_usd = 50.0
default_timeout_secs = 120
mcp_server_binary = "./target/release/aion-mcp-server"

[permission_policy]
low_risk = "validate_and_execute"
medium_risk = "validate_and_execute"
high_risk = "require_approval"

[[agents]]
id = "claude-primary"
kind = "claude_code"
display_name = "Claude Code"
binary_path = "/usr/local/bin/claude"
model = "sonnet"
enabled = true
priority = 1
max_cost_usd = 0.50
specializations = ["oom_analysis"]
capabilities = ["oom_analysis"]

[budget.claude_code]
max_invocations_per_hour = 20
max_concurrent = 3
"#;
        let config = AgentsConfig::from_str(toml).unwrap();
        let pipeline = MountPipeline::new(&config);

        // Use criteria that won't match any agent
        let criteria = SelectionCriteria {
            required_capabilities: vec!["nonexistent_capability_xyz".to_string()],
            preferred_specializations: vec![],
            max_cost_usd: None,
        };

        let result = pipeline.execute("a-1", Severity::Warning, &criteria, "test prompt").await;
        assert!(matches!(result, MountResult::NoAgentAvailable));
    }

    #[tokio::test]
    async fn test_execute_budget_exceeded() {
        let toml = r#"
[global]
daily_budget_usd = 0.01
default_timeout_secs = 120
mcp_server_binary = "./target/release/aion-mcp-server"

[permission_policy]
low_risk = "validate_and_execute"
medium_risk = "validate_and_execute"
high_risk = "require_approval"

[[agents]]
id = "claude-primary"
kind = "claude_code"
display_name = "Claude Code"
binary_path = "/nonexistent/binary/path"
model = "sonnet"
enabled = true
priority = 1
max_cost_usd = 0.50
specializations = ["oom_analysis"]
capabilities = ["infra_analysis"]

[budget.claude_code]
max_invocations_per_hour = 20
max_concurrent = 3
"#;
        let config = AgentsConfig::from_str(toml).unwrap();
        let pipeline = MountPipeline::new(&config);
        pipeline.init_budgets(&config);

        let criteria = SelectionCriteria {
            required_capabilities: vec!["infra_analysis".to_string()],
            preferred_specializations: vec![],
            max_cost_usd: None,
        };

        // Budget is 0.01 but agent costs 0.50, so budget check fails for all fallbacks
        let result = pipeline.execute("a-1", Severity::Warning, &criteria, "test").await;
        assert!(matches!(result, MountResult::NoAgentAvailable));
    }

    #[test]
    fn test_mount_result_variants_debug() {
        let approved = MountResult::Approved(Proposal::new(
            "a-1".into(), "claude".into(), "inv-1".into(),
            aion_propose::ActionType::RestartPod, "default".into(),
            "pod-1", "Pod", "test rationale",
        ));
        assert!(format!("{:?}", approved).contains("Approved"));

        let no_agent = MountResult::NoAgentAvailable;
        assert!(format!("{:?}", no_agent).contains("NoAgentAvailable"));

        let budget = MountResult::BudgetExceeded("daily limit".to_string());
        assert!(format!("{:?}", budget).contains("BudgetExceeded"));

        let agent_failed = MountResult::AgentFailed {
            agent_id: AgentId::new("claude"),
            error: "spawn error".to_string(),
        };
        assert!(format!("{:?}", agent_failed).contains("AgentFailed"));

        let rejected = MountResult::Rejected {
            proposal: Proposal::new(
                "a-1".into(), "claude".into(), "inv-1".into(),
                aion_propose::ActionType::RestartPod, "default".into(),
                "pod-1", "Pod", "test rationale",
            ),
            reasons: vec!["policy violation".to_string()],
        };
        assert!(format!("{:?}", rejected).contains("Rejected"));

        let awaiting = MountResult::AwaitingApproval(Proposal::new(
            "a-1".into(), "claude".into(), "inv-1".into(),
            aion_propose::ActionType::DrainNode, "default".into(),
            "node-1", "Node", "test rationale",
        ));
        assert!(format!("{:?}", awaiting).contains("AwaitingApproval"));
    }

    #[test]
    fn test_pipeline_init_budgets() {
        let config = test_config();
        let pipeline = MountPipeline::new(&config);
        pipeline.init_budgets(&config);

        // After init_budgets, the governor should have claude_code registered
        // Verify by trying to acquire budget for that kind
        let result = pipeline.governor.try_acquire("claude_code", 0.10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_proposal_from_json() {
        let proposal_json = serde_json::to_string(&Proposal::new(
            "a-1".into(),
            "claude".into(),
            "inv-1".into(),
            aion_propose::ActionType::ReschedulePod,
            "default".into(),
            "my-pod",
            "Pod",
            "Pod should be rescheduled to a node with more available memory",
        ))
        .unwrap();

        let descriptor = AgentDescriptor {
            id: AgentId::new("claude"),
            kind: AgentKind::ClaudeCode,
            display_name: "Claude".to_string(),
            binary_path: "/usr/local/bin/claude".to_string(),
            model: "sonnet".to_string(),
            enabled: true,
            priority: 1,
            max_tokens: 100000,
            timeout_secs: 120,
            max_cost_usd: 0.50,
            specializations: vec![],
            capabilities: vec![],
            env: HashMap::new(),
        };

        let token = CapabilityToken::issue(
            AgentId::new("claude"),
            "a-1".into(),
            Severity::Critical,
            120,
        );

        let result = MountPipeline::parse_proposal(&proposal_json, &descriptor, &token);
        assert!(result.is_some());
        assert_eq!(result.unwrap().target_name, "my-pod");
    }
}
