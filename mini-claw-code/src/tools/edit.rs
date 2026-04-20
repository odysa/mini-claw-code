use anyhow::{Context, bail};
use serde_json::Value;

use crate::types::*;

pub struct EditTool {
    definition: ToolDefinition,
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EditTool {
    pub fn new() -> Self {
        Self {
            definition: ToolDefinition::new(
                "edit",
                "Replace an exact string in a file (must appear exactly once).",
            )
            .param("path", "string", "The file path to edit", true)
            .param(
                "old_string",
                "string",
                "The exact string to find and replace",
                true,
            )
            .param("new_string", "string", "The replacement string", true),
        }
    }
}

#[async_trait::async_trait]
impl Tool for EditTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"].as_str().context("missing 'path' argument")?;
        let old = args["old_string"]
            .as_str()
            .context("missing 'old_string' argument")?;
        let new = args["new_string"]
            .as_str()
            .context("missing 'new_string' argument")?;

        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read '{path}'"))?;

        let count = content.matches(old).count();
        if count == 0 {
            bail!("old_string not found in '{path}'");
        }
        if count > 1 {
            bail!("old_string appears {count} times in '{path}', must be unique");
        }

        let updated = content.replacen(old, new, 1);
        tokio::fs::write(path, updated)
            .await
            .with_context(|| format!("failed to write '{path}'"))?;

        Ok(ToolResult::text(format!("edited {path}")))
    }
}
