use std::collections::VecDeque;
use std::sync::Arc;

use serde_json::json;

use crate::mock::MockProvider;
use crate::tools::{AskTool, InputHandler, MockInputHandler, ReadTool};
use crate::types::*;

// ---------------------------------------------------------------------------
// 1. AskTool definition has correct schema
// ---------------------------------------------------------------------------

#[test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
fn test_ask_ask_tool_definition() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::new()));
    let tool = AskTool::new(handler);
    let def = tool.definition();

    assert_eq!(def.name, "ask_user");

    // "question" should be required
    let required = def.parameters["required"].as_array().unwrap();
    assert!(required.contains(&json!("question")));

    // "question" property should exist with type "string"
    assert_eq!(def.parameters["properties"]["question"]["type"], "string");

    // "options" property should exist with type "array"
    assert_eq!(def.parameters["properties"]["options"]["type"], "array");
    assert_eq!(
        def.parameters["properties"]["options"]["items"]["type"],
        "string"
    );

    // "options" should NOT be in required
    assert!(!required.contains(&json!("options")));
}

// ---------------------------------------------------------------------------
// 2. Question only (no options)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_ask_question_only() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::from(["Yes".to_string()])));
    let tool = AskTool::new(handler);

    let result = tool
        .call(json!({"question": "Should I proceed?"}))
        .await
        .unwrap();

    assert_eq!(result, "Yes");
}

// ---------------------------------------------------------------------------
// 3. Question with options
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_ask_with_options() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::from([
        "Option B".to_string()
    ])));
    let tool = AskTool::new(handler);

    let result = tool
        .call(json!({
            "question": "Which approach?",
            "options": ["Option A", "Option B", "Option C"]
        }))
        .await
        .unwrap();

    assert_eq!(result, "Option B");
}

// ---------------------------------------------------------------------------
// 4. Missing question returns error
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_ask_missing_question() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::new()));
    let tool = AskTool::new(handler);

    let result = tool.call(json!({"options": ["a", "b"]})).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing required parameter")
    );
}

// ---------------------------------------------------------------------------
// 5. MockInputHandler exhausted returns error
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_mock_handler_exhausted() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::new()));
    let tool = AskTool::new(handler);

    let result = tool.call(json!({"question": "Anything?"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no more answers"));
}

// ---------------------------------------------------------------------------
// 6. Agent loop: ask_user then final answer
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_agent_ask_then_continue() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::from([
        "Use approach A".to_string()
    ])));

    let provider = MockProvider::new(VecDeque::from([
        // Turn 1: LLM calls ask_user
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "ask_user".into(),
                arguments: json!({"question": "Which approach?"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 2: LLM gives final answer using the user's response
        AssistantTurn {
            text: Some("Using approach A as requested.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = crate::SimpleAgent::new(provider).tool(AskTool::new(handler));

    let result = agent.run("Help me refactor").await.unwrap();
    assert_eq!(result, "Using approach A as requested.");
}

// ---------------------------------------------------------------------------
// 7. ask_user followed by another tool
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_ask_then_tool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.txt");
    std::fs::write(&path, "hello world").unwrap();

    let handler = Arc::new(MockInputHandler::new(VecDeque::from([path
        .to_str()
        .unwrap()
        .to_string()])));

    let provider = MockProvider::new(VecDeque::from([
        // Turn 1: LLM asks which file to read
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "ask_user".into(),
                arguments: json!({"question": "Which file should I read?"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 2: LLM reads the file the user specified
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
        // Turn 3: final answer
        AssistantTurn {
            text: Some("The file contains: hello world".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = crate::SimpleAgent::new(provider)
        .tool(AskTool::new(handler))
        .tool(ReadTool::new());

    let result = agent.run("Read a file for me").await.unwrap();
    assert_eq!(result, "The file contains: hello world");
}

// ---------------------------------------------------------------------------
// 8. Multiple sequential ask_user calls
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bonus: requires AskTool implementation (Ch11)"]
async fn test_ask_multiple_asks() {
    let handler = Arc::new(MockInputHandler::new(VecDeque::from([
        "Python".to_string(),
        "FastAPI".to_string(),
    ])));

    let provider = MockProvider::new(VecDeque::from([
        // Turn 1: ask language
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "ask_user".into(),
                arguments: json!({"question": "What language?", "options": ["Rust", "Python", "Go"]}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 2: ask framework
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "ask_user".into(),
                arguments: json!({"question": "What framework?", "options": ["Django", "FastAPI"]}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        // Turn 3: final answer
        AssistantTurn {
            text: Some("Setting up Python with FastAPI.".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let agent = crate::SimpleAgent::new(provider).tool(AskTool::new(handler));

    let result = agent.run("Set up a project for me").await.unwrap();
    assert_eq!(result, "Setting up Python with FastAPI.");
}

// ---------------------------------------------------------------------------
// 9. ChannelInputHandler roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ask_channel_handler_roundtrip() {
    use crate::tools::{ChannelInputHandler, UserInputRequest};

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UserInputRequest>();
    let handler = ChannelInputHandler::new(tx);

    // Spawn a task to answer the request
    tokio::spawn(async move {
        let req = rx.recv().await.unwrap();
        assert_eq!(req.question, "Pick one");
        assert_eq!(req.options, vec!["A", "B"]);
        req.response_tx.send("B".to_string()).unwrap();
    });

    let answer = handler
        .ask("Pick one", &["A".to_string(), "B".to_string()])
        .await
        .unwrap();
    assert_eq!(answer, "B");
}

// ---------------------------------------------------------------------------
// 10. param_raw adds array parameter correctly
// ---------------------------------------------------------------------------

#[test]
fn test_ask_param_raw() {
    let def = ToolDefinition::new("test", "A test tool").param_raw(
        "items",
        json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "A list of items"
        }),
        false,
    );

    assert_eq!(def.parameters["properties"]["items"]["type"], "array");
    assert_eq!(
        def.parameters["properties"]["items"]["items"]["type"],
        "string"
    );
    // Should not be in required
    let required = def.parameters["required"].as_array().unwrap();
    assert!(!required.contains(&json!("items")));

    // Test with required = true
    let def2 = ToolDefinition::new("test2", "Another test").param_raw(
        "tags",
        json!({"type": "array", "items": {"type": "string"}}),
        true,
    );
    let required2 = def2.parameters["required"].as_array().unwrap();
    assert!(required2.contains(&json!("tags")));
}
