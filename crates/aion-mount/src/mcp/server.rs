use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::ServerInfo;
use rmcp::ServerHandler;

use super::tools::read_only::*;
use super::tools::proposal::*;
use super::tools::execution::*;

/// AION MCP Server handler exposing infrastructure tools to AI agents.
///
/// The server exposes three categories of tools:
/// - Read-only: get_anomaly_context, get_cluster_overview, get_node_status, etc.
/// - Proposal: propose_action
/// - Execution: execute_action (Low risk only)
pub struct AionMcpServer {
    tool_router: ToolRouter<Self>,
    invocation_token: String,
}

#[rmcp::tool_router]
impl AionMcpServer {
    // ── Read-only tools ──

    /// Get the full context of an anomaly including event details,
    /// affected resources, and timeline.
    #[rmcp::tool(name = "get_anomaly_context")]
    async fn get_anomaly_context(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetAnomalyContextInput>,
    ) -> String {
        // In production, this queries the event store.
        // For MVP, return mock data that demonstrates the interface.
        let ctx = AnomalyContext {
            anomaly_id: input.anomaly_id.clone(),
            event_type: "oom_kill".to_string(),
            severity: "critical".to_string(),
            description: format!("OOM kill detected for anomaly {}", input.anomaly_id),
            affected_pod: Some("worker-pod-abc123".to_string()),
            affected_node: Some("worker-1".to_string()),
            namespace: Some("default".to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            details: serde_json::json!({
                "killed_pid": 12345,
                "memory_limit_bytes": 536870912,
                "memory_usage_bytes": 536870000,
                "oom_score_adj": 1000,
            }),
        };
        serde_json::to_string_pretty(&ctx).unwrap_or_else(|_| "Error serializing context".to_string())
    }

    /// Get an overview of the Kubernetes cluster status including
    /// node count, pod status, and overall health.
    #[rmcp::tool(name = "get_cluster_overview")]
    async fn get_cluster_overview(
        &self,
        rmcp::handler::server::wrapper::Parameters(_input): rmcp::handler::server::wrapper::Parameters<GetClusterOverviewInput>,
    ) -> String {
        let overview = ClusterOverview {
            total_nodes: 3,
            ready_nodes: 3,
            total_pods: 42,
            running_pods: 38,
            pending_pods: 2,
            failed_pods: 2,
        };
        serde_json::to_string_pretty(&overview).unwrap_or_else(|_| "Error".to_string())
    }

    /// Get detailed status of a specific Kubernetes node including
    /// resource capacity, allocatable resources, and conditions.
    #[rmcp::tool(name = "get_node_status")]
    async fn get_node_status(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetNodeStatusInput>,
    ) -> String {
        let status = NodeStatus {
            name: input.node_name.clone(),
            ready: true,
            cpu_capacity: "8".to_string(),
            memory_capacity: "32Gi".to_string(),
            cpu_allocatable: "7800m".to_string(),
            memory_allocatable: "30Gi".to_string(),
            pod_count: 15,
            conditions: vec!["Ready".to_string()],
        };
        serde_json::to_string_pretty(&status).unwrap_or_else(|_| "Error".to_string())
    }

    /// Get resource metrics for a specific pod including CPU usage,
    /// memory usage, and restart count.
    #[rmcp::tool(name = "get_pod_metrics")]
    async fn get_pod_metrics(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetPodMetricsInput>,
    ) -> String {
        let metrics = PodMetrics {
            name: input.pod_name.clone(),
            namespace: input.namespace.clone(),
            cpu_usage: "250m".to_string(),
            memory_usage: "512Mi".to_string(),
            restart_count: 3,
            status: "Running".to_string(),
            node_name: "worker-1".to_string(),
        };
        serde_json::to_string_pretty(&metrics).unwrap_or_else(|_| "Error".to_string())
    }

    /// Get recent OOM kill events, optionally filtered by namespace.
    #[rmcp::tool(name = "get_oom_events")]
    async fn get_oom_events(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetOomEventsInput>,
    ) -> String {
        let events = vec![OomEvent {
            pod_name: "worker-pod-abc123".to_string(),
            namespace: input.namespace.unwrap_or_else(|| "default".to_string()),
            container: "main".to_string(),
            killed_pid: 12345,
            memory_limit: "512Mi".to_string(),
            memory_usage: "511Mi".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            node_name: "worker-1".to_string(),
        }];
        serde_json::to_string_pretty(&events).unwrap_or_else(|_| "Error".to_string())
    }

    /// Get Kubernetes events for a specific pod.
    #[rmcp::tool(name = "get_pod_events")]
    async fn get_pod_events(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetPodEventsInput>,
    ) -> String {
        let events = vec![
            PodEvent {
                event_type: "Warning".to_string(),
                reason: "OOMKilled".to_string(),
                message: format!(
                    "Container in pod {}/{} was OOM killed",
                    input.namespace, input.pod_name
                ),
                timestamp: chrono::Utc::now().to_rfc3339(),
                count: 1,
            },
        ];
        serde_json::to_string_pretty(&events).unwrap_or_else(|_| "Error".to_string())
    }

    /// Get resource utilization for a node or the whole cluster.
    #[rmcp::tool(name = "get_resource_utilization")]
    async fn get_resource_utilization(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetResourceUtilizationInput>,
    ) -> String {
        let util = ResourceUtilization {
            node_name: input.node_name.unwrap_or_else(|| "worker-1".to_string()),
            cpu_usage_percent: 65.2,
            memory_usage_percent: 82.5,
            disk_usage_percent: 45.0,
            pod_count: 15,
            pod_capacity: 110,
        };
        serde_json::to_string_pretty(&util).unwrap_or_else(|_| "Error".to_string())
    }

    /// Get the status of a Kubernetes deployment.
    #[rmcp::tool(name = "get_deployment_status")]
    async fn get_deployment_status(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<GetDeploymentStatusInput>,
    ) -> String {
        let status = DeploymentStatus {
            name: input.deployment_name.clone(),
            namespace: input.namespace.clone(),
            desired_replicas: 3,
            ready_replicas: 2,
            available_replicas: 2,
            updated_replicas: 3,
            conditions: vec!["Available".to_string(), "Progressing".to_string()],
        };
        serde_json::to_string_pretty(&status).unwrap_or_else(|_| "Error".to_string())
    }

    // ── Proposal tool ──

    /// Submit a structured remediation proposal. The agent should call this
    /// after analyzing the anomaly and determining the best course of action.
    #[rmcp::tool(name = "propose_action")]
    async fn propose_action(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<ProposeActionInput>,
    ) -> String {
        let result = ProposeActionResult {
            proposal_id: uuid::Uuid::new_v4().to_string(),
            status: "pending".to_string(),
            risk_level: match input.action_type.as_str() {
                "restart_pod" | "adjust_resources" => "low",
                "reschedule_pod" | "scale_deployment" => "medium",
                "cordon_node" | "drain_node" | "uncordon_node" => "high",
                _ => "unknown",
            }.to_string(),
            message: format!(
                "Proposal submitted: {} on {}/{} in {}",
                input.action_type, input.target_kind, input.target_name, input.target_namespace
            ),
        };
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error".to_string())
    }

    // ── Execution tool ──

    /// Directly execute a previously approved low-risk proposal.
    /// This tool is only available when the CapabilityToken allows direct execution.
    #[rmcp::tool(name = "execute_action")]
    async fn execute_action(
        &self,
        rmcp::handler::server::wrapper::Parameters(input): rmcp::handler::server::wrapper::Parameters<ExecuteActionInput>,
    ) -> String {
        let dry_run = input.dry_run.unwrap_or(false);
        let result = ExecuteActionResult {
            proposal_id: input.proposal_id,
            executed: !dry_run,
            stage: if dry_run { "dry_run".to_string() } else { "completed".to_string() },
            message: if dry_run {
                "Dry run completed successfully".to_string()
            } else {
                "Action executed successfully".to_string()
            },
        };
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error".to_string())
    }
}

#[rmcp::tool_handler]
impl ServerHandler for AionMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "AION Infrastructure MCP Server. Use the available tools to inspect \
                infrastructure state and propose remediation actions."
                    .into(),
            ),
            ..Default::default()
        }
    }
}

impl AionMcpServer {
    pub fn new(invocation_token: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            invocation_token,
        }
    }

    pub fn invocation_token(&self) -> &str {
        &self.invocation_token
    }
}
