use std::collections::VecDeque;
use std::future::Future;

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::agent::{AgentEvent, tool_summary};
use crate::mock::MockProvider;
use crate::types::*;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A real-time event emitted while streaming an LLM response.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// A chunk of assistant text.
    TextDelta(String),
    /// A new tool call has started.
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    /// More argument JSON for a tool call in progress.
    ToolCallDelta { index: usize, arguments: String },
    /// The stream is complete.
    Done,
}

/// Collects [`StreamEvent`]s into a complete [`AssistantTurn`].
///
/// # Chapter 10: Streaming
///
/// The accumulator buffers text deltas and tool call fragments, then
/// assembles them into a complete turn when `finish()` is called.
pub struct StreamAccumulator {
    text: String,
    tool_calls: Vec<PartialToolCall>,
}

struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl StreamAccumulator {
    /// Create a new empty accumulator.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            tool_calls: Vec::new(),
        }
    }

    /// Process a single streaming event.
    ///
    /// Hints:
    /// - `TextDelta(s)` → append s to self.text
    /// - `ToolCallStart { index, id, name }` → pad self.tool_calls with empty
    ///   entries up to index, then set id and name at that index
    /// - `ToolCallDelta { index, arguments }` → append arguments to the
    ///   tool call at that index
    /// - `Done` → no-op
    pub fn feed(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TextDelta(s) => self.text.push_str(s),
            StreamEvent::ToolCallStart { index, id, name } => {
                while self.tool_calls.len() <= *index {
                    self.tool_calls.push(PartialToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                }
                self.tool_calls[*index].id = id.clone();
                self.tool_calls[*index].name = name.clone();
            }
            StreamEvent::ToolCallDelta { index, arguments } => {
                if let Some(tc) = self.tool_calls.get_mut(*index) {
                    tc.arguments.push_str(arguments);
                }
            }
            StreamEvent::Done => {}
        }
    }

    /// Consume the accumulator and produce a complete [`AssistantTurn`].
    ///
    /// Hints:
    /// - text: None if empty, Some otherwise
    /// - Filter out tool calls with empty names
    /// - Parse each tool call's arguments string as JSON (use serde_json::from_str,
    ///   fall back to Value::Null on parse error)
    /// - stop_reason: ToolUse if tool_calls is non-empty, Stop otherwise
    /// - usage: None
    pub fn finish(self) -> AssistantTurn {
        let text = if self.text.is_empty() {
            None
        } else {
            Some(self.text)
        };
        let tool_calls: Vec<ToolCall> = self
            .tool_calls
            .into_iter()
            .filter(|tc| !tc.name.is_empty())
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.name,
                arguments: serde_json::from_str(&tc.arguments).unwrap_or(Value::Null),
            })
            .collect();
        let stop_reason = if tool_calls.is_empty() {
            StopReason::Stop
        } else {
            StopReason::ToolUse
        };
        AssistantTurn {
            text,
            tool_calls,
            stop_reason,
            usage: None,
        }
    }
}

impl Default for StreamAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SSE parsing (OpenAI-compatible chunk format)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChunkResponse {
    choices: Vec<ChunkChoice>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    delta: Delta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Deserialize)]
struct DeltaToolCall {
    index: usize,
    id: Option<String>,
    function: Option<DeltaFunction>,
}

#[derive(Deserialize)]
struct DeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

/// Parse one SSE `data:` line into zero or more [`StreamEvent`]s.
///
/// # Chapter 10: SSE Parsing
///
/// Hints:
/// - Strip the `data: ` prefix. If no prefix, return None.
/// - If data is `[DONE]`, return `Some(vec![StreamEvent::Done])`
/// - Otherwise parse as JSON `ChunkResponse`
/// - Extract text deltas from `delta.content`
/// - Extract tool call events from `delta.tool_calls`:
///   - If `id` is present → `ToolCallStart`
///   - If `function.arguments` is present and non-empty → `ToolCallDelta`
/// - Return None if no events were produced
pub fn parse_sse_line(line: &str) -> Option<Vec<StreamEvent>> {
    let data = line.strip_prefix("data: ")?;
    if data == "[DONE]" {
        return Some(vec![StreamEvent::Done]);
    }

    let chunk: ChunkResponse = serde_json::from_str(data).ok()?;
    let choice = chunk.choices.into_iter().next()?;
    let mut events = Vec::new();

    if let Some(text) = choice.delta.content
        && !text.is_empty()
    {
        events.push(StreamEvent::TextDelta(text));
    }

    if let Some(tool_calls) = choice.delta.tool_calls {
        for tc in tool_calls {
            if let Some(id) = tc.id {
                let name = tc
                    .function
                    .as_ref()
                    .and_then(|f| f.name.clone())
                    .unwrap_or_default();
                events.push(StreamEvent::ToolCallStart {
                    index: tc.index,
                    id,
                    name,
                });
            }
            if let Some(ref func) = tc.function
                && let Some(ref args) = func.arguments
                && !args.is_empty()
            {
                events.push(StreamEvent::ToolCallDelta {
                    index: tc.index,
                    arguments: args.clone(),
                });
            }
        }
    }

    if events.is_empty() {
        None
    } else {
        Some(events)
    }
}

