pub mod claude;
pub mod codex;
pub mod gemini;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use aion_common::types::AgentId;

/// The result of an agent invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent_id: AgentId,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

/// Context passed to the agent at launch time.
#[derive(Debug, Clone)]
pub struct LaunchContext {
    /// The prompt/instruction for the agent
    pub prompt: String,
    /// Path to the MCP server binary
    pub mcp_server_binary: String,
    /// Arguments for the MCP server
    pub mcp_server_args: Vec<String>,
    /// Environment variables to set
    pub env: HashMap<String, String>,
    /// Maximum execution time
    pub timeout: Duration,
    /// Working directory
    pub working_dir: Option<String>,
}

/// Trait for launching AI agent subprocesses.
#[async_trait]
pub trait AgentLauncher: Send + Sync {
    /// Human-readable name of this launcher type.
    fn name(&self) -> &str;

    /// Launch the agent and wait for completion.
    async fn launch(
        &self,
        binary_path: &str,
        model: &str,
        context: &LaunchContext,
    ) -> Result<AgentOutput, LaunchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("binary not found: {0}")]
    BinaryNotFound(String),

    #[error("agent timed out after {0:?}")]
    Timeout(Duration),

    #[error("agent failed with exit code {code}: {stderr}")]
    ExitError { code: i32, stderr: String },

    #[error("failed to spawn process: {0}")]
    SpawnError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
