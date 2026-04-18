use std::sync::Arc;

use serde_json::Value;

use crate::types::*;

/// A tool that spawns a child agent to handle a subtask independently.
///
/// # Chapter 13: Subagents
///
/// The child gets its own message history and tools. The parent sees only
/// the final answer. Provider sharing uses `Arc<P>`.
pub struct SubagentTool<P: Provider> {
    provider: Arc<P>,
    tools_factory: Box<dyn Fn() -> ToolSet + Send + Sync>,
    system_prompt: Option<String>,
    max_turns: usize,
    definition: ToolDefinition,
}

impl<P: Provider> SubagentTool<P> {
    /// Create a SubagentTool with a provider and a tools factory.
    ///
    /// Hint: Build a ToolDefinition named "subagent" with a required "task" string param.
    /// Default `max_turns` to something small like 10.
    pub fn new(
        _provider: Arc<P>,
        _tools_factory: impl Fn() -> ToolSet + Send + Sync + 'static,
    ) -> Self {
        unimplemented!("TODO bonus: wire provider+factory and build the 'subagent' ToolDefinition")
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn max_turns(mut self, max: usize) -> Self {
        self.max_turns = max;
        self
    }
}

#[async_trait::async_trait]
impl<P: Provider + 'static> Tool for SubagentTool<P> {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    /// Run an isolated agent loop for the requested task and return its answer.
    ///
    /// Hints:
    /// - Extract the "task" string from args.
    /// - Build a fresh `ToolSet` from `(self.tools_factory)()` and start a new history.
    /// - Seed the history with an optional System prompt and the User task.
    /// - Loop up to `self.max_turns`: call provider → match stop_reason → execute tools.
    /// - On Stop return the text; if the budget runs out, return an error-shaped string.
    async fn call(&self, _args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "TODO bonus: run a self-contained agent loop for up to max_turns and return the final text"
        )
    }
}
