use anyhow::Context;
use serde_json::Value;

use crate::types::*;

pub struct WriteTool {
    definition: ToolDefinition,
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteTool {
    pub fn new() -> Self {
        Self {
            definition: ToolDefinition::new(
                "write",
                "Write content to a file, creating directories as needed.",
            )
            .param("path", "string", "The file path to write to", true)
            .param(
                "content",
                "string",
                "The content to write to the file",
                true,
            ),
        }
    }
}

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"].as_str().context("missing 'path' argument")?;
        let content = args["content"]
            .as_str()
            .context("missing 'content' argument")?;

        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create directories for '{path}'"))?;
        }

        tokio::fs::write(path, content)
            .await
            .with_context(|| format!("failed to write '{path}'"))?;

        Ok(ToolResult::text(format!("wrote {path}")))
    }
}
