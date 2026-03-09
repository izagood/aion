use aion_common::types::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::fs;
use tracing::info;

/// Actions tracked in the audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    EventDetected,
    AgentMounted,
    McpToolCalled,
    ProposalCreated,
    ProposalValidated,
    ProposalApproved,
    ProposalRejected,
    ExecutionStarted,
    ExecutionDryRun,
    ExecutionCanary,
    ExecutionCompleted,
    ExecutionRolledBack,
    ExecutionFailed,
}

/// A single entry in the hash-chain audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub entry_id: String,
    pub sequence: u64,
    pub previous_hash: String,
    pub content_hash: String,
    pub action: AuditAction,
    pub timestamp: Timestamp,
    pub actor_type: String,
    pub actor_id: String,
    pub anomaly_id: Option<String>,
    pub proposal_id: Option<String>,
    pub invocation_id: Option<String>,
    pub description: String,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl AuditEntry {
    fn compute_hash(&self) -> String {
        let payload = format!(
            "{}:{}:{}:{:?}:{}:{}:{}",
            self.sequence,
            self.previous_hash,
            self.timestamp.timestamp_nanos_opt().unwrap_or(0),
            self.action,
            self.actor_id,
            self.description,
            self.anomaly_id.as_deref().unwrap_or(""),
        );
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Append-only audit logger with SHA-256 hash chain for tamper detection.
pub struct AuditLogger {
    state: Mutex<AuditState>,
    log_dir: PathBuf,
}

struct AuditState {
    sequence: u64,
    last_hash: String,
    entries: Vec<AuditEntry>,
}

impl AuditLogger {
    /// Create a new audit logger writing to the specified directory.
    pub fn new(log_dir: impl AsRef<Path>) -> Self {
        Self {
            state: Mutex::new(AuditState {
                sequence: 0,
                last_hash: "genesis".to_string(),
                entries: Vec::new(),
            }),
            log_dir: log_dir.as_ref().to_path_buf(),
        }
    }

    /// Log an audit entry, chaining it to the previous entry.
    pub fn log(&self, action: AuditAction, actor_type: &str, actor_id: &str, description: &str) -> AuditEntry {
        self.log_detailed(action, actor_type, actor_id, description, None, None, None)
    }

    /// Log with full details.
    pub fn log_detailed(
        &self,
        action: AuditAction,
        actor_type: &str,
        actor_id: &str,
        description: &str,
        anomaly_id: Option<String>,
        proposal_id: Option<String>,
        invocation_id: Option<String>,
    ) -> AuditEntry {
        let mut state = self.state.lock().unwrap();
        state.sequence += 1;

        let mut entry = AuditEntry {
            entry_id: uuid::Uuid::new_v4().to_string(),
            sequence: state.sequence,
            previous_hash: state.last_hash.clone(),
            content_hash: String::new(),
            action,
            timestamp: aion_common::types::now(),
            actor_type: actor_type.to_string(),
            actor_id: actor_id.to_string(),
            anomaly_id,
            proposal_id,
            invocation_id,
            description: description.to_string(),
            metadata: std::collections::HashMap::new(),
        };

        entry.content_hash = entry.compute_hash();
        state.last_hash = entry.content_hash.clone();
        state.entries.push(entry.clone());

        entry
    }

    /// Verify the integrity of the hash chain.
    pub fn verify_integrity(&self) -> IntegrityResult {
        let state = self.state.lock().unwrap();
        let mut expected_prev = "genesis".to_string();

        for (i, entry) in state.entries.iter().enumerate() {
            // Check sequence
            if entry.sequence != (i as u64 + 1) {
                return IntegrityResult {
                    is_valid: false,
                    entries_verified: i as u64,
                    first_broken_sequence: Some(entry.sequence),
                    error: Some(format!("Sequence gap at entry {}", i)),
                };
            }

            // Check chain
            if entry.previous_hash != expected_prev {
                return IntegrityResult {
                    is_valid: false,
                    entries_verified: i as u64,
                    first_broken_sequence: Some(entry.sequence),
                    error: Some(format!("Hash chain broken at sequence {}", entry.sequence)),
                };
            }

            // Verify content hash
            let computed = entry.compute_hash();
            if entry.content_hash != computed {
                return IntegrityResult {
                    is_valid: false,
                    entries_verified: i as u64,
                    first_broken_sequence: Some(entry.sequence),
                    error: Some(format!("Content hash mismatch at sequence {}", entry.sequence)),
                };
            }

            expected_prev = entry.content_hash.clone();
        }

        IntegrityResult {
            is_valid: true,
            entries_verified: state.entries.len() as u64,
            first_broken_sequence: None,
            error: None,
        }
    }

    /// Get all entries (for querying).
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.state.lock().unwrap().entries.clone()
    }

    /// Get entries for a specific anomaly.
    pub fn entries_for_anomaly(&self, anomaly_id: &str) -> Vec<AuditEntry> {
        self.state
            .lock()
            .unwrap()
            .entries
            .iter()
            .filter(|e| e.anomaly_id.as_deref() == Some(anomaly_id))
            .cloned()
            .collect()
    }

    /// Write entries to disk as JSONL.
    pub async fn flush_to_disk(&self) -> Result<(), std::io::Error> {
        let entries = self.entries();
        if entries.is_empty() {
            return Ok(());
        }

        fs::create_dir_all(&self.log_dir).await?;
        let path = self.log_dir.join("audit.jsonl");
        let mut content = String::new();
        for entry in &entries {
            content.push_str(&serde_json::to_string(entry).unwrap());
            content.push('\n');
        }
        fs::write(&path, content).await?;
        info!(path = %path.display(), entries = entries.len(), "Audit log flushed to disk");
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityResult {
    pub is_valid: bool,
    pub entries_verified: u64,
    pub first_broken_sequence: Option<u64>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_chain() {
        let logger = AuditLogger::new("/tmp/test-audit");

        logger.log(
            AuditAction::EventDetected,
            "system",
            "oom-watcher",
            "OOM kill detected on worker-1",
        );

        logger.log(
            AuditAction::AgentMounted,
            "system",
            "mount-pipeline",
            "Claude Code agent mounted for analysis",
        );

        logger.log(
            AuditAction::ProposalCreated,
            "agent",
            "claude-primary",
            "Proposed pod reschedule",
        );

        let entries = logger.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].sequence, 1);
        assert_eq!(entries[1].sequence, 2);
        assert_eq!(entries[2].sequence, 3);

        // Chain integrity
        assert_eq!(entries[0].previous_hash, "genesis");
        assert_eq!(entries[1].previous_hash, entries[0].content_hash);
        assert_eq!(entries[2].previous_hash, entries[1].content_hash);
    }

    #[test]
    fn test_integrity_verification() {
        let logger = AuditLogger::new("/tmp/test-audit");

        for i in 0..5 {
            logger.log(
                AuditAction::McpToolCalled,
                "agent",
                "claude",
                &format!("Tool call {i}"),
            );
        }

        let result = logger.verify_integrity();
        assert!(result.is_valid);
        assert_eq!(result.entries_verified, 5);
    }

    #[test]
    fn test_tamper_detection() {
        let logger = AuditLogger::new("/tmp/test-audit");

        logger.log(AuditAction::EventDetected, "system", "watcher", "event 1");
        logger.log(AuditAction::AgentMounted, "system", "pipeline", "event 2");

        // Tamper with an entry
        {
            let mut state = logger.state.lock().unwrap();
            state.entries[0].description = "TAMPERED".to_string();
        }

        let result = logger.verify_integrity();
        assert!(!result.is_valid);
        assert_eq!(result.first_broken_sequence, Some(1));
    }

    #[test]
    fn test_entries_for_anomaly() {
        let logger = AuditLogger::new("/tmp/test-audit");

        logger.log_detailed(
            AuditAction::EventDetected, "system", "watcher", "OOM event",
            Some("a-1".to_string()), None, None,
        );
        logger.log_detailed(
            AuditAction::ProposalCreated, "agent", "claude", "proposal",
            Some("a-1".to_string()), Some("p-1".to_string()), None,
        );
        logger.log_detailed(
            AuditAction::EventDetected, "system", "watcher", "other event",
            Some("a-2".to_string()), None, None,
        );

        let entries = logger.entries_for_anomaly("a-1");
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_flush_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let logger = AuditLogger::new(dir.path());

        logger.log(AuditAction::EventDetected, "system", "test", "test entry");
        logger.flush_to_disk().await.unwrap();

        let path = dir.path().join("audit.jsonl");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("event_detected"));
    }

    #[test]
    fn test_empty_chain_verification() {
        let logger = AuditLogger::new("/tmp/test-audit-empty");
        let result = logger.verify_integrity();
        assert!(result.is_valid);
        assert_eq!(result.entries_verified, 0);
        assert!(result.first_broken_sequence.is_none());
    }

    #[test]
    fn test_single_entry_integrity() {
        let logger = AuditLogger::new("/tmp/test-audit-single");
        logger.log(AuditAction::EventDetected, "system", "test", "single entry");

        let result = logger.verify_integrity();
        assert!(result.is_valid);
        assert_eq!(result.entries_verified, 1);

        let entries = logger.entries();
        assert_eq!(entries[0].sequence, 1);
        assert_eq!(entries[0].previous_hash, "genesis");
        assert!(!entries[0].content_hash.is_empty());
    }

    #[test]
    fn test_tamper_middle_entry() {
        let logger = AuditLogger::new("/tmp/test-audit-tamper-mid");
        logger.log(AuditAction::EventDetected, "system", "watcher", "event 1");
        logger.log(AuditAction::AgentMounted, "system", "pipeline", "event 2");
        logger.log(AuditAction::ProposalCreated, "agent", "claude", "event 3");

        // Tamper with the second entry
        {
            let mut state = logger.state.lock().unwrap();
            state.entries[1].description = "TAMPERED MIDDLE".to_string();
        }

        let result = logger.verify_integrity();
        assert!(!result.is_valid);
        assert_eq!(result.first_broken_sequence, Some(2));
        assert!(result.error.as_ref().unwrap().contains("Content hash mismatch"));
    }

    #[tokio::test]
    async fn test_flush_empty_noop() {
        let dir = tempfile::tempdir().unwrap();
        let logger = AuditLogger::new(dir.path());

        // Flushing empty logger should be a no-op
        let result = logger.flush_to_disk().await;
        assert!(result.is_ok());

        // audit.jsonl file should not be created
        let path = dir.path().join("audit.jsonl");
        assert!(!path.exists());
    }
}
