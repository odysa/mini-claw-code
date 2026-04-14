use serde_json::Value;

/// An event that triggers hooks.
#[derive(Debug, Clone)]
pub enum HookEvent {
    PreToolCall {
        tool_name: String,
        args: Value,
    },
    PostToolCall {
        tool_name: String,
        args: Value,
        result: String,
    },
    AgentStart {
        prompt: String,
    },
    AgentEnd {
        response: String,
    },
}

/// What a hook tells the agent to do after firing.
#[derive(Debug, Clone, PartialEq)]
pub enum HookAction {
    Continue,
    Block(String),
    ModifyArgs(Value),
}

/// A hook that reacts to agent events.
///
/// # Chapter 12: Hooks
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
    async fn on_event(&self, event: &HookEvent) -> HookAction;
}

/// A registry that stores hooks and dispatches events to them.
///
/// # Chapter 12: Hooks
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: impl Hook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    pub fn with(mut self, hook: impl Hook + 'static) -> Self {
        self.register(hook);
        self
    }

    /// Dispatch an event to all hooks in order.
    ///
    /// Hints:
    /// - Iterate hooks in order
    /// - If any hook returns Block, return Block immediately
    /// - If any hook returns ModifyArgs, remember the new args
    /// - If all hooks return Continue (and no ModifyArgs), return Continue
    pub async fn dispatch(&self, event: &HookEvent) -> HookAction {
        unimplemented!("Iterate hooks, handle Block/ModifyArgs/Continue")
    }

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
    /// Log a short description of each event.
    ///
    /// Hint: Format as "pre:{tool_name}", "post:{tool_name}", "agent:start", "agent:end"
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        unimplemented!("Format event as string, push to log, return Continue")
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
    /// Block if event is PreToolCall and tool_name is in blocked_tools.
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        unimplemented!(
            "Check if PreToolCall tool_name is in blocked_tools, return Block or Continue"
        )
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
    /// Run the shell command on Pre/PostToolCall events that match the pattern.
    ///
    /// Hints:
    /// - Only handle PreToolCall and PostToolCall events
    /// - Check matches_tool() first
    /// - Run: tokio::process::Command::new("sh").arg("-c").arg(&self.command).output()
    /// - Exit code 0 → Continue, non-zero → Block with stderr
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        unimplemented!(
            "Extract tool_name, check pattern, run shell command, map exit code to action"
        )
    }
}
