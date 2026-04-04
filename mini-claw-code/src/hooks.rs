use serde_json::Value;

/// An event that triggers hooks.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Before a tool is executed.
    PreToolCall { tool_name: String, args: Value },
    /// After a tool finishes executing.
    PostToolCall {
        tool_name: String,
        args: Value,
        result: String,
    },
    /// The agent is starting a new run.
    AgentStart { prompt: String },
    /// The agent finished with a final response.
    AgentEnd { response: String },
}

/// What a hook tells the agent to do after firing.
#[derive(Debug, Clone, PartialEq)]
pub enum HookAction {
    /// Continue normally.
    Continue,
    /// Block the tool call with a reason.
    Block(String),
    /// Modify the tool arguments (PreToolCall only).
    ModifyArgs(Value),
}

/// A hook that reacts to agent events.
///
/// Implement this trait to add custom behavior at key points in the agent loop.
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
    /// Handle an event and return an action.
    async fn on_event(&self, event: &HookEvent) -> HookAction;
}

/// A registry that stores hooks and dispatches events to them.
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Register a hook.
    pub fn register(&mut self, hook: impl Hook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    /// Register a hook (builder pattern).
    pub fn with(mut self, hook: impl Hook + 'static) -> Self {
        self.register(hook);
        self
    }

    /// Dispatch an event to all hooks in order.
    ///
    /// Returns `Block` on the first blocking response, or `ModifyArgs` if
    /// any hook modifies the arguments, or `Continue` if all hooks pass.
    pub async fn dispatch(&self, event: &HookEvent) -> HookAction {
        let mut modified_args: Option<Value> = None;

        for hook in &self.hooks {
            match hook.on_event(event).await {
                HookAction::Continue => {}
                HookAction::Block(reason) => return HookAction::Block(reason),
                HookAction::ModifyArgs(new_args) => {
                    modified_args = Some(new_args);
                }
            }
        }

        match modified_args {
            Some(args) => HookAction::ModifyArgs(args),
            None => HookAction::Continue,
        }
    }

    /// Returns true if there are no registered hooks.
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in hooks
// ---------------------------------------------------------------------------

/// A hook that logs all events to a Vec (useful for testing).
pub struct LoggingHook {
    log: std::sync::Mutex<Vec<String>>,
}

impl LoggingHook {
    pub fn new() -> Self {
        Self {
            log: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Get a copy of all logged messages.
    pub fn messages(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl Default for LoggingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Hook for LoggingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        let msg = match event {
            HookEvent::PreToolCall { tool_name, .. } => format!("pre:{tool_name}"),
            HookEvent::PostToolCall { tool_name, .. } => format!("post:{tool_name}"),
            HookEvent::AgentStart { .. } => "agent:start".into(),
            HookEvent::AgentEnd { .. } => "agent:end".into(),
        };
        self.log.lock().unwrap().push(msg);
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

#[async_trait::async_trait]
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
pub struct ShellHook {
    command: String,
    tool_pattern: Option<glob::Pattern>,
}

impl ShellHook {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            tool_pattern: None,
        }
    }

    pub fn for_tool(mut self, pattern: &str) -> Self {
        self.tool_pattern = glob::Pattern::new(pattern).ok();
        self
    }

    fn matches_tool(&self, tool_name: &str) -> bool {
        match &self.tool_pattern {
            Some(pattern) => pattern.matches(tool_name),
            None => true,
        }
    }
}

#[async_trait::async_trait]
impl Hook for ShellHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        let tool_name = match event {
            HookEvent::PreToolCall { tool_name, .. } => tool_name,
            HookEvent::PostToolCall { tool_name, .. } => tool_name,
            _ => return HookAction::Continue,
        };

        if !self.matches_tool(tool_name) {
            return HookAction::Continue;
        }

        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .output()
            .await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    HookAction::Continue
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    HookAction::Block(format!("hook failed: {stderr}"))
                }
            }
            Err(e) => HookAction::Block(format!("hook error: {e}")),
        }
    }
}
