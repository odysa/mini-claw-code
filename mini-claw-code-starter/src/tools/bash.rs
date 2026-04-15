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
        unimplemented!("Create ToolDefinition with name 'bash', description, and a required 'command' string parameter")
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
        unimplemented!("Extract 'command', run via tokio::process::Command bash -c, capture stdout/stderr, return combined output")
    }
}
