use anyhow::Context;
use serde_json::Value;

use crate::types::*;

/// A tool that writes content to a file, creating directories as needed.
///
/// # Chapter 9: File Tools — Write
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
            "TODO ch9: build a ToolDefinition for 'write' with required 'path' and 'content' params"
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
    async fn call(&self, _args: Value) -> anyhow::Result<String> {
        unimplemented!("TODO ch9: extract path+content, mkdir parents, write the file")
    }
}
