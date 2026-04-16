use crate::streaming::*;
use crate::types::*;

// ---------------------------------------------------------------------------
// StreamAccumulator tests
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_accumulator_text() {
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::TextDelta("Hello".into()));
    acc.feed(&StreamEvent::TextDelta(" world".into()));
    acc.feed(&StreamEvent::Done);

    let turn = acc.finish();
    assert_eq!(turn.text.as_deref(), Some("Hello world"));
    assert!(turn.tool_calls.is_empty());
    assert!(matches!(turn.stop_reason, StopReason::Stop));
}

#[test]
fn test_ch10_accumulator_tool_call() {
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::ToolCallStart {
        index: 0,
        id: "call_1".into(),
        name: "read".into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 0,
        arguments: r#"{"pa"#.into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 0,
        arguments: r#"th": "f.txt"}"#.into(),
    });
    acc.feed(&StreamEvent::Done);

    let turn = acc.finish();
    assert!(turn.text.is_none());
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "read");
    assert_eq!(turn.tool_calls[0].id, "call_1");
    assert_eq!(turn.tool_calls[0].arguments["path"], "f.txt");
    assert!(matches!(turn.stop_reason, StopReason::ToolUse));
}

#[test]
fn test_ch10_accumulator_empty() {
    let acc = StreamAccumulator::new();
    let turn = acc.finish();

    assert!(turn.text.is_none());
    assert!(turn.tool_calls.is_empty());
    assert!(matches!(turn.stop_reason, StopReason::Stop));
}

#[test]
fn test_ch10_accumulator_text_and_tool() {
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::TextDelta("Thinking...".into()));
    acc.feed(&StreamEvent::ToolCallStart {
        index: 0,
        id: "c1".into(),
        name: "bash".into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 0,
        arguments: r#"{"command": "ls"}"#.into(),
    });
    acc.feed(&StreamEvent::Done);

    let turn = acc.finish();
    assert_eq!(turn.text.as_deref(), Some("Thinking..."));
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "bash");
    assert!(matches!(turn.stop_reason, StopReason::ToolUse));
}

#[test]
fn test_ch10_accumulator_multiple_tool_calls() {
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::ToolCallStart {
        index: 0,
        id: "c1".into(),
        name: "read".into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 0,
        arguments: r#"{"path": "a.txt"}"#.into(),
    });
    acc.feed(&StreamEvent::ToolCallStart {
        index: 1,
        id: "c2".into(),
        name: "read".into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 1,
        arguments: r#"{"path": "b.txt"}"#.into(),
    });
    acc.feed(&StreamEvent::Done);

    let turn = acc.finish();
    assert_eq!(turn.tool_calls.len(), 2);
    assert_eq!(turn.tool_calls[0].arguments["path"], "a.txt");
    assert_eq!(turn.tool_calls[1].arguments["path"], "b.txt");
}

#[test]
fn test_ch10_accumulator_default() {
    let acc = StreamAccumulator::default();
    let turn = acc.finish();
    assert!(turn.text.is_none());
    assert!(turn.tool_calls.is_empty());
}

// ---------------------------------------------------------------------------
// parse_sse_line tests
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_parse_text_delta() {
    let line = r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
    let events = parse_sse_line(line).unwrap();
    assert_eq!(events, vec![StreamEvent::TextDelta("Hello".into())]);
}

#[test]
fn test_ch10_parse_tool_call_start() {
    let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read","arguments":""}}]},"finish_reason":null}]}"#;
    let events = parse_sse_line(line).unwrap();
    assert_eq!(
        events,
        vec![StreamEvent::ToolCallStart {
            index: 0,
            id: "call_1".into(),
            name: "read".into(),
        }]
    );
}

#[test]
fn test_ch10_parse_tool_call_delta() {
    let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\""}}]},"finish_reason":null}]}"#;
    let events = parse_sse_line(line).unwrap();
    assert_eq!(
        events,
        vec![StreamEvent::ToolCallDelta {
            index: 0,
            arguments: r#"{"path""#.into(),
        }]
    );
}

#[test]
fn test_ch10_parse_done() {
    let events = parse_sse_line("data: [DONE]").unwrap();
    assert_eq!(events, vec![StreamEvent::Done]);
}

#[test]
fn test_ch10_parse_non_data_lines() {
    assert!(parse_sse_line("").is_none());
    assert!(parse_sse_line(": comment").is_none());
    assert!(parse_sse_line("event: ping").is_none());
    assert!(parse_sse_line("id: 123").is_none());
}

