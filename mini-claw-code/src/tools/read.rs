use anyhow::Context;
use serde_json::Value;

use crate::types::*;

pub struct ReadTool {
    definition: ToolDefinition,
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadTool {
    pub fn new() -> Self {
        Self {
            definition: ToolDefinition::new("read", "Read the contents of a file.").param(
                "path",
                "string",
                "The file path to read",
                true,
            ),
        }
    }
}

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"].as_str().context("missing 'path' argument")?;
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read '{path}'"))?;
        Ok(ToolResult::text(content))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
}
