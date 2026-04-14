# Your First LLM Call

> **File to edit:** `src/mock.rs`
> **Test to run:** `cargo test -p mini-claw-code-starter test_ch1`

Before building an agent, you need to talk to an LLM. In this chapter you will implement a `MockProvider` — a fake LLM that returns canned responses. No API key, no HTTP, no network. Just the protocol.

## The protocol

Every LLM interaction follows the same pattern:

```
You send:    messages + tool definitions
You receive: text and/or tool calls + a stop reason
```

In Rust, that's one trait with one method:

```rust
pub trait Provider: Send + Sync {
    fn chat(
        &self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send;
}
```

The LLM responds with an `AssistantTurn`:

```rust
pub struct AssistantTurn {
    pub text: Option<String>,          // what the LLM said
    pub tool_calls: Vec<ToolCall>,     // tools it wants to call
    pub stop_reason: StopReason,       // Stop or ToolUse
    pub usage: Option<TokenUsage>,     // token counts (optional)
}
```

Two outcomes:
- **`StopReason::Stop`** — the LLM is done, read `text` for the answer
- **`StopReason::ToolUse`** — the LLM wants to call tools, read `tool_calls`

That's it. Every coding agent — Claude Code, Cursor, Copilot — runs on this exact protocol.

## Your task: MockProvider

Open `src/mock.rs`. You'll see a struct with pre-configured responses and two `unimplemented!()` stubs.

The `MockProvider` stores a queue of `AssistantTurn` values. Each call to `chat()` pops the next one off the front. When the queue is empty, it returns an error. It ignores the messages and tools arguments completely — it's a mock.

### Step 1: `new()`

Wrap the `VecDeque` in a `Mutex` and store it:

```rust
pub fn new(responses: VecDeque<AssistantTurn>) -> Self {
    Self {
        responses: Mutex::new(responses),
    }
}
```

Why `Mutex`? The `Provider` trait takes `&self` (not `&mut self`) because providers are shared across async tasks. `Mutex` lets us mutate the queue through a shared reference.

### Step 2: `chat()`

Lock the mutex, pop the front response, convert `None` to an error:

```rust
async fn chat(
    &self,
    _messages: &[Message],
    _tools: &[&ToolDefinition],
) -> anyhow::Result<AssistantTurn> {
    self.responses
        .lock()
        .unwrap()
        .pop_front()
        .ok_or_else(|| anyhow::anyhow!("MockProvider: no more responses"))
}
```

That's the entire implementation — 3 lines of logic.

## Run the tests

```bash
cargo test -p mini-claw-code-starter test_ch1
```

14 tests verify your mock:
- `test_ch1_returns_text` — basic text response
- `test_ch1_returns_tool_calls` — response with tool calls
- `test_ch1_steps_through_sequence` — multiple responses in FIFO order
- `test_ch1_empty_responses_exhausted` — error when queue is empty
- `test_ch1_ignores_messages_and_tools` — mock doesn't look at inputs

## What just happened

You implemented the `Provider` trait — the interface every LLM backend must satisfy. The `MockProvider` is your testing workhorse. Every test in this entire course uses it instead of calling a real API.

Later (Chapter 2 of the deep-dive) you'll implement `OpenRouterProvider`, which makes real HTTP calls. But the trait is the same. Swap the provider, and the rest of the code doesn't change.

## Key takeaway

An LLM is a function: `messages in → (text, tool_calls, stop_reason) out`. Everything else is plumbing.

---

**Next:** [Your First Tool Call →](./intro02-first-tool.md)
