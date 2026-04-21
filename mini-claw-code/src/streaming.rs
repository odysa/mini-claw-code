use std::collections::VecDeque;
use std::future::Future;

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::agent::AgentEvent;
use crate::mock::MockProvider;
use crate::planning::stream_chat_loop;
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
    pub fn new() -> Self {
        Self {
            text: String::new(),
            tool_calls: Vec::new(),
        }
    }

    /// Process a single streaming event.
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
/// - Non-`data:` lines → `None`
/// - `data: [DONE]` → `Some(vec![StreamEvent::Done])`
/// - Valid JSON chunk → `Some(events)` with text deltas and/or tool call events
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
    async fn stream_chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<AssistantTurn> {
        let turn = self.inner.chat(messages, tools).await?;

        // Synthesize stream events from the complete turn
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
/// Instead of waiting for the full LLM response, it streams text tokens as
/// they arrive via an [`mpsc`] channel.
pub struct StreamingAgent<P: StreamProvider> {
    provider: P,
    tools: ToolSet,
    config: QueryConfig,
}

impl<P: StreamProvider> StreamingAgent<P> {
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

    /// Run the streaming agent loop with a fresh prompt.
    ///
    /// Text tokens are sent as [`AgentEvent::TextDelta`] through the channel.
    /// Returns the final accumulated text.
    pub async fn run(
        &self,
        prompt: &str,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        let mut messages = vec![Message::User(prompt.to_string())];
        self.chat(&mut messages, events).await
    }

    /// Run the streaming agent loop with existing message history.
    ///
    /// The caller pushes `Message::User(…)` before calling. On return the
    /// vec contains the full conversation including the assistant's final turn.
    pub async fn chat(
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
