use serde_json::json;
use std::io::Write;

use crate::tools::{EditTool, ReadTool, Tool, WriteTool};

// ── ReadTool ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_ch6_read_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.txt");
    std::fs::write(&path, "line one\nline two\nline three\n").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();

    assert!(result.content.contains("line one"));
    assert!(result.content.contains("line two"));
    assert!(result.content.contains("line three"));
}

#[tokio::test]
async fn test_ch6_read_with_line_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("numbered.txt");
    std::fs::write(&path, "alpha\nbeta\ngamma\n").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({ "path": path.to_str().unwrap() }))
        .await
        .unwrap();

    // Should contain line numbers
    assert!(result.content.contains("1\t"));
    assert!(result.content.contains("2\t"));
    assert!(result.content.contains("3\t"));
}

#[tokio::test]
async fn test_ch6_read_with_offset_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lines.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();

    let tool = ReadTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "offset": 2,
            "limit": 2
        }))
        .await
        .unwrap();

    assert!(result.content.contains("b"));
    assert!(result.content.contains("c"));
    assert!(!result.content.contains("1\ta"));
    assert!(!result.content.contains("d"));
}

#[tokio::test]
async fn test_ch6_read_nonexistent() {
    let tool = ReadTool::new();
    let result = tool.call(json!({ "path": "/nonexistent/file.txt" })).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_ch6_read_is_read_only() {
    let tool = ReadTool::new();
    assert!(tool.is_read_only());
    assert!(tool.is_concurrent_safe());
    assert!(!tool.is_destructive());
}

// ── WriteTool ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_ch6_write_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.txt");

    let tool = WriteTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "content": "hello world"
        }))
        .await
        .unwrap();

    assert!(result.content.contains("wrote"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
}

#[tokio::test]
async fn test_ch6_write_creates_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a/b/c/deep.txt");

    let tool = WriteTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "content": "deep content"
    }))
    .await
    .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "deep content");
}

#[tokio::test]
async fn test_ch6_write_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("overwrite.txt");
    std::fs::write(&path, "old content").unwrap();

    let tool = WriteTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "content": "new content"
    }))
    .await
    .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
}

// ── EditTool ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_ch6_edit_replace() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "hello world").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "world",
            "new_string": "rust"
        }))
        .await
        .unwrap();

    assert!(result.content.contains("edited"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello rust");
}

#[tokio::test]
async fn test_ch6_edit_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "hello world").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "xyz",
            "new_string": "abc"
        }))
        .await
        .unwrap();

    // Should return error as a value, not Err
    assert!(result.content.starts_with("error:"));
    assert!(result.content.contains("not found"));
}

#[tokio::test]
async fn test_ch6_edit_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "aa bb aa").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "aa",
            "new_string": "cc"
        }))
        .await
        .unwrap();

    assert!(result.content.starts_with("error:"));
    assert!(result.content.contains("2 times"));
}

#[tokio::test]
async fn test_ch6_edit_validation() {
    use crate::types::ValidationResult;

    let tool = EditTool::new();
    let result = tool.validate_input(&json!({"path": "x"}));
    assert!(matches!(result, ValidationResult::Error { .. }));

    let result = tool.validate_input(&json!({
        "path": "x",
        "old_string": "a",
        "new_string": "b"
    }));
    assert!(matches!(result, ValidationResult::Ok));
}

// ── Read + Write + Edit integration ─────────────────────────────────────────

#[tokio::test]
async fn test_ch6_write_then_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.txt");
    let path_str = path.to_str().unwrap();

    let write = WriteTool::new();
    write
        .call(json!({ "path": path_str, "content": "hello\nworld\n" }))
        .await
        .unwrap();

    let read = ReadTool::new();
    let result = read.call(json!({ "path": path_str })).await.unwrap();
    assert!(result.content.contains("hello"));
    assert!(result.content.contains("world"));
}

#[tokio::test]
async fn test_ch6_write_edit_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.txt");
    let path_str = path.to_str().unwrap();

    let write = WriteTool::new();
    write
        .call(json!({ "path": path_str, "content": "fn main() { println!(\"hello\"); }" }))
        .await
        .unwrap();

    let edit = EditTool::new();
    edit.call(json!({
        "path": path_str,
        "old_string": "hello",
        "new_string": "goodbye"
    }))
    .await
    .unwrap();

    let read = ReadTool::new();
    let result = read.call(json!({ "path": path_str })).await.unwrap();
    assert!(result.content.contains("goodbye"));
    assert!(!result.content.contains("hello"));
}

// ── Summary and definition ──────────────────────────────────────────────────

#[test]
fn test_ch6_tool_definitions() {
    let read = ReadTool::new();
    assert_eq!(read.definition().name, "read");

    let write = WriteTool::new();
    assert_eq!(write.definition().name, "write");

    let edit = EditTool::new();
    assert_eq!(edit.definition().name, "edit");
}

#[test]
fn test_ch6_tool_summaries() {
    let read = ReadTool::new();
    assert_eq!(read.summary(&json!({"path": "foo.rs"})), "[read: foo.rs]");

    let write = WriteTool::new();
    assert_eq!(write.summary(&json!({"path": "bar.rs"})), "[write: bar.rs]");

    let edit = EditTool::new();
    assert_eq!(edit.summary(&json!({"path": "baz.rs"})), "[edit: baz.rs]");
}