// ---------------------------------------------------------------------------
// StreamProvider trait
// ---------------------------------------------------------------------------

/// A provider that supports streaming responses via [`StreamEvent`]s.
pub trait StreamProvider: Send + Sync {
    fn stream_chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a;
}

// ---------------------------------------------------------------------------
// MockStreamProvider (for testing without HTTP)
// ---------------------------------------------------------------------------

/// A mock streaming provider that wraps [`MockProvider`] and synthesizes
/// [`StreamEvent`]s from each canned response.
///
/// # Chapter 10: MockStreamProvider
///
/// Wraps a MockProvider. When `stream_chat` is called:
/// 1. Call inner.chat() to get the complete turn
/// 2. Synthesize stream events: one TextDelta per char, ToolCallStart/Delta per tool call
/// 3. Send StreamEvent::Done
/// 4. Return the turn
pub struct MockStreamProvider {
    inner: MockProvider,
}

impl MockStreamProvider {
    pub fn new(responses: VecDeque<AssistantTurn>) -> Self {
        Self {
            inner: MockProvider::new(responses),
        }
    }
}

impl StreamProvider for MockStreamProvider {
    /// Synthesize stream events from a canned response.
    ///
    /// Hints:
    /// - Call `self.inner.chat(messages, tools).await?` to get the turn
    /// - For text: send one TextDelta per character
    /// - For tool calls: send ToolCallStart then ToolCallDelta for each
    /// - Send StreamEvent::Done at the end
    /// - Return Ok(turn)
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<AssistantTurn> {
        let turn = self.inner.chat(messages, tools).await?;

        if let Some(ref text) = turn.text {
            for ch in text.chars() {
                let _ = tx.send(StreamEvent::TextDelta(ch.to_string()));
            }
        }
        for (i, call) in turn.tool_calls.iter().enumerate() {
            let _ = tx.send(StreamEvent::ToolCallStart {
                index: i,
                id: call.id.clone(),
                name: call.name.clone(),
            });
            let _ = tx.send(StreamEvent::ToolCallDelta {
                index: i,
                arguments: call.arguments.to_string(),
            });
        }
        let _ = tx.send(StreamEvent::Done);

        Ok(turn)
    }
}

// ---------------------------------------------------------------------------
// StreamingAgent
// ---------------------------------------------------------------------------

/// A streaming agent that emits [`AgentEvent::TextDelta`] events in real time.
///
/// This is the streaming counterpart to [`SimpleAgent`](crate::agent::SimpleAgent).
pub struct StreamingAgent<P: StreamProvider> {
    provider: P,
    tools: ToolSet,
}

impl<P: StreamProvider> StreamingAgent<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            tools: ToolSet::new(),
        }
    }

    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        self.tools.push(t);
        self
    }

    /// Run the streaming agent loop with a fresh prompt.
    ///
    /// Hints:
    /// - Create messages vec with Message::User(prompt)
    /// - Call self.chat(&mut messages, events)
    pub async fn run(
        &self,
        prompt: &str,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        let mut messages = vec![Message::User(prompt.to_string())];
        self.chat(&mut messages, events).await
    }

    #[allow(clippy::ptr_arg)]
    pub async fn chat(
        &self,
        messages: &mut Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        let defs = self.tools.definitions();

        loop {
            let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();
            let events_clone = events.clone();

            let forwarder = tokio::spawn(async move {
                while let Some(event) = stream_rx.recv().await {
                    if let StreamEvent::TextDelta(ref text) = event {
                        let _ = events_clone.send(AgentEvent::TextDelta(text.clone()));
                    }
                }
            });

            let turn = self
                .provider
                .stream_chat(messages, &defs, stream_tx)
                .await?;
            let _ = forwarder.await;

            match turn.stop_reason {
                StopReason::Stop => {
                    let text = turn.text.clone().unwrap_or_default();
                    let _ = events.send(AgentEvent::Done(text.clone()));
                    messages.push(Message::Assistant(turn));
                    return Ok(text);
                }
                StopReason::ToolUse => {
                    let mut results = Vec::new();
                    for call in &turn.tool_calls {
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
                    messages.push(Message::Assistant(turn));
                    for (id, content) in results {
                        messages.push(Message::ToolResult { id, content });
                    }
                }
            }
        }
    }
}
