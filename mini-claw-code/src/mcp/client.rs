use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::types::*;
use crate::types::ToolDefinition;

/// A client that communicates with an MCP server over stdio (JSON-RPC 2.0).
pub struct McpClient {
    stdin: Mutex<tokio::process::ChildStdin>,
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
    _child: Mutex<Child>,
    next_id: AtomicU64,
    server_name: String,
}

impl McpClient {
    /// Spawn an MCP server process and perform the initialize handshake.
    pub async fn connect(
        server_name: impl Into<String>,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<Self> {
        let server_name = server_name.into();

        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn MCP server: {command}"))?;

        let stdin = child
            .stdin
            .take()
            .context("failed to get stdin of MCP server")?;
        let stdout = child
            .stdout
            .take()
            .context("failed to get stdout of MCP server")?;

        let client = Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            _child: Mutex::new(child),
            next_id: AtomicU64::new(1),
            server_name,
        };

        // Perform the initialize handshake
        client.initialize().await?;

        Ok(client)
    }

    /// Send a JSON-RPC request and read the response.
    async fn request(&self, method: &str, params: Option<Value>) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest::new(id, method, params);

        let mut payload = serde_json::to_string(&request)?;
        payload.push('\n');

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(payload.as_bytes())
                .await
                .context("failed to write to MCP server")?;
            stdin
                .flush()
                .await
                .context("failed to flush MCP server stdin")?;
        }

        // Read response
        let mut line = String::new();
        {
            let mut stdout = self.stdout.lock().await;
            // Skip empty lines and notifications
            loop {
                line.clear();
                let bytes_read = stdout
                    .read_line(&mut line)
                    .await
                    .context("failed to read from MCP server")?;
                if bytes_read == 0 {
                    anyhow::bail!("MCP server closed stdout unexpectedly");
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Try to parse as a response
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(trimmed) {
                    if let Some(error) = resp.error {
                        anyhow::bail!("MCP server error ({}): {}", error.code, error.message);
                    }
                    return Ok(resp.result.unwrap_or(Value::Null));
                }
                // Not a response (might be a notification), skip
            }
        }
    }

    /// Perform the MCP initialize handshake.
    async fn initialize(&self) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "mini-claw-code",
                "version": "0.1.0"
            }
        });

        let result = self.request("initialize", Some(params)).await?;
        let _: InitializeResult =
            serde_json::from_value(result).context("failed to parse initialize response")?;

        // Send initialized notification (no response expected, but send as request for simplicity)
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let notification = JsonRpcRequest::new(id, "notifications/initialized", None);
        let mut payload = serde_json::to_string(&notification)?;
        payload.push('\n');

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(payload.as_bytes()).await?;
        stdin.flush().await?;

        Ok(())
    }

    /// List tools available on the MCP server.
    pub async fn list_tools(&self) -> anyhow::Result<Vec<McpToolDef>> {
        let result = self.request("tools/list", None).await?;
        let list: ToolsListResult =
            serde_json::from_value(result).context("failed to parse tools/list response")?;
        Ok(list.tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> anyhow::Result<String> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let result = self.request("tools/call", Some(params)).await?;
        let call_result: ToolCallResult =
            serde_json::from_value(result).context("failed to parse tools/call response")?;

        let text: Vec<String> = call_result
            .content
            .into_iter()
            .filter_map(|c| c.text)
            .collect();

        Ok(text.join("\n"))
    }

    /// Convert MCP tool definitions to our `ToolDefinition` format.
    ///
    /// Since `ToolDefinition` uses `&'static str` for name/description,
    /// the strings are leaked. This is acceptable for long-lived tools
    /// loaded at startup.
    pub fn convert_tool_defs(tools: &[McpToolDef], prefix: &str) -> Vec<ToolDefinition> {
        tools
            .iter()
            .map(|t| {
                let name = format!("mcp__{prefix}__{}", t.name);
                let desc = t
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("MCP tool: {}", t.name));
                let params = t
                    .input_schema
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

                // Leak strings for 'static lifetime (loaded once at startup)
                let name: &'static str = Box::leak(name.into_boxed_str());
                let desc: &'static str = Box::leak(desc.into_boxed_str());

                ToolDefinition {
                    name,
                    description: desc,
                    parameters: params,
                }
            })
            .collect()
    }

    /// The name of this MCP server.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
}
