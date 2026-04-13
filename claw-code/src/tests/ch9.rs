use std::collections::VecDeque;

use serde_json::json;

use crate::engine::QueryEngine;
use crate::provider::MockProvider;
use crate::tools::*;
use crate::types::*;

// ── Tool Registry ───────────────────────────────────────────────────────────

#[test]
fn test_ch9_registry_all_tools() {
    let tools = ToolSet::new()
        .with(ReadTool::new())
        .with(WriteTool::new())
        .with(EditTool::new())
        .with(BashTool::new())
        .with(GlobTool::new())
        .with(GrepTool::new());

    assert_eq!(tools.len(), 6);
    assert!(tools.get("read").is_some());
    assert!(tools.get("write").is_some());
    assert!(tools.get("edit").is_some());
    assert!(tools.get("bash").is_some());
    assert!(tools.get("glob").is_some());
    assert!(tools.get("grep").is_some());
    assert!(tools.get("nonexistent").is_none());
}

#[test]
fn test_ch9_registry_definitions() {
    let tools = ToolSet::new()
        .with(ReadTool::new())
        .with(WriteTool::new())
        .with(BashTool::new());

    let defs = tools.definitions();
    assert_eq!(defs.len(), 3);

    let names: Vec<&str> = defs.iter().map(|d| d.name).collect();
    assert!(names.contains(&"read"));
    assert!(names.contains(&"write"));
    assert!(names.contains(&"bash"));
}

#[test]
fn test_ch9_registry_names() {
    let tools = ToolSet::new().with(GlobTool::new()).with(GrepTool::new());

    let mut names = tools.names();
    names.sort();
    assert_eq!(names, vec!["glob", "grep"]);
}

#[test]
fn test_ch9_read_only_tools() {
    let tools = ToolSet::new()
        .with(ReadTool::new())
        .with(WriteTool::new())
        .with(BashTool::new())
        .with(GlobTool::new())
        .with(GrepTool::new());

    let read_only: Vec<&str> = tools
        .definitions()
        .iter()
        .filter(|d| tools.get(d.name).map(|t| t.is_read_only()).unwrap_or(false))
        .map(|d| d.name)
        .collect();

    assert!(read_only.contains(&"read"));
    assert!(read_only.contains(&"glob"));
    assert!(read_only.contains(&"grep"));
    assert!(!read_only.contains(&"write"));
    assert!(!read_only.contains(&"bash"));
}

#[test]
fn test_ch9_destructive_tools() {
    let bash = BashTool::new();
    let write = WriteTool::new();
    let read = ReadTool::new();

    assert!(bash.is_destructive());
    assert!(!write.is_destructive());
    assert!(!read.is_destructive());
}

// ── QueryEngine integration with real tools ─────────────────────────────────

#[tokio::test]
async fn test_ch9_engine_with_file_tools() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    let path_str = path.to_str().unwrap().to_string();

    // Mock: first call writes a file, second call reads it, third returns text
    let provider = MockProvider::new(VecDeque::from([
        // Turn 1: write a file
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({
                    "path": path_str,
                    "content": "hello from agent"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 2: read it back
        AssistantMessage {
            id: "2".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "read".into(),
                arguments: json!({ "path": path_str }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 3: final answer
        AssistantMessage {
            id: "3".into(),
            text: Some("Done! I wrote and read the file.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    let result = engine.run("write and read a file").await.unwrap();
    assert!(result.contains("Done!"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello from agent");
}

#[tokio::test]
async fn test_ch9_engine_with_bash() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({ "command": "echo hello-from-bash" }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: Some("The command output 'hello-from-bash'.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider).tool(BashTool::new());
    let result = engine.run("run echo").await.unwrap();
    assert!(result.contains("hello-from-bash"));
}

#[tokio::test]
async fn test_ch9_engine_unknown_tool_recovery() {
    let provider = MockProvider::new(VecDeque::from([
        // LLM hallucinates a tool
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "imaginary_tool".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // LLM recovers after seeing the error
        AssistantMessage {
            id: "2".into(),
            text: Some("Sorry, that tool doesn't exist.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider).tool(ReadTool::new());
    let result = engine.run("do something").await.unwrap();
    assert!(result.contains("doesn't exist"));
}

/// Helper to build the standard tool set.
#[test]
fn test_ch9_default_toolset_builder() {
    fn default_tools() -> ToolSet {
        ToolSet::new()
            .with(ReadTool::new())
            .with(WriteTool::new())
            .with(EditTool::new())
            .with(BashTool::new())
            .with(GlobTool::new())
            .with(GrepTool::new())
    }

    let tools = default_tools();
    assert_eq!(tools.len(), 6);

    // All tools produce valid definitions
    for def in tools.definitions() {
        assert!(!def.name.is_empty());
        assert!(!def.description.is_empty());
    }
}
