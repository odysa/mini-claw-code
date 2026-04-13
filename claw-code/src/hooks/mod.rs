use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;

/// Events that hooks can respond to.
///
/// Mirrors Claude Code's hook event system. Hooks fire before and after
/// tool execution, and at agent lifecycle boundaries.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Fired before a tool call is executed.
    PreToolCall { tool_name: String, args: Value },
    /// Fired after a tool call completes.
    PostToolCall {
        tool_name: String,
        args: Value,
        result: String,
    },
    /// Fired when the agent starts processing a prompt.
    AgentStart { prompt: String },
    /// Fired when the agent finishes.
    AgentEnd { response: String },
}

/// What the hook wants the engine to do.
#[derive(Debug, Clone, PartialEq)]
pub enum HookAction {
    /// Continue with normal execution.
    Continue,
    /// Block the tool call with a reason.
    Block(String),
    /// Modify the tool call arguments.
    ModifyArgs(Value),
}

/// The hook trait — implement this to add custom behavior around tool calls.
///
/// Hooks are checked sequentially. If any hook returns `Block`, the tool call
/// is cancelled. If any hook returns `ModifyArgs`, the arguments are replaced
/// before execution.
#[async_trait]
pub trait Hook: Send + Sync {
    /// React to a hook event. Return an action telling the engine what to do.
    async fn on_event(&self, event: &HookEvent) -> HookAction;
}

/// Runs a list of hooks sequentially for a given event.
///
/// The runner evaluates hooks in order. The first `Block` action wins and
/// short-circuits. `ModifyArgs` actions accumulate (later hooks see modified
/// args). `Continue` is the default.
pub struct HookRunner {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRunner {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn with(mut self, hook: impl Hook + 'static) -> Self {
        self.hooks.push(Box::new(hook));
        self
    }

    pub fn push(&mut self, hook: impl Hook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// Run all hooks for a given event.
    ///
    /// Returns the final action. `Block` short-circuits immediately.
    /// `ModifyArgs` is accumulated. `Continue` is the default.
    pub async fn run(&self, event: &HookEvent) -> HookAction {
        let mut final_action = HookAction::Continue;

        for hook in &self.hooks {
            let action = hook.on_event(event).await;
            match action {
                HookAction::Block(_) => return action,
                HookAction::ModifyArgs(_) => final_action = action,
                HookAction::Continue => {}
            }
        }

        final_action
    }
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in hooks
// ---------------------------------------------------------------------------

/// A hook that records all events for inspection (testing/debugging).
pub struct LoggingHook {
    events: Mutex<Vec<HookEvent>>,
}

impl LoggingHook {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    /// Get a snapshot of all recorded events.
    pub fn events(&self) -> Vec<HookEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn event_count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

impl Default for LoggingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for LoggingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        self.events.lock().unwrap().push(event.clone());
        HookAction::Continue
    }
}

/// A hook that blocks specific tools by name.
pub struct BlockingHook {
    blocked_tools: Vec<String>,
    reason: String,
}

impl BlockingHook {
    pub fn new(blocked_tools: Vec<String>, reason: impl Into<String>) -> Self {
        Self {
            blocked_tools,
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl Hook for BlockingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        if let HookEvent::PreToolCall { tool_name, .. } = event
            && self.blocked_tools.iter().any(|b| b == tool_name)
        {
            return HookAction::Block(self.reason.clone());
        }
        HookAction::Continue
    }
}

/// A hook that runs a shell command on pre/post tool events.
///
/// The command receives the tool name and arguments as environment variables:
/// - `HOOK_TOOL_NAME` — the tool being called
/// - `HOOK_EVENT` — "pre_tool_call" or "post_tool_call"
///
/// If the command exits with a non-zero status on a pre-tool event,
/// the tool call is blocked. Commands that exceed the timeout are killed
/// and treated as failures.
pub struct ShellHook {
    /// Shell command to run.
    command: String,
    /// Only fire for these events. Empty means fire for all.
    event_filter: Vec<String>,
    /// Timeout for the shell command.
    timeout: std::time::Duration,
}

impl ShellHook {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            event_filter: Vec::new(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn on_events(mut self, events: Vec<String>) -> Self {
        self.event_filter = events;
        self
    }

    fn should_fire(&self, event_name: &str) -> bool {
        self.event_filter.is_empty() || self.event_filter.iter().any(|e| e == event_name)
    }
}

#[async_trait]
impl Hook for ShellHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        let (event_name, tool_name) = match event {
            HookEvent::PreToolCall { tool_name, .. } => ("pre_tool_call", tool_name.as_str()),
            HookEvent::PostToolCall { tool_name, .. } => ("post_tool_call", tool_name.as_str()),
            HookEvent::AgentStart { .. } => ("agent_start", ""),
            HookEvent::AgentEnd { .. } => ("agent_end", ""),
        };

        if !self.should_fire(event_name) {
            return HookAction::Continue;
        }

        let fut = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .env("HOOK_TOOL_NAME", tool_name)
            .env("HOOK_EVENT", event_name)
            .output();

        match tokio::time::timeout(self.timeout, fut).await {
            Ok(Ok(output)) => {
                if !output.status.success() && event_name == "pre_tool_call" {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    HookAction::Block(format!("hook command failed: {}", stderr.trim()))
                } else {
                    HookAction::Continue
                }
            }
            Ok(Err(e)) => HookAction::Block(format!("hook command error: {}", e)),
            Err(_) => HookAction::Block(format!("hook command timed out after {:?}", self.timeout)),
        }
    }
}
