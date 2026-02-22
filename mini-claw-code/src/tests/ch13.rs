use std::collections::VecDeque;
use std::sync::Arc;

use serde_json::json;

use crate::agent::SimpleAgent;
use crate::mock::MockProvider;
use crate::subagent::SubagentTool;
use crate::tools::{ReadTool, WriteTool};
use crate::types::*;

// ---------------------------------------------------------------------------
// 1. Child returns text immediately (no tool calls)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_text_response() {
    let provider = Arc::new(MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Child result".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
    }])));

    let tool = SubagentTool::new(provider, || ToolSet::new());
    let result = tool.call(json!({"task": "Do something"})).await.unwrap();

    assert_eq!(result, "Child result");
}

// ---------------------------------------------------------------------------
// 2. Child uses ReadTool before answering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_with_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.txt");
    std::fs::write(&path, "secret data").unwrap();
    let path_str = path.to_str().unwrap().to_string();

    let provider = Arc::new(MockProvider::new(VecDeque::from([
        // Child turn 1: call read
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child turn 2: return answer
        AssistantTurn {
            text: Some("The file says: secret data".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
    ])));

    let tool = SubagentTool::new(provider, || ToolSet::new().with(ReadTool::new()));
    let result = tool.call(json!({"task": "Read the file"})).await.unwrap();

    assert_eq!(result, "The file says: secret data");
}

// ---------------------------------------------------------------------------
// 3. Child makes multiple tool calls across turns
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_multi_step() {
    let dir = tempfile::tempdir().unwrap();
    let path_a = dir.path().join("a.txt");
    let path_b = dir.path().join("b.txt");
    std::fs::write(&path_a, "alpha").unwrap();
    std::fs::write(&path_b, "beta").unwrap();
    let a_str = path_a.to_str().unwrap().to_string();
    let b_str = path_b.to_str().unwrap().to_string();

    let provider = Arc::new(MockProvider::new(VecDeque::from([
        // Turn 1: read file a
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": a_str}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Turn 2: read file b
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "read".into(),
                arguments: json!({"path": b_str}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Turn 3: final answer
        AssistantTurn {
            text: Some("alpha and beta".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
    ])));

    let tool = SubagentTool::new(provider, || ToolSet::new().with(ReadTool::new()));
    let result = tool.call(json!({"task": "Read both files"})).await.unwrap();

    assert_eq!(result, "alpha and beta");
}

// ---------------------------------------------------------------------------
// 4. Max turns exceeded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_max_turns_exceeded() {
    // Mock always returns a tool call — child will never stop on its own
    let provider = Arc::new(MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": "/dev/null"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "read".into(),
                arguments: json!({"path": "/dev/null"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "read".into(),
                arguments: json!({"path": "/dev/null"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
    ])));

    let tool = SubagentTool::new(provider, || ToolSet::new().with(ReadTool::new())).max_turns(2);
    let result = tool.call(json!({"task": "Loop forever"})).await.unwrap();

    assert_eq!(result, "error: max turns exceeded");
}

// ---------------------------------------------------------------------------
// 5. Missing task parameter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_missing_task() {
    let provider = Arc::new(MockProvider::new(VecDeque::new()));
    let tool = SubagentTool::new(provider, || ToolSet::new());
    let result = tool.call(json!({})).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing required parameter: task")
    );
}

// ---------------------------------------------------------------------------
// 6. Child provider error propagates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_child_provider_error() {
    // Empty mock → error on first call
    let provider = Arc::new(MockProvider::new(VecDeque::new()));
    let tool = SubagentTool::new(provider, || ToolSet::new());
    let result = tool.call(json!({"task": "Do something"})).await;

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 7. Child handles unknown tool gracefully
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_unknown_tool_in_child() {
    let provider = Arc::new(MockProvider::new(VecDeque::from([
        // Child calls a tool that doesn't exist in its ToolSet
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "nonexistent".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child recovers
        AssistantTurn {
            text: Some("Tool not found, but I can still answer.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
    ])));

    let tool = SubagentTool::new(provider, || ToolSet::new());
    let result = tool.call(json!({"task": "Try unknown"})).await.unwrap();

    assert_eq!(result, "Tool not found, but I can still answer.");
}

// ---------------------------------------------------------------------------
// 8. Builder pattern compiles
// ---------------------------------------------------------------------------

#[test]
fn test_ch13_builder_pattern() {
    let provider = Arc::new(MockProvider::new(VecDeque::new()));
    let _tool = SubagentTool::new(provider, || ToolSet::new().with(ReadTool::new()))
        .system_prompt("You are a helper.")
        .max_turns(5);
    // If this compiles and runs, the builder pattern works.
}

// ---------------------------------------------------------------------------
// 9. System prompt in child
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_system_prompt_in_child() {
    // MockProvider doesn't inspect messages, so we verify the tool works
    // correctly when a system prompt is configured.
    let provider = Arc::new(MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Audited.".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
    }])));

    let tool =
        SubagentTool::new(provider, || ToolSet::new()).system_prompt("You are a security auditor.");
    let result = tool.call(json!({"task": "Audit this code"})).await.unwrap();

    assert_eq!(result, "Audited.");
}

