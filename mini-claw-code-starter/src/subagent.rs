use std::sync::Arc;

use serde_json::Value;

use crate::types::*;

/// A tool that spawns a child agent to handle a subtask independently.
///
/// # Bonus: Subagents (no V2 chapter yet)
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
        provider: Arc<P>,
        tools_factory: impl Fn() -> ToolSet + Send + Sync + 'static,
    ) -> Self {
        Self {
            provider,
            tools_factory: Box::new(tools_factory),
            system_prompt: None,
            max_turns: 10,
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
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter: task"))?;

        let tools = (self.tools_factory)();
        let defs = tools.definitions();

        let mut messages = Vec::new();
        if let Some(ref prompt) = self.system_prompt {
            messages.push(Message::System(prompt.clone()));
        }
        messages.push(Message::User(task.to_string()));

        for _ in 0..self.max_turns {
            let turn = self.provider.chat(&messages, &defs).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    return Ok(turn.text.unwrap_or_default());
                }
                StopReason::ToolUse => {
                    let mut results = Vec::with_capacity(turn.tool_calls.len());
                    for call in &turn.tool_calls {
                        let content = match tools.get(&call.name) {
                            Some(t) => t
                                .call(call.arguments.clone())
                                .await
                                .unwrap_or_else(|e| format!("error: {e}")),
                            None => format!("error: unknown tool `{}`", call.name),
                        };
                        results.push((call.id.clone(), content));
                    }
                    messages.push(Message::Assistant(turn));
                    for (id, content) in results {
                        messages.push(Message::ToolResult { id, content });
                    }
                }
            }
        }

        Ok("error: max turns exceeded".to_string())
    }
}
