use std::collections::VecDeque;

use serde_json::json;
use tokio::sync::mpsc;

use crate::agent::AgentEvent;
use crate::planning::PlanAgent;
use crate::streaming::MockStreamProvider;
use crate::tools::{BashTool, EditTool, ReadTool, WriteTool};
use crate::types::*;

// ---------------------------------------------------------------------------
// 1. plan() text-only response
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_plan_text_response() {
    let provider = MockStreamProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Here is my plan.".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Plan something".into())];
    let result = agent.plan(&mut messages, tx).await.unwrap();

    assert_eq!(result, "Here is my plan.");
    // System prompt injected + User + Assistant
    assert_eq!(messages.len(), 3);
    assert!(matches!(&messages[0], Message::System(_)));
}

// ---------------------------------------------------------------------------
// 2. plan() allows read tool
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_plan_with_read_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("info.txt");
    std::fs::write(&path, "important data").unwrap();

    let provider = MockStreamProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("File contains: important data".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Read the file".into())];
    let result = agent.plan(&mut messages, tx).await.unwrap();

    assert_eq!(result, "File contains: important data");
}

// ---------------------------------------------------------------------------
// 3. plan() blocks write tool
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_plan_blocks_write_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blocked.txt");

    let provider = MockStreamProvider::new(VecDeque::from([
        // LLM tries to call write during planning
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path.to_str().unwrap(), "content": "hacked"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // LLM acknowledges the error
        AssistantTurn {
            text: Some("Cannot write in plan mode.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Write a file".into())];
    let result = agent.plan(&mut messages, tx).await.unwrap();

    assert_eq!(result, "Cannot write in plan mode.");
    // File must NOT have been created
    assert!(!path.exists());

    // Verify the error tool result was sent back
    let tool_result = messages
        .iter()
        .find(|m| matches!(m, Message::ToolResult { .. }));
    assert!(tool_result.is_some());
    if let Some(Message::ToolResult { content, .. }) = tool_result {
        assert!(content.contains("not available in planning mode"));
    }
}

// ---------------------------------------------------------------------------
// 4. plan() blocks edit tool
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_plan_blocks_edit_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.txt");
    std::fs::write(&path, "original").unwrap();

    let provider = MockStreamProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": path.to_str().unwrap(),
                    "old": "original",
                    "new": "modified"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Edit blocked.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(EditTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Edit the file".into())];
    let result = agent.plan(&mut messages, tx).await.unwrap();

    assert_eq!(result, "Edit blocked.");
    // File must be unchanged
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}

// ---------------------------------------------------------------------------
// 5. execute() allows write tool
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_execute_allows_write_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.txt");

    let provider = MockStreamProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path.to_str().unwrap(), "content": "written!"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("File written.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Approved. Execute.".into())];
    let result = agent.execute(&mut messages, tx).await.unwrap();

    assert_eq!(result, "File written.");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "written!");
}

