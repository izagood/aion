//! Integration tests for cross-crate pipeline flows.
//!
//! These tests verify that components from different crates work together correctly.

use aion_audit::{AuditAction, AuditLogger};
use aion_common::types::*;
use aion_execute::{DeterministicExecutor, ExecutionStage};
use aion_mount::governor::BudgetGovernor;
use aion_propose::{ActionType, Proposal};
use aion_validate::PolicyChain;

fn make_proposal(action: ActionType, ns: &str, kind: &str, name: &str) -> Proposal {
    Proposal::new(
        AnomalyId::new("a-integration"),
        AgentId::new("claude"),
        InvocationId::new("inv-integration"),
        action,
        Namespace::new(ns),
        name,
        kind,
        "Integration test rationale for proposal validation",
    )
}

#[tokio::test]
async fn test_proposal_validate_then_execute() {
    // 1. Create proposal
    let mut proposal = make_proposal(ActionType::RestartPod, "default", "Pod", "my-pod")
        .with_blast_radius(5);

    // 2. Validate through PolicyChain
    let chain = PolicyChain::default_chain();
    let (passed, results) = chain.validate_all(&proposal);
    assert!(passed, "Proposal should pass validation: {:?}", results);

    // 3. Execute
    let executor = DeterministicExecutor::new();
    let result = executor.execute(&mut proposal).await.unwrap();
    assert!(result.is_success());
    assert_eq!(result.final_stage, ExecutionStage::Completed);
}

#[tokio::test]
async fn test_proposal_rejected_by_namespace() {
    // kube-system is a protected namespace
    let proposal = make_proposal(ActionType::RestartPod, "kube-system", "Pod", "coredns")
        .with_blast_radius(1);

    let chain = PolicyChain::default_chain();
    let (passed, results) = chain.validate_all(&proposal);
    assert!(!passed);
    assert!(
        results.iter().any(|r| !r.passed && r.validator_name == "namespace_scope"),
        "Should be rejected by namespace_scope validator"
    );
}

#[test]
fn test_capability_token_issue_verify_roundtrip() {
    use aion_capability::TokenManager;

    let manager = TokenManager::with_random_key();

    for severity in [Severity::Info, Severity::Warning, Severity::Critical] {
        let token = manager.issue(
            AgentId::new("claude"),
            AnomalyId::new("a-1"),
            severity,
            120,
        );
        assert!(manager.verify(&token).is_ok(), "Token for {:?} should verify", severity);
    }
}

#[test]
fn test_audit_full_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let logger = AuditLogger::new(dir.path());

    logger.log_detailed(
        AuditAction::EventDetected, "system", "oom-watcher", "OOM detected",
        Some("a-lifecycle".to_string()), None, None,
    );
    logger.log_detailed(
        AuditAction::AgentMounted, "system", "mount-pipeline", "Agent mounted",
        Some("a-lifecycle".to_string()), None, Some("inv-1".to_string()),
    );
    logger.log_detailed(
        AuditAction::ProposalCreated, "agent", "claude", "Proposal created",
        Some("a-lifecycle".to_string()), Some("p-1".to_string()), Some("inv-1".to_string()),
    );
    logger.log_detailed(
        AuditAction::ExecutionCompleted, "system", "executor", "Execution completed",
        Some("a-lifecycle".to_string()), Some("p-1".to_string()), Some("inv-1".to_string()),
    );

    let result = logger.verify_integrity();
    assert!(result.is_valid);
    assert_eq!(result.entries_verified, 4);

    let entries = logger.entries_for_anomaly("a-lifecycle");
    assert_eq!(entries.len(), 4);

    // Verify hash chain continuity
    assert_eq!(entries[0].previous_hash, "genesis");
    for i in 1..entries.len() {
        assert_eq!(entries[i].previous_hash, entries[i - 1].content_hash);
    }
}

#[test]
fn test_budget_exhaustion_prevents_further() {
    let governor = BudgetGovernor::new(1.0);
    governor.register_kind("claude_code", 100, 10);

    let _g1 = governor.try_acquire("claude_code", 0.50).unwrap();
    let _g2 = governor.try_acquire("claude_code", 0.50).unwrap();

    let result = governor.try_acquire("claude_code", 0.01);
    assert!(result.is_err());
    assert!((governor.daily_remaining() - 0.0).abs() < 0.01);
}
