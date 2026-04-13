use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A JSON-RPC 2.0 request.
#[derive(Serialize)]
pub(crate) struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response.
#[derive(Deserialize)]
pub(crate) struct JsonRpcResponse {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[allow(dead_code)]
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error.
#[derive(Deserialize, Debug)]
pub(crate) struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

/// An MCP tool definition from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Option<Value>,
}

/// The result of the `initialize` handshake.
#[derive(Deserialize)]
pub(crate) struct InitializeResult {
    #[allow(dead_code)]
    pub capabilities: Option<Value>,
}

/// The result of `tools/list`.
#[derive(Deserialize)]
pub(crate) struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
}

/// The result of `tools/call`.
#[derive(Deserialize)]
pub(crate) struct ToolCallResult {
    pub content: Vec<ToolCallContent>,
}

#[derive(Deserialize)]
pub(crate) struct ToolCallContent {
    #[allow(dead_code)]
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub text: Option<String>,
}
