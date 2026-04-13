use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct WriteTool {
    def: ToolDefinition,
}

impl WriteTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new(
                "write",
                "Write content to a file, creating directories as needed",
            )
            .param("path", "string", "Absolute path to write to", true)
            .param("content", "string", "Content to write", true),
        }
    }
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

        // Create parent directories
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| anyhow::anyhow!("failed to create directories for '{path}': {e}"))?;
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write '{path}': {e}"))?;

        let bytes = content.len();
        Ok(ToolResult::text(format!("wrote {bytes} bytes to {path}")))
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Writing file...".into())
    }
}
