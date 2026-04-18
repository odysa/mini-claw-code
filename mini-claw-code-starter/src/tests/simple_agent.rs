use std::collections::VecDeque;

use crate::agent::SimpleAgent;
use crate::mock::MockProvider;
use crate::tools::ReadTool;
use crate::types::*;
use serde_json::json;

#[tokio::test]
async fn test_simple_agent_text_response() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Hello!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = SimpleAgent::new(provider);
    let result = agent.run("Hi").await.unwrap();

    assert_eq!(result, "Hello!");
}

#[tokio::test]
async fn test_simple_agent_single_tool_call() {
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

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read test.txt").await.unwrap();

    assert_eq!(result, "The file contains: file content");
}

#[tokio::test]
async fn test_simple_agent_unknown_tool() {
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

    let agent = SimpleAgent::new(provider);
    let result = agent.run("Use nonexistent tool").await.unwrap();

    assert_eq!(result, "Sorry, that tool doesn't exist.");
}

#[tokio::test]
async fn test_simple_agent_multi_step_loop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("step.txt");
    std::fs::write(&path, "step content").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Step 1: call read
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
        // Step 2: call read again
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_2".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 3: return text
        AssistantTurn {
            text: Some("Done reading twice".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read twice").await.unwrap();

    assert_eq!(result, "Done reading twice");
}

#[tokio::test]
async fn test_simple_agent_empty_response() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = SimpleAgent::new(provider);
    let result = agent.run("Hi").await.unwrap();

    assert_eq!(result, "");
}

#[test]
fn test_simple_agent_builder_chain() {
    let provider = MockProvider::new(VecDeque::new());
    let _agent = SimpleAgent::new(provider)
        .tool(ReadTool::new())
        .tool(ReadTool::new());
    // If this compiles and runs, the builder pattern works.
}

// --- New tests ---

#[tokio::test]
async fn test_simple_agent_multiple_tools_registered() {
    use crate::tools::{BashTool, EditTool, WriteTool};

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
async fn test_simple_agent_three_step_loop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("three.txt");
    std::fs::write(&path, "data").unwrap();

    let provider = MockProvider::new(VecDeque::from([
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
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Read three times".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read three times").await.unwrap();

    assert_eq!(result, "Read three times");
}

#[tokio::test]
async fn test_simple_agent_provider_error() {
    let provider = MockProvider::new(VecDeque::new());
    let agent = SimpleAgent::new(provider);
    let result = agent.run("Hi").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_simple_agent_tool_error_propagates() {
    // Tool call on a missing file sends error back to LLM as tool result
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: json!({"path": "/tmp/__mini_claw_code_no_such_file_ch5__.txt"}),
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

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read missing").await.unwrap();

    assert_eq!(result, "File not found");
}

#[tokio::test]
async fn test_simple_agent_multiple_tool_calls_single_turn() {
    let dir = tempfile::tempdir().unwrap();
    let path_a = dir.path().join("a.txt");
    let path_b = dir.path().join("b.txt");
    std::fs::write(&path_a, "alpha").unwrap();
    std::fs::write(&path_b, "beta").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: json!({"path": path_a.to_str().unwrap()}),
                },
                ToolCall {
                    id: "c2".into(),
                    name: "read".into(),
                    arguments: json!({"path": path_b.to_str().unwrap()}),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Read both".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read both files").await.unwrap();

    assert_eq!(result, "Read both");
}

#[tokio::test]
async fn test_simple_agent_bash_tool_in_loop() {
    use crate::tools::BashTool;

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
async fn test_simple_agent_write_then_read() {
    use crate::tools::WriteTool;

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
async fn test_simple_agent_unknown_among_known() {
    // Mix of known and unknown tool calls in one turn
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mix.txt");
    std::fs::write(&path, "content").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: json!({"path": path.to_str().unwrap()}),
                },
                ToolCall {
                    id: "c2".into(),
                    name: "not_a_real_tool".into(),
                    arguments: json!({}),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("handled".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Mixed tools").await.unwrap();

    assert_eq!(result, "handled");
}

#[tokio::test]
async fn test_simple_agent_immediate_stop_with_tools_registered() {
    use crate::tools::{BashTool, WriteTool};

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

#[tokio::test]
async fn test_simple_agent_text_in_tool_use_turn() {
    // Provider returns both text and a tool call
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text_tool.txt");
    std::fs::write(&path, "data").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: Some("Let me check that file.".into()),
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": path.to_str().unwrap()}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("The file contains: data".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read file").await.unwrap();

    assert_eq!(result, "The file contains: data");
}
