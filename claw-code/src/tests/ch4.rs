use std::collections::VecDeque;

use serde_json::json;

use crate::engine::{QueryConfig, QueryEngine};
use crate::provider::MockProvider;
use crate::types::*;

// Simple test tool for the query engine
struct AddTool {
    def: ToolDefinition,
}

impl AddTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition::new("add", "Add two numbers")
                .param("a", "number", "First number", true)
                .param("b", "number", "Second number", true),
        }
    }
}

#[async_trait::async_trait]
impl Tool for AddTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let a = args["a"].as_f64().unwrap_or(0.0);
        let b = args["b"].as_f64().unwrap_or(0.0);
        Ok(ToolResult::text(format!("{}", a + b)))
    }
}

#[tokio::test]
async fn test_ch4_direct_text_response() {
    let provider = MockProvider::new(VecDeque::from([AssistantMessage {
        id: "1".into(),
        text: Some("Hello!".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let engine = QueryEngine::new(provider);
    let result = engine.run("Hi").await.unwrap();
    assert_eq!(result, "Hello!");
}

#[tokio::test]
async fn test_ch4_single_tool_call() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "add".into(),
                arguments: json!({"a": 2, "b": 3}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: Some("The sum is 5".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider).tool(AddTool::new());
    let result = engine.run("What is 2 + 3?").await.unwrap();
    assert_eq!(result, "The sum is 5");
}

#[tokio::test]
async fn test_ch4_multi_step_loop() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "add".into(),
                arguments: json!({"a": 1, "b": 2}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c2".into(),
                name: "add".into(),
                arguments: json!({"a": 3, "b": 4}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "3".into(),
            text: Some("Results: 3 and 7".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider).tool(AddTool::new());
    let result = engine.run("Add stuff").await.unwrap();
    assert_eq!(result, "Results: 3 and 7");
}

#[tokio::test]
async fn test_ch4_unknown_tool() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "nonexistent".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: Some("Tool not found".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider);
    let result = engine.run("Use tool").await.unwrap();
    assert_eq!(result, "Tool not found");
}

#[tokio::test]
async fn test_ch4_max_turns() {
    // Provider always returns tool calls — should hit max turns
    let mut responses = VecDeque::new();
    for i in 0..60 {
        responses.push_back(AssistantMessage {
            id: format!("{i}"),
            text: None,
            tool_calls: vec![ToolCall {
                id: format!("c{i}"),
                name: "add".into(),
                arguments: json!({"a": 1, "b": 1}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        });
    }
    let provider = MockProvider::new(responses);
    let engine = QueryEngine::new(provider)
        .tool(AddTool::new())
        .config(QueryConfig {
            max_turns: 3,
            ..Default::default()
        });
    let result = engine.run("loop forever").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("max turns"));
}

#[tokio::test]
async fn test_ch4_chat_preserves_history() {
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

    let engine = QueryEngine::new(provider);
    let mut messages = vec![Message::user("Hello")];
    let r1 = engine.chat(&mut messages).await.unwrap();
    assert_eq!(r1, "First");
    assert_eq!(messages.len(), 2); // User + Assistant

    messages.push(Message::user("Again"));
    let r2 = engine.chat(&mut messages).await.unwrap();
    assert_eq!(r2, "Second");
    assert_eq!(messages.len(), 4); // User + Assistant + User + Assistant
}

#[tokio::test]
async fn test_ch4_events_emitted() {
    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "add".into(),
                arguments: json!({"a": 1, "b": 2}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: Some("Done".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider).tool(AddTool::new());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.run_with_events("test", tx).await;

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }

    use crate::engine::QueryEvent;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, QueryEvent::ToolStart { name, .. } if name == "add"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, QueryEvent::ToolEnd { name, .. } if name == "add"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, QueryEvent::Done(s) if s == "Done"))
    );
}

#[tokio::test]
async fn test_ch4_result_truncation() {
    struct BigTool {
        def: ToolDefinition,
    }
    impl BigTool {
        fn new() -> Self {
            Self {
                def: ToolDefinition::new("big", "Returns big output"),
            }
        }
    }
    #[async_trait::async_trait]
    impl Tool for BigTool {
        fn definition(&self) -> &ToolDefinition {
            &self.def
        }
        async fn call(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::text("x".repeat(200)))
        }
    }

    let provider = MockProvider::new(VecDeque::from([
        AssistantMessage {
            id: "1".into(),
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "big".into(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        },
        AssistantMessage {
            id: "2".into(),
            text: Some("ok".into()),
            tool_calls: vec![],
            stop_reason: StopReason::Stop,
            usage: None,
        },
    ]));

    let engine = QueryEngine::new(provider)
        .tool(BigTool::new())
        .config(QueryConfig {
            max_result_chars: 50,
            ..Default::default()
        });
    let result = engine.run("test").await.unwrap();
    assert_eq!(result, "ok");
}

#[tokio::test]
async fn test_ch4_provider_error() {
    let provider = MockProvider::new(VecDeque::new());
    let engine = QueryEngine::new(provider);
    assert!(engine.run("Hi").await.is_err());
}
