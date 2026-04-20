use std::sync::Arc;

use serde_json::Value;

use crate::agent::chat_loop;
use crate::types::*;

/// A tool that spawns a child agent to handle a subtask independently.
///
/// When the parent LLM calls this tool with a `task` description, it creates an
/// ephemeral child agent with its own message history and tools, runs it to
/// completion, and returns the result as tool output. The parent sees only the
/// final answer — the child's internal messages never leak into the parent's
/// conversation.
///
/// Provider sharing uses `Arc<P>` — the blanket `impl Provider for Arc<P>`
/// (in `types.rs`) makes this work without cloning the provider.
///
/// Tools are produced by a closure factory because `Box<dyn Tool>` is not
/// cloneable. Each child spawn gets a fresh `ToolSet`.
pub struct SubagentTool<P: Provider> {
    provider: Arc<P>,
    tools_factory: Box<dyn Fn() -> ToolSet + Send + Sync>,
    system_prompt: Option<String>,
    config: QueryConfig,
    definition: ToolDefinition,
}

impl<P: Provider> SubagentTool<P> {
    /// Create a new `SubagentTool` with a shared provider and a closure that
    /// produces a fresh `ToolSet` for each child spawn.
    pub fn new(
        provider: Arc<P>,
        tools_factory: impl Fn() -> ToolSet + Send + Sync + 'static,
    ) -> Self {
        Self {
            provider,
            tools_factory: Box::new(tools_factory),
            system_prompt: None,
            // Subagents default to a tighter turn limit than the top-level agent
            // — a child that spins 50 times is almost certainly stuck.
            config: QueryConfig {
                max_turns: 10,
                ..QueryConfig::default()
            },
            definition: ToolDefinition::new(
                "subagent",
                "Spawn a child agent to handle a subtask independently. \
                 The child has its own message history and tools.",
            )
            .param(
                "task",
                "string",
                "A clear description of the subtask for the child agent to complete.",
                true,
            ),
        }
    }

    /// Set an optional system prompt for the child agent.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the maximum number of agent loop turns before the child is stopped.
    /// Defaults to 10.
    pub fn max_turns(mut self, max: usize) -> Self {
        self.config.max_turns = max;
        self
    }

    /// Override the full config (max_turns, max_result_chars).
    pub fn config(mut self, config: QueryConfig) -> Self {
        self.config = config;
        self
    }
}

#[async_trait::async_trait]
impl<P: Provider + 'static> Tool for SubagentTool<P> {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter: task"))?;

        let tools = (self.tools_factory)();

        let mut messages = Vec::new();
        if let Some(ref prompt) = self.system_prompt {
            messages.push(Message::System(prompt.clone()));
        }
        messages.push(Message::User(task.to_string()));

        match chat_loop(&*self.provider, &tools, &self.config, &mut messages, None).await {
            Ok(text) => Ok(ToolResult::text(text)),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}
