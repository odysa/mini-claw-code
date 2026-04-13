use std::collections::HashSet;

use tokio::sync::mpsc;

use crate::engine::{QueryConfig, QueryEvent};
use crate::provider::Provider;
use crate::types::*;

/// A two-phase agent: plan first with read-only tools, then execute with all tools.
///
/// Mirrors Claude Code's plan mode. During planning, the agent can only use
/// read-only tools (read, glob, grep) to gather information. During execution,
/// all tools are available. This separation ensures the agent understands the
/// task before modifying anything.
///
/// The plan phase ends when:
/// - The model calls `exit_plan` (explicit signal)
/// - The model produces a final text response (implicit signal)
///
/// The caller controls the transition: after `plan()` returns, the caller
/// can inspect the plan, ask the user for approval, and then call `execute()`.
pub struct PlanEngine<P: Provider> {
    provider: P,
    tools: ToolSet,
    config: QueryConfig,
    /// Tool names allowed during planning. Overrides `is_read_only()`.
    plan_tools: HashSet<String>,
    /// System prompt injected during planning phase.
    plan_prompt: Option<String>,
    /// The exit_plan tool definition.
    exit_plan_def: ToolDefinition,
}

impl<P: Provider> PlanEngine<P> {
    pub fn new(provider: P) -> Self {
        let exit_plan_def = ToolDefinition::new(
            "exit_plan",
            "Signal that the plan is complete. Call this when you have finished \
             analyzing the task and are ready to present your plan.",
        );

        Self {
            provider,
            tools: ToolSet::new(),
            config: QueryConfig::default(),
            plan_tools: HashSet::new(),
            plan_prompt: None,
            exit_plan_def,
        }
    }

