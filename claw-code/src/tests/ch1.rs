use crate::types::*;

// --- Message creation ---

#[test]
fn test_ch1_create_user_message() {
    let msg = Message::user("Hello");
    if let Message::User(u) = msg {
        assert_eq!(u.content, "Hello");
        assert!(!u.id.is_empty());
    } else {
        panic!("expected User message");
    }
}

#[test]
fn test_ch1_create_system_message() {
    let msg = Message::system("You are helpful");
    if let Message::System(s) = msg {
        assert_eq!(s.content, "You are helpful");
        assert!(s.tag.is_none());
    } else {
        panic!("expected System message");
    }
}

#[test]
fn test_ch1_create_tool_result() {
    let msg = Message::tool_result("call_1", "file contents");
    if let Message::ToolResult(r) = msg {
        assert_eq!(r.tool_use_id, "call_1");
        assert_eq!(r.content, "file contents");
        assert!(!r.is_truncated);
    } else {
        panic!("expected ToolResult message");
    }
}

#[test]
fn test_ch1_create_assistant_message() {
    let msg = Message::assistant(Some("Hello!".into()), vec![], StopReason::Stop, None);
    if let Message::Assistant(a) = msg {
        assert_eq!(a.text.as_deref(), Some("Hello!"));
        assert!(a.tool_calls.is_empty());
        assert_eq!(a.stop_reason, StopReason::Stop);
        assert!(a.usage.is_none());
    } else {
        panic!("expected Assistant message");
    }
}

#[test]
fn test_ch1_assistant_with_tool_calls() {
    let msg = Message::assistant(
        None,
        vec![ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "test.txt"}),
        }],
        StopReason::ToolUse,
        Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        }),
    );
    if let Message::Assistant(a) = msg {
        assert!(a.text.is_none());
        assert_eq!(a.tool_calls.len(), 1);
        assert_eq!(a.tool_calls[0].name, "read");
        assert_eq!(a.stop_reason, StopReason::ToolUse);
        assert_eq!(a.usage.as_ref().unwrap().input_tokens, 100);
    } else {
        panic!("expected Assistant message");
    }
}

#[test]
fn test_ch1_unique_message_ids() {
    let m1 = Message::user("a");
    let m2 = Message::user("b");
    let id1 = match &m1 {
        Message::User(u) => &u.id,
        _ => unreachable!(),
    };
    let id2 = match &m2 {
        Message::User(u) => &u.id,
        _ => unreachable!(),
    };
    assert_ne!(id1, id2);
}

// --- ToolDefinition ---

#[test]
fn test_ch1_tool_definition_builder() {
    let def = ToolDefinition::new("read", "Read a file").param("path", "string", "File path", true);
    assert_eq!(def.name, "read");
    assert_eq!(def.description, "Read a file");
    let props = &def.parameters["properties"];
    assert!(props["path"].is_object());
    let required = def.parameters["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("path")));
}

#[test]
fn test_ch1_tool_definition_optional_param() {
    let def = ToolDefinition::new("read", "Read")
        .param("path", "string", "File path", true)
        .param("offset", "number", "Line offset", false);
    let required = def.parameters["required"].as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&serde_json::json!("path")));
}

// --- TokenUsage ---

#[test]
fn test_ch1_token_usage_default() {
    let usage = TokenUsage::default();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
    assert_eq!(usage.total_tokens(), 0);
}

#[test]
fn test_ch1_token_usage_total() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        ..Default::default()
    };
    assert_eq!(usage.total_tokens(), 150);
}

// --- ToolSet ---

#[test]
fn test_ch1_toolset_empty() {
    let ts = ToolSet::new();
    assert!(ts.is_empty());
    assert_eq!(ts.len(), 0);
}

// --- ToolResult ---

#[test]
fn test_ch1_tool_result_text() {
    let r = ToolResult::text("hello");
    assert_eq!(r.content, "hello");
    assert!(!r.is_truncated);
}

#[test]
fn test_ch1_tool_result_error() {
    let r = ToolResult::error("not found");
    assert_eq!(r.content, "error: not found");
}

// --- StopReason ---

#[test]
fn test_ch1_stop_reason_equality() {
    assert_eq!(StopReason::Stop, StopReason::Stop);
    assert_eq!(StopReason::ToolUse, StopReason::ToolUse);
    assert_ne!(StopReason::Stop, StopReason::ToolUse);
}

// --- ModelUsage ---

#[test]
fn test_ch1_model_usage_record() {
    let mut mu = ModelUsage::default();
    mu.record(
        &TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        },
        0.005,
    );
    mu.record(
        &TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            ..Default::default()
        },
        0.01,
    );
    assert_eq!(mu.input_tokens, 300);
    assert_eq!(mu.output_tokens, 150);
    assert_eq!(mu.turn_count, 2);
    assert!((mu.cost_usd - 0.015).abs() < 0.001);
}
