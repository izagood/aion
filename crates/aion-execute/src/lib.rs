use aion_propose::{ActionType, Proposal, ProposalStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{info, warn};

/// Errors during execution.
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("dry run failed: {0}")]
    DryRunFailed(String),
    #[error("canary failed: {0}")]
    CanaryFailed(String),
    #[error("rollout failed: {0}")]
    RolloutFailed(String),
    #[error("rollback failed: {0}")]
    RollbackFailed(String),
    #[error("unsupported action: {0:?}")]
    UnsupportedAction(ActionType),
    #[error("kubernetes error: {0}")]
    KubeError(String),
    #[error("timeout during execution")]
    Timeout,
}

/// The stage of deterministic execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStage {
    DryRun,
    Canary,
    Rollout,
    Completed,
    RolledBack,
    Failed,
}

/// Result of a single execution stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage: ExecutionStage,
    pub success: bool,
    pub message: String,
    pub details: HashMap<String, serde_json::Value>,
}

/// Full execution result across all stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub proposal_id: String,
    pub final_stage: ExecutionStage,
    pub stages: Vec<StageResult>,
    pub rolled_back: bool,
}

impl ExecutionResult {
    pub fn is_success(&self) -> bool {
        self.final_stage == ExecutionStage::Completed && !self.rolled_back
    }
}

/// Trait for action executors. Each action type has its own executor.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Validate the proposal can be executed (dry run).
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError>;

    /// Apply to a subset (canary). Returns result after monitoring period.
    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError>;

    /// Full rollout.
    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError>;

    /// Rollback a previously applied change.
    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError>;
}

/// The deterministic executor orchestrates the dry-run → canary → rollout pipeline.
pub struct DeterministicExecutor {
    executors: HashMap<ActionType, Box<dyn ActionExecutor>>,
}

impl DeterministicExecutor {
    pub fn new() -> Self {
        let mut executors: HashMap<ActionType, Box<dyn ActionExecutor>> = HashMap::new();
        executors.insert(ActionType::RestartPod, Box::new(RestartPodExecutor));
        executors.insert(ActionType::ReschedulePod, Box::new(ReschedulePodExecutor));
        executors.insert(
            ActionType::AdjustResources,
            Box::new(AdjustResourcesExecutor),
        );
        executors.insert(
            ActionType::ScaleDeployment,
            Box::new(ScaleDeploymentExecutor),
        );
        executors.insert(ActionType::CordonNode, Box::new(CordonNodeExecutor));
        executors.insert(ActionType::DrainNode, Box::new(DrainNodeExecutor));
        executors.insert(ActionType::UncordonNode, Box::new(UncordonNodeExecutor));
        Self { executors }
    }

