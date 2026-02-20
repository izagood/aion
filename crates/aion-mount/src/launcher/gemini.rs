use async_trait::async_trait;
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, info};

use aion_common::types::AgentId;

use super::{AgentLauncher, AgentOutput, LaunchContext, LaunchError};

/// Launches Gemini CLI as a subprocess.
///
/// Command format:
/// ```text
/// gemini -p "<prompt>" --json
/// ```
pub struct GeminiCliLauncher;

#[async_trait]
impl AgentLauncher for GeminiCliLauncher {
    fn name(&self) -> &str {
        "gemini-cli"
    }

    async fn launch(
        &self,
        binary_path: &str,
        model: &str,
        context: &LaunchContext,
    ) -> Result<AgentOutput, LaunchError> {
        let mut cmd = Command::new(binary_path);
        cmd.arg("-p")
            .arg(&context.prompt)
            .arg("--json")
            .arg("--model")
            .arg(model);

        for (key, value) in &context.env {
            cmd.env(key, value);
        }

        if let Some(ref dir) = context.working_dir {
            cmd.current_dir(dir);
        }

        info!(
            binary = binary_path,
            model = model,
            "Launching Gemini CLI agent"
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
            "Gemini CLI agent completed"
        );

        Ok(AgentOutput {
            agent_id: AgentId::new("gemini"),
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
        })
    }
}
