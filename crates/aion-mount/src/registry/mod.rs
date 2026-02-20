use std::collections::HashMap;

use aion_common::config::{AgentDescriptorConfig, AgentsConfig};
use aion_common::types::{AgentId, AgentKind};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Runtime descriptor for a registered AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub id: AgentId,
    pub kind: AgentKind,
    pub display_name: String,
    pub binary_path: String,
    pub model: String,
    pub enabled: bool,
    pub priority: u32,
    pub max_tokens: u64,
    pub timeout_secs: u64,
    pub max_cost_usd: f64,
    pub specializations: Vec<String>,
    pub capabilities: Vec<String>,
    pub env: HashMap<String, String>,
}

impl From<&AgentDescriptorConfig> for AgentDescriptor {
    fn from(cfg: &AgentDescriptorConfig) -> Self {
        Self {
            id: AgentId::new(&cfg.id),
            kind: cfg.kind,
            display_name: cfg.display_name.clone(),
            binary_path: cfg.binary_path.clone(),
            model: cfg.model.clone(),
            enabled: cfg.enabled,
            priority: cfg.priority,
            max_tokens: cfg.max_tokens,
            timeout_secs: cfg.timeout_secs,
            max_cost_usd: cfg.max_cost_usd,
            specializations: cfg.specializations.clone(),
            capabilities: cfg.capabilities.clone(),
            env: cfg.env.clone(),
        }
    }
}

/// Registry of available AI agents.
///
/// Loads agent configurations and provides lookup/filtering operations.
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentDescriptor>,
}

impl AgentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Load agents from an AgentsConfig.
    pub fn from_config(config: &AgentsConfig) -> Self {
        let mut registry = Self::new();
        for agent_cfg in &config.agents {
            let descriptor = AgentDescriptor::from(agent_cfg);
            info!(
                agent_id = %descriptor.id,
                kind = %descriptor.kind,
                enabled = descriptor.enabled,
                "Registered agent"
            );
            registry.agents.insert(descriptor.id.clone(), descriptor);
        }
        registry
    }

    /// Register a single agent.
    pub fn register(&mut self, descriptor: AgentDescriptor) {
        self.agents.insert(descriptor.id.clone(), descriptor);
    }

    /// Get an agent by ID.
    pub fn get(&self, id: &AgentId) -> Option<&AgentDescriptor> {
        self.agents.get(id)
    }

    /// List all registered agents.
    pub fn list(&self) -> Vec<&AgentDescriptor> {
        self.agents.values().collect()
    }

    /// List only enabled agents, sorted by priority (lower number = higher priority).
    pub fn enabled_agents(&self) -> Vec<&AgentDescriptor> {
        let mut agents: Vec<_> = self.agents.values().filter(|a| a.enabled).collect();
        agents.sort_by_key(|a| a.priority);
        agents
    }

    /// Find agents with a specific capability.
    pub fn agents_with_capability(&self, capability: &str) -> Vec<&AgentDescriptor> {
        self.enabled_agents()
            .into_iter()
            .filter(|a| a.capabilities.iter().any(|c| c == capability))
            .collect()
    }

    /// Find agents with a specific specialization.
    pub fn agents_with_specialization(&self, specialization: &str) -> Vec<&AgentDescriptor> {
        self.enabled_agents()
            .into_iter()
            .filter(|a| a.specializations.iter().any(|s| s == specialization))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.agents.len()
    }

    pub fn enabled_count(&self) -> usize {
        self.agents.values().filter(|a| a.enabled).count()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::config::AgentsConfig;

    fn test_agents_toml() -> &'static str {
        r#"
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
specializations = ["oom_analysis", "resource_optimization"]
capabilities = ["infra_analysis", "remediation_proposal", "oom_analysis"]

[[agents]]
id = "codex-primary"
kind = "codex_cli"
display_name = "Codex CLI"
binary_path = "/usr/local/bin/codex"
model = "gpt-5-codex"
enabled = true
priority = 2
specializations = ["scheduling_analysis"]
capabilities = ["infra_analysis", "remediation_proposal"]

[[agents]]
id = "gemini-disabled"
kind = "gemini_cli"
display_name = "Gemini CLI"
binary_path = "/usr/local/bin/gemini"
model = "gemini-2.5-pro"
enabled = false
priority = 3
"#
    }

    #[test]
    fn test_registry_from_config() {
        let config = AgentsConfig::from_str(test_agents_toml()).unwrap();
        let registry = AgentRegistry::from_config(&config);

        assert_eq!(registry.count(), 3);
        assert_eq!(registry.enabled_count(), 2);
    }

    #[test]
    fn test_enabled_agents_sorted_by_priority() {
        let config = AgentsConfig::from_str(test_agents_toml()).unwrap();
        let registry = AgentRegistry::from_config(&config);

        let enabled = registry.enabled_agents();
        assert_eq!(enabled.len(), 2);
        assert_eq!(enabled[0].id.as_str(), "claude-primary");
        assert_eq!(enabled[1].id.as_str(), "codex-primary");
    }

    #[test]
    fn test_agents_with_capability() {
        let config = AgentsConfig::from_str(test_agents_toml()).unwrap();
        let registry = AgentRegistry::from_config(&config);

        let oom = registry.agents_with_capability("oom_analysis");
        assert_eq!(oom.len(), 1);
        assert_eq!(oom[0].id.as_str(), "claude-primary");
    }

    #[test]
    fn test_agents_with_specialization() {
        let config = AgentsConfig::from_str(test_agents_toml()).unwrap();
        let registry = AgentRegistry::from_config(&config);

        let sched = registry.agents_with_specialization("scheduling_analysis");
        assert_eq!(sched.len(), 1);
        assert_eq!(sched[0].id.as_str(), "codex-primary");
    }

    #[test]
    fn test_get_agent() {
        let config = AgentsConfig::from_str(test_agents_toml()).unwrap();
        let registry = AgentRegistry::from_config(&config);

        let agent = registry.get(&AgentId::new("claude-primary"));
        assert!(agent.is_some());
        assert_eq!(agent.unwrap().kind, AgentKind::ClaudeCode);

        let missing = registry.get(&AgentId::new("nonexistent"));
        assert!(missing.is_none());
    }
}
