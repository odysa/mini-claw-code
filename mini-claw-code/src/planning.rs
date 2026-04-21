use std::collections::HashSet;

use tokio::sync::mpsc;

use crate::agent::{AgentEvent, emit_tool_events, push_tool_results};
use crate::streaming::{StreamEvent, StreamProvider};
use crate::types::*;

const DEFAULT_PLAN_PROMPT: &str = "\
You are in PLANNING MODE. Explore the codebase using the available tools and \
create a plan. You can read files, run shell commands, and ask the user \
questions — but you CANNOT write, edit, or create files.\n\n\
When your plan is ready, call the `exit_plan` tool to submit it for review.";

/// A two-phase agent that separates planning (read-only) from execution (all tools).
///
/// During the **plan** phase a system prompt is injected telling the LLM it is
/// in planning mode, and only tools whose [`Tool::is_read_only`] returns `true`
/// (plus `exit_plan`) are visible. The LLM calls `exit_plan` when its plan is
/// ready, or stops naturally. During the **execute** phase all registered tools
/// are available. The caller drives the approval flow between the two phases.
///
/// An explicit `plan_tool_names` override is available for the occasional tool
/// whose read-only-ness depends on arguments.
pub struct PlanAgent<P: StreamProvider> {
    provider: P,
    tools: ToolSet,
    plan_tool_override: Option<HashSet<String>>,
    plan_system_prompt: String,
    config: QueryConfig,
    exit_plan_def: ToolDefinition,
}

impl<P: StreamProvider> PlanAgent<P> {
    /// Create a new `PlanAgent` that uses `Tool::is_read_only()` to decide
    /// which tools are available during planning.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            tools: ToolSet::new(),
            plan_tool_override: None,
            plan_system_prompt: DEFAULT_PLAN_PROMPT.to_string(),
            config: QueryConfig::default(),
            exit_plan_def: ToolDefinition::new(
                "exit_plan",
                "Signal that your plan is complete and ready for user review. \
                 Call this when you have finished exploring and are ready to present your plan.",
            ),
        }
    }

    /// Register a tool (builder pattern, same as `SimpleAgent`).
    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    /// Override which tools are allowed during planning by name. When unset,
    /// planning uses `Tool::is_read_only()`; when set, only names in the
    /// override are visible (plus `exit_plan`).
    pub fn plan_tool_names<I>(mut self, names: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.plan_tool_override = Some(names.into_iter().map(Into::into).collect());
        self
    }

    /// Override the system prompt injected at the start of the planning phase.
    pub fn plan_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.plan_system_prompt = prompt.into();
        self
    }

    /// Override the loop limits (max turns, max tool result size).
    pub fn config(mut self, config: QueryConfig) -> Self {
        self.config = config;
        self
    }

    /// Is `tool_name` allowed during the planning phase?
    fn is_plan_allowed(&self, tool_name: &str) -> bool {
        if let Some(names) = &self.plan_tool_override {
            return names.contains(tool_name);
        }
        self.tools.get(tool_name).is_some_and(|t| t.is_read_only())
    }

    /// Definitions visible to the LLM during planning: read-only tools + `exit_plan`.
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

    /// Execute only the plan-allowed tools; blocked calls yield an error result
    /// so the model learns it can't use them here.
    async fn execute_plan_calls(&self, calls: &[ToolCall]) -> Vec<(String, ToolResult)> {
        let mut allowed: Vec<ToolCall> = Vec::new();
        let mut blocked: Vec<(String, ToolResult)> = Vec::new();

        for call in calls {
            if call.name == "exit_plan" {
                continue; // handled by the caller
            }
            if self.is_plan_allowed(&call.name) {
                allowed.push(ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
            } else {
                blocked.push((
                    call.id.clone(),
                    ToolResult::error(format!(
                        "tool '{}' is not available in planning mode",
                        call.name
                    )),
                ));
            }
        }

        let mut results = self
            .tools
            .execute_calls(&allowed, self.config.max_result_chars)
            .await;
        results.extend(blocked);
        results
    }

    /// Run the **planning** phase: only read-only tools (plus `exit_plan`) are visible.
    ///
    /// Injects a system prompt if one is not already present, telling the LLM
    /// it is in planning mode. Returns when the LLM either calls `exit_plan`
    /// or stops naturally.
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

        let defs = self.plan_definitions();

        for _ in 0..self.config.max_turns {
            let turn = run_streaming_turn(&self.provider, messages, &defs, &events).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    let _ = events.send(AgentEvent::Done(text.clone()));
                    messages.push(Message::Assistant(turn));
                    return Ok(text);
                }
                StopReason::ToolUse => {
                    // Detect exit_plan: finish the phase, signal completion.
                    if let Some(exit_id) = turn
                        .tool_calls
                        .iter()
                        .find(|c| c.name == "exit_plan")
                        .map(|c| c.id.clone())
                    {
                        let text = turn.text.clone().unwrap_or_default();
                        messages.push(Message::Assistant(turn));
                        messages.push(Message::ToolResult {
                            id: exit_id,
                            content: "Plan submitted for review.".into(),
                        });
                        let _ = events.send(AgentEvent::Done(text.clone()));
                        return Ok(text);
                    }

                    let results = self.execute_plan_calls(&turn.tool_calls).await;
                    emit_tool_events(&events, &turn.tool_calls, &results, &self.tools);
                    push_tool_results(messages, turn, results);
                }
            }
        }

        let err = format!(
            "exceeded max turns ({}) during planning",
            self.config.max_turns
        );
        let _ = events.send(AgentEvent::Error(err.clone()));
        anyhow::bail!(err)
    }

    /// Run the **execution** phase: all registered tools are available.
    pub async fn execute(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        stream_chat_loop(
            &self.provider,
            &self.tools,
            &self.config,
            messages,
            Some(&events),
        )
        .await
    }
}

