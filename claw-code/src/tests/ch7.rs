use serde_json::json;

use crate::tools::{BashTool, Tool};

#[tokio::test]
async fn test_ch7_bash_echo() {
    let tool = BashTool::new();
    let result = tool.call(json!({ "command": "echo hello" })).await.unwrap();
    assert!(result.content.trim().contains("hello"));
}

#[tokio::test]
async fn test_ch7_bash_exit_code() {
    let tool = BashTool::new();
    let result = tool.call(json!({ "command": "exit 42" })).await.unwrap();
    assert!(result.content.contains("exit code: 42"));
}

#[tokio::test]
async fn test_ch7_bash_stderr() {
    let tool = BashTool::new();
    let result = tool
        .call(json!({ "command": "echo oops >&2" }))
        .await
        .unwrap();
    assert!(result.content.contains("stderr:"));
    assert!(result.content.contains("oops"));
}

#[tokio::test]
async fn test_ch7_bash_stdout_and_stderr() {
    let tool = BashTool::new();
    let result = tool
        .call(json!({ "command": "echo out; echo err >&2" }))
        .await
        .unwrap();
    assert!(result.content.contains("out"));
    assert!(result.content.contains("stderr:"));
    assert!(result.content.contains("err"));
}

#[tokio::test]
async fn test_ch7_bash_no_output() {
    let tool = BashTool::new();
    let result = tool.call(json!({ "command": "true" })).await.unwrap();
    assert_eq!(result.content, "(no output)");
}

#[tokio::test]
async fn test_ch7_bash_timeout() {
    let tool = BashTool::new();
    let result = tool
        .call(json!({ "command": "sleep 10", "timeout": 1 }))
        .await
        .unwrap();
    assert!(result.content.contains("timed out"));
}

#[tokio::test]
async fn test_ch7_bash_is_destructive() {
    let tool = BashTool::new();
    assert!(tool.is_destructive());
    assert!(!tool.is_read_only());
    assert!(!tool.is_concurrent_safe());
}

#[test]
fn test_ch7_bash_definition() {
    let tool = BashTool::new();
    assert_eq!(tool.definition().name, "bash");
    assert!(tool.definition().description.contains("bash"));
}

#[test]
fn test_ch7_bash_summary() {
    let tool = BashTool::new();
    assert_eq!(
        tool.summary(&json!({"command": "ls -la"})),
        "[bash: ls -la]"
    );
}

#[tokio::test]
async fn test_ch7_bash_multiline() {
    let tool = BashTool::new();
    let result = tool
        .call(json!({ "command": "echo one; echo two; echo three" }))
        .await
        .unwrap();
    assert!(result.content.contains("one"));
    assert!(result.content.contains("two"));
    assert!(result.content.contains("three"));
}

// Integration: bash + file tools
#[tokio::test]
async fn test_ch7_bash_with_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");

    let tool = BashTool::new();
    let cmd = format!("echo 'created by bash' > {}", path.display());
    tool.call(json!({ "command": cmd })).await.unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("created by bash"));
}
