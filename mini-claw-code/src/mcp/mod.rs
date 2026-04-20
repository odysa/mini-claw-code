pub(crate) mod client;
pub(crate) mod types;

pub use client::McpClient;
pub use types::McpToolDef;

use async_trait::async_trait;
use serde_json::Value;

use crate::types::{Tool, ToolDefinition, ToolResult};

/// Wraps a remote MCP tool as a local `Tool` implementation.
///
/// When `call()` is invoked, the arguments are forwarded to the MCP server
/// via the client, and the result is returned.
pub struct McpTool {
    client: std::sync::Arc<McpClient>,
    definition: ToolDefinition,
    remote_name: String,
}

impl McpTool {
    /// Create a new MCP tool wrapper.
    pub fn new(
        client: std::sync::Arc<McpClient>,
        remote_name: String,
        definition: ToolDefinition,
    ) -> Self {
        Self {
            client,
            definition,
            remote_name,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let content = self.client.call_tool(&self.remote_name, args).await?;
        Ok(ToolResult::text(content))
    }
}
