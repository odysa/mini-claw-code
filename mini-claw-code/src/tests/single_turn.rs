use std::collections::VecDeque;

use crate::agent::single_turn;
use crate::mock::MockProvider;
use crate::tools::ReadTool;
use crate::types::*;
use serde_json::json;

#[tokio::test]
async fn test_single_turn_direct_response() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Hello!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let tools = ToolSet::new();
    let result = single_turn(&provider, &tools, "Hi").await.unwrap();

    assert_eq!(result, "Hello!");
}

#[tokio::test]
async fn test_single_turn_one_tool_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "file content").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("The file contains: file content".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read test.txt")
        .await
        .unwrap();

    assert_eq!(result, "The file contains: file content");
}

#[tokio::test]
async fn test_single_turn_unknown_tool() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "nonexistent".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Sorry, that tool doesn't exist.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new();
    let result = single_turn(&provider, &tools, "Use nonexistent tool")
        .await
        .unwrap();

    assert_eq!(result, "Sorry, that tool doesn't exist.");
}

// --- New tests ---

#[tokio::test]
async fn test_single_turn_empty_text_response() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let tools = ToolSet::new();
    let result = single_turn(&provider, &tools, "Hi").await.unwrap();

    assert_eq!(result, "");
}

#[tokio::test]
async fn test_single_turn_tool_call_with_text() {
    // Provider returns text alongside a tool call, then a final answer.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.txt");
    std::fs::write(&path, "data").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: Some("Let me read that.".into()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("The data is: data".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read data.txt")
        .await
        .unwrap();

    assert_eq!(result, "The data is: data");
}

#[tokio::test]
async fn test_single_turn_multiple_tool_calls_one_round() {
    // Two tool calls in a single turn
    let dir = tempfile::tempdir().unwrap();
    let path_a = dir.path().join("a.txt");
    let path_b = dir.path().join("b.txt");
    std::fs::write(&path_a, "aaa").unwrap();
    std::fs::write(&path_b, "bbb").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "call_1".into(),
                    name: "read".into(),
                    arguments: json!({"path": path_a.to_str().unwrap()}),
                },
                ToolCall {
                    id: "call_2".into(),
                    name: "read".into(),
                    arguments: json!({"path": path_b.to_str().unwrap()}),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Both read".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read both files")
        .await
        .unwrap();

    assert_eq!(result, "Both read");
}

#[tokio::test]
async fn test_single_turn_tool_error_propagates() {
    // Tool call on a missing file sends error back to LLM as tool result
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({"path": "/tmp/__mini_claw_code_no_such_file_ch3__.txt"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("File not found".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read missing file")
        .await
        .unwrap();

    assert_eq!(result, "File not found");
}

#[tokio::test]
async fn test_single_turn_no_tools_registered() {
    // Provider returns Stop immediately, no tools needed
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("I can answer without tools".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let tools = ToolSet::new();
    let result = single_turn(&provider, &tools, "What is 2+2?")
        .await
        .unwrap();

    assert_eq!(result, "I can answer without tools");
}

#[tokio::test]
async fn test_single_turn_provider_error_propagates() {
    // Empty mock provider returns error on first chat
    let provider = MockProvider::new(VecDeque::new());

    let tools = ToolSet::new();
    let result = single_turn(&provider, &tools, "Hi").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_single_turn_final_response_none_text() {
    // After tool call, provider returns None text
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.txt");
    std::fs::write(&path, "content").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: None,
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read").await.unwrap();

    assert_eq!(result, "");
}

#[tokio::test]
async fn test_single_turn_tool_call_missing_arg() {
    // Tool call with empty arguments: the tool error is caught by unwrap_or_else
    // and sent back to the LLM as a ToolResult string, not propagated as Err.
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Missing path argument".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read something")
        .await
        .unwrap();

    // The error was handled gracefully — the LLM received it and responded
    assert_eq!(result, "Missing path argument");
}

#[tokio::test]
async fn test_single_turn_mixed_known_and_unknown_tools() {
    // One known tool call and one unknown in the same turn
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.txt");
    std::fs::write(&path, "mixed content").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "call_1".into(),
                    name: "read".into(),
                    arguments: json!({"path": path.to_str().unwrap()}),
                },
                ToolCall {
                    id: "call_2".into(),
                    name: "unknown_tool".into(),
                    arguments: json!({}),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("done".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new());
    let result = single_turn(&provider, &tools, "Read and also use unknown tool")
        .await
        .unwrap();

    assert_eq!(result, "done");
}

#[tokio::test]
async fn test_single_turn_long_prompt() {
    let long_prompt = "x".repeat(10_000);

    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Got it".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let tools = ToolSet::new();
    let result = single_turn(&provider, &tools, &long_prompt).await.unwrap();

    assert_eq!(result, "Got it");
}

#[tokio::test]
async fn test_single_turn_multiple_tools_available() {
    // Register multiple tools but only one is called
    use crate::tools::BashTool;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, "hello").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("hello".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let tools = ToolSet::new().with(ReadTool::new()).with(BashTool::new());
    let result = single_turn(&provider, &tools, "Read the file")
        .await
        .unwrap();

    assert_eq!(result, "hello");
}
