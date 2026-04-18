use std::collections::HashSet;

use tokio::sync::mpsc;

use crate::agent::{AgentEvent, tool_summary};
use crate::streaming::{StreamEvent, StreamProvider};
use crate::types::*;

const DEFAULT_PLAN_PROMPT: &str = "\
You are in PLANNING MODE. Explore the codebase using the available tools and \
create a plan. You can read files, run shell commands, and ask the user \
questions — but you CANNOT write, edit, or create files.\n\n\
When your plan is ready, call the `exit_plan` tool to submit it for review.";

/// A two-phase agent that separates planning (read-only) from execution (all tools).
///
/// # Chapter 16: Plan Mode
///
/// During the **plan** phase only read-only tools (plus `exit_plan`) are visible.
/// During the **execute** phase all registered tools are available.
pub struct PlanAgent<P: StreamProvider> {
    provider: P,
    tools: ToolSet,
    read_only: HashSet<&'static str>,
    plan_system_prompt: String,
    exit_plan_def: ToolDefinition,
}

impl<P: StreamProvider> PlanAgent<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            tools: ToolSet::new(),
            read_only: HashSet::from(["bash", "read", "ask_user"]),
            plan_system_prompt: DEFAULT_PLAN_PROMPT.to_string(),
            exit_plan_def: ToolDefinition::new(
                "exit_plan",
                "Signal that your plan is complete and ready for user review. \
                 Call this when you have finished exploring and are ready to present your plan.",
            ),
        }
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    /// Override the set of tool names allowed during planning.
    pub fn read_only(mut self, names: &[&'static str]) -> Self {
        self.read_only = names.iter().copied().collect();
        self
    }

    /// Override the system prompt for planning.
    pub fn plan_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.plan_system_prompt = prompt.into();
        self
    }

    /// Run the planning phase: only read-only tools + exit_plan.
    #[allow(clippy::ptr_arg)]
    pub async fn plan(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        if !messages
            .first()
            .is_some_and(|m| matches!(m, Message::System(_)))
        {
            messages.insert(0, Message::System(self.plan_system_prompt.clone()));
        }
        self.run_loop(messages, Some(&self.read_only), events).await
    }

    /// Run the execution phase: all tools available.
    #[allow(clippy::ptr_arg)]
    pub async fn execute(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        self.run_loop(messages, None, events).await
    }

    /// Shared agent loop. When `allowed` is Some, only those tools + exit_plan are permitted.
    ///
    /// Hints:
    /// - Build the tool list: if `allowed` is Some, keep matching tools and append `exit_plan_def`.
    /// - Loop: stream_chat → on Stop return text; on ToolUse execute each call.
    /// - During planning, reject non-allowed tools with an error-shaped ToolResult,
    ///   and treat `exit_plan` as a synthetic tool whose result is "Plan submitted for review."
    /// - After executing tools, push Message::Assistant(turn) then each Message::ToolResult.
    /// - If the plan was submitted, emit AgentEvent::Done(plan_text) and return.
    #[allow(clippy::ptr_arg)]
    async fn run_loop(
        &self,
        _messages: &mut Vec<Message>,
        _allowed: Option<&HashSet<&'static str>>,
        _events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        unimplemented!(
            "TODO ch16: streaming agent loop gated by `allowed`; handle exit_plan specially"
        )
    }
}