    /// Execute a proposal through the full deterministic pipeline.
    ///
    /// Pipeline: dry_run → canary → rollout
    /// If any stage fails, rollback is attempted automatically.
    pub async fn execute(
        &self,
        proposal: &mut Proposal,
    ) -> Result<ExecutionResult, ExecutionError> {
        let executor = self
            .executors
            .get(&proposal.action_type)
            .ok_or_else(|| ExecutionError::UnsupportedAction(proposal.action_type.clone()))?;

        let mut stages = Vec::new();
        proposal.status = ProposalStatus::Executing;

        // Stage 1: Dry Run
        info!(
            proposal_id = %proposal.id,
            action = ?proposal.action_type,
            "Starting dry run"
        );

        let dry_result = executor.dry_run(proposal).await?;
        let dry_passed = dry_result.success;
        stages.push(dry_result);

        if !dry_passed {
            proposal.status = ProposalStatus::Failed;
            return Ok(ExecutionResult {
                proposal_id: proposal.id.to_string(),
                final_stage: ExecutionStage::Failed,
                stages,
                rolled_back: false,
            });
        }

        // Stage 2: Canary
        info!(proposal_id = %proposal.id, "Starting canary");

        let canary_result = executor.canary(proposal).await?;
        let canary_passed = canary_result.success;
        stages.push(canary_result);

        if !canary_passed {
            warn!(proposal_id = %proposal.id, "Canary failed, initiating rollback");
            match executor.rollback(proposal).await {
                Ok(rb) => {
                    stages.push(rb);
                    proposal.status = ProposalStatus::RolledBack;
                    return Ok(ExecutionResult {
                        proposal_id: proposal.id.to_string(),
                        final_stage: ExecutionStage::RolledBack,
                        stages,
                        rolled_back: true,
                    });
                }
                Err(e) => {
                    proposal.status = ProposalStatus::Failed;
                    return Err(ExecutionError::RollbackFailed(e.to_string()));
                }
            }
        }

        // Stage 3: Full Rollout
        info!(proposal_id = %proposal.id, "Starting full rollout");

        let rollout_result = executor.rollout(proposal).await?;
        let rollout_passed = rollout_result.success;
        stages.push(rollout_result);

        if !rollout_passed {
            warn!(proposal_id = %proposal.id, "Rollout failed, initiating rollback");
            match executor.rollback(proposal).await {
                Ok(rb) => {
                    stages.push(rb);
                    proposal.status = ProposalStatus::RolledBack;
                    return Ok(ExecutionResult {
                        proposal_id: proposal.id.to_string(),
                        final_stage: ExecutionStage::RolledBack,
                        stages,
                        rolled_back: true,
                    });
                }
                Err(e) => {
                    proposal.status = ProposalStatus::Failed;
                    return Err(ExecutionError::RollbackFailed(e.to_string()));
                }
            }
        }

        proposal.status = ProposalStatus::Completed;
        proposal.executed_at = Some(aion_common::types::now());

        Ok(ExecutionResult {
            proposal_id: proposal.id.to_string(),
            final_stage: ExecutionStage::Completed,
            stages,
            rolled_back: false,
        })
    }
}

impl Default for DeterministicExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ── Action Executors ──
// In production these would call kube-rs APIs.
// For MVP, they perform validation and log actions.

struct RestartPodExecutor;

#[async_trait]
impl ActionExecutor for RestartPodExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        // Validate target is a Pod
        if proposal.target_kind != "Pod" {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!(
                    "Expected target kind 'Pod', got '{}'",
                    proposal.target_kind
                ),
                details: HashMap::new(),
            });
        }

        Ok(StageResult {
            stage: ExecutionStage::DryRun,
            success: true,
            message: format!(
                "Dry run: would restart pod {}/{}",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        // For pod restart, canary = the restart itself (atomic operation)
        info!(
            pod = %proposal.target_name,
            namespace = %proposal.target_namespace,
            "Canary: restarting pod"
        );

        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!(
                "Pod {}/{} restart initiated",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!(
                "Pod {}/{} restarted successfully",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        // Pod restart is not truly reversible, but we log the attempt
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!(
                "Rollback noted for pod {}/{} restart (restart is atomic)",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }
}

struct ReschedulePodExecutor;

#[async_trait]
impl ActionExecutor for ReschedulePodExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        if proposal.target_kind != "Pod" {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!(
                    "Expected target kind 'Pod', got '{}'",
                    proposal.target_kind
                ),
                details: HashMap::new(),
            });
        }

        let target_node = proposal
            .parameters
            .get("target_node")
            .and_then(|v| v.as_str())
            .unwrap_or("auto-selected");

        Ok(StageResult {
            stage: ExecutionStage::DryRun,
            success: true,
            message: format!(
                "Dry run: would reschedule pod {}/{} to node {}",
                proposal.target_namespace, proposal.target_name, target_node
            ),
            details: HashMap::new(),
        })
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        info!(
            pod = %proposal.target_name,
            namespace = %proposal.target_namespace,
            "Canary: rescheduling pod"
        );

        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!(
                "Pod {}/{} eviction initiated for rescheduling",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!(
                "Pod {}/{} rescheduled successfully",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!(
                "Reschedule of pod {}/{} rolled back",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }
}

struct AdjustResourcesExecutor;

#[async_trait]
impl ActionExecutor for AdjustResourcesExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        // Validate resource parameters exist
        let has_cpu = proposal.parameters.contains_key("cpu_limit");
        let has_memory = proposal.parameters.contains_key("memory_limit");

        if !has_cpu && !has_memory {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: "No resource adjustments specified (need cpu_limit or memory_limit)"
                    .to_string(),
                details: HashMap::new(),
            });
        }

        Ok(StageResult {
            stage: ExecutionStage::DryRun,
            success: true,
            message: format!(
                "Dry run: would adjust resources for {}/{}",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!(
                "Resource adjustment canary for {}/{}",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!(
                "Resources adjusted for {}/{}",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!(
                "Resource adjustment rolled back for {}/{}",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }
}

struct ScaleDeploymentExecutor;

#[async_trait]
impl ActionExecutor for ScaleDeploymentExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        if proposal.target_kind != "Deployment" {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!(
                    "Expected target kind 'Deployment', got '{}'",
                    proposal.target_kind
                ),
                details: HashMap::new(),
            });
        }

        let replicas = proposal
            .parameters
            .get("replicas")
            .and_then(|v| v.as_u64());

        match replicas {
            Some(n) if n <= 100 => Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: true,
                message: format!(
                    "Dry run: would scale {}/{} to {} replicas",
                    proposal.target_namespace, proposal.target_name, n
                ),
                details: HashMap::new(),
            }),
            Some(n) => Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!("Replica count {} exceeds maximum of 100", n),
                details: HashMap::new(),
            }),
            None => Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: "Missing 'replicas' parameter".to_string(),
                details: HashMap::new(),
            }),
        }
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!(
                "Scale canary for {}/{}",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!(
                "Deployment {}/{} scaled successfully",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!(
                "Scale of {}/{} rolled back",
                proposal.target_namespace, proposal.target_name
            ),
            details: HashMap::new(),
        })
    }
}

