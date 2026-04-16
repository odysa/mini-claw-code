use anyhow::{Context, bail};
use serde_json::Value;

use crate::types::*;

/// A tool that replaces an exact string in a file (must appear exactly once).
///
/// # Chapter 4: More Tools — Edit
pub struct EditTool {
    definition: ToolDefinition,
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EditTool {
    /// Create a new EditTool. Schema: required "path", "old_string", "new_string" parameters.
    pub fn new() -> Self {
        Self {
            definition: ToolDefinition::new(
                "edit",
                "Replace an exact string in a file. The old_string must appear exactly once.",
            )
            .param("path", "string", "Absolute path to the file to edit", true)
            .param("old_string", "string", "The exact string to find", true)
            .param("new_string", "string", "The replacement string", true),
        }
    }
}

#[async_trait::async_trait]
impl Tool for EditTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Replace an exact string in a file (must appear exactly once).
    ///
    /// Hints:
    /// - Extract "path", "old_string", "new_string" from args
    /// - Read the file, count occurrences with `content.matches(old).count()`
    /// - If 0: `bail!("old_string not found in '{path}'")`
    /// - If >1: `bail!("old_string appears {count} times in '{path}', must be unique")`
    /// - Replace with `content.replacen(old, new, 1)`, write back
    /// - Return confirmation: `format!("edited {path}")`
    async fn call(&self, args: Value) -> anyhow::Result<String> {
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
        tokio::fs::write(path, &updated).await?;

        Ok(format!("edited {path}"))
    }
}
