use std::collections::VecDeque;

use crate::agent::SimpleAgent;
use crate::mock::MockProvider;
use crate::tools::{BashTool, EditTool, ReadTool, WriteTool};
use crate::types::*;
use serde_json::json;

#[tokio::test]
async fn test_multi_tool_write_and_read_flow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Step 1: Write a file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "Hello from agent!"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 2: Read it back
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_2".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 3: Report
        AssistantTurn {
            text: Some("I wrote and read the file. It contains: Hello from agent!".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(WriteTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Write and read a file").await.unwrap();

    assert!(result.contains("Hello from agent!"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello from agent!");
}

#[tokio::test]
async fn test_multi_tool_edit_flow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.txt");
    let path_str = path.to_str().unwrap();
    std::fs::write(&path, "Hello World").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Step 1: Edit the file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": path_str,
                    "old_string": "World",
                    "new_string": "Rust"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 2: Read it back
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_2".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 3: Report
        AssistantTurn {
            text: Some("Done! The file now says: Hello Rust".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(EditTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Edit the file").await.unwrap();

    assert!(result.contains("Hello Rust"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello Rust");
}

// --- New tests ---

#[tokio::test]
async fn test_multi_tool_bash_then_report() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({"command": "echo hello-from-bash"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Bash returned: hello-from-bash".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(BashTool::new());
    let result = agent.run("Run a command").await.unwrap();

    assert!(result.contains("hello-from-bash"));
}

#[tokio::test]
async fn test_multi_tool_write_edit_read_flow() {
    // Full pipeline: write -> edit -> read
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pipeline.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Write initial content
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "first version"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Edit it
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": path_str,
                    "old_string": "first",
                    "new_string": "second"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Read it back
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Report
        AssistantTurn {
            text: Some("File says: second version".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(WriteTool::new())
        .tool(EditTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Write, edit, then read").await.unwrap();

    assert!(result.contains("second version"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "second version");
}

#[tokio::test]
async fn test_multi_tool_all_four_tools() {
    // Agent uses all four tools in one session
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("all_tools.txt");
    let file_str = file_path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Bash: check directory
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({"command": "echo starting"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Write file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "write".into(),
                arguments: json!({"path": file_str, "content": "original content"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Edit file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": file_str,
                    "old_string": "original",
                    "new_string": "modified"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Read file
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c4".into(),
                name: "read".into(),
                arguments: json!({"path": file_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Report
        AssistantTurn {
            text: Some("All four tools used. File: modified content".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(BashTool::new())
        .tool(WriteTool::new())
        .tool(EditTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Use all tools").await.unwrap();

    assert!(result.contains("modified content"));
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "modified content"
    );
}

#[tokio::test]
async fn test_multi_tool_multiple_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path_a = dir.path().join("a.txt");
    let path_b = dir.path().join("b.txt");
    let a_str = path_a.to_str().unwrap();
    let b_str = path_b.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": a_str, "content": "file A"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "write".into(),
                arguments: json!({"path": b_str, "content": "file B"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Created both files".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(WriteTool::new());
    let result = agent.run("Write two files").await.unwrap();

    assert_eq!(result, "Created both files");
    assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "file A");
    assert_eq!(std::fs::read_to_string(&path_b).unwrap(), "file B");
}

#[tokio::test]
async fn test_multi_tool_read_multiple_files() {
    let dir = tempfile::tempdir().unwrap();
    let path_a = dir.path().join("r1.txt");
    let path_b = dir.path().join("r2.txt");
    std::fs::write(&path_a, "alpha").unwrap();
    std::fs::write(&path_b, "beta").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Read both in one turn
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
            text: Some("alpha and beta".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let result = agent.run("Read both files").await.unwrap();

    assert!(result.contains("alpha"));
    assert!(result.contains("beta"));
}

#[tokio::test]
async fn test_multi_tool_bash_and_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bash_write.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Run bash
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({"command": "echo file-data"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Write the output
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "file-data\n"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Saved bash output to file".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(BashTool::new())
        .tool(WriteTool::new());

    let result = agent.run("Save bash output").await.unwrap();

    assert!(result.contains("Saved"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "file-data\n");
}

#[tokio::test]
async fn test_multi_tool_edit_twice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit_twice.txt");
    let path_str = path.to_str().unwrap();
    std::fs::write(&path, "aaa bbb ccc").unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // First edit
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": path_str,
                    "old_string": "aaa",
                    "new_string": "xxx"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Second edit
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": path_str,
                    "old_string": "ccc",
                    "new_string": "zzz"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Edited twice".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(EditTool::new());
    let result = agent.run("Edit the file twice").await.unwrap();

    assert_eq!(result, "Edited twice");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "xxx bbb zzz");
}

#[tokio::test]
async fn test_multi_tool_write_nested_dirs_then_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep/nested/dir/file.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Write to deeply nested path
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "deep file"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Read it back
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
            text: Some("Deep file: deep file".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(WriteTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Write deeply nested").await.unwrap();

    assert!(result.contains("deep file"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "deep file");
}

#[tokio::test]
async fn test_multi_tool_five_step_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("five.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Step 1: bash
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: json!({"command": "echo step1"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 2: write
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "initial"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 3: read
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 4: edit
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c4".into(),
                name: "edit".into(),
                arguments: json!({
                    "path": path_str,
                    "old_string": "initial",
                    "new_string": "final"
                }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Step 5: read again
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c5".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Final
        AssistantTurn {
            text: Some("Complete: final".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(BashTool::new())
        .tool(WriteTool::new())
        .tool(ReadTool::new())
        .tool(EditTool::new());

    let result = agent.run("Five step flow").await.unwrap();

    assert!(result.contains("final"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "final");
}

#[tokio::test]
async fn test_multi_tool_overwrite_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("overwrite.txt");
    let path_str = path.to_str().unwrap();

    let provider = MockProvider::new(VecDeque::from([
        // Write initial
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "version 1"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Overwrite
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "write".into(),
                arguments: json!({"path": path_str, "content": "version 2"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Read
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c3".into(),
                name: "read".into(),
                arguments: json!({"path": path_str}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantTurn {
            text: Some("Final version: version 2".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider)
        .tool(WriteTool::new())
        .tool(ReadTool::new());

    let result = agent.run("Overwrite file").await.unwrap();

    assert!(result.contains("version 2"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "version 2");
}

// --- chat() tests ---

#[tokio::test]
async fn test_multi_tool_chat_basic() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Hello!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let agent = SimpleAgent::new(provider);
    let mut messages = vec![Message::User("Hi".into())];
    let result = agent.chat(&mut messages).await.unwrap();

    assert_eq!(result, "Hello!");
    // Messages should contain User + Assistant
    assert_eq!(messages.len(), 2);
    assert!(matches!(&messages[0], Message::User(t) if t == "Hi"));
    assert!(matches!(&messages[1], Message::Assistant(_)));
}

#[tokio::test]
async fn test_multi_tool_chat_with_tool_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_tool.txt");
    std::fs::write(&path, "chat content").unwrap();

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
            text: Some("File says: chat content".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider).tool(ReadTool::new());
    let mut messages = vec![Message::User("Read chat_tool.txt".into())];
    let result = agent.chat(&mut messages).await.unwrap();

    assert_eq!(result, "File says: chat content");
    // User + Assistant(tool_call) + ToolResult + Assistant(final)
    assert_eq!(messages.len(), 4);
}

#[tokio::test]
async fn test_multi_tool_chat_multi_turn() {
    // Simulate a two-turn conversation using chat()
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: Some("First reply".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        AssistantTurn {
            text: Some("Second reply".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = SimpleAgent::new(provider);

    // First turn
    let mut messages = vec![Message::User("Turn 1".into())];
    let result1 = agent.chat(&mut messages).await.unwrap();
    assert_eq!(result1, "First reply");
    assert_eq!(messages.len(), 2);

    // Second turn -- push new user message onto existing history
    messages.push(Message::User("Turn 2".into()));
    let result2 = agent.chat(&mut messages).await.unwrap();
    assert_eq!(result2, "Second reply");
    assert_eq!(messages.len(), 4); // User + Assistant + User + Assistant
}
