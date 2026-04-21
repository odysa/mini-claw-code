use crate::tools::EditTool;
use crate::types::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// EditTool
// ---------------------------------------------------------------------------

#[test]
fn test_edit_definition() {
    let tool = EditTool::new();
    let def = tool.definition();
    assert_eq!(def.name, "edit");
    assert!(!def.description.is_empty());
}

#[tokio::test]
async fn test_edit_replaces_string() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "hello world").unwrap();

    let tool = EditTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "old_string": "hello",
        "new_string": "goodbye"
    }))
    .await
    .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "goodbye world");
}

#[tokio::test]
async fn test_edit_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "hello world").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "missing",
            "new_string": "replacement"
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_not_unique() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "aaa").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "a",
            "new_string": "b"
        }))
        .await;

    assert!(result.is_err());
}

// --- New EditTool tests ---

#[test]
fn test_edit_default() {
    let tool = EditTool::default();
    assert_eq!(tool.definition().name, "edit");
}

#[test]
fn test_edit_definition_required_params() {
    let tool = EditTool::new();
    let def = tool.definition();
    let required = def.parameters["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "path"));
    assert!(required.iter().any(|v| v == "old_string"));
    assert!(required.iter().any(|v| v == "new_string"));
}

#[tokio::test]
async fn test_edit_returns_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("confirm.txt");
    std::fs::write(&path, "foo bar").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "foo",
            "new_string": "baz"
        }))
        .await
        .unwrap();

    assert!(result.content.contains("edited"));
    assert!(result.content.contains(path.to_str().unwrap()));
}

#[tokio::test]
async fn test_edit_missing_file() {
    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": "/tmp/__mini_claw_code_no_such_file_ch4__.txt",
            "old_string": "old",
            "new_string": "new"
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_replace_with_empty() {
    // Effectively delete a substring
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("del.txt");
    std::fs::write(&path, "remove this part").unwrap();

    let tool = EditTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "old_string": " this part",
        "new_string": ""
    }))
    .await
    .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "remove");
}

#[tokio::test]
async fn test_edit_replace_with_longer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("grow.txt");
    std::fs::write(&path, "short").unwrap();

    let tool = EditTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "old_string": "short",
        "new_string": "a much longer replacement string"
    }))
    .await
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "a much longer replacement string"
    );
}

#[tokio::test]
async fn test_edit_multiline_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multi.txt");
    std::fs::write(&path, "line1\nline2\nline3").unwrap();

    let tool = EditTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "old_string": "line2",
        "new_string": "replaced2\nextra_line"
    }))
    .await
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "line1\nreplaced2\nextra_line\nline3"
    );
}

#[tokio::test]
async fn test_edit_missing_old_string_arg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "content").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "new_string": "replacement"
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_missing_new_string_arg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    std::fs::write(&path, "content").unwrap();

    let tool = EditTool::new();
    let result = tool
        .call(json!({
            "path": path.to_str().unwrap(),
            "old_string": "content"
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_preserves_rest_of_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preserve.txt");
    std::fs::write(&path, "header\ntarget_line\nfooter").unwrap();

    let tool = EditTool::new();
    tool.call(json!({
        "path": path.to_str().unwrap(),
        "old_string": "target_line",
        "new_string": "replaced_line"
    }))
    .await
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("header\n"));
    assert!(content.contains("replaced_line"));
    assert!(content.ends_with("\nfooter"));
}