struct CordonNodeExecutor;

#[async_trait]
impl ActionExecutor for CordonNodeExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        if proposal.target_kind != "Node" {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!(
                    "Expected target kind 'Node', got '{}'",
                    proposal.target_kind
                ),
                details: HashMap::new(),
            });
        }

        Ok(StageResult {
            stage: ExecutionStage::DryRun,
            success: true,
            message: format!("Dry run: would cordon node {}", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!("Node {} cordon initiated", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!("Node {} cordoned successfully", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        // Rollback cordon = uncordon
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!("Node {} uncordoned (cordon rolled back)", proposal.target_name),
            details: HashMap::new(),
        })
    }
}

struct DrainNodeExecutor;

#[async_trait]
impl ActionExecutor for DrainNodeExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        if proposal.target_kind != "Node" {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!(
                    "Expected target kind 'Node', got '{}'",
                    proposal.target_kind
                ),
                details: HashMap::new(),
            });
        }

        Ok(StageResult {
            stage: ExecutionStage::DryRun,
            success: true,
            message: format!("Dry run: would drain node {}", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!("Node {} drain initiated (canary: cordoned first)", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!("Node {} drained successfully", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!("Node {} drain rolled back (uncordoned)", proposal.target_name),
            details: HashMap::new(),
        })
    }
}

struct UncordonNodeExecutor;

#[async_trait]
impl ActionExecutor for UncordonNodeExecutor {
    async fn dry_run(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        if proposal.target_kind != "Node" {
            return Ok(StageResult {
                stage: ExecutionStage::DryRun,
                success: false,
                message: format!(
                    "Expected target kind 'Node', got '{}'",
                    proposal.target_kind
                ),
                details: HashMap::new(),
            });
        }

        Ok(StageResult {
            stage: ExecutionStage::DryRun,
            success: true,
            message: format!("Dry run: would uncordon node {}", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn canary(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Canary,
            success: true,
            message: format!("Node {} uncordon initiated", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn rollout(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::Rollout,
            success: true,
            message: format!("Node {} uncordoned successfully", proposal.target_name),
            details: HashMap::new(),
        })
    }

    async fn rollback(&self, proposal: &Proposal) -> Result<StageResult, ExecutionError> {
        Ok(StageResult {
            stage: ExecutionStage::RolledBack,
            success: true,
            message: format!("Node {} re-cordoned (uncordon rolled back)", proposal.target_name),
            details: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::types::*;

    fn make_proposal(action: ActionType, kind: &str, name: &str) -> Proposal {
        Proposal::new(
            AnomalyId::new("a-1"),
            AgentId::new("claude"),
            InvocationId::new("inv-1"),
            action,
            Namespace::new("default"),
            name,
            kind,
            "test rationale",
        )
    }

    #[tokio::test]
    async fn test_restart_pod_full_pipeline() {
        let executor = DeterministicExecutor::new();
        let mut proposal = make_proposal(ActionType::RestartPod, "Pod", "my-pod");

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.final_stage, ExecutionStage::Completed);
        assert_eq!(result.stages.len(), 3); // dry_run + canary + rollout
        assert_eq!(proposal.status, ProposalStatus::Completed);
        assert!(proposal.executed_at.is_some());
    }

    #[tokio::test]
    async fn test_dry_run_failure_stops_pipeline() {
        let executor = DeterministicExecutor::new();
        // Wrong target_kind for RestartPod — should fail at dry run
        let mut proposal = make_proposal(ActionType::RestartPod, "Deployment", "my-deploy");

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(!result.is_success());
        assert_eq!(result.final_stage, ExecutionStage::Failed);
        assert_eq!(result.stages.len(), 1); // only dry_run
        assert_eq!(proposal.status, ProposalStatus::Failed);
    }

    #[tokio::test]
    async fn test_reschedule_pod() {
        let executor = DeterministicExecutor::new();
        let mut params = HashMap::new();
        params.insert(
            "target_node".to_string(),
            serde_json::json!("worker-2"),
        );
        let mut proposal = make_proposal(ActionType::ReschedulePod, "Pod", "oom-pod")
            .with_parameters(params);

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(result.is_success());
        assert!(result.stages[0].message.contains("worker-2"));
    }

    #[tokio::test]
    async fn test_scale_deployment_missing_replicas() {
        let executor = DeterministicExecutor::new();
        let mut proposal = make_proposal(ActionType::ScaleDeployment, "Deployment", "my-app");

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(!result.is_success());
        assert!(result.stages[0].message.contains("Missing 'replicas'"));
    }

    #[tokio::test]
    async fn test_scale_deployment_success() {
        let executor = DeterministicExecutor::new();
        let mut params = HashMap::new();
        params.insert("replicas".to_string(), serde_json::json!(5));
        let mut proposal = make_proposal(ActionType::ScaleDeployment, "Deployment", "my-app")
            .with_parameters(params);

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_scale_deployment_excessive_replicas() {
        let executor = DeterministicExecutor::new();
        let mut params = HashMap::new();
        params.insert("replicas".to_string(), serde_json::json!(200));
        let mut proposal = make_proposal(ActionType::ScaleDeployment, "Deployment", "my-app")
            .with_parameters(params);

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(!result.is_success());
        assert!(result.stages[0].message.contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_cordon_node() {
        let executor = DeterministicExecutor::new();
        let mut proposal = make_proposal(ActionType::CordonNode, "Node", "worker-1");

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_drain_node() {
        let executor = DeterministicExecutor::new();
        let mut proposal = make_proposal(ActionType::DrainNode, "Node", "worker-3");

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_adjust_resources_needs_params() {
        let executor = DeterministicExecutor::new();
        let mut proposal = make_proposal(ActionType::AdjustResources, "Pod", "my-pod");

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(!result.is_success());
        assert!(result.stages[0]
            .message
            .contains("No resource adjustments"));
    }

    #[tokio::test]
    async fn test_adjust_resources_with_memory() {
        let executor = DeterministicExecutor::new();
        let mut params = HashMap::new();
        params.insert(
            "memory_limit".to_string(),
            serde_json::json!("2Gi"),
        );
        let mut proposal = make_proposal(ActionType::AdjustResources, "Pod", "my-pod")
            .with_parameters(params);

        let result = executor.execute(&mut proposal).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_execution_result_serde() {
        let result = ExecutionResult {
            proposal_id: "p-1".to_string(),
            final_stage: ExecutionStage::Completed,
            stages: vec![StageResult {
                stage: ExecutionStage::DryRun,
                success: true,
                message: "ok".to_string(),
                details: HashMap::new(),
            }],
            rolled_back: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.final_stage, ExecutionStage::Completed);
    }
}
