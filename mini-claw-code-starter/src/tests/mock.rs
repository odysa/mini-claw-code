use std::collections::VecDeque;

use crate::mock::MockProvider;
use crate::types::*;

#[tokio::test]
async fn test_mock_returns_text() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Hello, world!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let turn = provider
        .chat(&[Message::User("Hi".into())], &[])
        .await
        .unwrap();

    assert_eq!(turn.text.as_deref(), Some("Hello, world!"));
    assert!(turn.tool_calls.is_empty());
}

#[tokio::test]
async fn test_mock_returns_tool_calls() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "test.txt"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    }]));

    let turn = provider
        .chat(&[Message::User("read test.txt".into())], &[])
        .await
        .unwrap();

    assert!(turn.text.is_none());
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "read");
    assert_eq!(turn.tool_calls[0].id, "call_1");
}

#[tokio::test]
async fn test_mock_steps_through_sequence() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantTurn {
            text: Some("First".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        AssistantTurn {
            text: Some("Second".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
        AssistantTurn {
            text: Some("Third".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let t1 = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(t1.text.as_deref(), Some("First"));

    let t2 = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(t2.text.as_deref(), Some("Second"));

    let t3 = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(t3.text.as_deref(), Some("Third"));
}

// --- New tests ---

#[tokio::test]
async fn test_mock_empty_responses_exhausted() {
    let provider = MockProvider::new(VecDeque::new());
    let result = provider.chat(&[], &[]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_returns_none_text() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert!(turn.text.is_none());
    assert!(turn.tool_calls.is_empty());
}

#[tokio::test]
async fn test_mock_text_with_tool_calls() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("I'll read that file".into()),
        tool_calls: vec![ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "foo.txt"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(turn.text.as_deref(), Some("I'll read that file"));
    assert_eq!(turn.tool_calls.len(), 1);
}

#[tokio::test]
async fn test_mock_multiple_tool_calls_in_one_turn() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![
            ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "a.txt"}),
            },
            ToolCall {
                id: "call_2".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "b.txt"}),
            },
            ToolCall {
                id: "call_3".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"command": "ls"}),
            },
        ],
        stop_reason: StopReason::ToolUse,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(turn.tool_calls.len(), 3);
    assert_eq!(turn.tool_calls[0].id, "call_1");
    assert_eq!(turn.tool_calls[1].id, "call_2");
    assert_eq!(turn.tool_calls[2].name, "bash");
}

#[tokio::test]
async fn test_mock_exhausted_after_all_consumed() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("only one".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let _ = provider.chat(&[], &[]).await.unwrap();
    let result = provider.chat(&[], &[]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_ignores_messages_and_tools() {
    // MockProvider ignores the messages and tools arguments entirely.
    let def = ToolDefinition::new("test", "a test tool");
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("fixed response".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let messages = vec![
        Message::User("Hello".into()),
        Message::ToolResult {
            id: "x".into(),
            content: "y".into(),
        },
    ];
    let turn = provider.chat(&messages, &[&def]).await.unwrap();
    assert_eq!(turn.text.as_deref(), Some("fixed response"));
}

#[tokio::test]
async fn test_mock_tool_call_arguments_preserved() {
    let args = serde_json::json!({
        "path": "/some/file.txt",
        "nested": {"key": [1, 2, 3]}
    });

    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: args.clone(),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(turn.tool_calls[0].arguments, args);
}

#[tokio::test]
async fn test_mock_stop_reason_stop() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("done".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert!(matches!(turn.stop_reason, StopReason::Stop));
}

#[tokio::test]
async fn test_mock_stop_reason_tool_use() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: None,
        tool_calls: vec![ToolCall {
            id: "c".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert!(matches!(turn.stop_reason, StopReason::ToolUse));
}

#[tokio::test]
async fn test_mock_empty_text_is_some() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some(String::new()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let turn = provider.chat(&[], &[]).await.unwrap();
    assert_eq!(turn.text.as_deref(), Some(""));
}

#[tokio::test]
async fn test_mock_long_sequence() {
    let responses: VecDeque<AssistantTurn> = (0..10)
        .map(|i| AssistantTurn {
            text: Some(format!("response_{i}")),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        })
        .collect();

    let provider = MockProvider::new(responses);

    for i in 0..10 {
        let turn = provider.chat(&[], &[]).await.unwrap();
        assert_eq!(turn.text.as_deref(), Some(format!("response_{i}").as_str()));
    }

    assert!(provider.chat(&[], &[]).await.is_err());
}
