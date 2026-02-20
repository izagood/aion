use std::sync::Arc;

use tracing::{error, info, warn};

use aion_audit::{AuditAction, AuditLogger};
use aion_common::config::{AgentsConfig, AionConfig};
use aion_execute::DeterministicExecutor;
use aion_mount::pipeline::{MountPipeline, MountResult};
use aion_mount::selector::SelectionCriteria;
use aion_observe::collector::EventBus;
use aion_observe::event::{EventKind, InfraEvent};
use aion_observe::oom_watcher::OomWatcherConfig;
use aion_validate::PolicyChain;

/// Top-level pipeline that integrates all AION subsystems.
///
/// Observe → Mount → Validate → Execute → Audit
pub struct AionPipeline {
    event_bus: EventBus,
    mount_pipeline: MountPipeline,
    executor: DeterministicExecutor,
    policy_chain: PolicyChain,
    oom_watcher_config: OomWatcherConfig,
}

impl AionPipeline {
    pub fn new(daemon_config: &AionConfig, agents_config: &AgentsConfig) -> Self {
        let event_bus = EventBus::new(256);
        let mount_pipeline = MountPipeline::new(agents_config);
        mount_pipeline.init_budgets(agents_config);
        let executor = DeterministicExecutor::new();
        let policy_chain = PolicyChain::default_chain();

        // Start in mock mode for MVP (no eBPF required)
        let oom_watcher_config = OomWatcherConfig {
            mock_mode: daemon_config.observe.mock_mode.unwrap_or(true),
            ..Default::default()
        };

        Self {
            event_bus,
            mount_pipeline,
            executor,
            policy_chain,
            oom_watcher_config,
        }
    }

    /// Main event loop: subscribe to events, analyze with agents, validate, execute.
    pub async fn run(&self, audit_logger: Arc<AuditLogger>) {
        let mut rx = self.event_bus.subscribe();

        info!("Pipeline event loop started, waiting for infrastructure events...");

        loop {
            match rx.recv().await {
                Ok(event) => {
                    info!(
                        event_id = %event.id,
                        kind = ?event.kind,
                        severity = ?event.severity,
                        "Infrastructure event received"
                    );

                    audit_logger.log_detailed(
                        AuditAction::EventDetected,
                        "system",
                        "event-bus",
                        &event.description,
                        Some(event.id.to_string()),
                        None,
                        None,
                    );

                    self.handle_event(&event, &audit_logger).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Event bus lagged, some events were dropped");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("Event bus closed, shutting down pipeline");
                    break;
                }
            }
        }
    }

