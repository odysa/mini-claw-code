use anyhow::Context;
use serde_json::Value;

use crate::types::*;

/// A tool that reads the contents of a file.
///
/// # Chapter 2: Your First Tool
///
/// Each tool has three parts:
/// - A `ToolDefinition` describing its name, description, and JSON schema parameters
/// - A `definition()` method that returns a reference to the definition
/// - A `call()` method that executes the tool with given arguments
pub struct ReadTool {
    definition: ToolDefinition,
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadTool {
    /// Create a new ReadTool with its JSON schema definition.
    ///
    /// The schema should declare one required parameter: "path" (string).
    /// Use `ToolDefinition::new("read", "Read the contents of a file.").param(...)`.
    pub fn new() -> Self {
        unimplemented!(
            "Create ToolDefinition with name 'read', description, and a required 'path' string parameter"
        )
    }
}

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Read a file and return its contents.
    ///
    /// Hints:
    /// - Extract path: `args["path"].as_str().context("missing 'path' argument")?`
    /// - Read file: `tokio::fs::read_to_string(path).await.with_context(|| ...)?`
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "Extract 'path' from args, read file with tokio::fs::read_to_string, return contents"
        )
    }
}
