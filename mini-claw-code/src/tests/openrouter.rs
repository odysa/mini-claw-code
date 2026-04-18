use crate::providers::openrouter::*;
use crate::types::*;
use serde_json::json;

#[test]
fn test_openrouter_new() {
    let _provider = OpenRouterProvider::new("test-key", "test-model");
}

#[test]
fn test_openrouter_base_url() {
    let _provider = OpenRouterProvider::new("key", "model").base_url("http://localhost:1234");
}

#[test]
fn test_openrouter_from_env_with_model() {
    unsafe { std::env::set_var("OPENROUTER_API_KEY", "test-key-ch6") };
    let result = OpenRouterProvider::from_env_with_model("custom-model");
    assert!(result.is_ok());
}

#[test]
fn test_openrouter_from_env() {
    unsafe { std::env::set_var("OPENROUTER_API_KEY", "test-key-ch6") };
    let result = OpenRouterProvider::from_env();
    assert!(result.is_ok());
}

#[test]
fn test_openrouter_convert_messages() {
    let messages = vec![
        Message::User("hello".into()),
        Message::ToolResult {
            id: "call_1".into(),
            content: "result".into(),
        },
    ];

    let converted = OpenRouterProvider::convert_messages(&messages);
    assert_eq!(converted.len(), 2);

    assert_eq!(converted[0].role, "user");
    assert_eq!(converted[0].content.as_deref(), Some("hello"));

    assert_eq!(converted[1].role, "tool");
    assert_eq!(converted[1].content.as_deref(), Some("result"));
    assert_eq!(converted[1].tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn test_openrouter_convert_tools() {
    let def = ToolDefinition {
        name: "test_tool",
        description: "A test tool",
        parameters: json!({"type": "object"}),
    };

    let converted = OpenRouterProvider::convert_tools(&[&def]);
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].type_, "function");
    assert_eq!(converted[0].function.name, "test_tool");
    assert_eq!(converted[0].function.description, "A test tool");
}

#[tokio::test]
async fn test_openrouter_chat_mock_server() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let response_body = serde_json::to_string(&json!({
        "choices": [{
            "message": {
                "content": "Hello from mock!",
                "tool_calls": null
            },
            "finish_reason": "stop"
        }]
    }))
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let body = response_body.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = socket.read(&mut buf).await;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let provider =
        OpenRouterProvider::new("fake-key", "fake-model").base_url(format!("http://{addr}"));

    let messages = vec![Message::User("Hi".into())];
    let turn = provider.chat(&messages, &[]).await.unwrap();

    assert_eq!(turn.text.as_deref(), Some("Hello from mock!"));
    assert!(turn.tool_calls.is_empty());
}

#[tokio::test]
async fn test_openrouter_live_api() {
    // Only run when a real API key is available (not a test placeholder).
    let _ = dotenvy::dotenv();
    match std::env::var("OPENROUTER_API_KEY") {
        Ok(key) if !key.starts_with("test-") => {}
        _ => return,
    }

    let provider = OpenRouterProvider::from_env().unwrap();
    let messages = vec![Message::User(
        "Say exactly 'hello' and nothing else.".into(),
    )];
    let turn = provider.chat(&messages, &[]).await.unwrap();
    assert!(turn.text.is_some());
}

// --- New tests ---

#[test]
fn test_openrouter_convert_messages_user() {
    let messages = vec![Message::User("test message".into())];
    let converted = OpenRouterProvider::convert_messages(&messages);

    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role, "user");
    assert_eq!(converted[0].content.as_deref(), Some("test message"));
    assert!(converted[0].tool_calls.is_none());
    assert!(converted[0].tool_call_id.is_none());
}

#[test]
fn test_openrouter_convert_messages_assistant_with_text() {
    let messages = vec![Message::Assistant(AssistantTurn {
        text: Some("response".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    })];

    let converted = OpenRouterProvider::convert_messages(&messages);
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role, "assistant");
    assert_eq!(converted[0].content.as_deref(), Some("response"));
    assert!(converted[0].tool_calls.is_none());
}

#[test]
fn test_openrouter_convert_messages_assistant_with_tool_calls() {
    let messages = vec![Message::Assistant(AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "tc_1".into(),
            name: "read".into(),
            arguments: json!({"path": "file.txt"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    })];

    let converted = OpenRouterProvider::convert_messages(&messages);
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role, "assistant");
    assert!(converted[0].content.is_none());

    let tool_calls = converted[0].tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "tc_1");
    assert_eq!(tool_calls[0].type_, "function");
    assert_eq!(tool_calls[0].function.name, "read");
}

