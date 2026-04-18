use crate::mcp::McpToolDef;
use crate::mcp::client::McpClient;
use serde_json::json;

#[test]
fn test_mcp_convert_tool_defs_empty() {
    let defs = McpClient::convert_tool_defs(&[], "test");
    assert!(defs.is_empty());
}

#[test]
fn test_mcp_convert_tool_defs_single() {
    let mcp_tools = vec![McpToolDef {
        name: "read_file".into(),
        description: Some("Read a file".into()),
        input_schema: Some(json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            }
        })),
    }];

    let defs = McpClient::convert_tool_defs(&mcp_tools, "fs");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "mcp__fs__read_file");
    assert_eq!(defs[0].description, "Read a file");
    assert!(defs[0].parameters.get("properties").is_some());
}

#[test]
fn test_mcp_convert_tool_defs_no_description() {
    let mcp_tools = vec![McpToolDef {
        name: "my_tool".into(),
        description: None,
        input_schema: None,
    }];

    let defs = McpClient::convert_tool_defs(&mcp_tools, "srv");
    assert_eq!(defs[0].name, "mcp__srv__my_tool");
    assert!(defs[0].description.contains("MCP tool"));
}

#[test]
fn test_mcp_convert_tool_defs_multiple() {
    let mcp_tools = vec![
        McpToolDef {
            name: "tool_a".into(),
            description: Some("Tool A".into()),
            input_schema: None,
        },
        McpToolDef {
            name: "tool_b".into(),
            description: Some("Tool B".into()),
            input_schema: None,
        },
    ];

    let defs = McpClient::convert_tool_defs(&mcp_tools, "test");
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0].name, "mcp__test__tool_a");
    assert_eq!(defs[1].name, "mcp__test__tool_b");
}

#[test]
fn test_mcp_mcp_tool_def_deserialize() {
    let json = json!({
        "name": "get_weather",
        "description": "Get weather for a location",
        "inputSchema": {
            "type": "object",
            "properties": {
                "location": {"type": "string"}
            },
            "required": ["location"]
        }
    });

    let def: McpToolDef = serde_json::from_value(json).unwrap();
    assert_eq!(def.name, "get_weather");
    assert_eq!(def.description.unwrap(), "Get weather for a location");
    assert!(def.input_schema.is_some());
}

#[test]
fn test_mcp_mcp_tool_def_minimal() {
    let json = json!({"name": "simple"});
    let def: McpToolDef = serde_json::from_value(json).unwrap();
    assert_eq!(def.name, "simple");
    assert!(def.description.is_none());
    assert!(def.input_schema.is_none());
}

// Note: Integration tests for McpClient::connect require a real MCP server
// process. These would typically be run in CI with a test server binary.
// For unit tests, we verify the protocol types and conversion logic.

#[test]
fn test_mcp_jsonrpc_request_format() {
    use crate::mcp::types::JsonRpcRequest;

    let req = JsonRpcRequest::new(1, "tools/list", None);
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["method"], "tools/list");
    assert!(json.get("params").is_none()); // skipped when None
}

#[test]
fn test_mcp_jsonrpc_request_with_params() {
    use crate::mcp::types::JsonRpcRequest;

    let params = json!({"name": "test", "arguments": {}});
    let req = JsonRpcRequest::new(42, "tools/call", Some(params.clone()));
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["params"], params);
}

#[test]
fn test_mcp_tool_call_result_deserialize() {
    use crate::mcp::types::ToolCallResult;

    let json = json!({
        "content": [
            {"type": "text", "text": "Hello from MCP"}
        ]
    });
    let result: ToolCallResult = serde_json::from_value(json).unwrap();
    assert_eq!(result.content.len(), 1);
    assert_eq!(result.content[0].text.as_deref(), Some("Hello from MCP"));
}
