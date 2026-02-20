use aion_common::types::{AgentId, AnomalyId, InvocationId, RiskLevel, Severity, Timestamp};
use chrono::Duration;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// A signed capability token for agent invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedCapabilityToken {
    pub invocation_id: InvocationId,
    pub agent_id: AgentId,
    pub anomaly_id: AnomalyId,
    pub max_risk_level: RiskLevel,
    pub max_blast_radius: u32,
    pub allowed_namespaces: Vec<String>,
    pub denied_namespaces: Vec<String>,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub allow_direct_execution: bool,
    /// HMAC-SHA256 signature of the token payload
    pub signature: String,
}

/// Issues and verifies capability tokens.
pub struct TokenManager {
    signing_key: Vec<u8>,
}

impl TokenManager {
    pub fn new(signing_key: impl Into<Vec<u8>>) -> Self {
        Self {
            signing_key: signing_key.into(),
        }
    }

    /// Create from a random key (for testing).
    pub fn with_random_key() -> Self {
        use rand::RngCore;
        let mut key = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Self::new(key)
    }

    /// Issue a signed capability token.
    pub fn issue(
        &self,
        agent_id: AgentId,
        anomaly_id: AnomalyId,
        severity: Severity,
        timeout_secs: u64,
    ) -> SignedCapabilityToken {
        let now = aion_common::types::now();
        let invocation_id = InvocationId::new(uuid::Uuid::new_v4().to_string());

        let (max_risk, max_blast, allow_exec) = match severity {
            Severity::Info => (RiskLevel::Low, 5, true),
            Severity::Warning => (RiskLevel::Medium, 10, false),
            Severity::Critical => (RiskLevel::High, 50, false),
        };

        let mut token = SignedCapabilityToken {
            invocation_id,
            agent_id,
            anomaly_id,
            max_risk_level: max_risk,
            max_blast_radius: max_blast,
            allowed_namespaces: Vec::new(),
            denied_namespaces: vec!["kube-system".to_string()],
            issued_at: now,
            expires_at: now + Duration::seconds(timeout_secs as i64),
            allow_direct_execution: allow_exec,
            signature: String::new(),
        };

        token.signature = self.sign(&token);
        token
    }

    /// Verify a token's signature and expiry.
    pub fn verify(&self, token: &SignedCapabilityToken) -> Result<(), TokenError> {
        // Check expiry
        if aion_common::types::now() >= token.expires_at {
            return Err(TokenError::Expired);
        }

        // Check signature
        let expected = self.sign(token);
        if token.signature != expected {
            return Err(TokenError::InvalidSignature);
        }

        Ok(())
    }

    fn sign(&self, token: &SignedCapabilityToken) -> String {
        let payload = format!(
            "{}:{}:{}:{}:{:?}:{}",
            token.invocation_id,
            token.agent_id,
            token.anomaly_id,
            token.issued_at.timestamp(),
            token.max_risk_level,
            token.max_blast_radius,
        );

        let mut mac =
            HmacSha256::new_from_slice(&self.signing_key).expect("HMAC key length is valid");
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("token has expired")]
    Expired,
    #[error("invalid token signature")]
    InvalidSignature,
}

// hex encoding helper (no external dependency)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_and_verify() {
        let manager = TokenManager::with_random_key();
        let token = manager.issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Critical,
            120,
        );

        assert!(!token.signature.is_empty());
        assert!(manager.verify(&token).is_ok());
    }

    #[test]
    fn test_tampered_signature() {
        let manager = TokenManager::with_random_key();
        let mut token = manager.issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Warning,
            120,
        );

        token.signature = "tampered".to_string();
        assert!(matches!(
            manager.verify(&token),
            Err(TokenError::InvalidSignature)
        ));
    }

    #[test]
    fn test_different_keys_reject() {
        let issuer = TokenManager::with_random_key();
        let verifier = TokenManager::with_random_key();

        let token = issuer.issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Info,
            120,
        );

        assert!(matches!(
            verifier.verify(&token),
            Err(TokenError::InvalidSignature)
        ));
    }

    #[test]
    fn test_severity_determines_risk() {
        let manager = TokenManager::with_random_key();

        let info_token = manager.issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            Severity::Info,
            120,
        );
        assert_eq!(info_token.max_risk_level, RiskLevel::Low);
        assert!(info_token.allow_direct_execution);

        let critical_token = manager.issue(
            AgentId::new("claude"),
            AnomalyId::new("a-2"),
            Severity::Critical,
            120,
        );
        assert_eq!(critical_token.max_risk_level, RiskLevel::High);
        assert!(!critical_token.allow_direct_execution);
    }
}
