use std::collections::VecDeque;

use crate::provider::*;
use crate::types::*;

// --- MockProvider ---

#[tokio::test]
async fn test_ch2_mock_returns_text() {
    let provider = MockProvider::new(VecDeque::from([AssistantMessage {
        id: "1".into(),
        text: Some("Hello!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));
    let turn = provider.chat(&[Message::user("Hi")], &[]).await.unwrap();
    assert_eq!(turn.text.as_deref(), Some("Hello!"));
}

#[tokio::test]
async fn test_ch2_mock_returns_tool_calls() {
    let provider = MockProvider::new(VecDeque::from([AssistantMessage {
        id: "1".into(),
        text: None,
        tool_calls: vec![ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "test.txt"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    }]));
    let turn = provider.chat(&[], &[]).await.unwrap();
    assert!(turn.text.is_none());
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.stop_reason, StopReason::ToolUse);
}

#[tokio::test]
async fn test_ch2_mock_sequence() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: Some("First".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: Some("Second".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));
    let t1 = provider.chat(&[], &[]).await.unwrap();
    let t2 = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(t1.text.as_deref(), Some("First"));
    assert_eq!(t2.text.as_deref(), Some("Second"));
}

#[tokio::test]
async fn test_ch2_mock_exhausted() {
    let provider = MockProvider::new(VecDeque::new());
    assert!(provider.chat(&[], &[]).await.is_err());
}

// --- MockStreamProvider ---

#[tokio::test]
async fn test_ch2_mock_stream_text() {
    let provider = MockStreamProvider::new(VecDeque::from([AssistantMessage {
        id: "1".into(),
        text: Some("Hi".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let turn = provider.stream_chat(&[], &[], tx).await.unwrap();
    assert_eq!(turn.text.as_deref(), Some("Hi"));

    // Should have received char-by-char deltas + Done
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta(s) if s == "H"))
    );
    assert!(events.iter().any(|e| matches!(e, StreamEvent::Done)));
}

// --- SSE parsing ---

#[test]
fn test_ch2_parse_sse_text_delta() {
    use crate::provider::openrouter::parse_sse_line;
    let line = r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
    let events = parse_sse_line(line).unwrap();
    assert_eq!(events, vec![StreamEvent::TextDelta("Hello".into())]);
}

#[test]
fn test_ch2_parse_sse_done() {
    use crate::provider::openrouter::parse_sse_line;
    let events = parse_sse_line("data: [DONE]").unwrap();
    assert_eq!(events, vec![StreamEvent::Done]);
}

#[test]
fn test_ch2_parse_sse_non_data() {
    use crate::provider::openrouter::parse_sse_line;
    assert!(parse_sse_line("event: ping").is_none());
    assert!(parse_sse_line("").is_none());
}

// --- StreamAccumulator ---

#[test]
fn test_ch2_accumulator_text() {
    use crate::provider::openrouter::StreamAccumulator;
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::TextDelta("Hello".into()));
    acc.feed(&StreamEvent::TextDelta(" world".into()));
    acc.feed(&StreamEvent::Done);
    let turn = acc.finish();
    assert_eq!(turn.text.as_deref(), Some("Hello world"));
    assert_eq!(turn.stop_reason, StopReason::Stop);
}

#[test]
fn test_ch2_accumulator_tool_call() {
    use crate::provider::openrouter::StreamAccumulator;
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::ToolCallStart {
        index: 0,
        id: "c1".into(),
        name: "read".into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 0,
        arguments: r#"{"path":"#.into(),
    });
    acc.feed(&StreamEvent::ToolCallDelta {
        index: 0,
        arguments: r#""test.txt"}"#.into(),
    });
    acc.feed(&StreamEvent::Done);
    let turn = acc.finish();
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "read");
    assert_eq!(turn.stop_reason, StopReason::ToolUse);
}
