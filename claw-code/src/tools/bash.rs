use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct BashTool {
    def: ToolDefinition,
}

impl BashTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("bash", "Run a bash command and return its output")
                .param("command", "string", "The bash command to run", true)
                .param(
                    "timeout",
                    "integer",
                    "Timeout in seconds (default: 120)",
                    false,
                ),
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;

        let timeout_secs = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(120);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await;

        let output = match output {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Ok(ToolResult::error(format!("failed to run command: {e}"))),
            Err(_) => {
                return Ok(ToolResult::error(format!(
                    "command timed out after {timeout_secs}s"
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("stderr: ");
            result.push_str(&stderr);
        }

        if exit_code != 0 {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("exit code: {exit_code}"));
        }

        if result.is_empty() {
            result.push_str("(no output)");
        }

        Ok(ToolResult::text(result))
    }

    fn is_destructive(&self) -> bool {
        true
    }

    fn summary(&self, args: &Value) -> String {
        match args.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => {
                let short = if cmd.len() > 60 {
                    format!("{}...", &cmd[..57])
                } else {
                    cmd.to_string()
                };
                format!("[bash: {short}]")
            }
            None => "[bash]".into(),
        }
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Running command...".into())
    }
}
