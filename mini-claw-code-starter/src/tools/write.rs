use anyhow::Context;
use serde_json::Value;

use crate::types::*;

/// A tool that writes content to a file, creating directories as needed.
///
/// # Chapter 4: More Tools — Write
pub struct WriteTool {
    definition: ToolDefinition,
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteTool {
    /// Create a new WriteTool. Schema: required "path" and "content" parameters.
    pub fn new() -> Self {
        unimplemented!(
            "Create ToolDefinition with name 'write', description, and required 'path' and 'content' string parameters"
        )
    }
}

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Write content to a file, creating parent directories as needed.
    ///
    /// Hints:
    /// - Extract "path" and "content" from args
    /// - Create parent dirs: `tokio::fs::create_dir_all(parent).await?`
    /// - Write file: `tokio::fs::write(path, content).await?`
    /// - Return confirmation: `format!("wrote {path}")`
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "Extract 'path' and 'content' from args, create parent dirs with tokio::fs::create_dir_all, write file, return confirmation"
        )
    }
}