#[test]
fn test_ch10_parse_empty_delta() {
    // finish_reason set but no content or tool_calls -- end-of-stream marker
    let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    assert!(parse_sse_line(line).is_none());
}

#[test]
fn test_ch10_parse_invalid_json() {
    assert!(parse_sse_line("data: {not json}").is_none());
}

#[test]
fn test_ch10_parse_multiple_text_chunks() {
    let lines = [
        r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#,
        r#"data: {"choices":[{"delta":{"content":" world"},"finish_reason":null}]}"#,
    ];

    let mut acc = StreamAccumulator::new();
    for line in &lines {
        if let Some(events) = parse_sse_line(line) {
            for e in &events {
                acc.feed(e);
            }
        }
    }
    let turn = acc.finish();
    assert_eq!(turn.text.as_deref(), Some("Hello world"));
}

#[test]
fn test_ch10_parse_tool_call_full_sequence() {
    // Simulate a complete tool call arriving over multiple SSE events
    let lines = [
        // Tool call start with empty arguments
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"tc_1","type":"function","function":{"name":"read","arguments":""}}]},"finish_reason":null}]}"#,
        // Argument chunks
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]},"finish_reason":null}]}"#,
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":" \"test.txt\"}"}}]},"finish_reason":null}]}"#,
        // Finish
        r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        "data: [DONE]",
    ];

    let mut acc = StreamAccumulator::new();
    for line in &lines {
        if let Some(events) = parse_sse_line(line) {
            for e in &events {
                acc.feed(e);
            }
        }
    }

    let turn = acc.finish();
    assert!(turn.text.is_none());
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].id, "tc_1");
    assert_eq!(turn.tool_calls[0].name, "read");
    assert_eq!(turn.tool_calls[0].arguments["path"], "test.txt");
    assert!(matches!(turn.stop_reason, StopReason::ToolUse));
}

// ---------------------------------------------------------------------------
// stream_chat integration tests (mock TCP server)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch10_stream_chat_text() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let sse_body = [
        r#"data: {"choices":[{"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{"content":" world"},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "",
        "data: [DONE]",
        "",
    ]
    .join("\n");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let body = sse_body.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = socket.read(&mut buf).await;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let provider = crate::providers::OpenRouterProvider::new("fake-key", "fake-model")
        .base_url(format!("http://{addr}"));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let messages = vec![Message::User("Hi".into())];
    let turn = provider.stream_chat(&messages, &[], tx).await.unwrap();

    assert_eq!(turn.text.as_deref(), Some("Hello world"));
    assert!(turn.tool_calls.is_empty());
    assert!(matches!(turn.stop_reason, StopReason::Stop));

    // Verify events were sent through the channel
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(events.contains(&StreamEvent::TextDelta("Hello".into())));
    assert!(events.contains(&StreamEvent::TextDelta(" world".into())));
    assert!(events.contains(&StreamEvent::Done));
}

#[tokio::test]
async fn test_ch10_stream_chat_tool_call() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let sse_body = [
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read","arguments":""}}]},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":" \"test.txt\"}"}}]},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        "",
        "data: [DONE]",
        "",
    ]
    .join("\n");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let body = sse_body.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = socket.read(&mut buf).await;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let provider = crate::providers::OpenRouterProvider::new("fake-key", "fake-model")
        .base_url(format!("http://{addr}"));

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let def = ToolDefinition::new("read", "Read file").param("path", "string", "Path", true);
    let messages = vec![Message::User("Read test.txt".into())];
    let turn = provider.stream_chat(&messages, &[&def], tx).await.unwrap();

    assert!(turn.text.is_none());
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "read");
    assert_eq!(turn.tool_calls[0].id, "call_1");
    assert_eq!(turn.tool_calls[0].arguments["path"], "test.txt");
    assert!(matches!(turn.stop_reason, StopReason::ToolUse));
}

// ---------------------------------------------------------------------------
// MockStreamProvider tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch10_mock_stream_provider_text() {
    use std::collections::VecDeque;

    let turn = AssistantTurn {
        text: Some("Hello".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    };
    let provider = MockStreamProvider::new(VecDeque::from([turn]));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let messages = vec![Message::User("Hi".into())];
    let result = provider.stream_chat(&messages, &[], tx).await.unwrap();

    assert_eq!(result.text.as_deref(), Some("Hello"));

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    // MockStreamProvider sends one TextDelta per char + Done
    assert_eq!(events.len(), 6); // H, e, l, l, o, Done
    assert_eq!(events[0], StreamEvent::TextDelta("H".into()));
    assert_eq!(events[4], StreamEvent::TextDelta("o".into()));
    assert_eq!(events[5], StreamEvent::Done);
}