/// Run one streaming turn, forwarding text deltas to `events` and returning
/// the assembled [`AssistantTurn`].
pub(crate) async fn run_streaming_turn<P: StreamProvider>(
    provider: &P,
    messages: &[Message],
    defs: &[&ToolDefinition],
    events: &mpsc::UnboundedSender<AgentEvent>,
) -> anyhow::Result<AssistantTurn> {
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();
    let events_clone = events.clone();
    let forwarder = tokio::spawn(async move {
        while let Some(event) = stream_rx.recv().await {
            if let StreamEvent::TextDelta(text) = event {
                let _ = events_clone.send(AgentEvent::TextDelta(text));
            }
        }
    });

    let result = provider.stream_chat(messages, defs, stream_tx).await;
    let _ = forwarder.await;

    match result {
        Ok(turn) => Ok(turn),
        Err(e) => {
            let _ = events.send(AgentEvent::Error(e.to_string()));
            Err(e)
        }
    }
}

/// The streaming counterpart to [`chat_loop`](crate::agent::chat_loop).
///
/// Shared by [`PlanAgent::execute`] and [`StreamingAgent`](crate::streaming::StreamingAgent).
/// `events` is optional in signature for symmetry with `chat_loop`, but in
/// practice streaming agents always pass `Some` because the UI needs the
/// text deltas.
pub(crate) async fn stream_chat_loop<P: StreamProvider>(
    provider: &P,
    tools: &ToolSet,
    config: &QueryConfig,
    messages: &mut Vec<Message>,
    events: Option<&mpsc::UnboundedSender<AgentEvent>>,
) -> anyhow::Result<String> {
    let defs = tools.definitions();

    for _ in 0..config.max_turns {
        let turn = match events {
            Some(tx) => run_streaming_turn(provider, messages, &defs, tx).await?,
            None => {
                // Silent path: still need a channel for the provider, just discard.
                let (noop_tx, _noop_rx) = mpsc::unbounded_channel();
                provider.stream_chat(messages, &defs, noop_tx).await?
            }
        };

        match turn.stop_reason {
            StopReason::Stop => {
                let text = turn.text.clone().unwrap_or_default();
                if let Some(tx) = events {
                    let _ = tx.send(AgentEvent::Done(text.clone()));
                }
                messages.push(Message::Assistant(turn));
                return Ok(text);
            }
            StopReason::ToolUse => {
                let results = tools
                    .execute_calls(&turn.tool_calls, config.max_result_chars)
                    .await;
                if let Some(tx) = events {
                    emit_tool_events(tx, &turn.tool_calls, &results, tools);
                }
                push_tool_results(messages, turn, results);
            }
        }
    }

    let err = format!("exceeded max turns ({})", config.max_turns);
    if let Some(tx) = events {
        let _ = tx.send(AgentEvent::Error(err.clone()));
    }
    anyhow::bail!(err)
}