// ---------------------------------------------------------------------------
// 6. Full plan-then-execute flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_full_plan_then_execute() {
    let dir = tempfile::tempdir().unwrap();
    let read_path = dir.path().join("source.txt");
    let write_path = dir.path().join("dest.txt");
    std::fs::write(&read_path, "source data").unwrap();

    // Plan phase: LLM reads the source file, then responds with plan text
    // Execute phase: LLM writes the dest file, then responds with done text
    let provider = MockStreamProvider::new(VecDeque::from([
        // Plan turn 1: read file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": read_path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Plan turn 2: return plan
        AssistantTurn {
            text: Some("Plan: copy source data to dest.txt".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        // Execute turn 1: write file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "write".into(),
                arguments: json!({"path": write_path.to_str().unwrap(), "content": "source data"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Execute turn 2: done
        AssistantTurn {
            text: Some("Done. Copied to dest.txt".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Copy source.txt to dest.txt".into())];

    // Phase 1: Plan
    let plan = agent.plan(&mut messages, tx).await.unwrap();
    assert_eq!(plan, "Plan: copy source data to dest.txt");
    assert!(!write_path.exists()); // not written yet

    // Phase 2: Approve and execute
    messages.push(Message::User("Approved. Execute.".into()));
    let (tx2, _rx2) = mpsc::unbounded_channel();
    let result = agent.execute(&mut messages, tx2).await.unwrap();
    assert_eq!(result, "Done. Copied to dest.txt");
    assert_eq!(std::fs::read_to_string(&write_path).unwrap(), "source data");
}

// ---------------------------------------------------------------------------
// 7. Message continuity between phases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_message_continuity() {
    let provider = MockStreamProvider::new(VecDeque::from([
        // Plan phase
        AssistantTurn {
            text: Some("My plan".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        // Execute phase
        AssistantTurn {
            text: Some("Executed".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Task".into())];

    let _ = agent.plan(&mut messages, tx).await.unwrap();
    // After plan: [System, User, Assistant]
    assert_eq!(messages.len(), 3);

    messages.push(Message::User("Approved".into()));
    // Before execute: [System, User, Assistant, User]
    assert_eq!(messages.len(), 4);

    let (tx2, _rx2) = mpsc::unbounded_channel();
    let _ = agent.execute(&mut messages, tx2).await.unwrap();
    // After execute: [System, User, Assistant, User, Assistant]
    assert_eq!(messages.len(), 5);
}

// ---------------------------------------------------------------------------
// 8. read_only override
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_read_only_override() {
    // Override read_only to only include "read" (no "bash")
    let provider = MockStreamProvider::new(VecDeque::from([
        // LLM tries bash during planning
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({"command": "rm -rf /"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Bash blocked.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(BashTool::new())
        .read_only(&["read"]); // bash excluded

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Plan".into())];
    let result = agent.plan(&mut messages, tx).await.unwrap();

    assert_eq!(result, "Bash blocked.");
    // Verify error was sent back
    let tool_result = messages
        .iter()
        .find(|m| matches!(m, Message::ToolResult { .. }));
    assert!(tool_result.is_some());
    if let Some(Message::ToolResult { content, .. }) = tool_result {
        assert!(content.contains("not available in planning mode"));
    }
}

// ---------------------------------------------------------------------------
// 9. Streaming events during plan
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_streaming_events_during_plan() {
    let provider = MockStreamProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Plan text".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Plan".into())];
    let _ = agent.plan(&mut messages, tx).await.unwrap();

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }

    // Should have TextDelta events (MockStreamProvider sends one per char)
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TextDelta(_))));
    // Should end with Done
    assert!(events.iter().any(|e| matches!(e, AgentEvent::Done(_))));
}

// ---------------------------------------------------------------------------
// 10. Provider error propagated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_plan_provider_error() {
    // Empty mock → error on first call
    let provider = MockStreamProvider::new(VecDeque::new());
    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Plan".into())];
    let result = agent.plan(&mut messages, tx).await;

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 11. Builder pattern compile test
// ---------------------------------------------------------------------------

#[test]
fn test_ch12_builder_pattern() {
    let provider = MockStreamProvider::new(VecDeque::new());
    let _agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new())
        .tool(EditTool::new())
        .tool(BashTool::new())
        .read_only(&["read", "bash"])
        .plan_prompt("Custom planning instructions.");
    // If this compiles and runs, the builder pattern works.
}

// ---------------------------------------------------------------------------
// 12. System prompt injection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_system_prompt_injected() {
    let provider = MockStreamProvider::new(VecDeque::from([AssistantTurn {
        text: Some("ok".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Task".into())];
    let _ = agent.plan(&mut messages, tx).await.unwrap();

    // System prompt was injected at position 0
    assert!(matches!(&messages[0], Message::System(s) if s.contains("PLANNING MODE")));
}

// ---------------------------------------------------------------------------
// 13. System prompt not duplicated on re-plan
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_system_prompt_not_duplicated() {
    let provider = MockStreamProvider::new(VecDeque::from([
        AssistantTurn {
            text: Some("Plan A".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        AssistantTurn {
            text: Some("Plan B".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Task".into())];
    let _ = agent.plan(&mut messages, tx).await.unwrap();

    // Re-plan with feedback
    messages.push(Message::User("Try again".into()));
    let (tx2, _rx2) = mpsc::unbounded_channel();
    let _ = agent.plan(&mut messages, tx2).await.unwrap();

    // Should still have exactly one System message
    let system_count = messages
        .iter()
        .filter(|m| matches!(m, Message::System(_)))
        .count();
    assert_eq!(system_count, 1);
}

// ---------------------------------------------------------------------------
// 14. Caller-provided system prompt is respected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_caller_system_prompt_respected() {
    let provider = MockStreamProvider::new(VecDeque::from([AssistantTurn {
        text: Some("ok".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    // Caller already set a system prompt
    let mut messages = vec![
        Message::System("You are a helpful assistant.".into()),
        Message::User("Task".into()),
    ];
    let _ = agent.plan(&mut messages, tx).await.unwrap();

    // plan() should NOT overwrite the caller's system prompt
    assert!(matches!(&messages[0], Message::System(s) if s.contains("helpful assistant")));
    let system_count = messages
        .iter()
        .filter(|m| matches!(m, Message::System(_)))
        .count();
    assert_eq!(system_count, 1);
}

// ---------------------------------------------------------------------------
// 15. exit_plan tool ends planning
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_exit_plan_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("code.rs");
    std::fs::write(&path, "fn main() {}").unwrap();

    let provider = MockStreamProvider::new(VecDeque::from([
        // Turn 1: read the file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 2: present plan and call exit_plan
        AssistantTurn {
            text: Some("Plan: add error handling to main".into()),
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "exit_plan".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Refactor this".into())];
    let plan = agent.plan(&mut messages, tx).await.unwrap();

    assert_eq!(plan, "Plan: add error handling to main");

    // Verify exit_plan result was added to messages
    let has_exit_result = messages.iter().any(|m| {
        matches!(m, Message::ToolResult { content, .. } if content.contains("submitted for review"))
    });
    assert!(has_exit_result);
}

// ---------------------------------------------------------------------------
// 16. exit_plan not visible during execute
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_exit_plan_not_in_execute() {
    // During execute, exit_plan is NOT intercepted — if the LLM somehow
    // called it, it would be treated as an unknown tool.
    let provider = MockStreamProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "exit_plan".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("ok".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider).tool(ReadTool::new());

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Execute".into())];
    let result = agent.execute(&mut messages, tx).await.unwrap();

    assert_eq!(result, "ok");
    // exit_plan was treated as unknown tool during execute
    let has_unknown_error = messages.iter().any(
        |m| matches!(m, Message::ToolResult { content, .. } if content.contains("unknown tool")),
    );
    assert!(has_unknown_error);
}

// ---------------------------------------------------------------------------
// 17. Custom plan prompt
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_custom_plan_prompt() {
    let provider = MockStreamProvider::new(VecDeque::from([AssistantTurn {
        text: Some("ok".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .plan_prompt("You are a security auditor. Read code and report vulnerabilities.");

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Audit".into())];
    let _ = agent.plan(&mut messages, tx).await.unwrap();

    assert!(matches!(&messages[0], Message::System(s) if s.contains("security auditor")));
}

// ---------------------------------------------------------------------------
// 18. Full flow with exit_plan (plan → approve → execute)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_full_flow_with_exit_plan() {
    let dir = tempfile::tempdir().unwrap();
    let read_path = dir.path().join("src.txt");
    let write_path = dir.path().join("out.txt");
    std::fs::write(&read_path, "hello world").unwrap();

    let provider = MockStreamProvider::new(VecDeque::from([
        // Plan: read file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": read_path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Plan: present plan and call exit_plan
        AssistantTurn {
            text: Some("Plan: uppercase the content and write to out.txt".into()),
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "exit_plan".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Execute: write file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "write".into(),
                arguments: json!({"path": write_path.to_str().unwrap(), "content": "HELLO WORLD"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Execute: done
        AssistantTurn {
            text: Some("Done.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = PlanAgent::new(provider)
        .tool(ReadTool::new())
        .tool(WriteTool::new());

    // Plan phase
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Uppercase src.txt into out.txt".into())];
    let plan = agent.plan(&mut messages, tx).await.unwrap();
    assert_eq!(plan, "Plan: uppercase the content and write to out.txt");
    assert!(!write_path.exists());

    // Approve and execute
    messages.push(Message::User("Approved.".into()));
    let (tx2, _rx2) = mpsc::unbounded_channel();
    let result = agent.execute(&mut messages, tx2).await.unwrap();
    assert_eq!(result, "Done.");
    assert_eq!(std::fs::read_to_string(&write_path).unwrap(), "HELLO WORLD");
}
