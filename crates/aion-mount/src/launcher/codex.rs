use async_trait::async_trait;
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, info};

use aion_common::types::AgentId;

use super::{AgentLauncher, AgentOutput, LaunchContext, LaunchError};

/// Launches Codex CLI as a subprocess.
///
/// Command format:
/// ```text
/// codex --quiet --full-auto \
///   --model <model> \
///   "<prompt>"
/// ```
pub struct CodexCliLauncher;

#[async_trait]
impl AgentLauncher for CodexCliLauncher {
    fn name(&self) -> &str {
        "codex-cli"
    }

    async fn launch(
        &self,
        binary_path: &str,
        model: &str,
        context: &LaunchContext,
    ) -> Result<AgentOutput, LaunchError> {
        let mut cmd = Command::new(binary_path);
        cmd.arg("--quiet")
            .arg("--full-auto")
            .arg("--model")
            .arg(model)
            .arg(&context.prompt);

        for (key, value) in &context.env {
            cmd.env(key, value);
        }

        if let Some(ref dir) = context.working_dir {
            cmd.current_dir(dir);
        }

        info!(
            binary = binary_path,
            model = model,
            "Launching Codex CLI agent"
        );

        let start = Instant::now();

        let output = tokio::time::timeout(context.timeout, cmd.output())
            .await
            .map_err(|_| LaunchError::Timeout(context.timeout))?
            .map_err(|e| LaunchError::SpawnError(e.to_string()))?;

        let duration = start.elapsed();

        debug!(
            exit_code = output.status.code().unwrap_or(-1),
            duration_ms = duration.as_millis() as u64,
            "Codex CLI agent completed"
        );

        Ok(AgentOutput {
            agent_id: AgentId::new("codex"),
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
        })
    }
}
