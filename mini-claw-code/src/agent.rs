use crate::types::*;
use tokio::sync::mpsc;

/// Events emitted by the agent during execution.
#[derive(Debug)]
pub enum AgentEvent {
    /// A chunk of text streamed from the LLM (streaming mode only).
    TextDelta(String),
    /// A tool is about to run. Carries the tool name and a one-line summary
    /// suitable for a terminal log.
    ToolStart { name: String, summary: String },
    /// A tool finished. Carries the tool name and a preview of its result
    /// (UI-truncated — may be shorter than the content fed back to the model).
    ToolEnd { name: String, result: String },
    /// The agent finished with a final response.
    Done(String),
    /// The agent encountered an error.
    Error(String),
}

/// Maximum characters for tool result previews in `ToolEnd` events.
/// The full (already model-truncated) content still goes into the message
/// history; this only caps what the UI sees.
const DISPLAY_TRUNCATE_LIMIT: usize = 200;

/// Emit `ToolStart` and `ToolEnd` events for a set of tool calls and their results.
///
/// Factored out so every agent loop — streaming and non-streaming — fires
/// events in the same order and with the same shape.
pub(crate) fn emit_tool_events(
    events: &mpsc::UnboundedSender<AgentEvent>,
    calls: &[ToolCall],
    results: &[(String, ToolResult)],
    tools: &ToolSet,
) {
    for call in calls {
        let summary = tools
            .get(&call.name)
            .map(|t| t.summary(&call.arguments))
            .unwrap_or_else(|| format!("[{}]", call.name));
        let _ = events.send(AgentEvent::ToolStart {
            name: call.name.clone(),
            summary,
        });
    }
    for (id, result) in results {
        let name = calls
            .iter()
            .find(|c| c.id == *id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let preview = if result.content.len() > DISPLAY_TRUNCATE_LIMIT {
            let at = truncate_utf8(&result.content, DISPLAY_TRUNCATE_LIMIT);
            format!("{}...", &result.content[..at])
        } else {
            result.content.clone()
        };
        let _ = events.send(AgentEvent::ToolEnd {
            name,
            result: preview,
        });
    }
}

/// Push an assistant turn and its tool results onto the message history.
pub(crate) fn push_tool_results(
    messages: &mut Vec<Message>,
    turn: AssistantTurn,
    results: Vec<(String, ToolResult)>,
) {
    messages.push(Message::Assistant(turn));
    for (id, result) in results {
        messages.push(Message::ToolResult {
            id,
            content: result.content,
        });
    }
}

/// The core agent loop.
///
/// One place to match on `StopReason`, dispatch tools via
/// [`ToolSet::execute_calls`], and push results — used by
/// [`SimpleAgent`], [`SubagentTool`](crate::subagent::SubagentTool),
/// and the non-streaming path of anything that speaks [`Provider`].
///
/// `events` is optional: when `Some`, fires `ToolStart` / `ToolEnd` / `Done` /
/// `Error` events. When `None`, silently drives the loop and returns the result.
/// Either way, the caller owns `messages` and gets the final text back.
pub async fn chat_loop<P: Provider>(
    provider: &P,
    tools: &ToolSet,
    config: &QueryConfig,
    messages: &mut Vec<Message>,
    events: Option<&mpsc::UnboundedSender<AgentEvent>>,
) -> anyhow::Result<String> {
    let defs = tools.definitions();

    for _ in 0..config.max_turns {
        let turn = match provider.chat(messages, &defs).await {
            Ok(t) => t,
            Err(e) => {
                if let Some(tx) = events {
                    let _ = tx.send(AgentEvent::Error(e.to_string()));
                }
                return Err(e);
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

/// Handle a single prompt with at most one round of tool calls.
///
/// This function demonstrates the raw protocol:
/// 1. Send the prompt to the provider
/// 2. Match on stop_reason
/// 3. If Stop → return text
/// 4. If ToolUse → execute tools, send results, get final answer
pub async fn single_turn<P: Provider>(
    provider: &P,
    tools: &ToolSet,
    prompt: &str,
) -> anyhow::Result<String> {
    let defs = tools.definitions();
    let mut messages = vec![Message::User(prompt.to_string())];

    let turn = provider.chat(&messages, &defs).await?;

    match turn.stop_reason {
        StopReason::Stop => Ok(turn.text.unwrap_or_default()),
        StopReason::ToolUse => {
            let config = QueryConfig::default();
            for call in &turn.tool_calls {
                let summary = tools
                    .get(&call.name)
                    .map(|t| t.summary(&call.arguments))
                    .unwrap_or_else(|| format!("[{}]", call.name));
                print!("\x1b[2K\r    {summary}\n");
            }

            let results = tools
                .execute_calls(&turn.tool_calls, config.max_result_chars)
                .await;
            push_tool_results(&mut messages, turn, results);

            let final_turn = provider.chat(&messages, &defs).await?;
            Ok(final_turn.text.unwrap_or_default())
        }
    }
}

pub struct SimpleAgent<P: Provider> {
    provider: P,
    tools: ToolSet,
    config: QueryConfig,
}

impl<P: Provider> SimpleAgent<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            tools: ToolSet::new(),
            config: QueryConfig::default(),
        }
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    /// Override the loop limits (max turns, max tool result size).
    pub fn config(mut self, config: QueryConfig) -> Self {
        self.config = config;
        self
    }

    /// Run the agent loop against an existing message history.
    ///
    /// The caller pushes `Message::User(…)` before calling. On return the
    /// vec contains the full conversation including the assistant's final turn.
    pub async fn run_with_history(
        &self,
        mut messages: Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> Vec<Message> {
        let _ = chat_loop(
            &self.provider,
            &self.tools,
            &self.config,
            &mut messages,
            Some(&events),
        )
        .await;
        messages
    }

    /// Run the agent loop, sending events through the channel instead of
    /// printing to stdout.
    pub async fn run_with_events(&self, prompt: &str, events: mpsc::UnboundedSender<AgentEvent>) {
        let mut messages = vec![Message::User(prompt.to_string())];
        let _ = chat_loop(
            &self.provider,
            &self.tools,
            &self.config,
            &mut messages,
            Some(&events),
        )
        .await;
    }

    /// Run the agent loop, accumulating into the provided message history.
    ///
    /// The caller pushes `Message::User(…)` before calling; on return the
    /// vec contains the full conversation including the assistant's final
    /// turn. Returns the text of the final response. Tool summaries are
    /// printed to stdout so the CLI demo can show progress; use
    /// [`run_with_events`](Self::run_with_events) for a silent loop.
    pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let forwarder = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let AgentEvent::ToolStart { summary, .. } = event {
                    print!("\x1b[2K\r    {summary}\n");
                }
            }
        });
        let result = chat_loop(
            &self.provider,
            &self.tools,
            &self.config,
            messages,
            Some(&tx),
        )
        .await;
        drop(tx);
        let _ = forwarder.await;
        result
    }

    pub async fn run(&self, prompt: &str) -> anyhow::Result<String> {
        let mut messages = vec![Message::User(prompt.to_string())];
        self.chat(&mut messages).await
    }
}
