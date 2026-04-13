use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct EditTool {
    def: ToolDefinition,
}

impl EditTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new(
                "edit",
                "Replace an exact string in a file. The old_string must appear exactly once.",
            )
            .param("path", "string", "Absolute path to the file to edit", true)
            .param("old_string", "string", "The exact string to find", true)
            .param("new_string", "string", "The replacement string", true),
        }
    }
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
        let old = args["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'old_string' argument"))?;
        let new = args["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'new_string' argument"))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read '{path}': {e}"))?;

        let count = content.matches(old).count();
        if count == 0 {
            return Ok(ToolResult::error(format!(
                "old_string not found in '{path}'"
            )));
        }
        if count > 1 {
            return Ok(ToolResult::error(format!(
                "old_string appears {count} times in '{path}', must be unique"
            )));
        }

        let updated = content.replacen(old, new, 1);
        tokio::fs::write(path, &updated)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write '{path}': {e}"))?;

        Ok(ToolResult::text(format!("edited {path}")))
    }

    fn validate_input(&self, args: &Value) -> ValidationResult {
        if args.get("old_string").and_then(|v| v.as_str()).is_none() {
            return ValidationResult::Error {
                message: "missing 'old_string' argument".into(),
                code: 400,
            };
        }
        if args.get("new_string").and_then(|v| v.as_str()).is_none() {
            return ValidationResult::Error {
                message: "missing 'new_string' argument".into(),
                code: 400,
            };
        }
        ValidationResult::Ok
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Editing file...".into())
    }
}
