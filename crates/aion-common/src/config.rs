use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{AionError, AionResult};
use crate::types::AgentKind;

// ──────────────────────────────────────────────
// Top-level AION configuration
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AionConfig {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub observe: ObserveConfig,
    #[serde(default)]
    pub pipeline: PipelineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
    #[serde(default = "default_rest_port")]
    pub rest_port: u16,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_audit_dir")]
    pub audit_dir: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            grpc_port: default_grpc_port(),
            rest_port: default_rest_port(),
            log_level: default_log_level(),
            audit_dir: default_audit_dir(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0".to_string()
}
fn default_grpc_port() -> u16 {
    50051
}
fn default_rest_port() -> u16 {
    8080
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_audit_dir() -> String {
    "/var/lib/aion/audit".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserveConfig {
    #[serde(default = "default_true")]
    pub enable_ebpf: bool,
    #[serde(default = "default_true")]
    pub enable_cgroup: bool,
    #[serde(default = "default_true")]
    pub enable_kube_watcher: bool,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub mock_mode: Option<bool>,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            enable_ebpf: true,
            enable_cgroup: true,
            enable_kube_watcher: true,
            poll_interval_secs: default_poll_interval_secs(),
            mock_mode: None,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_poll_interval_secs() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default = "default_max_concurrent_mounts")]
    pub max_concurrent_mounts: usize,
    #[serde(default = "default_canary_duration_secs")]
    pub canary_duration_secs: u64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_concurrent_mounts: default_max_concurrent_mounts(),
            canary_duration_secs: default_canary_duration_secs(),
        }
    }
}

fn default_max_concurrent_mounts() -> usize {
    3
}
fn default_canary_duration_secs() -> u64 {
    30
}

// ──────────────────────────────────────────────
// Agent configuration (agents.toml)
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    pub global: GlobalAgentConfig,
    pub permission_policy: PermissionPolicyConfig,
    #[serde(default)]
    pub agents: Vec<AgentDescriptorConfig>,
    #[serde(default)]
    pub budget: std::collections::HashMap<String, BudgetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalAgentConfig {
    #[serde(default = "default_daily_budget")]
    pub daily_budget_usd: f64,
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,
    pub mcp_server_binary: String,
}

fn default_daily_budget() -> f64 {
    50.0
}
fn default_timeout_secs() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicyConfig {
    #[serde(default = "default_low_risk_policy")]
    pub low_risk: String,
    #[serde(default = "default_medium_risk_policy")]
    pub medium_risk: String,
    #[serde(default = "default_high_risk_policy")]
    pub high_risk: String,
}

