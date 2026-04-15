use crate::types::*;
use tokio::sync::mpsc;

/// Events emitted by the agent during execution.
#[derive(Debug)]
pub enum AgentEvent {
    /// A chunk of text streamed from the LLM (streaming mode only).
    TextDelta(String),
    /// A tool is being called.
    ToolCall { name: String, summary: String },
    /// The agent finished with a final response.
    Done(String),
    /// The agent encountered an error.
    Error(String),
}

/// Format a one-line summary of a tool call for terminal output.
pub(crate) fn tool_summary(call: &ToolCall) -> String {
    let detail = call
        .arguments
        .get("command")
        .or_else(|| call.arguments.get("path"))
        .or_else(|| call.arguments.get("question"))
        .and_then(|v| v.as_str());

    match detail {
        Some(s) => format!("    [{}: {}]", call.name, s),
        None => format!("    [{}]", call.name),
    }
}

/// Handle a single prompt with at most one round of tool calls.
///
/// # Chapter 3: Single Turn
///
/// Steps:
/// 1. Collect tool definitions with `tools.definitions()`
/// 2. Create messages starting with `vec![Message::User(prompt.to_string())]`
/// 3. Call `provider.chat(&messages, &defs).await?`
/// 4. Match on `turn.stop_reason`:
///    - `StopReason::Stop` → return `turn.text.unwrap_or_default()`
///    - `StopReason::ToolUse` → for each tool call:
///      a. Look up tool with `tools.get(&call.name)`
///      b. If found, call it. Catch errors with `.unwrap_or_else(|e| format!("error: {e}"))`.
///      If not found, return error string. Never crash on tool failure.
///      c. Collect results BEFORE pushing `Message::Assistant(turn)` (ownership!)
///      d. Push `Message::Assistant(turn)` then `Message::ToolResult` for each result
///      e. Call provider again to get the final answer
pub async fn single_turn<P: Provider>(
    provider: &P,
    tools: &ToolSet,
    prompt: &str,
) -> anyhow::Result<String> {
    unimplemented!(
        "Collect tool defs, send prompt to provider, match on stop_reason: Stop returns text, ToolUse executes tools and calls provider again"
    )
}

/// A simple AI agent that connects a provider to tools via a loop.
///
/// # Chapter 5: The Agent Loop
///
/// The agent loop is just `single_turn()` wrapped in a loop:
/// 1. Send the user's prompt to the provider
/// 2. Match on stop_reason
/// 3. If Stop → return text
/// 4. If ToolUse → execute tools, feed results back, continue the loop
pub struct SimpleAgent<P: Provider> {
    provider: P,
    tools: ToolSet,
}

impl<P: Provider> SimpleAgent<P> {
    /// Create a new agent with the given provider and no tools.
    pub fn new(provider: P) -> Self {
        unimplemented!("Store provider and create an empty ToolSet")
    }

    /// Register a tool with the agent. Returns self for chaining (builder pattern).
    pub fn tool(mut self, t: impl Tool + 'static) -> Self {
        unimplemented!("Push tool into self.tools and return self for chaining")
    }

    /// Execute all tool calls and return `(call_id, result_string)` pairs.
    ///
    /// Hints:
    /// - For each call, look up the tool with self.tools.get(&call.name)
    /// - If found, call it. Catch errors with unwrap_or_else.
    /// - If not found, return "error: unknown tool `{name}`"
    async fn execute_tools(&self, calls: &[ToolCall]) -> Vec<(String, String)> {
        unimplemented!("For each call, look up tool by name, call it, collect (id, result) pairs")
    }

    /// Push an assistant turn and its tool results into the message history.
    // Vec (not slice) because we push new elements.
    #[allow(clippy::ptr_arg)]
    fn push_results(
        messages: &mut Vec<Message>,
        turn: AssistantTurn,
        results: Vec<(String, String)>,
    ) {
        unimplemented!(
            "Push Message::Assistant(turn), then Message::ToolResult for each (id, content)"
        )
    }

    /// Run the agent loop with existing message history and emit events.
    ///
    /// # Chapter 9: Events
    ///
    /// Like `chat()` but takes ownership of messages and sends AgentEvents
    /// through the channel instead of printing. Returns the full message history.
    pub async fn run_with_history(
        &self,
        mut messages: Vec<Message>,
        events: mpsc::UnboundedSender<AgentEvent>,
    ) -> Vec<Message> {
        unimplemented!(
            "Loop: call provider, on Stop send Done event and return, on ToolUse send ToolCall events, execute tools, push results, continue"
        )
    }

    /// Run the agent loop, sending events through the channel.
    pub async fn run_with_events(&self, prompt: &str, events: mpsc::UnboundedSender<AgentEvent>) {
        let messages = vec![Message::User(prompt.to_string())];
        self.run_with_history(messages, events).await;
    }

    /// Run the agent loop, accumulating into the provided message history.
    ///
    /// # Chapter 7: The CLI
    ///
    /// This is `run()` adapted for multi-turn conversation:
    /// 1. The caller pushes `Message::User(…)` before calling
    /// 2. The loop is the same as `run()` — provider → match → tools → repeat
    /// 3. On `StopReason::Stop`, clone `turn.text` BEFORE pushing
    ///    `Message::Assistant(turn)` (the push moves `turn`)
    /// 4. Push the assistant turn into messages so the history is complete
    /// 5. Return the cloned text
    #[allow(clippy::ptr_arg)]
    pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
        unimplemented!(
            "Loop: call provider, on Stop clone text then push Assistant and return, on ToolUse execute tools and push results"
        )
    }

    /// Run the agent loop with the given prompt.
    pub async fn run(&self, prompt: &str) -> anyhow::Result<String> {
        unimplemented!("Create messages vec with User prompt, call self.chat()")
    }
}