// ---------------------------------------------------------------------------
// 10. Child writes a file, parent continues
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_subagent_with_write_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.txt");
    let path_str = path.to_str().unwrap().to_string();

    let provider = Arc::new(MockProvider::new(VecDeque::from([
        // Parent turn 1: call subagent
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "p1".into(),
                name: "subagent".into(),
                arguments: json!({"task": "Write hello to the file"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // --- child turns start ---
        // Child turn 1: call write
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "hello"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child turn 2: done
        AssistantTurn {
            text: Some("File written.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
        // --- child turns end ---
        // Parent turn 2: done
        AssistantTurn {
            text: Some("Subagent completed the write.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
    ])));

    let p = provider.clone();
    let agent = SimpleAgent::new(provider).tool(SubagentTool::new(p, || {
        ToolSet::new().with(WriteTool::new())
    }));

    let result = agent.run("Write a file via subagent").await.unwrap();

    assert_eq!(result, "Subagent completed the write.");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
}

// ---------------------------------------------------------------------------
// 11. Parent resumes after subagent completes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_parent_continues_after_subagent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("info.txt");
    std::fs::write(&path, "important").unwrap();
    let path_str = path.to_str().unwrap().to_string();

    let provider = Arc::new(MockProvider::new(VecDeque::from([
        // Parent turn 1: call subagent
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "p1".into(),
                name: "subagent".into(),
                arguments: json!({"task": "Summarize the file"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child turn 1: read file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child turn 2: done
        AssistantTurn {
            text: Some("File contains: important".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
        // Parent turn 2: read the same file directly
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "p2".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Parent turn 3: final answer
        AssistantTurn {
            text: Some("Confirmed: important".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
    ])));

    let p = provider.clone();
    let agent = SimpleAgent::new(provider)
        .tool(ReadTool::new())
        .tool(SubagentTool::new(p, || {
            ToolSet::new().with(ReadTool::new())
        }));

    let result = agent.run("Verify file contents").await.unwrap();

    assert_eq!(result, "Confirmed: important");
}

// ---------------------------------------------------------------------------
// 12. Child messages don't leak into parent history
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_isolated_message_history() {
    let provider = Arc::new(MockProvider::new(VecDeque::from([
        // Parent turn 1: call subagent
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "p1".into(),
                name: "subagent".into(),
                arguments: json!({"task": "Do child work"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child turn 1: call a tool (this generates internal child messages)
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": "/dev/null"}),
            }],
            stop_reason: StopReason::ToolUse,
        },
        // Child turn 2: done
        AssistantTurn {
            text: Some("Child done".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
        // Parent turn 2: done
        AssistantTurn {
            text: Some("All done".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
        },
    ])));

    let p = provider.clone();
    let agent = SimpleAgent::new(provider).tool(SubagentTool::new(p, || {
        ToolSet::new().with(ReadTool::new())
    }));

    let mut messages = vec![Message::User("Test isolation".into())];
    let result = agent.chat(&mut messages).await.unwrap();

    assert_eq!(result, "All done");

    // Parent messages: User, Assistant(ToolUse subagent), ToolResult, Assistant(Stop)
    // Child's internal messages (User, Assistant, ToolResult) should NOT appear.
    assert_eq!(messages.len(), 4);
    assert!(matches!(&messages[0], Message::User(s) if s == "Test isolation"));
    assert!(matches!(&messages[1], Message::Assistant(_)));
    assert!(matches!(&messages[2], Message::ToolResult { content, .. } if content == "Child done"));
    assert!(matches!(&messages[3], Message::Assistant(_)));
}
