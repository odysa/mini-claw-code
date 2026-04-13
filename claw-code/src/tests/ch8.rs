use serde_json::json;

use crate::tools::{GlobTool, GrepTool, Tool};

// ── GlobTool ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_ch8_glob_find_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "").unwrap();
    std::fs::write(dir.path().join("b.rs"), "").unwrap();
    std::fs::write(dir.path().join("c.txt"), "").unwrap();

    let tool = GlobTool::new();
    let result = tool
        .call(json!({
            "pattern": "*.rs",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("a.rs"));
    assert!(result.content.contains("b.rs"));
    assert!(!result.content.contains("c.txt"));
}

#[tokio::test]
async fn test_ch8_glob_recursive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("top.rs"), "").unwrap();
    std::fs::write(dir.path().join("sub/deep.rs"), "").unwrap();

    let tool = GlobTool::new();
    let result = tool
        .call(json!({
            "pattern": "**/*.rs",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("top.rs"));
    assert!(result.content.contains("deep.rs"));
}

#[tokio::test]
async fn test_ch8_glob_no_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();

    let tool = GlobTool::new();
    let result = tool
        .call(json!({
            "pattern": "*.xyz",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("no files matched"));
}

#[tokio::test]
async fn test_ch8_glob_is_read_only() {
    let tool = GlobTool::new();
    assert!(tool.is_read_only());
    assert!(tool.is_concurrent_safe());
}

#[test]
fn test_ch8_glob_definition() {
    let tool = GlobTool::new();
    assert_eq!(tool.definition().name, "glob");
}

// ── GrepTool ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_ch8_grep_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("code.rs");
    std::fs::write(&path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

    let tool = GrepTool::new();
    let result = tool
        .call(json!({
            "pattern": "println",
            "path": path.to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("println"));
    assert!(result.content.contains(":2:"));
}

#[tokio::test]
async fn test_ch8_grep_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
    std::fs::write(dir.path().join("b.rs"), "fn bar() {}\nfn foo() {}\n").unwrap();

    let tool = GrepTool::new();
    let result = tool
        .call(json!({
            "pattern": "fn foo",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("a.rs"));
    assert!(result.content.contains("b.rs"));
    assert!(result.content.contains("fn foo"));
}

#[tokio::test]
async fn test_ch8_grep_with_include() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("code.rs"), "hello world\n").unwrap();
    std::fs::write(dir.path().join("data.txt"), "hello world\n").unwrap();

    let tool = GrepTool::new();
    let result = tool
        .call(json!({
            "pattern": "hello",
            "path": dir.path().to_str().unwrap(),
            "include": "*.rs"
        }))
        .await
        .unwrap();

    assert!(result.content.contains("code.rs"));
    assert!(!result.content.contains("data.txt"));
}

#[tokio::test]
async fn test_ch8_grep_no_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.rs"), "nothing here\n").unwrap();

    let tool = GrepTool::new();
    let result = tool
        .call(json!({
            "pattern": "xyz123",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("no matches found"));
}

#[tokio::test]
async fn test_ch8_grep_regex() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.txt");
    std::fs::write(&path, "foo123\nbar456\nbaz789\n").unwrap();

    let tool = GrepTool::new();
    let result = tool
        .call(json!({
            "pattern": "\\d{3}",
            "path": path.to_str().unwrap()
        }))
        .await
        .unwrap();

    assert!(result.content.contains("foo123"));
    assert!(result.content.contains("bar456"));
    assert!(result.content.contains("baz789"));
}

#[tokio::test]
async fn test_ch8_grep_nonexistent_path() {
    let tool = GrepTool::new();
    let result = tool
        .call(json!({
            "pattern": "test",
            "path": "/nonexistent/path"
        }))
        .await
        .unwrap();

    assert!(result.content.starts_with("error:"));
}

#[tokio::test]
async fn test_ch8_grep_is_read_only() {
    let tool = GrepTool::new();
    assert!(tool.is_read_only());
    assert!(tool.is_concurrent_safe());
}

#[test]
fn test_ch8_grep_definition() {
    let tool = GrepTool::new();
    assert_eq!(tool.definition().name, "grep");
}
