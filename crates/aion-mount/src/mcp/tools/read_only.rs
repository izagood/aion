// Read-only MCP tools for infrastructure inspection.
// These tools are called by AI agents via MCP to gather context about anomalies.

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

/// Input for get_anomaly_context tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetAnomalyContextInput {
    /// The anomaly ID to retrieve context for.
    pub anomaly_id: String,
}

/// Anomaly context returned to the agent.
#[derive(Debug, Serialize)]
pub struct AnomalyContext {
    pub anomaly_id: String,
    pub event_type: String,
    pub severity: String,
    pub description: String,
    pub affected_pod: Option<String>,
    pub affected_node: Option<String>,
    pub namespace: Option<String>,
    pub timestamp: String,
    pub details: serde_json::Value,
}

/// Input for get_cluster_overview tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetClusterOverviewInput {}

/// Cluster overview data.
#[derive(Debug, Serialize)]
pub struct ClusterOverview {
    pub total_nodes: u32,
    pub ready_nodes: u32,
    pub total_pods: u32,
    pub running_pods: u32,
    pub pending_pods: u32,
    pub failed_pods: u32,
}

/// Input for get_node_status tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNodeStatusInput {
    /// Node name to query.
    pub node_name: String,
}

/// Node status information.
#[derive(Debug, Serialize)]
pub struct NodeStatus {
    pub name: String,
    pub ready: bool,
    pub cpu_capacity: String,
    pub memory_capacity: String,
    pub cpu_allocatable: String,
    pub memory_allocatable: String,
    pub pod_count: u32,
    pub conditions: Vec<String>,
}

/// Input for get_pod_metrics tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPodMetricsInput {
    /// Namespace of the pod.
    pub namespace: String,
    /// Pod name.
    pub pod_name: String,
}

/// Pod metrics data.
#[derive(Debug, Serialize)]
pub struct PodMetrics {
    pub name: String,
    pub namespace: String,
    pub cpu_usage: String,
    pub memory_usage: String,
    pub restart_count: u32,
    pub status: String,
    pub node_name: String,
}

/// Input for get_oom_events tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetOomEventsInput {
    /// Optional namespace filter.
    pub namespace: Option<String>,
    /// Maximum number of events to return.
    pub limit: Option<u32>,
}

/// OOM event data.
#[derive(Debug, Serialize)]
pub struct OomEvent {
    pub pod_name: String,
    pub namespace: String,
    pub container: String,
    pub killed_pid: u32,
    pub memory_limit: String,
    pub memory_usage: String,
    pub timestamp: String,
    pub node_name: String,
}

/// Input for get_pod_events tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPodEventsInput {
    /// Namespace of the pod.
    pub namespace: String,
    /// Pod name.
    pub pod_name: String,
}

/// Kubernetes event for a pod.
#[derive(Debug, Serialize)]
pub struct PodEvent {
    pub event_type: String,
    pub reason: String,
    pub message: String,
    pub timestamp: String,
    pub count: u32,
}

/// Input for get_resource_utilization tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetResourceUtilizationInput {
    /// Optional node name to filter by.
    pub node_name: Option<String>,
}

/// Resource utilization data.
#[derive(Debug, Serialize)]
pub struct ResourceUtilization {
    pub node_name: String,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub disk_usage_percent: f64,
    pub pod_count: u32,
    pub pod_capacity: u32,
}

/// Input for get_deployment_status tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDeploymentStatusInput {
    /// Namespace.
    pub namespace: String,
    /// Deployment name.
    pub deployment_name: String,
}

/// Deployment status.
#[derive(Debug, Serialize)]
pub struct DeploymentStatus {
    pub name: String,
    pub namespace: String,
    pub desired_replicas: u32,
    pub ready_replicas: u32,
    pub available_replicas: u32,
    pub updated_replicas: u32,
    pub conditions: Vec<String>,
}
