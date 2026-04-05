use serde_json::Value;

use crate::types::*;

// --- Tool trait and ToolSet ---

struct EchoTool {
    def: ToolDefinition,
}

impl EchoTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition::new("echo", "Echo the input").param(
                "text",
                "string",
                "Text to echo",
                true,
            ),
        }
    }
}

#[async_trait::async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let text = args["text"].as_str().unwrap_or("(no text)");
        Ok(ToolResult::text(text))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
}

#[test]
fn test_ch3_tool_definition() {
    let tool = EchoTool::new();
    let def = tool.definition();
    assert_eq!(def.name, "echo");
    assert_eq!(def.description, "Echo the input");
}

#[tokio::test]
async fn test_ch3_tool_call() {
    let tool = EchoTool::new();
    let result = tool
        .call(serde_json::json!({"text": "hello"}))
        .await
        .unwrap();
    assert_eq!(result.content, "hello");
}

#[test]
fn test_ch3_tool_is_read_only() {
    let tool = EchoTool::new();
    assert!(tool.is_read_only());
    assert!(tool.is_concurrent_safe());
    assert!(!tool.is_destructive());
}

#[test]
fn test_ch3_tool_summary() {
    let tool = EchoTool::new();
    let summary = tool.summary(&serde_json::json!({"text": "hello"}));
    assert!(summary.contains("echo"));
}

#[test]
fn test_ch3_tool_default_validation() {
    let tool = EchoTool::new();
    assert!(matches!(
        tool.validate_input(&serde_json::json!({})),
        ValidationResult::Ok
    ));
}

#[test]
fn test_ch3_toolset_register_and_get() {
    let ts = ToolSet::new().with(EchoTool::new());
    assert_eq!(ts.len(), 1);
    assert!(!ts.is_empty());
    assert!(ts.get("echo").is_some());
    assert!(ts.get("nonexistent").is_none());
}

#[test]
fn test_ch3_toolset_definitions() {
    let ts = ToolSet::new().with(EchoTool::new());
    let defs = ts.definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "echo");
}

#[test]
fn test_ch3_toolset_names() {
    let ts = ToolSet::new().with(EchoTool::new());
    let names = ts.names();
    assert!(names.contains(&"echo"));
}

#[test]
fn test_ch3_toolset_push() {
    let mut ts = ToolSet::new();
    assert!(ts.is_empty());
    ts.push(EchoTool::new());
    assert_eq!(ts.len(), 1);
}