    /// Handle a single infrastructure event through the full pipeline.
    async fn handle_event(&self, event: &InfraEvent, audit_logger: &AuditLogger) {
        let anomaly_id = event.id.to_string();

        // Build selection criteria based on event kind
        let criteria = self.build_criteria(event);

        // Build the analysis prompt
        let prompt = self.build_prompt(event);

        // Execute mount pipeline: select agent → launch → get proposal
        let mount_result = self
            .mount_pipeline
            .execute(&anomaly_id, event.severity, &criteria, &prompt)
            .await;

        match mount_result {
            MountResult::Approved(mut proposal) => {
                info!(
                    proposal_id = %proposal.id,
                    action = ?proposal.action_type,
                    "Proposal approved, validating with policy chain"
                );

                audit_logger.log_detailed(
                    AuditAction::ProposalCreated,
                    "agent",
                    &proposal.agent_id.to_string(),
                    &format!("Proposal: {:?} on {}", proposal.action_type, proposal.target_name),
                    Some(anomaly_id.clone()),
                    Some(proposal.id.to_string()),
                    Some(proposal.invocation_id.to_string()),
                );

                // Run policy chain validation
                let (passed, results) = self.policy_chain.validate_all(&proposal);

                if passed {
                    audit_logger.log_detailed(
                        AuditAction::ProposalValidated,
                        "system",
                        "policy-chain",
                        "All policy validators passed",
                        Some(anomaly_id.clone()),
                        Some(proposal.id.to_string()),
                        None,
                    );

                    // Execute through deterministic executor
                    audit_logger.log_detailed(
                        AuditAction::ExecutionStarted,
                        "system",
                        "executor",
                        &format!("Executing {:?}", proposal.action_type),
                        Some(anomaly_id.clone()),
                        Some(proposal.id.to_string()),
                        None,
                    );

                    match self.executor.execute(&mut proposal).await {
                        Ok(result) => {
                            if result.is_success() {
                                info!(
                                    proposal_id = %proposal.id,
                                    "Execution completed successfully"
                                );
                                audit_logger.log_detailed(
                                    AuditAction::ExecutionCompleted,
                                    "system",
                                    "executor",
                                    "Execution completed successfully",
                                    Some(anomaly_id),
                                    Some(proposal.id.to_string()),
                                    None,
                                );
                            } else if result.rolled_back {
                                warn!(
                                    proposal_id = %proposal.id,
                                    "Execution rolled back"
                                );
                                audit_logger.log_detailed(
                                    AuditAction::ExecutionRolledBack,
                                    "system",
                                    "executor",
                                    "Execution was rolled back",
                                    Some(anomaly_id),
                                    Some(proposal.id.to_string()),
                                    None,
                                );
                            } else {
                                error!(
                                    proposal_id = %proposal.id,
                                    "Execution failed"
                                );
                                audit_logger.log_detailed(
                                    AuditAction::ExecutionFailed,
                                    "system",
                                    "executor",
                                    "Execution failed",
                                    Some(anomaly_id),
                                    Some(proposal.id.to_string()),
                                    None,
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                proposal_id = %proposal.id,
                                error = %e,
                                "Execution error"
                            );
                            audit_logger.log_detailed(
                                AuditAction::ExecutionFailed,
                                "system",
                                "executor",
                                &format!("Execution error: {e}"),
                                Some(anomaly_id),
                                Some(proposal.id.to_string()),
                                None,
                            );
                        }
                    }
                } else {
                    let reasons: Vec<String> = results
                        .iter()
                        .filter(|r| !r.passed)
                        .map(|r| format!("{}: {}", r.validator_name, r.reason))
                        .collect();
                    warn!(
                        proposal_id = %proposal.id,
                        reasons = ?reasons,
                        "Proposal rejected by policy chain"
                    );
                    audit_logger.log_detailed(
                        AuditAction::ProposalRejected,
                        "system",
                        "policy-chain",
                        &format!("Rejected: {}", reasons.join("; ")),
                        Some(anomaly_id),
                        Some(proposal.id.to_string()),
                        None,
                    );
                }
            }
            MountResult::AwaitingApproval(proposal) => {
                info!(
                    proposal_id = %proposal.id,
                    "Proposal awaiting human approval"
                );
                audit_logger.log_detailed(
                    AuditAction::ProposalCreated,
                    "agent",
                    &proposal.agent_id.to_string(),
                    &format!("Awaiting approval: {:?}", proposal.action_type),
                    Some(anomaly_id),
                    Some(proposal.id.to_string()),
                    None,
                );
            }
            MountResult::Rejected { proposal, reasons } => {
                warn!(
                    proposal_id = %proposal.id,
                    reasons = ?reasons,
                    "Proposal rejected by mount pipeline"
                );
                audit_logger.log_detailed(
                    AuditAction::ProposalRejected,
                    "system",
                    "mount-pipeline",
                    &format!("Rejected: {}", reasons.join("; ")),
                    Some(anomaly_id),
                    Some(proposal.id.to_string()),
                    None,
                );
            }
            MountResult::AgentFailed { agent_id, error } => {
                error!(
                    agent_id = %agent_id,
                    error = %error,
                    "Agent invocation failed"
                );
            }
            MountResult::NoAgentAvailable => {
                warn!("No suitable agent available for this anomaly");
            }
            MountResult::BudgetExceeded(reason) => {
                warn!(reason = %reason, "Budget exceeded, skipping analysis");
            }
        }
    }

    fn build_criteria(&self, event: &InfraEvent) -> SelectionCriteria {
        match event.kind {
            EventKind::OomKill => SelectionCriteria {
                required_capabilities: vec!["oom_analysis".to_string()],
                preferred_specializations: vec![
                    "oom_analysis".to_string(),
                    "resource_optimization".to_string(),
                ],
                max_cost_usd: None,
            },
            EventKind::CpuThrottle => SelectionCriteria {
                required_capabilities: vec!["infra_analysis".to_string()],
                preferred_specializations: vec!["resource_optimization".to_string()],
                max_cost_usd: None,
            },
            _ => SelectionCriteria {
                required_capabilities: vec!["infra_analysis".to_string()],
                preferred_specializations: vec![],
                max_cost_usd: None,
            },
        }
    }

    fn build_prompt(&self, event: &InfraEvent) -> String {
        format!(
            "You are an infrastructure operations AI agent. An anomaly has been detected:\n\n\
            Event Type: {:?}\n\
            Severity: {:?}\n\
            Description: {}\n\n\
            Your task:\n\
            1. Use get_anomaly_context to understand the full context\n\
            2. Use get_node_status, get_pod_metrics, and other tools to gather more data\n\
            3. Analyze the situation and determine the best remediation action\n\
            4. Call propose_action with your recommended action\n\n\
            Available actions: restart_pod, reschedule_pod, adjust_resources, \
            scale_deployment, cordon_node, drain_node, uncordon_node\n\n\
            Be conservative in your recommendations. Prefer low-risk actions when possible.",
            event.kind, event.severity, event.description
        )
    }

    /// Get access to the event bus for injecting events (testing/API).
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}
