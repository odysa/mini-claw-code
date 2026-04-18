use serde_json::json;

use crate::hooks::*;

#[tokio::test]
async fn test_hooks_logging_hook() {
    let hook = LoggingHook::new();
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({"command": "ls"}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);

    let messages = hook.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "pre:bash");
}

#[tokio::test]
async fn test_hooks_logging_hook_multiple_events() {
    let hook = LoggingHook::new();

    hook.on_event(&HookEvent::AgentStart {
        prompt: "hello".into(),
    })
    .await;
    hook.on_event(&HookEvent::PreToolCall {
        tool_name: "read".into(),
        args: json!({}),
    })
    .await;
    hook.on_event(&HookEvent::PostToolCall {
        tool_name: "read".into(),
        args: json!({}),
        result: "content".into(),
    })
    .await;
    hook.on_event(&HookEvent::AgentEnd {
        response: "done".into(),
    })
    .await;

    let messages = hook.messages();
    assert_eq!(
        messages,
        vec!["agent:start", "pre:read", "post:read", "agent:end"]
    );
}

#[tokio::test]
async fn test_hooks_blocking_hook() {
    let hook = BlockingHook::new(vec!["bash".into()], "bash is disabled");
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Block("bash is disabled".into()));
}

#[tokio::test]
async fn test_hooks_blocking_hook_allows_other_tools() {
    let hook = BlockingHook::new(vec!["bash".into()], "blocked");
    let event = HookEvent::PreToolCall {
        tool_name: "read".into(),
        args: json!({}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_hooks_registry_dispatch_continue() {
    let registry = HookRegistry::new().with(LoggingHook::new());

    let event = HookEvent::PreToolCall {
        tool_name: "read".into(),
        args: json!({}),
    };
    let action = registry.dispatch(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_hooks_registry_dispatch_block() {
    let registry = HookRegistry::new()
        .with(LoggingHook::new())
        .with(BlockingHook::new(vec!["bash".into()], "no bash"));

    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = registry.dispatch(&event).await;
    assert_eq!(action, HookAction::Block("no bash".into()));
}

#[tokio::test]
async fn test_hooks_registry_multiple_hooks_order() {
    let log1 = std::sync::Arc::new(LoggingHook::new());
    let log2 = std::sync::Arc::new(LoggingHook::new());

    // Create a custom hook that uses the Arc'd loggers
    struct ArcHook(std::sync::Arc<LoggingHook>);
    #[async_trait::async_trait]
    impl Hook for ArcHook {
        async fn on_event(&self, event: &HookEvent) -> HookAction {
            self.0.on_event(event).await
        }
    }

    let registry = HookRegistry::new()
        .with(ArcHook(log1.clone()))
        .with(ArcHook(log2.clone()));

    let event = HookEvent::PreToolCall {
        tool_name: "read".into(),
        args: json!({}),
    };
    registry.dispatch(&event).await;

    // Both hooks should have been called
    assert_eq!(log1.messages().len(), 1);
    assert_eq!(log2.messages().len(), 1);
}

#[tokio::test]
async fn test_hooks_registry_block_short_circuits() {
    let log = std::sync::Arc::new(LoggingHook::new());

    struct ArcHook(std::sync::Arc<LoggingHook>);
    #[async_trait::async_trait]
    impl Hook for ArcHook {
        async fn on_event(&self, event: &HookEvent) -> HookAction {
            self.0.on_event(event).await
        }
    }

    let registry = HookRegistry::new()
        .with(BlockingHook::new(vec!["bash".into()], "blocked"))
        .with(ArcHook(log.clone()));

    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = registry.dispatch(&event).await;
    assert_eq!(action, HookAction::Block("blocked".into()));

    // The second hook should NOT have been called
    assert_eq!(log.messages().len(), 0);
}

#[test]
fn test_hooks_registry_is_empty() {
    let registry = HookRegistry::new();
    assert!(registry.is_empty());

    let registry = registry.with(LoggingHook::new());
    assert!(!registry.is_empty());
}

#[tokio::test]
async fn test_hooks_post_tool_event() {
    let hook = LoggingHook::new();
    let event = HookEvent::PostToolCall {
        tool_name: "write".into(),
        args: json!({"path": "test.txt"}),
        result: "ok".into(),
    };
    hook.on_event(&event).await;
    assert_eq!(hook.messages(), vec!["post:write"]);
}
