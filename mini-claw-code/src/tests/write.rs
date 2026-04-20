use crate::tools::WriteTool;
use crate::types::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// WriteTool
// ---------------------------------------------------------------------------

#[test]
fn test_write_definition() {
    let tool = WriteTool::new();
    let def = tool.definition();
    assert_eq!(def.name, "write");
    assert!(!def.description.is_empty());
}

#[tokio::test]
async fn test_write_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");

    let tool = WriteTool::new();
    tool.call(json!({"path": path.to_str().unwrap(), "content": "hello"}))
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
}

#[tokio::test]
async fn test_write_creates_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a/b/c/out.txt");

    let tool = WriteTool::new();
    tool.call(json!({"path": path.to_str().unwrap(), "content": "deep"}))
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "deep");
}

#[tokio::test]
async fn test_write_missing_arg() {
    let tool = WriteTool::new();
    let result = tool.call(json!({"path": "/tmp/x.txt"})).await;
    assert!(result.is_err());
}

// --- New WriteTool tests ---

#[test]
fn test_write_default() {
    let tool = WriteTool::default();
    assert_eq!(tool.definition().name, "write");
}

#[test]
fn test_write_definition_required_params() {
    let tool = WriteTool::new();
    let def = tool.definition();
    let required = def.parameters["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "path"));
    assert!(required.iter().any(|v| v == "content"));
}

#[tokio::test]
async fn test_write_overwrites_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("overwrite.txt");
    std::fs::write(&path, "old content").unwrap();

    let tool = WriteTool::new();
    tool.call(json!({"path": path.to_str().unwrap(), "content": "new content"}))
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
}

#[tokio::test]
async fn test_write_empty_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");

    let tool = WriteTool::new();
    tool.call(json!({"path": path.to_str().unwrap(), "content": ""}))
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
}

#[tokio::test]
async fn test_write_returns_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("confirm.txt");

    let tool = WriteTool::new();
    let result = tool
        .call(json!({"path": path.to_str().unwrap(), "content": "data"}))
        .await
        .unwrap();

    assert!(result.content.contains("wrote"));
    assert!(result.content.contains(path.to_str().unwrap()));
}

#[tokio::test]
async fn test_write_missing_path() {
    let tool = WriteTool::new();
    let result = tool.call(json!({"content": "data"})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_write_multiline_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multi.txt");

    let tool = WriteTool::new();
    tool.call(json!({"path": path.to_str().unwrap(), "content": "line1\nline2\nline3"}))
        .await
        .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "line1\nline2\nline3");
}
