use anyhow::{Context, Result};
use reqwest::Client;

use crate::types::*;

pub struct AionClient {
    base_url: String,
    http: Client,
}

impl AionClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let http = Client::builder()
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to connect to {url}"))?;
        Self::handle_response(resp).await
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        resp: reqwest::Response,
    ) -> Result<T> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Server returned {status}: {body}");
        }
        resp.json::<T>()
            .await
            .context("Failed to parse server response")
    }

    pub async fn get_status(&self) -> Result<StatusResponse> {
        self.get("/api/v1/status").await
    }

    pub async fn get_agents(&self) -> Result<Vec<AgentResponse>> {
        self.get("/api/v1/agents").await
    }

    pub async fn get_audit_log(&self) -> Result<Vec<AuditEntryResponse>> {
        self.get("/api/v1/audit").await
    }

    pub async fn verify_audit(&self) -> Result<IntegrityResult> {
        self.get("/api/v1/audit/verify").await
    }

    pub async fn get_proposals(&self) -> Result<Vec<serde_json::Value>> {
        self.get("/api/v1/proposals").await
    }

    pub async fn approve_proposal(&self, id: &str) -> Result<ApproveResponse> {
        let url = format!("{}/api/v1/proposals/{}/approve", self.base_url, id);
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .with_context(|| format!("Failed to connect to {url}"))?;
        Self::handle_response(resp).await
    }

    pub async fn trigger(&self, request: &TriggerRequest) -> Result<TriggerResponse> {
        let url = format!("{}/api/v1/trigger", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(request)
            .send()
            .await
            .with_context(|| format!("Failed to connect to {url}"))?;
        Self::handle_response(resp).await
    }
}
