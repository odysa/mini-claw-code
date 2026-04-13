use serde_json::json;

use crate::hooks::*;

// ---------------------------------------------------------------------------
// HookAction basics
// ---------------------------------------------------------------------------

#[test]
fn test_ch12_hook_action_continue() {
    let action = HookAction::Continue;
    assert_eq!(action, HookAction::Continue);
}

#[test]
fn test_ch12_hook_action_block() {
    let action = HookAction::Block("reason".into());
    assert_eq!(action, HookAction::Block("reason".into()));
}

#[test]
fn test_ch12_hook_action_modify_args() {
    let args = json!({"path": "/new/path"});
    let action = HookAction::ModifyArgs(args.clone());
    assert_eq!(action, HookAction::ModifyArgs(args));
}

// ---------------------------------------------------------------------------
// LoggingHook
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_logging_hook_records_events() {
    let hook = LoggingHook::new();
    assert_eq!(hook.event_count(), 0);

    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({"command": "ls"}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
    assert_eq!(hook.event_count(), 1);

    let events = hook.events();
    assert!(matches!(&events[0], HookEvent::PreToolCall { tool_name, .. } if tool_name == "bash"));
}

#[tokio::test]
async fn test_ch12_logging_hook_multiple_events() {
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

    assert_eq!(hook.event_count(), 4);
}

// ---------------------------------------------------------------------------
// BlockingHook
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_blocking_hook_blocks_tool() {
    let hook = BlockingHook::new(vec!["bash".into()], "bash is blocked");
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({"command": "rm -rf /"}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Block("bash is blocked".into()));
}

#[tokio::test]
async fn test_ch12_blocking_hook_allows_other_tools() {
    let hook = BlockingHook::new(vec!["bash".into()], "bash is blocked");
    let event = HookEvent::PreToolCall {
        tool_name: "read".into(),
        args: json!({"path": "/etc/passwd"}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_ch12_blocking_hook_ignores_post_events() {
    let hook = BlockingHook::new(vec!["bash".into()], "bash is blocked");
    let event = HookEvent::PostToolCall {
        tool_name: "bash".into(),
        args: json!({}),
        result: "output".into(),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
}

// ---------------------------------------------------------------------------
// HookRunner
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_runner_empty() {
    let runner = HookRunner::new();
    assert!(runner.is_empty());
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = runner.run(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_ch12_runner_logging_continues() {
    let runner = HookRunner::new().with(LoggingHook::new());
    assert_eq!(runner.len(), 1);
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = runner.run(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_ch12_runner_block_short_circuits() {
    let logging = LoggingHook::new();
    // We can't inspect the logging hook after it's moved into the runner,
    // so we test that Block is returned.
    let runner = HookRunner::new()
        .with(BlockingHook::new(vec!["bash".into()], "blocked"))
        .with(logging);

    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = runner.run(&event).await;
    assert_eq!(action, HookAction::Block("blocked".into()));
}

#[tokio::test]
async fn test_ch12_runner_multiple_hooks() {
    let runner = HookRunner::new()
        .with(LoggingHook::new())
        .with(LoggingHook::new());

    assert_eq!(runner.len(), 2);

    let event = HookEvent::PreToolCall {
        tool_name: "read".into(),
        args: json!({}),
    };
    let action = runner.run(&event).await;
    assert_eq!(action, HookAction::Continue);
}

// ---------------------------------------------------------------------------
// ShellHook
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ch12_shell_hook_success() {
    let hook = ShellHook::new("true"); // `true` always exits 0
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_ch12_shell_hook_failure_blocks() {
    let hook = ShellHook::new("false"); // `false` always exits 1
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = hook.on_event(&event).await;
    assert!(matches!(action, HookAction::Block(_)));
}

#[tokio::test]
async fn test_ch12_shell_hook_post_failure_continues() {
    // Post-tool failures don't block (the tool already ran)
    let hook = ShellHook::new("false");
    let event = HookEvent::PostToolCall {
        tool_name: "bash".into(),
        args: json!({}),
        result: "output".into(),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_ch12_shell_hook_event_filter() {
    let hook = ShellHook::new("true").on_events(vec!["pre_tool_call".into()]);

    // Pre-tool event — fires
    let pre = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = hook.on_event(&pre).await;
    assert_eq!(action, HookAction::Continue);

    // Agent start — filtered out, so should continue
    let start = HookEvent::AgentStart {
        prompt: "hello".into(),
    };
    let action = hook.on_event(&start).await;
    assert_eq!(action, HookAction::Continue);
}

#[tokio::test]
async fn test_ch12_shell_hook_receives_env_vars() {
    // Verify the hook passes HOOK_TOOL_NAME and HOOK_EVENT to the command
    let hook = ShellHook::new(
        r#"test "$HOOK_TOOL_NAME" = "bash" && test "$HOOK_EVENT" = "pre_tool_call""#,
    );
    let event = HookEvent::PreToolCall {
        tool_name: "bash".into(),
        args: json!({}),
    };
    let action = hook.on_event(&event).await;
    assert_eq!(action, HookAction::Continue);
}
