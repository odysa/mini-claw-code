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
/// # Chapter 13: Plan Mode
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
        unimplemented!(
            "Initialize with provider, empty ToolSet, default read_only set, default plan prompt, and exit_plan ToolDefinition"
        )
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        unimplemented!("Push tool, return self")
    }

    /// Override the set of tool names allowed during planning.
    pub fn read_only(mut self, names: &[&'static str]) -> Self {
        unimplemented!("Replace self.read_only with the given names")
    }

    /// Override the system prompt for planning.
    pub fn plan_prompt(mut self, prompt: impl Into<String>) -> Self {
        unimplemented!("Set self.plan_system_prompt")
    }

    /// Run the planning phase: only read-only tools + exit_plan.
    ///
    /// Hints:
    /// - Inject system prompt at position 0 if not already present
    /// - Call run_loop with Some(&self.read_only)
    pub async fn plan(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        unimplemented!("Inject system prompt if needed, call run_loop with allowed set")
    }

    /// Run the execution phase: all tools available.
    pub async fn execute(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        unimplemented!("Call run_loop with None (no restrictions)")
    }

    /// Shared agent loop. When `allowed` is Some, only those tools + exit_plan are permitted.
    ///
    /// Hints:
    /// - Filter tool definitions based on allowed set
    /// - If allowed is Some, add exit_plan_def to the definitions
    /// - Loop: stream_chat -> match stop_reason
    /// - For ToolUse: handle exit_plan specially (return plan text),
    ///   block tools not in allowed set, execute allowed tools normally
    async fn run_loop(
        &self,
        messages: &mut Vec<Message>,
        allowed: Option<&HashSet<&'static str>>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        unimplemented!("Streaming agent loop with tool filtering and exit_plan handling")
    }
}
