use std::collections::VecDeque;

use crate::context::ContextManager;
use crate::mock::MockProvider;
use crate::types::*;

#[test]
fn test_context_manager_below_threshold_no_compact() {
    let cm = ContextManager::new(10000, 4);
    assert!(!cm.should_compact());
}

#[test]
fn test_context_manager_triggers_at_threshold() {
    let mut cm = ContextManager::new(1000, 4);
    cm.record(&TokenUsage {
        input_tokens: 600,
        output_tokens: 500,
    });
    assert!(cm.should_compact());
}

#[test]
fn test_context_manager_tracks_tokens() {
    let mut cm = ContextManager::new(10000, 4);
    cm.record(&TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
    });
    cm.record(&TokenUsage {
        input_tokens: 200,
        output_tokens: 100,
    });
    assert_eq!(cm.tokens_used(), 450);
}

#[tokio::test]
async fn test_context_manager_compact_preserves_system_prompt() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Summary of conversation.".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let mut cm = ContextManager::new(100, 2);
    cm.record(&TokenUsage {
        input_tokens: 200,
        output_tokens: 0,
    });

    let mut messages = vec![
        Message::System("You are a helpful agent.".into()),
        Message::User("Hello".into()),
        Message::User("How are you?".into()),
        Message::User("What's the weather?".into()),
        Message::User("Recent 1".into()),
        Message::User("Recent 2".into()),
    ];

    cm.compact(&provider, &mut messages).await.unwrap();

    // Should have: system prompt + summary + 2 recent messages = 4
    assert!(messages.len() <= 4);
    // First message should still be the system prompt
    assert!(matches!(messages[0], Message::System(ref s) if s.contains("helpful agent")));
    // Summary should be present
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::System(s) if s.contains("summary")))
    );
}

#[tokio::test]
async fn test_context_manager_compact_too_few_messages() {
    let provider = MockProvider::new(VecDeque::new());

    let mut cm = ContextManager::new(100, 10);
    cm.record(&TokenUsage {
        input_tokens: 200,
        output_tokens: 0,
    });

    let mut messages = vec![Message::User("Hello".into())];
    let original_len = messages.len();

    // Should not compact when there aren't enough messages
    cm.compact(&provider, &mut messages).await.unwrap();
    assert_eq!(messages.len(), original_len);
}

#[tokio::test]
async fn test_context_manager_maybe_compact_skips_when_not_needed() {
    let provider = MockProvider::new(VecDeque::new());

    let mut cm = ContextManager::new(10000, 2);
    // Don't record any tokens — should stay below threshold

    let mut messages = vec![Message::User("Hello".into()), Message::User("World".into())];
    let original_len = messages.len();

    cm.maybe_compact(&provider, &mut messages).await.unwrap();
    assert_eq!(messages.len(), original_len);
}

#[tokio::test]
async fn test_context_manager_compact_preserves_recent() {
    let provider = MockProvider::new(VecDeque::from([AssistantTurn {
        text: Some("Earlier discussion summarized.".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    }]));

    let mut cm = ContextManager::new(100, 2);
    cm.record(&TokenUsage {
        input_tokens: 200,
        output_tokens: 0,
    });

    let mut messages = vec![
        Message::User("Old message 1".into()),
        Message::User("Old message 2".into()),
        Message::User("Old message 3".into()),
        Message::User("Recent A".into()),
        Message::User("Recent B".into()),
    ];

    cm.compact(&provider, &mut messages).await.unwrap();

    // The last two messages should be preserved
    let last = &messages[messages.len() - 1];
    let second_last = &messages[messages.len() - 2];
    assert!(matches!(last, Message::User(s) if s == "Recent B"));
    assert!(matches!(second_last, Message::User(s) if s == "Recent A"));
}
