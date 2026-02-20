use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::info;

use aion_audit::{AuditLogger, IntegrityResult};
use aion_mount::registry::AgentRegistry;

/// Shared application state for API handlers.
pub struct AppState {
    pub audit_logger: Arc<AuditLogger>,
    pub agent_registry: Arc<AgentRegistry>,
}

/// Build the REST API router.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/status", get(get_status))
        .route("/api/v1/agents", get(get_agents))
        .route("/api/v1/audit", get(get_audit_log))
        .route("/api/v1/audit/verify", get(verify_audit_integrity))
        .route("/api/v1/proposals", get(get_proposals))
        .route("/api/v1/proposals/{id}/approve", post(approve_proposal))
        .route("/api/v1/trigger", post(trigger_analysis))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Response types ──

#[derive(Debug, Serialize, Deserialize)]
struct StatusResponse {
    status: String,
    version: String,
    uptime_secs: u64,
    agents_registered: usize,
    agents_enabled: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentResponse {
    id: String,
    kind: String,
    display_name: String,
    model: String,
    enabled: bool,
    priority: u32,
    specializations: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditEntryResponse {
    entry_id: String,
    sequence: u64,
    action: String,
    actor_type: String,
    actor_id: String,
    description: String,
    anomaly_id: Option<String>,
    proposal_id: Option<String>,
    timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TriggerRequest {
    event_type: String,
    namespace: Option<String>,
    target: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TriggerResponse {
    accepted: bool,
    message: String,
    anomaly_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApproveResponse {
    proposal_id: String,
    approved: bool,
    message: String,
}

// ── Handlers ──

async fn get_status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let agents = state.agent_registry.list();
    let enabled = agents.iter().filter(|a| a.enabled).count();

    Json(StatusResponse {
        status: "running".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: 0, // TODO: track actual uptime
        agents_registered: agents.len(),
        agents_enabled: enabled,
    })
}

async fn get_agents(State(state): State<Arc<AppState>>) -> Json<Vec<AgentResponse>> {
    let agents = state.agent_registry.list();

    Json(
        agents
            .iter()
            .map(|a| AgentResponse {
                id: a.id.to_string(),
                kind: format!("{}", a.kind),
                display_name: a.display_name.clone(),
                model: a.model.clone(),
                enabled: a.enabled,
                priority: a.priority,
                specializations: a.specializations.clone(),
                capabilities: a.capabilities.clone(),
            })
            .collect(),
    )
}

async fn get_audit_log(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AuditEntryResponse>> {
    let entries = state.audit_logger.entries();

    Json(
        entries
            .iter()
            .map(|e| AuditEntryResponse {
                entry_id: e.entry_id.clone(),
                sequence: e.sequence,
                action: format!("{:?}", e.action),
                actor_type: e.actor_type.clone(),
                actor_id: e.actor_id.clone(),
                description: e.description.clone(),
                anomaly_id: e.anomaly_id.clone(),
                proposal_id: e.proposal_id.clone(),
                timestamp: e.timestamp.to_rfc3339(),
            })
            .collect(),
    )
}

async fn verify_audit_integrity(
    State(state): State<Arc<AppState>>,
) -> Json<IntegrityResult> {
    Json(state.audit_logger.verify_integrity())
}

async fn get_proposals() -> Json<Vec<serde_json::Value>> {
    // TODO: integrate with proposal store
    Json(vec![])
}

async fn approve_proposal(
    Path(id): Path<String>,
) -> Json<ApproveResponse> {
    info!(proposal_id = %id, "Approval request received");

    // TODO: integrate with proposal store and pipeline
    Json(ApproveResponse {
        proposal_id: id,
        approved: true,
        message: "Proposal approval recorded (not yet integrated with pipeline)".to_string(),
    })
}

async fn trigger_analysis(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TriggerRequest>,
) -> Json<TriggerResponse> {
    info!(
        event_type = %request.event_type,
        namespace = ?request.namespace,
        target = ?request.target,
        "Manual analysis triggered"
    );

    let anomaly_id = uuid::Uuid::new_v4().to_string();

    state.audit_logger.log_detailed(
        aion_audit::AuditAction::EventDetected,
        "api",
        "manual-trigger",
        &request.description.unwrap_or_else(|| format!("Manual trigger: {}", request.event_type)),
        Some(anomaly_id.clone()),
        None,
        None,
    );

    Json(TriggerResponse {
        accepted: true,
        message: format!(
            "Analysis triggered for event type: {}",
            request.event_type
        ),
        anomaly_id: Some(anomaly_id),
    })
}

/// Start the REST API server on the given address.
pub async fn serve(addr: &str, state: Arc<AppState>) -> Result<(), std::io::Error> {
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(addr = %addr, "REST API server listening");
    axum::serve(listener, router).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let config_toml = r#"
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
specializations = ["oom_analysis"]
capabilities = ["infra_analysis"]

[budget.claude_code]
max_invocations_per_hour = 20
max_concurrent = 3
"#;
        let agents_config = aion_common::config::AgentsConfig::from_str(config_toml).unwrap();
        let registry = Arc::new(AgentRegistry::from_config(&agents_config));
        let audit_logger = Arc::new(AuditLogger::new("/tmp/aion-api-test-audit"));

        Arc::new(AppState {
            audit_logger,
            agent_registry: registry,
        })
    }

    #[tokio::test]
    async fn test_get_status() {
        let state = test_state();
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let status: StatusResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(status.status, "running");
        assert_eq!(status.agents_registered, 1);
        assert_eq!(status.agents_enabled, 1);
    }

    #[tokio::test]
    async fn test_get_agents() {
        let state = test_state();
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/agents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let agents: Vec<AgentResponse> = serde_json::from_slice(&body).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "claude-primary");
    }

    #[tokio::test]
    async fn test_get_audit_log() {
        let state = test_state();
        state.audit_logger.log(
            aion_audit::AuditAction::EventDetected,
            "system",
            "test",
            "test event",
        );

        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/audit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let entries: Vec<AuditEntryResponse> = serde_json::from_slice(&body).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actor_id, "test");
    }

    #[tokio::test]
    async fn test_verify_audit_integrity() {
        let state = test_state();
        state.audit_logger.log(
            aion_audit::AuditAction::EventDetected,
            "system",
            "test",
            "test event",
        );

        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/audit/verify")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result: IntegrityResult = serde_json::from_slice(&body).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.entries_verified, 1);
    }

    #[tokio::test]
    async fn test_trigger_analysis() {
        let state = test_state();
        let app = build_router(state);

        let trigger = TriggerRequest {
            event_type: "oom_kill".to_string(),
            namespace: Some("default".to_string()),
            target: Some("my-pod".to_string()),
            description: Some("Manual OOM trigger".to_string()),
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/trigger")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&trigger).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result: TriggerResponse = serde_json::from_slice(&body).unwrap();
        assert!(result.accepted);
        assert!(result.anomaly_id.is_some());
    }
}
