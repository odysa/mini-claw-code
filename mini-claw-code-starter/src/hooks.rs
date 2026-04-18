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
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
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

    pub fn register(&mut self, hook: impl Hook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    pub fn with(mut self, hook: impl Hook + 'static) -> Self {
        self.register(hook);
        self
    }

    /// Fire every hook for `event` and fold their responses into one `HookAction`.
    ///
    /// Hints:
    /// - `Block(reason)` short-circuits — return it immediately.
    /// - `ModifyArgs(new_args)` accumulates; the last modification wins.
    /// - Return `ModifyArgs` if any hook modified args, otherwise `Continue`.
    pub async fn dispatch(&self, _event: &HookEvent) -> HookAction {
        unimplemented!(
            "TODO ch15: run every hook; Block short-circuits, ModifyArgs accumulates (last wins)"
        )
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
    /// Record a short tag for each event and always continue.
    ///
    /// Hint: Formats are `pre:{tool}`, `post:{tool}`, `agent:start`, `agent:end`.
    async fn on_event(&self, _event: &HookEvent) -> HookAction {
        unimplemented!("TODO ch15: push a summary of the event into self.log, return Continue")
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
    /// Block `PreToolCall` events whose tool name is in `blocked_tools`.
    async fn on_event(&self, _event: &HookEvent) -> HookAction {
        unimplemented!(
            "TODO ch15: on PreToolCall for a blocked tool, return Block(reason); else Continue"
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
    /// Run `self.command` when the event matches; block if the shell exits non-zero.
    ///
    /// Hints:
    /// - Only Pre/PostToolCall fire the shell; other events return Continue.
    /// - Skip the shell if `self.matches_tool(tool_name)` is false.
    /// - Non-zero exit → `Block(format!("hook failed: {stderr}"))`.
    /// - Spawn error → `Block(format!("hook error: {e}"))`.
    async fn on_event(&self, _event: &HookEvent) -> HookAction {
        unimplemented!(
            "TODO ch15: for matching tool events run sh -c command; non-zero or spawn error → Block"
        )
    }
}
