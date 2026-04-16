use anyhow::Context;
use serde_json::Value;

use crate::types::*;

/// A tool that runs bash commands and returns their output.
///
/// # Chapter 4: More Tools — Bash
///
/// This tool runs a command via `bash -c` and captures stdout + stderr.
pub struct BashTool {
    definition: ToolDefinition,
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BashTool {
    /// Create a new BashTool. Schema: one required "command" parameter (string).
    pub fn new() -> Self {
        Self {
            definition: ToolDefinition::new("bash", "Run a bash command and return its output.")
                .param("command", "string", "The bash command to run", true),
        }
    }
}

#[async_trait::async_trait]
impl Tool for BashTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Run a bash command and return its output.
    ///
    /// Hints:
    /// - Extract "command" from args with `.as_str().context(...)?`
    /// - Run: `tokio::process::Command::new("bash").arg("-c").arg(cmd).output().await?`
    /// - Convert stdout/stderr: `String::from_utf8_lossy(&output.stdout)`
    /// - Build result: stdout first, then stderr prefixed with `"stderr: "`
    /// - If both empty, return `"(no output)"`
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let cmd = args["command"]
            .as_str()
            .context("missing 'command' argument")?;

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(cmd)
            .output()
            .await
            .with_context(|| format!("failed to run command: {cmd}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

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
        if result.is_empty() {
            result.push_str("(no output)");
        }

        Ok(result)
    }
}
