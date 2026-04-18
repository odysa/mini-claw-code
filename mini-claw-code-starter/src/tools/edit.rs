use anyhow::{Context, bail};
use serde_json::Value;

use crate::types::*;

/// A tool that replaces an exact string in a file (must appear exactly once).
///
/// # Chapter 9: File Tools — Edit
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
        unimplemented!(
            "TODO ch9: build a ToolDefinition for 'edit' with required path/old_string/new_string"
        )
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
    async fn call(&self, _args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "TODO ch9: read file, require exactly one match of old_string, replace, write"
        )
    }
}
