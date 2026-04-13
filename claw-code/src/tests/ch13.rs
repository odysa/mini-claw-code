use std::collections::VecDeque;

use serde_json::json;

use crate::agents::PlanEngine;
use crate::provider::MockProvider;
use crate::tools::ReadTool;
use crate::types::*;

// ---------------------------------------------------------------------------
// Helper to build a mock response
// ---------------------------------------------------------------------------

fn text_response(text: &str) -> AssistantMessage {
    AssistantMessage {
        id: "1".into(),
        text: Some(text.into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }
}

fn tool_response(calls: Vec<ToolCall>) -> AssistantMessage {
    AssistantMessage {
        id: "1".into(),
        text: None,
        tool_calls: calls,
        stop_reason: StopReason::ToolUse,
        usage: None,
    }
}

// ---------------------------------------------------------------------------
// Plan phase: text-only
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_plan_text_only() {
    let provider = MockProvider::new(VecDeque::from([text_response(
        "Here is my plan: read the files first.",
    )]));
    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Fix the bug")];
    let plan = engine.plan(&mut messages).await.unwrap();
    assert!(plan.contains("plan"));
}

// ---------------------------------------------------------------------------
// Plan phase: read-only tools allowed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_plan_allows_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "hello").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Plan: read a file
        tool_response(vec![ToolCall {
            id: "c1".into(),
            name: "read".into(),
            arguments: json!({"path": path.to_str().unwrap()}),
        }]),
        // Final plan text
        text_response("I read the file. It contains 'hello'."),
    ]));

    let engine = PlanEngine::new(provider).tool(ReadTool::new());
    let mut messages = vec![Message::user("What's in the file?")];
    let plan = engine.plan(&mut messages).await.unwrap();
    assert!(plan.contains("hello"));
}

// ---------------------------------------------------------------------------
// Plan phase: write tools blocked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_plan_blocks_write_tools() {
    let provider = MockProvider::new(VecDeque::from([
        // Plan tries to write (should be blocked)
        tool_response(vec![ToolCall {
            id: "c1".into(),
            name: "write".into(),
            arguments: json!({"path": "/tmp/x.txt", "content": "bad"}),
        }]),
        // Model recovers after seeing the error
        text_response("Sorry, I can't write in plan mode."),
    ]));

    let engine = PlanEngine::new(provider).tool(crate::tools::WriteTool::new());
    let mut messages = vec![Message::user("Write something")];
    let plan = engine.plan(&mut messages).await.unwrap();
    assert!(plan.contains("can't write"));

    // The tool result should contain an error about planning mode
    let has_plan_error = messages.iter().any(|m| {
        matches!(m, Message::ToolResult(r) if r.content.contains("not available in planning mode"))
    });
    assert!(has_plan_error);
}

// ---------------------------------------------------------------------------
// Plan phase: exit_plan tool
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_exit_plan_ends_planning() {
    let provider = MockProvider::new(VecDeque::from([tool_response(vec![ToolCall {
        id: "c1".into(),
        name: "exit_plan".into(),
        arguments: json!({}),
    }])]));

    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Plan something")];
    let plan = engine.plan(&mut messages).await.unwrap();
    // exit_plan returns the (empty) text from the assistant turn
    assert!(plan.is_empty() || plan.len() < 100);
}

// ---------------------------------------------------------------------------
// Execute phase: all tools available
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_execute_allows_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.txt");

    let provider = MockProvider::new(VecDeque::from([
        // Write a file
        tool_response(vec![ToolCall {
            id: "c1".into(),
            name: "write".into(),
            arguments: json!({
                "path": path.to_str().unwrap(),
                "content": "written during execute"
            }),
        }]),
        // Done
        text_response("File written successfully."),
    ]));

    let engine = PlanEngine::new(provider).tool(crate::tools::WriteTool::new());
    let mut messages = vec![Message::user("Write the file")];
    let result = engine.execute(&mut messages).await.unwrap();
    assert!(result.contains("written"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "written during execute"
    );
}