fn default_low_risk_policy() -> String {
    "validate_and_execute".to_string()
}
fn default_medium_risk_policy() -> String {
    "validate_and_execute".to_string()
}
fn default_high_risk_policy() -> String {
    "require_approval".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDescriptorConfig {
    pub id: String,
    pub kind: AgentKind,
    #[serde(default)]
    pub display_name: String,
    pub binary_path: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_priority")]
    pub priority: u32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_cost")]
    pub max_cost_usd: f64,
    #[serde(default)]
    pub specializations: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

fn default_priority() -> u32 {
    10
}
fn default_max_tokens() -> u64 {
    100_000
}
fn default_max_cost() -> f64 {
    0.50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    #[serde(default = "default_max_invocations_per_hour")]
    pub max_invocations_per_hour: u32,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
}

fn default_max_invocations_per_hour() -> u32 {
    20
}
fn default_max_concurrent() -> u32 {
    3
}

// ──────────────────────────────────────────────
// Config loading
// ──────────────────────────────────────────────

impl AionConfig {
    /// Load configuration from a TOML file path.
    pub fn from_file(path: impl AsRef<Path>) -> AionResult<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| {
            AionError::Config(format!("failed to read config file '{}': {e}", path.display()))
        })?;
        Self::from_str(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn from_str(toml_str: &str) -> AionResult<Self> {
        toml::from_str(toml_str)
            .map_err(|e| AionError::Config(format!("failed to parse TOML: {e}")))
    }
}

impl AgentsConfig {
    /// Load agents configuration from a TOML file path.
    pub fn from_file(path: impl AsRef<Path>) -> AionResult<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| {
            AionError::Config(format!(
                "failed to read agents config '{}': {e}",
                path.display()
            ))
        })?;
        Self::from_str(&content)
    }

    /// Parse agents configuration from a TOML string.
    pub fn from_str(toml_str: &str) -> AionResult<Self> {
        toml::from_str(toml_str)
            .map_err(|e| AionError::Config(format!("failed to parse agents TOML: {e}")))
    }

    /// Get only enabled agents, sorted by priority (lower = higher priority).
    pub fn enabled_agents(&self) -> Vec<&AgentDescriptorConfig> {
        let mut agents: Vec<_> = self.agents.iter().filter(|a| a.enabled).collect();
        agents.sort_by_key(|a| a.priority);
        agents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_aion_config() {
        let config: AionConfig = toml::from_str("").unwrap();
        assert_eq!(config.daemon.grpc_port, 50051);
        assert_eq!(config.daemon.rest_port, 8080);
        assert!(config.observe.enable_ebpf);
        assert_eq!(config.pipeline.max_concurrent_mounts, 3);
    }

    #[test]
    fn test_aion_config_from_toml() {
        let toml_str = r#"
[daemon]
grpc_port = 50052
rest_port = 9090
log_level = "debug"

[observe]
enable_ebpf = false
poll_interval_secs = 30

[pipeline]
max_concurrent_mounts = 5
canary_duration_secs = 60
"#;
        let config = AionConfig::from_str(toml_str).unwrap();
        assert_eq!(config.daemon.grpc_port, 50052);
        assert_eq!(config.daemon.rest_port, 9090);
        assert!(!config.observe.enable_ebpf);
        assert_eq!(config.observe.poll_interval_secs, 30);
        assert_eq!(config.pipeline.max_concurrent_mounts, 5);
    }

    #[test]
    fn test_agents_config_from_toml() {
        let toml_str = r#"
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
display_name = "Claude Code (Primary)"
binary_path = "/usr/local/bin/claude"
model = "sonnet"
enabled = true
priority = 1
max_tokens = 100000
timeout_secs = 180
max_cost_usd = 0.50
specializations = ["oom_analysis", "resource_optimization"]
capabilities = ["infra_analysis", "remediation_proposal"]

[[agents]]
id = "codex-primary"
kind = "codex_cli"
display_name = "Codex CLI"
binary_path = "/usr/local/bin/codex"
model = "gpt-5-codex"
enabled = false
priority = 2

[budget.claude_code]
max_invocations_per_hour = 20
max_concurrent = 3
"#;
        let config = AgentsConfig::from_str(toml_str).unwrap();
        assert_eq!(config.global.daily_budget_usd, 50.0);
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents[0].id, "claude-primary");
        assert_eq!(config.agents[0].kind, AgentKind::ClaudeCode);
        assert_eq!(config.agents[0].specializations.len(), 2);

        // Only enabled agents
        let enabled = config.enabled_agents();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "claude-primary");

        // Budget
        assert!(config.budget.contains_key("claude_code"));
        assert_eq!(config.budget["claude_code"].max_concurrent, 3);
    }

    #[test]
    fn test_config_file_not_found() {
        let result = AionConfig::from_file("/nonexistent/path.toml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AionError::Config(_)));
    }

    #[test]
    fn test_invalid_toml() {
        let result = AionConfig::from_str("this is not valid toml [[[");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        std::fs::write(
            &path,
            r#"
[daemon]
log_level = "trace"
"#,
        )
        .unwrap();
        let config = AionConfig::from_file(&path).unwrap();
        assert_eq!(config.daemon.log_level, "trace");
    }

    #[test]
    fn test_invalid_port_type() {
        let toml_str = r#"
[daemon]
grpc_port = "not_a_number"
"#;
        let result = AionConfig::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AionError::Config(_)));
    }

    #[test]
    fn test_partial_config_uses_defaults() {
        let toml_str = r#"
[daemon]
log_level = "debug"
"#;
        let config = AionConfig::from_str(toml_str).unwrap();
        assert_eq!(config.daemon.log_level, "debug");
        // Rest should be defaults
        assert_eq!(config.daemon.grpc_port, 50051);
        assert_eq!(config.daemon.rest_port, 8080);
        assert!(config.observe.enable_ebpf);
        assert_eq!(config.pipeline.max_concurrent_mounts, 3);
    }

    #[test]
    fn test_empty_toml_uses_all_defaults() {
        let config = AionConfig::from_str("").unwrap();
        assert_eq!(config.daemon.listen_addr, "0.0.0.0");
        assert_eq!(config.daemon.grpc_port, 50051);
        assert_eq!(config.daemon.rest_port, 8080);
        assert_eq!(config.daemon.log_level, "info");
        assert_eq!(config.daemon.audit_dir, "/var/lib/aion/audit");
        assert!(config.observe.enable_ebpf);
        assert!(config.observe.enable_cgroup);
        assert!(config.observe.enable_kube_watcher);
        assert_eq!(config.observe.poll_interval_secs, 10);
        assert_eq!(config.pipeline.max_concurrent_mounts, 3);
        assert_eq!(config.pipeline.canary_duration_secs, 30);
    }

    #[test]
    fn test_agents_config_missing_global() {
        let toml_str = r#"
[[agents]]
id = "claude-primary"
kind = "claude_code"
binary_path = "/usr/local/bin/claude"
"#;
        // AgentsConfig requires [global] with mcp_server_binary (no default)
        let result = AgentsConfig::from_str(toml_str);
        assert!(result.is_err());
    }
}
