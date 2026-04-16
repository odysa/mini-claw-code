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
    #[allow(clippy::ptr_arg)]
    async fn run_loop(
        &self,
        messages: &mut Vec<Message>,
        allowed: Option<&HashSet<&'static str>>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        let all_defs = self.tools.definitions();
        let defs: Vec<&ToolDefinition> = match allowed {
            Some(names) => {
                let mut filtered: Vec<&ToolDefinition> = all_defs
                    .into_iter()
                    .filter(|d| names.contains(d.name))
                    .collect();
                filtered.push(&self.exit_plan_def);
                filtered
            }
            None => all_defs,
        };

        loop {
            let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();
            let events_clone = events.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(event) = stream_rx.recv().await {
                    if let StreamEvent::TextDelta(text) = event {
                        let _ = events_clone.send(AgentEvent::TextDelta(text));
                    }
                }
            });

            let turn = match self.provider.stream_chat(messages, &defs, stream_tx).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = events.send(AgentEvent::Error(e.to_string()));
                    return Err(e);
                }
            };
            let _ = forwarder.await;

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    let _ = events.send(AgentEvent::Done(text.clone()));
                    messages.push(Message::Assistant(turn));
                    return Ok(text);
                }
                StopReason::ToolUse => {
                    let mut results = Vec::with_capacity(turn.tool_calls.len());
                    let mut exit_plan = false;

                    for call in &turn.tool_calls {
                        if allowed.is_some() && call.name == "exit_plan" {
                            results.push((call.id.clone(), "Plan submitted for review.".into()));
                            exit_plan = true;
                            continue;
                        }

                        if let Some(names) = allowed
                            && !names.contains(call.name.as_str())
                        {
                            results.push((
                                call.id.clone(),
                                format!(
                                    "error: tool '{}' is not available in planning mode",
                                    call.name
                                ),
                            ));
                            continue;
                        }

                        let _ = events.send(AgentEvent::ToolCall {
                            name: call.name.clone(),
                            summary: tool_summary(call),
                        });
                        let content = match self.tools.get(&call.name) {
                            Some(t) => t
                                .call(call.arguments.clone())
                                .await
                                .unwrap_or_else(|e| format!("error: {e}")),
                            None => format!("error: unknown tool `{}`", call.name),
                        };
                        results.push((call.id.clone(), content));
                    }

                    let plan_text = turn.text.clone().unwrap_or_default();
                    messages.push(Message::Assistant(turn));
                    for (id, content) in results {
                        messages.push(Message::ToolResult { id, content });
                    }

                    if exit_plan {
                        let _ = events.send(AgentEvent::Done(plan_text.clone()));
                        return Ok(plan_text);
                    }
                }
            }
        }
    }
}