#[test]
fn test_openrouter_convert_messages_full_conversation() {
    let messages = vec![
        Message::User("Read file.txt".into()),
        Message::Assistant(AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                id: "tc_1".into(),
                name: "read".into(),
                arguments: json!({"path": "file.txt"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        }),
        Message::ToolResult {
            id: "tc_1".into(),
            content: "file contents".into(),
        },
        Message::Assistant(AssistantTurn {
            text: Some("The file contains: file contents".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        }),
    ];

    let converted = OpenRouterProvider::convert_messages(&messages);
    assert_eq!(converted.len(), 4);
    assert_eq!(converted[0].role, "user");
    assert_eq!(converted[1].role, "assistant");
    assert_eq!(converted[2].role, "tool");
    assert_eq!(converted[3].role, "assistant");
}

#[test]
fn test_openrouter_convert_empty_messages() {
    let messages: Vec<Message> = vec![];
    let converted = OpenRouterProvider::convert_messages(&messages);
    assert!(converted.is_empty());
}

#[test]
fn test_openrouter_convert_empty_tools() {
    let tools: Vec<&ToolDefinition> = vec![];
    let converted = OpenRouterProvider::convert_tools(&tools);
    assert!(converted.is_empty());
}

#[test]
fn test_openrouter_convert_multiple_tools() {
    let def1 =
        ToolDefinition::new("read", "Read a file").param("path", "string", "File path", true);
    let def2 =
        ToolDefinition::new("bash", "Run a command").param("command", "string", "Command", true);

    let converted = OpenRouterProvider::convert_tools(&[&def1, &def2]);
    assert_eq!(converted.len(), 2);
    assert_eq!(converted[0].function.name, "read");
    assert_eq!(converted[1].function.name, "bash");
}

#[test]
fn test_openrouter_convert_tool_preserves_parameters() {
    let def = ToolDefinition::new("test", "desc")
        .param("arg1", "string", "first", true)
        .param("arg2", "number", "second", false);

    let converted = OpenRouterProvider::convert_tools(&[&def]);
    let params = &converted[0].function.parameters;

    assert!(params["properties"]["arg1"].is_object());
    assert!(params["properties"]["arg2"].is_object());
    let required = params["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "arg1"));
    assert!(!required.iter().any(|v| v == "arg2"));
}

#[tokio::test]
async fn test_openrouter_chat_mock_server_tool_calls() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let response_body = serde_json::to_string(&json!({
        "choices": [{
            "message": {
                "content": null,
                "tool_calls": [{
                    "id": "tc_1",
                    "type": "function",
                    "function": {
                        "name": "read",
                        "arguments": "{\"path\": \"test.txt\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    }))
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let body = response_body.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = socket.read(&mut buf).await;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let provider =
        OpenRouterProvider::new("fake-key", "fake-model").base_url(format!("http://{addr}"));

    let messages = vec![Message::User("Read test.txt".into())];
    let def = ToolDefinition::new("read", "Read file").param("path", "string", "Path", true);
    let turn = provider.chat(&messages, &[&def]).await.unwrap();

    assert!(turn.text.is_none());
    assert!(matches!(turn.stop_reason, StopReason::ToolUse));
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "read");
    assert_eq!(turn.tool_calls[0].arguments["path"], "test.txt");
}

#[test]
fn test_openrouter_convert_assistant_multiple_tool_calls() {
    let messages = vec![Message::Assistant(AssistantTurn {
        text: Some("Let me read both".into()),
        tool_calls: vec![
            ToolCall {
                id: "tc_1".into(),
                name: "read".into(),
                arguments: json!({"path": "a.txt"}),
            },
            ToolCall {
                id: "tc_2".into(),
                name: "read".into(),
                arguments: json!({"path": "b.txt"}),
            },
        ],
        stop_reason: StopReason::ToolUse,
        usage: None,
    })];

    let converted = OpenRouterProvider::convert_messages(&messages);
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].content.as_deref(), Some("Let me read both"));

    let tool_calls = converted[0].tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].id, "tc_1");
    assert_eq!(tool_calls[1].id, "tc_2");
}

#[test]
fn test_openrouter_convert_tool_call_arguments_serialized() {
    let messages = vec![Message::Assistant(AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "tc_1".into(),
            name: "write".into(),
            arguments: json!({"path": "out.txt", "content": "hello"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    })];

    let converted = OpenRouterProvider::convert_messages(&messages);
    let tool_calls = converted[0].tool_calls.as_ref().unwrap();
    // Arguments should be serialized as JSON string
    let args_str = &tool_calls[0].function.arguments;
    let parsed: serde_json::Value = serde_json::from_str(args_str).unwrap();
    assert_eq!(parsed["path"], "out.txt");
    assert_eq!(parsed["content"], "hello");
}
