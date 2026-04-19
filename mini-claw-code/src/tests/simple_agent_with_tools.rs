//! SimpleAgent tests that depend on ch9 (Write/Edit) and ch10 (Bash) tools.
//!
//! Kept separate from `simple_agent.rs` so Ch3's `cargo test test_simple_agent_`
//! glob does not sweep in tests that construct tool stubs the learner has not
//! implemented yet.

use std::collections::VecDeque;

use crate::agent::SimpleAgent;
use crate::mock::MockProvider;
use crate::tools::{BashTool, EditTool, ReadTool, WriteTool};
use crate::types::*;
use serde_json::json;

#[tokio::test]
async fn test_agent_multiple_tools_registered() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Ready".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = SimpleAgent::new(provider)
        .tool(ReadTool::new())
        .tool(BashTool::new())
        .tool(WriteTool::new())
        .tool(EditTool::new());

    let result = agent.run("Hello").await.unwrap();
    assert_eq!(result, "Ready");
}

#[tokio::test]
async fn test_agent_bash_tool_in_loop() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({"command": "echo hi"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("bash said hi".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(BashTool::new());
    let result = agent.run("Run bash").await.unwrap();

    assert_eq!(result, "bash said hi");
}

#[tokio::test]
async fn test_agent_write_then_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wr.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "written data"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("File says: written data".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(WriteTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Write and read").await.unwrap();
    assert_eq!(result, "File says: written data");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "written data");
}

#[tokio::test]
async fn test_agent_immediate_stop_with_tools_registered() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("No tools needed".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = SimpleAgent::new(provider)
        .tool(ReadTool::new())
        .tool(BashTool::new())
        .tool(WriteTool::new());

    let result = agent.run("Just answer").await.unwrap();
    assert_eq!(result, "No tools needed");
}
