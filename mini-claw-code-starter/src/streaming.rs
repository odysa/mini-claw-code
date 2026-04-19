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
/// # Chapter 5a: Provider & Streaming Foundations
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
    pub fn feed(&mut self, _event: &StreamEvent) {
        unimplemented!(
            "TODO ch5a: match on the StreamEvent and update text/tool_calls accordingly"
        )
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
        unimplemented!("TODO ch5a: assemble buffered deltas into a complete AssistantTurn")
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
/// # Chapter 5a: SSE Parsing
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
pub fn parse_sse_line(_line: &str) -> Option<Vec<StreamEvent>> {
    unimplemented!("TODO ch5a: strip 'data: ' prefix, handle [DONE], parse JSON into StreamEvents")
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
/// # Chapter 5a: MockStreamProvider
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
        _messages: &[Message],
        _tools: &[&ToolDefinition],
        _tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> anyhow::Result<AssistantTurn> {
        unimplemented!("TODO ch5a: fetch a canned turn and synthesize StreamEvents over tx")
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

    /// Run the streaming agent loop, forwarding TextDelta events and executing tool calls.
    ///
    /// Hints:
    /// - Loop: create an mpsc channel, spawn a task that forwards StreamEvent::TextDelta
    ///   as AgentEvent::TextDelta, call provider.stream_chat, match stop_reason.
    /// - On Stop: send AgentEvent::Done, push turn into messages, return text.
    /// - On ToolUse: emit AgentEvent::ToolCall for each, execute tools, push results.
    #[allow(clippy::ptr_arg)]
    pub async fn chat(
        &self,
        _messages: &mut Vec<Message>,
        _events: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        unimplemented!(
            "TODO ch5b: implement the streaming agent loop (stream → forward deltas → tools)"
        )
    }
}
