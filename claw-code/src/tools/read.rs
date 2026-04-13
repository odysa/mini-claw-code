use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct ReadTool {
    def: ToolDefinition,
}

impl ReadTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("read", "Read the contents of a file")
                .param("path", "string", "Absolute path to the file", true)
                .param("offset", "integer", "Line number to start reading from (1-based)", false)
                .param("limit", "integer", "Maximum number of lines to read", false),
        }
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read '{path}': {e}"))?;

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let start = offset.saturating_sub(1).min(total);

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(total);

        let end = (start + limit).min(total);
        let selected = &lines[start..end];

        let numbered: Vec<String> = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}\t{}", start + i + 1, line))
            .collect();

        Ok(ToolResult::text(numbered.join("\n")))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Reading file...".into())
    }
}
