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
    pub fn new(
        provider: Arc<P>,
        tools_factory: impl Fn() -> ToolSet + Send + Sync + 'static,
    ) -> Self {
        unimplemented!("Initialize with provider, factory, None system_prompt, max_turns=10, ToolDefinition")
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        unimplemented!("Set self.system_prompt")
    }

    pub fn max_turns(mut self, max: usize) -> Self {
        unimplemented!("Set self.max_turns")
    }
}

#[async_trait::async_trait]
impl<P: Provider + 'static> Tool for SubagentTool<P> {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        unimplemented!("Extract task, create child agent loop, return result")
    }
}
