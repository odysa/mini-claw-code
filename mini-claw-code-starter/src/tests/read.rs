use crate::tools::ReadTool;
use crate::types::*;
use serde_json::json;

#[test]
fn test_read_read_definition() {
    let tool = ReadTool::new();
    let def = tool.definition();
    assert_eq!(def.name, "read");
    assert!(!def.description.is_empty());
    let required = def.parameters["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "path"));
}

#[tokio::test]
async fn test_read_read_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.txt");
    std::fs::write(&path, "hello world").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap()}))
        .await
        .unwrap();

    assert_eq!(result, "hello world");
}

#[tokio::test]
async fn test_read_read_missing_file() {
    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": "/tmp/__mini_claw_code_nonexistent_test_file__.txt"}))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_read_read_missing_arg() {
    let tool = ReadTool::new();
    let result = tool.call(json!({})).await;

    assert!(result.is_err());
}

// --- New tests ---

#[test]
fn test_read_read_definition_has_path_property() {
    let tool = ReadTool::new();
    let def = tool.definition();
    let props = &def.parameters["properties"];
    assert!(props["path"].is_object());
    assert_eq!(props["path"]["type"], "string");
}

#[test]
fn test_read_read_default() {
    // ReadTool implements Default
    let tool = ReadTool::default();
    assert_eq!(tool.definition().name, "read");
}

#[tokio::test]
async fn test_read_read_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, "").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap()}))
        .await
        .unwrap();

    assert_eq!(result, "");
}

#[tokio::test]
async fn test_read_read_multiline_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multi.txt");
    std::fs::write(&path, "line1\nline2\nline3").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap()}))
        .await
        .unwrap();

    assert_eq!(result, "line1\nline2\nline3");
}

#[tokio::test]
async fn test_read_read_unicode_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.txt");
    std::fs::write(&path, "hello\nworld").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap()}))
        .await
        .unwrap();

    assert!(result.contains("hello"));
    assert!(result.contains("world"));
}

#[tokio::test]
async fn test_read_read_large_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.txt");
    let content = "x".repeat(100_000);
    std::fs::write(&path, &content).unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap()}))
        .await
        .unwrap();

    assert_eq!(result.len(), 100_000);
}

#[tokio::test]
async fn test_read_read_wrong_arg_type() {
    let tool = ReadTool::new();
    // path is a number, not a string
    let result = tool.call(json!({"path": 42})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_read_read_null_path() {
    let tool = ReadTool::new();
    let result = tool.call(json!({"path": null})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_read_read_extra_args_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extra.txt");
    std::fs::write(&path, "content").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap(), "extra": "ignored"}))
        .await
        .unwrap();

    assert_eq!(result, "content");
}

#[tokio::test]
async fn test_read_read_preserves_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ws.txt");
    std::fs::write(&path, "  leading\n\ttab\ntrailing  \n").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap()}))
        .await
        .unwrap();

    assert_eq!(result, "  leading\n\ttab\ntrailing  \n");
}

#[tokio::test]
async fn test_read_read_directory_fails() {
    let dir = tempfile::tempdir().unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({"path": dir.path().to_str().unwrap()}))
        .await;

    assert!(result.is_err());
}
