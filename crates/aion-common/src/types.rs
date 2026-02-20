use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

// ──────────────────────────────────────────────
// Newtype IDs — compile-time type safety
// ──────────────────────────────────────────────

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

define_id!(AnomalyId, "Unique identifier for a detected anomaly");
define_id!(AgentId, "Unique identifier for a registered AI agent");
define_id!(ProposalId, "Unique identifier for a remediation proposal");
define_id!(InvocationId, "Unique identifier for a single agent invocation");
define_id!(NodeName, "Kubernetes node name");
define_id!(PodName, "Kubernetes pod name");
define_id!(Namespace, "Kubernetes namespace");

// ──────────────────────────────────────────────
// Severity — anomaly severity at detection time
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Critical => write!(f, "critical"),
        }
    }
}

// ──────────────────────────────────────────────
// RiskLevel — action risk at execution time
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Resource adjustment, pod restart → auto-validate and execute
    Low,
    /// Pod reschedule, scale adjustment → auto-validate (+ optional approval)
    Medium,
    /// Node cordon/drain → human approval required
    High,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
        }
    }
}

// ──────────────────────────────────────────────
// Agent kind
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    ClaudeCode,
    CodexCli,
    GeminiCli,
    Custom,
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentKind::ClaudeCode => write!(f, "claude_code"),
            AgentKind::CodexCli => write!(f, "codex_cli"),
            AgentKind::GeminiCli => write!(f, "gemini_cli"),
            AgentKind::Custom => write!(f, "custom"),
        }
    }
}

// ──────────────────────────────────────────────
// Timestamps
// ──────────────────────────────────────────────

pub type Timestamp = DateTime<Utc>;

pub fn now() -> Timestamp {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_types_are_distinct() {
        let anomaly = AnomalyId::new("a-123");
        let agent = AgentId::new("a-123");
        // Same string value but different types — cannot be confused at compile time
        assert_eq!(anomaly.as_str(), agent.as_str());
        assert_eq!(format!("{anomaly}"), "a-123");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Critical);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
    }

    #[test]
    fn test_severity_serde_roundtrip() {
        let json = serde_json::to_string(&Severity::Critical).unwrap();
        assert_eq!(json, "\"critical\"");
        let parsed: Severity = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Severity::Critical);
    }

    #[test]
    fn test_agent_kind_serde() {
        let json = serde_json::to_string(&AgentKind::ClaudeCode).unwrap();
        assert_eq!(json, "\"claude_code\"");
    }
}