// ---------------------------------------------------------------------------
// Full plan → execute workflow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_full_plan_execute_flow() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    std::fs::write(&src, "source content").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Plan phase: read the source file
        tool_response(vec![ToolCall {
            id: "c1".into(),
            name: "read".into(),
            arguments: json!({"path": src.to_str().unwrap()}),
        }]),
        // Plan text
        text_response("I'll copy src.txt to dst.txt."),
        // Execute phase: write the destination
        tool_response(vec![ToolCall {
            id: "c2".into(),
            name: "write".into(),
            arguments: json!({
                "path": dst.to_str().unwrap(),
                "content": "source content"
            }),
        }]),
        // Done
        text_response("Done! Copied the file."),
    ]));

    let engine = PlanEngine::new(provider)
        .tool(ReadTool::new())
        .tool(crate::tools::WriteTool::new());

    let mut messages = vec![Message::user("Copy src.txt to dst.txt")];

    // Plan phase
    let plan = engine.plan(&mut messages).await.unwrap();
    assert!(plan.contains("copy"));

    // Simulate user approval by pushing a message
    messages.push(Message::user("Approved. Go ahead."));

    // Execute phase
    let result = engine.execute(&mut messages).await.unwrap();
    assert!(result.contains("Done"));
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "source content");
}

// ---------------------------------------------------------------------------
// Message continuity between phases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_message_continuity() {
    let provider = MockProvider::new(VecDeque::from([
        text_response("My plan: do X."),
        text_response("Executed X."),
    ]));

    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Do something")];

    let _plan = engine.plan(&mut messages).await.unwrap();
    let msg_count_after_plan = messages.len();

    messages.push(Message::user("Go ahead."));
    let _result = engine.execute(&mut messages).await.unwrap();

    // Messages accumulated across both phases
    assert!(messages.len() > msg_count_after_plan);
}

// ---------------------------------------------------------------------------
// Custom plan_tool_names override
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_custom_plan_tools() {
    // Override plan tools to include "bash" (normally not read-only)
    let provider = MockProvider::new(VecDeque::from([
        tool_response(vec![ToolCall {
            id: "c1".into(),
            name: "bash".into(),
            arguments: json!({"command": "echo hello"}),
        }]),
        text_response("Bash output: hello"),
    ]));

    let engine = PlanEngine::new(provider)
        .tool(crate::tools::BashTool::new())
        .plan_tool_names(&["bash"]);

    let mut messages = vec![Message::user("Check something")];
    let plan = engine.plan(&mut messages).await.unwrap();
    assert!(plan.contains("hello"));
}

// ---------------------------------------------------------------------------
// System prompt injection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_system_prompt_injected() {
    let provider = MockProvider::new(VecDeque::from([text_response("Plan done.")]));

    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Plan something")];
    let _plan = engine.plan(&mut messages).await.unwrap();

    // Check that a system message with plan_mode tag was injected
    let has_plan_system = messages
        .iter()
        .any(|m| matches!(m, Message::System(s) if s.tag.as_deref() == Some("plan_mode")));
    assert!(has_plan_system);
}

#[tokio::test]
async fn test_ch13_system_prompt_not_duplicated() {
    let provider = MockProvider::new(VecDeque::from([
        text_response("Plan A."),
        text_response("Plan B."),
    ]));

    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Plan something")];

    // First plan
    let _plan = engine.plan(&mut messages).await.unwrap();

    // Second plan (e.g., user asks to revise)
    messages.push(Message::user("Revise the plan."));
    let _plan = engine.plan(&mut messages).await.unwrap();

    // Only one plan_mode system message
    let plan_system_count = messages
        .iter()
        .filter(|m| matches!(m, Message::System(s) if s.tag.as_deref() == Some("plan_mode")))
        .count();
    assert_eq!(plan_system_count, 1);
}

// ---------------------------------------------------------------------------
// Custom plan prompt
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_custom_plan_prompt() {
    let provider = MockProvider::new(VecDeque::from([text_response("Custom plan.")]));

    let engine = PlanEngine::new(provider).plan_prompt("You are a security auditor.");
    let mut messages = vec![Message::user("Audit this code")];
    let _plan = engine.plan(&mut messages).await.unwrap();

    let plan_msg = messages.iter().find_map(|m| {
        if let Message::System(s) = m {
            if s.tag.as_deref() == Some("plan_mode") {
                return Some(s.content.as_str());
            }
        }
        None
    });
    assert_eq!(plan_msg, Some("You are a security auditor."));
}

// ---------------------------------------------------------------------------
// Provider error propagation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch13_provider_error_in_plan() {
    let provider = MockProvider::new(VecDeque::new()); // No responses → error
    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Plan")];
    let result = engine.plan(&mut messages).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_ch13_provider_error_in_execute() {
    let provider = MockProvider::new(VecDeque::new());
    let engine = PlanEngine::new(provider);
    let mut messages = vec![Message::user("Execute")];
    let result = engine.execute(&mut messages).await;
    assert!(result.is_err());
}