    pub fn config(mut self, config: QueryConfig) -> Self {
        self.config = config;
        self
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    pub fn tools(mut self, tools: ToolSet) -> Self {
        self.tools = tools;
        self
    }

    /// Override which tools are allowed during planning.
    ///
    /// By default, tools where `is_read_only()` returns true are allowed.
    /// This method replaces that set entirely.
    pub fn plan_tool_names(mut self, names: &[&str]) -> Self {
        self.plan_tools = names.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Set a custom system prompt for the planning phase.
    pub fn plan_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.plan_prompt = Some(prompt.into());
        self
    }

    /// Run the planning phase.
    ///
    /// Only read-only tools (or those in `plan_tools`) are available.
    /// The `exit_plan` tool is injected to let the model signal completion.
    /// Returns the plan text.
    pub async fn plan(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
        // Inject planning system prompt if not already present
        self.maybe_inject_plan_prompt(messages);

        let plan_defs = self.plan_definitions();
        let mut turns = 0;

        loop {
            if turns >= self.config.max_turns {
                anyhow::bail!(
                    "exceeded max turns ({}) during planning",
                    self.config.max_turns
                );
            }

            let turn = self.provider.chat(messages, &plan_defs).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    messages.push(Message::Assistant(turn));
                    return Ok(text);
                }
                StopReason::ToolUse => {
                    // Check for exit_plan — capture ID before moving turn
                    if let Some(exit_id) = turn
                        .tool_calls
                        .iter()
                        .find(|c| c.name == "exit_plan")
                        .map(|c| c.id.clone())
                    {
                        let text = turn.text.clone().unwrap_or_default();
                        messages.push(Message::Assistant(turn));
                        messages.push(Message::tool_result(exit_id, "Plan phase complete."));
                        return Ok(text);
                    }

                    // Execute allowed tools, block others
                    let results = self.execute_plan_tools(&turn.tool_calls).await;
                    messages.push(Message::Assistant(turn));
                    for (id, result) in results {
                        messages.push(Message::tool_result(id, result.content));
                    }
                }
            }

            turns += 1;
        }
    }

    /// Run the planning phase with event streaming.
    pub async fn plan_with_events(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<QueryEvent>,
    ) -> Option<String> {
        self.maybe_inject_plan_prompt(messages);

        let plan_defs = self.plan_definitions();
        let mut turns = 0;

        loop {
            if turns >= self.config.max_turns {
                let _ = events.send(QueryEvent::Error(format!(
                    "exceeded max turns ({}) during planning",
                    self.config.max_turns
                )));
                return None;
            }

            let turn = match self.provider.chat(messages, &plan_defs).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = events.send(QueryEvent::Error(e.to_string()));
                    return None;
                }
            };

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    let _ = events.send(QueryEvent::Done(text.clone()));
                    messages.push(Message::Assistant(turn));
                    return Some(text);
                }
                StopReason::ToolUse => {
                    if let Some(exit_id) = turn
                        .tool_calls
                        .iter()
                        .find(|c| c.name == "exit_plan")
                        .map(|c| c.id.clone())
                    {
                        let text = turn.text.clone().unwrap_or_default();
                        let _ = events.send(QueryEvent::Done(text.clone()));
                        messages.push(Message::Assistant(turn));
                        messages.push(Message::tool_result(exit_id, "Plan phase complete."));
                        return Some(text);
                    }

                    for call in &turn.tool_calls {
                        if let Some(t) = self.tools.get(&call.name) {
                            let _ = events.send(QueryEvent::ToolStart {
                                name: call.name.clone(),
                                summary: t.summary(&call.arguments),
                            });
                        }
                    }

                    let results = self.execute_plan_tools(&turn.tool_calls).await;

                    for (id, result) in &results {
                        let call_name = turn
                            .tool_calls
                            .iter()
                            .find(|c| c.id == *id)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        let _ = events.send(QueryEvent::ToolEnd {
                            name: call_name,
                            result: if result.content.len() > 200 {
                                format!("{}...", &result.content[..200])
                            } else {
                                result.content.clone()
                            },
                        });
                    }

                    messages.push(Message::Assistant(turn));
                    for (id, result) in results {
                        messages.push(Message::tool_result(id, result.content));
                    }
                }
            }

            turns += 1;
        }
    }

    /// Run the execution phase with all tools available.
    ///
    /// Call this after `plan()` returns and the user has approved.
    /// The message history from planning is preserved.
    pub async fn execute(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
        let defs = self.tools.definitions();
        let mut turns = 0;

        loop {
            if turns >= self.config.max_turns {
                anyhow::bail!(
                    "exceeded max turns ({}) during execution",
                    self.config.max_turns
                );
            }

            let turn = self.provider.chat(messages, &defs).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    messages.push(Message::Assistant(turn));
                    return Ok(text);
                }
                StopReason::ToolUse => {
                    let results = self
                        .tools
                        .execute_calls(&turn.tool_calls, self.config.max_result_chars)
                        .await;
                    messages.push(Message::Assistant(turn));
                    for (id, result) in results {
                        messages.push(Message::tool_result(id, result.content));
                    }
                }
            }

            turns += 1;
        }
    }

    // --- Private helpers ---

    fn maybe_inject_plan_prompt(&self, messages: &mut Vec<Message>) {
        let prompt = self.plan_prompt.as_deref().unwrap_or(
            "You are in PLANNING mode. Analyze the task using read-only tools. \
             Do NOT modify any files. When your analysis is complete, call exit_plan \
             or provide your plan as a text response.",
        );

        // Don't inject if already present
        let already_has = messages
            .iter()
            .any(|m| matches!(m, Message::System(s) if s.tag.as_deref() == Some("plan_mode")));

        if !already_has {
            messages.insert(
                0,
                Message::System(SystemMessage {
                    id: crate::types::new_id(),
                    content: prompt.to_string(),
                    tag: Some("plan_mode".into()),
                }),
            );
        }
    }

    fn is_plan_allowed(&self, tool_name: &str) -> bool {
        if !self.plan_tools.is_empty() {
            return self.plan_tools.contains(tool_name);
        }
        // Default: check is_read_only on the tool
        self.tools
            .get(tool_name)
            .map(|t| t.is_read_only())
            .unwrap_or(false)
    }

    fn plan_definitions(&self) -> Vec<&ToolDefinition> {
        let mut defs: Vec<&ToolDefinition> = self
            .tools
            .definitions()
            .into_iter()
            .filter(|d| self.is_plan_allowed(d.name))
            .collect();
        defs.push(&self.exit_plan_def);
        defs
    }

    async fn execute_plan_tools(&self, calls: &[ToolCall]) -> Vec<(String, ToolResult)> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            if call.name == "exit_plan" {
                continue;
            }
            let result = if !self.is_plan_allowed(&call.name) {
                (
                    call.id.clone(),
                    ToolResult::error(format!("`{}` is not available in planning mode", call.name)),
                )
            } else {
                // Delegate single call to shared execute_calls
                let mut r = self
                    .tools
                    .execute_calls(std::slice::from_ref(call), self.config.max_result_chars)
                    .await;
                r.pop().unwrap()
            };
            results.push(result);
        }
        results
    }
}
