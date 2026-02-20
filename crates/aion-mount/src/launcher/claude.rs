use async_trait::async_trait;
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, info};

use aion_common::types::AgentId;

use super::{AgentLauncher, AgentOutput, LaunchContext, LaunchError};

/// Launches Claude Code as a subprocess.
///
/// Command format:
/// ```text
/// claude -p "<prompt>" \
///   --output-format json \
///   --allowedTools "mcp__aion__*" \
///   --mcp-config '{"mcpServers":{"aion":{"command":"<mcp_binary>","args":[...]}}}'
/// ```
pub struct ClaudeCodeLauncher;

#[async_trait]
impl AgentLauncher for ClaudeCodeLauncher {
    fn name(&self) -> &str {
        "claude-code"
    }

    async fn launch(
        &self,
        binary_path: &str,
        model: &str,
        context: &LaunchContext,
    ) -> Result<AgentOutput, LaunchError> {
        // Build MCP config JSON
        let mcp_config = serde_json::json!({
            "mcpServers": {
                "aion": {
                    "command": context.mcp_server_binary,
                    "args": context.mcp_server_args,
                }
            }
        });

        let mut cmd = Command::new(binary_path);
        cmd.arg("-p")
            .arg(&context.prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--allowedTools")
            .arg("mcp__aion__*")
            .arg("--model")
            .arg(model)
            .arg("--mcp-config")
            .arg(mcp_config.to_string());

        // Set environment variables
        for (key, value) in &context.env {
            cmd.env(key, value);
        }

        if let Some(ref dir) = context.working_dir {
            cmd.current_dir(dir);
        }

        info!(
            binary = binary_path,
            model = model,
            "Launching Claude Code agent"
        );

        let start = Instant::now();

        let output = tokio::time::timeout(context.timeout, cmd.output())
            .await
            .map_err(|_| LaunchError::Timeout(context.timeout))?
            .map_err(|e| LaunchError::SpawnError(e.to_string()))?;

        let duration = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        debug!(
            exit_code = exit_code,
            duration_ms = duration.as_millis() as u64,
            "Claude Code agent completed"
        );

        Ok(AgentOutput {
            agent_id: AgentId::new("claude"),
            exit_code,
            stdout,
            stderr,
            duration,
        })
    }
}