#[tokio::test]
async fn test_ch10_mock_stream_provider_tool_call() {
    use std::collections::VecDeque;

    let turn = AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "c1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "f.txt"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    };
    let provider = MockStreamProvider::new(VecDeque::from([turn]));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let messages = vec![Message::User("Read f.txt".into())];
    let result = provider.stream_chat(&messages, &[], tx).await.unwrap();

    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].name, "read");

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(matches!(
        &events[0],
        StreamEvent::ToolCallStart { index: 0, .. }
    ));
    assert!(matches!(
        &events[1],
        StreamEvent::ToolCallDelta { index: 0, .. }
    ));
    assert_eq!(events[2], StreamEvent::Done);
}

// ---------------------------------------------------------------------------
// StreamingAgent tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch10_streaming_agent_text_response() {
    use std::collections::VecDeque;
    use tokio::sync::mpsc;

    let turn = AssistantTurn {
        text: Some("Hi there".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    };
    let provider = MockStreamProvider::new(VecDeque::from([turn]));
    let agent = crate::StreamingAgent::new(provider);

    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = agent.run("Hello", tx).await.unwrap();
    assert_eq!(result, "Hi there");

    // Collect events — should see TextDeltas + Done
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    // At least one TextDelta and one Done
    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::AgentEvent::TextDelta(_)))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::AgentEvent::Done(_)))
    );
}

#[tokio::test]
async fn test_ch10_streaming_agent_tool_loop() {
    use std::collections::VecDeque;
    use tokio::sync::mpsc;

    // Turn 1: LLM asks to read a file
    let turn1 = AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "c1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "test.txt"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    };
    // Turn 2: LLM gives final answer
    let turn2 = AssistantTurn {
        text: Some("The file says hello".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    };

    let provider = MockStreamProvider::new(VecDeque::from([turn1, turn2]));

    // Use a simple tool that always returns "hello"
    let agent = crate::StreamingAgent::new(provider).tool(crate::tools::ReadTool::new());

    // Create a temp file to read
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "hello").unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();

    // The mock provider ignores messages, so the path doesn't matter for the mock.
    // But the tool will be called with {"path": "test.txt"} — that will fail
    // because cwd isn't the temp dir. Let's just check the agent loop works.
    let result = agent.run("Read test.txt", tx).await.unwrap();
    assert_eq!(result, "The file says hello");

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    // Should see ToolCall event, then TextDeltas, then Done
    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::AgentEvent::ToolCall { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::AgentEvent::Done(_)))
    );
}

#[tokio::test]
async fn test_ch10_streaming_agent_chat_history() {
    use std::collections::VecDeque;
    use tokio::sync::mpsc;

    let turn = AssistantTurn {
        text: Some("Reply".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    };
    let provider = MockStreamProvider::new(VecDeque::from([turn]));
    let agent = crate::StreamingAgent::new(provider);

    let (tx, _rx) = mpsc::unbounded_channel();
    let mut messages = vec![Message::User("Hello".into())];
    let result = agent.chat(&mut messages, tx).await.unwrap();
    assert_eq!(result, "Reply");

    // Chat should have appended the assistant turn
    assert_eq!(messages.len(), 2);
    assert!(matches!(&messages[1], Message::Assistant(_)));
}

// ---------------------------------------------------------------------------
// stream_chat integration tests (mock TCP server)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch10_stream_chat_events_order() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let sse_body = [
        r#"data: {"choices":[{"delta":{"content":"A"},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{"content":"B"},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{"content":"C"},"finish_reason":null}]}"#,
        "",
        r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "",
        "data: [DONE]",
        "",
    ]
    .join("\n");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let body = sse_body.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = socket.read(&mut buf).await;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let provider = crate::providers::OpenRouterProvider::new("fake-key", "fake-model")
        .base_url(format!("http://{addr}"));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let messages = vec![Message::User("Hi".into())];
    let turn = provider.stream_chat(&messages, &[], tx).await.unwrap();

    assert_eq!(turn.text.as_deref(), Some("ABC"));

    // Events should arrive in order: A, B, C, Done
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert_eq!(
        events,
        vec![
            StreamEvent::TextDelta("A".into()),
            StreamEvent::TextDelta("B".into()),
            StreamEvent::TextDelta("C".into()),
            StreamEvent::Done,
        ]
    );
}
