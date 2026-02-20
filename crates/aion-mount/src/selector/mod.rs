use aion_common::types::AgentId;

use crate::registry::{AgentDescriptor, AgentRegistry};

/// Result of agent selection with scoring details.
#[derive(Debug, Clone)]
pub struct ScoredSelection {
    pub agent_id: AgentId,
    pub score: f64,
    pub breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone)]
pub struct ScoreBreakdown {
    pub specialization_score: f64,
    pub capability_score: f64,
    pub success_rate_score: f64,
    pub cost_penalty: f64,
}

/// An ordered list of agents to try for a given anomaly.
#[derive(Debug, Clone)]
pub struct FallbackChain {
    pub selections: Vec<ScoredSelection>,
}

impl FallbackChain {
    pub fn primary(&self) -> Option<&ScoredSelection> {
        self.selections.first()
    }

    pub fn fallbacks(&self) -> &[ScoredSelection] {
        if self.selections.len() > 1 {
            &self.selections[1..]
        } else {
            &[]
        }
    }

    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }
}

/// Criteria for selecting an agent.
#[derive(Debug, Clone)]
pub struct SelectionCriteria {
    /// Required capabilities (e.g., "oom_analysis")
    pub required_capabilities: Vec<String>,
    /// Preferred specializations (score boost)
    pub preferred_specializations: Vec<String>,
    /// Maximum cost per invocation
    pub max_cost_usd: Option<f64>,
}

/// Selects the best agent for a given anomaly type using score-based ranking.
///
/// Scoring formula:
///   specialization match: 50 points
///   capability match:     30 points
///   success rate:         20 points (TODO: track actual success rates)
///   cost penalty:         -cost_usd * 10
pub struct AgentSelector;

impl AgentSelector {
    /// Select the best agents for the given criteria, returning a fallback chain.
    pub fn select(
        registry: &AgentRegistry,
        criteria: &SelectionCriteria,
    ) -> FallbackChain {
        let agents = registry.enabled_agents();

        let mut scored: Vec<ScoredSelection> = agents
            .iter()
            .filter_map(|agent| {
                let scored = Self::score_agent(agent, criteria);
                // Filter out agents below minimum threshold
                if scored.score > 0.0 {
                    Some(scored)
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Apply max cost filter
        if let Some(max_cost) = criteria.max_cost_usd {
            scored.retain(|s| {
                registry
                    .get(&s.agent_id)
                    .map(|a| a.max_cost_usd <= max_cost)
                    .unwrap_or(false)
            });
        }

        FallbackChain {
            selections: scored,
        }
    }

    fn score_agent(agent: &AgentDescriptor, criteria: &SelectionCriteria) -> ScoredSelection {
        // Specialization match: 50 points max
        let spec_matches = criteria
            .preferred_specializations
            .iter()
            .filter(|s| agent.specializations.contains(s))
            .count();
        let spec_total = criteria.preferred_specializations.len().max(1);
        let specialization_score = (spec_matches as f64 / spec_total as f64) * 50.0;

        // Capability match: 30 points max
        let cap_matches = criteria
            .required_capabilities
            .iter()
            .filter(|c| agent.capabilities.contains(c))
            .count();
        let cap_total = criteria.required_capabilities.len().max(1);
        let capability_score = (cap_matches as f64 / cap_total as f64) * 30.0;

        // Success rate: 20 points (TODO: track actual rates, for now use priority as proxy)
        // Lower priority number = higher success rate assumption
        let success_rate_score = 20.0 * (1.0 / agent.priority as f64).min(1.0);

        // Cost penalty
        let cost_penalty = agent.max_cost_usd * 10.0;

        let total = specialization_score + capability_score + success_rate_score - cost_penalty;

        ScoredSelection {
            agent_id: agent.id.clone(),
            score: total,
            breakdown: ScoreBreakdown {
                specialization_score,
                capability_score,
                success_rate_score,
                cost_penalty,
            },
        }
    }

    /// Quick select for a specific anomaly type (convenience method).
    pub fn select_for_oom(registry: &AgentRegistry) -> FallbackChain {
        Self::select(
            registry,
            &SelectionCriteria {
                required_capabilities: vec!["oom_analysis".to_string()],
                preferred_specializations: vec![
                    "oom_analysis".to_string(),
                    "resource_optimization".to_string(),
                ],
                max_cost_usd: None,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::config::AgentsConfig;

    fn test_registry() -> AgentRegistry {
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

[[agents]]
id = "codex-primary"
kind = "codex_cli"
display_name = "Codex CLI"
binary_path = "/usr/local/bin/codex"
model = "gpt-5-codex"
enabled = true
priority = 2
max_cost_usd = 0.40
specializations = ["scheduling_analysis", "resource_optimization"]
capabilities = ["infra_analysis", "remediation_proposal"]

[[agents]]
id = "gemini-primary"
kind = "gemini_cli"
display_name = "Gemini CLI"
binary_path = "/usr/local/bin/gemini"
model = "gemini-2.5-pro"
enabled = true
priority = 3
max_cost_usd = 0.30
specializations = ["capacity_planning"]
capabilities = ["infra_analysis", "remediation_proposal"]
"#;
        let config = AgentsConfig::from_str(toml).unwrap();
        AgentRegistry::from_config(&config)
    }

    #[test]
    fn test_select_for_oom() {
        let registry = test_registry();
        let chain = AgentSelector::select_for_oom(&registry);

        assert!(!chain.is_empty());
        let primary = chain.primary().unwrap();
        assert_eq!(primary.agent_id.as_str(), "claude-primary");
        assert!(primary.score > 0.0);
    }

    #[test]
    fn test_fallback_chain() {
        let registry = test_registry();
        let chain = AgentSelector::select_for_oom(&registry);

        assert!(chain.selections.len() >= 2);
        // Primary should be Claude (best OOM specialist)
        assert_eq!(chain.primary().unwrap().agent_id.as_str(), "claude-primary");
        // Fallbacks should exist
        assert!(!chain.fallbacks().is_empty());
    }

    #[test]
    fn test_scoring_breakdown() {
        let registry = test_registry();
        let chain = AgentSelector::select_for_oom(&registry);

        let primary = chain.primary().unwrap();
        let breakdown = &primary.breakdown;
        // Claude matches both OOM specializations
        assert!(breakdown.specialization_score > 0.0);
        // Claude has oom_analysis capability
        assert!(breakdown.capability_score > 0.0);
    }

    #[test]
    fn test_select_with_cost_filter() {
        let registry = test_registry();
        let chain = AgentSelector::select(
            &registry,
            &SelectionCriteria {
                required_capabilities: vec!["infra_analysis".to_string()],
                preferred_specializations: vec![],
                max_cost_usd: Some(0.35),
            },
        );

        // Only Gemini (0.30) should pass the cost filter
        for selection in &chain.selections {
            let agent = registry.get(&selection.agent_id).unwrap();
            assert!(agent.max_cost_usd <= 0.35);
        }
    }
}
