use crate::tools::BashTool;
use crate::types::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// BashTool
// ---------------------------------------------------------------------------

#[test]
fn test_bash_definition() {
    let tool = BashTool::new();
    let def = tool.definition();
    assert_eq!(def.name, "bash");
    assert!(!def.description.is_empty());
    let required = def.parameters["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "command"));
}

#[tokio::test]
async fn test_bash_runs_command() {
    let tool = BashTool::new();
    let result = tool.call(json!({"command": "echo hello"})).await.unwrap();
    assert!(result.content.contains("hello"));
}

#[tokio::test]
async fn test_bash_captures_stderr() {
    let tool = BashTool::new();
    let result = tool.call(json!({"command": "echo err >&2"})).await.unwrap();
    assert!(result.content.contains("err"));
}

#[tokio::test]
async fn test_bash_missing_arg() {
    let tool = BashTool::new();
    let result = tool.call(json!({})).await;
    assert!(result.is_err());
}

// --- New BashTool tests ---

#[test]
fn test_bash_default() {
    let tool = BashTool::default();
    assert_eq!(tool.definition().name, "bash");
}

#[tokio::test]
async fn test_bash_stdout_and_stderr() {
    let tool = BashTool::new();
    let result = tool
        .call(json!({"command": "echo out && echo err >&2"}))
        .await
        .unwrap();
    assert!(result.content.contains("out"));
    assert!(result.content.contains("stderr:"));
    assert!(result.content.contains("err"));
}

#[tokio::test]
async fn test_bash_no_output() {
    let tool = BashTool::new();
    let result = tool.call(json!({"command": "true"})).await.unwrap();
    assert_eq!(result.content, "(no output)");
}

#[tokio::test]
async fn test_bash_exit_code_nonzero() {
    // bash tool still returns output even for non-zero exit code
    let tool = BashTool::new();
    let result = tool
        .call(json!({"command": "echo fail && exit 1"}))
        .await
        .unwrap();
    assert!(result.content.contains("fail"));
}

#[tokio::test]
async fn test_bash_multiline_output() {
    let tool = BashTool::new();
    let result = tool
        .call(json!({"command": "echo line1 && echo line2 && echo line3"}))
        .await
        .unwrap();
    assert!(result.content.contains("line1"));
    assert!(result.content.contains("line2"));
    assert!(result.content.contains("line3"));
}

#[tokio::test]
async fn test_bash_wrong_arg_type() {
    let tool = BashTool::new();
    let result = tool.call(json!({"command": 123})).await;
    assert!(result.is_err());
}
