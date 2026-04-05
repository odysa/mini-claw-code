# Chapter 4: Query Engine

This is the chapter where everything clicks.

In the previous chapters you built the vocabulary (messages), the mouth (provider), and the hands (tools). Now you build the brain -- the loop that ties them all together. The `QueryEngine` is the heart of a coding agent. It is the thing that takes a user prompt, talks to an LLM, executes tools, feeds results back, and keeps going until the job is done.

Every coding agent -- Claude Code, Cursor, Aider, OpenCode -- has some version of this loop. The details vary (streaming, permissions, compaction), but the skeleton is identical. Get this right and you have a working agent. Everything else in this book is refinement.

## What the QueryEngine does

Here is the entire agent lifecycle in one sentence: **prompt the LLM, check if it wants to use tools, execute those tools, send the results back, repeat until the LLM says it is done.**

That is it. The `QueryEngine` implements this loop. It owns three things:

1. A **provider** -- the LLM backend (from Chapter 2)
2. A **tool set** -- the registered tools (from Chapter 3)
3. A **config** -- safety limits and behavior knobs

```
User prompt
    |
    v
+-------------------+
| QueryEngine::chat |<---------+
+-------------------+          |
    |                          |
    v                          |
  Provider.chat()              |
    |                          |
    v                          |
  StopReason?                  |
    |         |                |
  Stop     ToolUse             |
    |         |                |
    v         v                |
  return    execute_tools()    |
  text        |                |
              v                |
            push Assistant     |
            push ToolResults   |
              |                |
              +----------------+
```

If you have read Claude Code's source, this maps to `QueryEngine.ts` and the `query` function in `query.ts`. Our version strips away streaming, permissions, hooks, and compaction -- those come in later chapters -- leaving the pure control flow.

## QueryConfig: safety from day one

Before writing any loop logic, we define the guardrails. A coding agent without limits is a runaway process that burns through your API budget and fills your context window with garbage.

```rust
pub struct QueryConfig {
    /// Maximum number of agent loop iterations before stopping.
    pub max_turns: usize,
    /// Maximum tool result size in characters before truncation.
    pub max_result_chars: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            max_turns: 50,
            max_result_chars: 100_000,
        }
    }
}
```

Two fields, both critical:

- **`max_turns`** -- The hard ceiling on loop iterations. Without this, a confused model that keeps calling tools will loop forever. Claude Code defaults to 200 in headless mode; we use 50 as a sensible tutorial default. When the limit is reached, `chat()` returns an error. The agent stops. The user is told what happened.

- **`max_result_chars`** -- The maximum size of any single tool result before it gets truncated. A `cat` on a 2MB file should not consume your entire context window. When a result exceeds this limit, we slice it and append a `[truncated]` marker. The model sees enough to understand what happened without the full payload.

These two values are the difference between a prototype that works on toy examples and a system you can point at a real codebase. Production safety from day one.

## The QueryEngine struct

```rust
pub struct QueryEngine<P: Provider> {
    provider: P,
    tools: ToolSet,
    config: QueryConfig,
}
```

Generic over `P: Provider`, so the same engine works with `OpenRouterProvider` in production and `MockProvider` in tests. The builder pattern lets you configure it fluently -- `new()` creates an engine with defaults, then `config()`, `tool()`, and `tools()` chain on configuration. Each takes `mut self` and returns `Self`:

```rust
let engine = QueryEngine::new(provider)
    .tool(BashTool::new())
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .config(QueryConfig { max_turns: 20, ..Default::default() });
```

No surprises. The interesting part is the methods that actually run.

## execute_tools: the tool dispatch pipeline

Before tackling the main loop, we need a helper that takes a slice of `ToolCall`s from the LLM and produces results. This is `execute_tools`, and it handles three concerns: lookup, validation, and truncation.

```rust
async fn execute_tools(&self, calls: &[ToolCall]) -> Vec<(String, ToolResult)> {
    let mut results = Vec::with_capacity(calls.len());
    for call in calls {
        let result = match self.tools.get(&call.name) {
            Some(t) => {
                // Validate input
                match t.validate_input(&call.arguments) {
                    ValidationResult::Ok => {}
                    ValidationResult::Error { message, .. } => {
                        results.push((call.id.clone(), ToolResult::error(message)));
                        continue;
                    }
                }
                // Execute
                match t.call(call.arguments.clone()).await {
                    Ok(mut r) => {
                        // Truncate large results
                        if r.content.len() > self.config.max_result_chars {
                            r.content = format!(
                                "{}... [truncated, {} chars total]",
                                &r.content[..self.config.max_result_chars],
                                r.content.len()
                            );
                            r.is_truncated = true;
                        }
                        r
                    }
                    Err(e) => ToolResult::error(e.to_string()),
                }
            }
            None => ToolResult::error(format!("unknown tool `{}`", call.name)),
        };
        results.push((call.id.clone(), result));
    }
    results
}
```

Walk through the three stages:

1. **Tool lookup** -- If the LLM hallucinates a tool name that does not exist, we return `ToolResult::error(...)`. The model sees `"error: unknown tool \`foo\`"` and can recover. This happens more than you might expect, especially with smaller models.

2. **Input validation** -- Before executing anything, we call `validate_input()`. The default implementation always returns `Ok`, but tools can override it to check required fields, validate paths, or reject dangerous arguments. Validation failures skip execution entirely -- push the error and `continue` to the next call.

3. **Execute and truncate** -- Run the tool. If it succeeds but the output exceeds `max_result_chars`, slice it and append a `[truncated, N chars total]` marker. If it fails, convert the error to `ToolResult::error(...)`.

This is a key design decision: **tool errors become results, not panics**. The agent loop never crashes because a tool failed. The model reads the error, adjusts its approach, and tries again. This is how Claude Code works too -- if `bash` returns a non-zero exit code, the agent sees the stderr output and adapts.

### Why sequential, not parallel?

You may notice we execute tools sequentially in a `for` loop rather than spawning them concurrently with `join_all`. This is deliberate for correctness: tools can have side effects. If the LLM calls `write_file("foo.rs", ...)` and then `bash("cargo build")` in the same turn, those must run in order. Claude Code does support concurrent execution for read-only tools (tools where `is_concurrent_safe()` returns `true`), but we keep it simple here. You can add concurrency later using `tokio::JoinSet` -- the `is_concurrent_safe()` method on the `Tool` trait is already there waiting for you.

## The chat() method: the core loop

This is it. The agentic loop. Read it carefully -- it is shorter than you expect.

```rust
pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
    let defs = self.tools.definitions();
    let mut turns = 0;

    loop {
        if turns >= self.config.max_turns {
            anyhow::bail!("exceeded max turns ({})", self.config.max_turns);
        }

        let turn = self.provider.chat(messages, &defs).await?;

        match turn.stop_reason {
            StopReason::Stop => {
                let text = turn.text.clone().unwrap_or_default();
                messages.push(Message::Assistant(turn));
                return Ok(text);
            }
            StopReason::ToolUse => {
                let results = self.execute_tools(&turn.tool_calls).await;
                messages.push(Message::Assistant(turn));
                for (id, result) in results {
                    messages.push(Message::tool_result(id, result.content));
                }
            }
        }

        turns += 1;
    }
}
```

Let's break it down.

### Tool definitions: collected once

```rust
let defs = self.tools.definitions();
```

We gather tool definitions outside the loop. They do not change between iterations -- the tool set is fixed for the lifetime of the engine. Every call to `provider.chat()` includes these definitions so the LLM knows which tools are available.

### The max turns guard

```rust
if turns >= self.config.max_turns {
    anyhow::bail!("exceeded max turns ({})", self.config.max_turns);
}
```

Checked at the top of each iteration, before calling the provider. If we have hit the limit, bail immediately. The `anyhow::bail!` macro returns an `Err`, which propagates up to the caller. No infinite loops, no surprise bills.

### Call the provider

```rust
let turn = self.provider.chat(messages, &defs).await?;
```

Send the full message history and tool definitions to the LLM. The `?` propagates provider errors (network failure, auth error, rate limit) directly to the caller. Provider errors are not recoverable by the agent loop -- they need human intervention.

### Match the stop reason

```rust
match turn.stop_reason {
    StopReason::Stop => { /* final answer */ }
    StopReason::ToolUse => { /* tool dispatch */ }
}
```

The LLM tells us why it stopped generating. Two possibilities:

- **`Stop`** -- The model is done. It has a final text answer. Extract it, push the assistant message into history, return.
- **`ToolUse`** -- The model wants to use tools. It has populated `tool_calls` with one or more calls. Execute them, push results, loop.

### The two branches

**`StopReason::Stop`** -- Clone the text, push the assistant message into history, return. The conversation ends with an `Assistant` message, ready for the next user turn.

**`StopReason::ToolUse`** -- Execute the tools, then push messages in this exact order:

1. **First**, `Message::Assistant(turn)` -- the assistant's response including its tool calls
2. **Then**, `Message::tool_result(...)` for each tool result

This ordering matters. The LLM API expects tool results to follow the assistant message that requested them. Each `ToolResult` is linked to its `ToolCall` by the `id` field. If you push them in the wrong order, the provider will reject the request.

After pushing results, the loop continues. The next iteration sends the entire history -- including the tool calls and their results -- back to the LLM. The model sees what happened and decides what to do next.

### Why &mut Vec\<Message\>?

The caller owns the message history and passes it as `&mut Vec<Message>`. This is deliberate:

1. **Multi-turn conversations** -- The caller can push a new `Message::user(...)` and call `chat()` again. The engine picks up where it left off with the full context.
2. **Inspection** -- After `chat()` returns, the caller can examine the full message history to see every tool call, every result, every intermediate step.
3. **Persistence** -- The caller can serialize the messages to disk for session save/resume (Chapter 19).

## run(): the convenience wrapper

Most of the time you just want to send a prompt and get a response. That is `run()`:

```rust
pub async fn run(&self, prompt: &str) -> anyhow::Result<String> {
    let mut messages = vec![Message::user(prompt)];
    self.chat(&mut messages).await
}
```

Two lines. Creates a fresh message history with the user prompt, delegates to `chat()`. The message history is discarded after the call -- use `chat()` directly if you need to preserve it.

## QueryEvent: making it observable

The `chat()` method returns when the agent is done. That is fine for tests, but a real UI needs to show progress while the loop is running. What tool is being called? How long has it been running? Is it done?

The `QueryEvent` enum models these updates:

```rust
#[derive(Debug)]
pub enum QueryEvent {
    /// A chunk of text streamed from the LLM.
    TextDelta(String),
    /// A tool is about to be called.
    ToolStart { name: String, summary: String },
    /// A tool finished executing.
    ToolEnd { name: String, result: String },
    /// The engine finished with a final response.
    Done(String),
    /// The engine encountered an error.
    Error(String),
}
```

Five variants covering the full lifecycle:

| Event | When | UI use |
|-------|------|--------|
| `TextDelta` | LLM streams a text chunk | Append to terminal output |
| `ToolStart` | About to execute a tool | Show spinner: "Running bash: ls -la..." |
| `ToolEnd` | Tool finished | Show result preview, stop spinner |
| `Done` | Agent loop finished | Display final answer |
| `Error` | Unrecoverable error | Show error message |

The `summary` field in `ToolStart` comes from `Tool::summary()` -- recall from Chapter 3 that this produces compact descriptions like `[bash: ls -la]` or `[read: src/main.rs]`. The `result` field in `ToolEnd` is capped at 200 characters to keep event payloads small.

## run_with_events / chat_with_events

These methods duplicate the core loop logic but emit events through a `tokio::sync::mpsc::UnboundedSender<QueryEvent>` channel. The caller creates the channel, passes the sender, and consumes events from the receiver -- typically in a separate task that drives the UI.

```rust
pub async fn run_with_events(
    &self,
    prompt: &str,
    events: mpsc::UnboundedSender<QueryEvent>,
) -> Vec<Message> {
    let mut messages = vec![Message::user(prompt)];
    self.chat_with_events(&mut messages, events).await;
    messages
}
```

`run_with_events` returns `Vec<Message>` instead of `Result<String>` -- errors are sent as `QueryEvent::Error` rather than propagated. This keeps the event-driven API simple: the receiver always gets a terminal event (`Done` or `Error`).

`chat_with_events` has the same structure as `chat()` but with events woven in. Rather than reproducing the full listing, here are the key differences from `chat()`:

1. **Provider errors** are caught with `match` instead of `?`, and sent as `QueryEvent::Error`.
2. **Max turns** sends an error event instead of returning `Err`.
3. **ToolStart** events fire before `execute_tools` -- the UI shows the spinner before the tool runs.
4. **ToolEnd** events fire after execution with a truncated result preview (200 chars max).
5. **Done** event fires before pushing the final assistant message, so the UI gets the text immediately.

Note the `let _ = events.send(...)` pattern. The send can fail if the receiver has been dropped (the UI task crashed or exited early). We ignore the error because the engine should finish its work regardless of whether anyone is watching.

### Using events in practice

The caller creates an unbounded channel, passes the sender to the engine, and reads events from the receiver -- typically in a separate task:

```rust
let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

let engine_handle = tokio::spawn(async move {
    engine.run_with_events("Fix the bug in main.rs", tx).await
});

while let Some(event) = rx.recv().await {
    match event {
        QueryEvent::ToolStart { summary, .. } => println!("  -> {summary}"),
        QueryEvent::Done(text) => { println!("{text}"); break; }
        QueryEvent::Error(e) => { eprintln!("Error: {e}"); break; }
        _ => {}
    }
}
```

This two-task pattern is what Chapter 26 (TUI) builds on. The UI task renders events; the engine task runs the loop. They communicate through the channel.

## Tool result truncation in depth

Truncation deserves special attention because getting it wrong breaks the agent. When a tool returns a huge result -- say, `bash("find / -name '*.rs'")` producing 500KB of output -- the engine keeps the first `max_result_chars` characters and appends `"... [truncated, 523847 chars total]"`. This gives the model:

1. **Enough data** to understand the result (100K characters is usually more than sufficient)
2. **A signal** that data was lost, so it can request a more targeted query if needed
3. **The total size**, so it can reason about how much it missed

The `is_truncated` flag on `ToolResult` is carried for downstream consumers (session logging, cost tracking) but does not affect the agent loop itself. Claude Code does something similar but more sophisticated -- separate limits per tool type and token counting instead of character counting. Our character-based approach is simpler and good enough.

## Error handling philosophy

The engine has two distinct error strategies, and the boundary between them is intentional.

### Tool errors become results

When a tool fails -- validation error, execution error, unknown tool -- the error becomes a `ToolResult::error(...)` that the model sees as a normal tool result. The loop continues. The model reads the error and adapts.

```
Tool error flow:
  LLM requests bash("rm -rf /")
  -> Tool validates -> ValidationResult::Error("prohibited command")
  -> ToolResult::error("prohibited command")
  -> Pushed as Message::ToolResult
  -> LLM sees error, tries different approach
```

This is essential for robust agents. Models make mistakes. Tools fail for legitimate reasons. The agent should recover, not crash.

### Provider errors propagate

When the provider fails -- network timeout, authentication error, rate limit, malformed response -- the error propagates up via `?` (in `chat()`) or via `QueryEvent::Error` (in `chat_with_events()`). The loop stops.

```
Provider error flow:
  Engine calls provider.chat()
  -> Provider returns Err(network timeout)
  -> chat() returns Err(network timeout)
  -> Caller handles it (retry, show error, etc.)
```

Provider errors are not the agent's problem. They need human or system-level intervention (check your API key, wait for rate limits, fix your network). The engine does not try to recover.

## The max turns safety guard

The max turns check is simple but its placement matters:

```rust
loop {
    if turns >= self.config.max_turns {
        anyhow::bail!("exceeded max turns ({})", self.config.max_turns);
    }

    let turn = self.provider.chat(messages, &defs).await?;
    // ... handle turn ...

    turns += 1;
}
```

A few things to notice:

1. **Checked before the provider call**, not after. We do not waste an API call if we are already at the limit.
2. **Incremented after the full iteration** (tool execution + message pushing), not before. The first iteration runs with `turns == 0`.
3. **Only tool-use turns count**. If the model responds with `StopReason::Stop`, we return before hitting `turns += 1`. A direct text response does not count as a "turn" because it does not add to the looping risk.

This means `max_turns: 3` allows exactly 3 tool-use iterations. If the model has not finished after 3 rounds of tool use, it is probably stuck.

## Message history management

The order in which messages are pushed into the history is load-bearing. After a tool-use turn:

```rust
StopReason::ToolUse => {
    let results = self.execute_tools(&turn.tool_calls).await;
    messages.push(Message::Assistant(turn));    // 1. Assistant message (with tool_calls)
    for (id, result) in results {
        messages.push(Message::tool_result(id, result.content));  // 2. Tool results
    }
}
```

The resulting message sequence looks like:

```
[User]        "What files are in src/?"
[Assistant]   tool_calls: [bash("ls src/")]      <- includes the tool call
[ToolResult]  "main.rs\nlib.rs\n"                <- linked by call ID
[Assistant]   "There are two files: ..."          <- next LLM response
```

Why this order?

1. **API requirement**: The Claude API (and OpenAI-compatible APIs) require that `tool_result` messages immediately follow the `assistant` message that generated the corresponding `tool_use`. Violating this causes a 400 error.

2. **ID linking**: Each `ToolResult` has a `tool_use_id` that matches a `ToolCall.id` in the preceding assistant message. The LLM uses this to associate results with requests when there are multiple parallel tool calls.

3. **Context for the next turn**: The LLM needs to see its own tool calls to understand what it asked for, and the results to know what happened. Both must be present in the history for the next `provider.chat()` call.

## Putting it all together: a complete trace

Let's trace through a realistic scenario. The user asks: "What is 2 + 3?"

The engine has an `AddTool` registered. The mock provider is configured to return a tool call first, then a final answer.

**Turn 0:**
```
messages: [User("What is 2 + 3?")]
  -> provider.chat() returns: ToolUse, tool_calls: [add(a=2, b=3)]
  -> execute_tools: AddTool.call({a:2, b:3}) -> Ok("5")
  -> push: Assistant(tool_calls: [add(a=2, b=3)])
  -> push: ToolResult(id=call_1, content="5")
  -> turns = 1
```

**Turn 1:**
```
messages: [User, Assistant, ToolResult]
  -> provider.chat() returns: Stop, text: "The sum is 5"
  -> push: Assistant(text: "The sum is 5")
  -> return Ok("The sum is 5")
```

Two provider calls, one tool execution, clean exit. The final message history has 4 entries: User, Assistant (with tool call), ToolResult, Assistant (with text).

## How this compares to Claude Code

Our `QueryEngine` is a teaching implementation. Claude Code's real engine is considerably more complex. Here is what it adds:

| Feature | Our engine | Claude Code |
|---------|-----------|-------------|
| Core loop | `loop { match stop_reason }` | Same pattern, but with async hooks at every stage |
| Streaming | Separate `chat_with_events` | Integrated SSE streaming with `StreamProvider` |
| Permissions | None | Full permission pipeline checked before every tool call |
| Compaction | None | Auto-compacts when approaching token limit |
| Hooks | None | Pre/post tool hooks with shell command execution |
| Concurrency | Sequential tool execution | Parallel execution for `is_concurrent_safe` tools |
| Error recovery | Tool errors as results | Same, plus retry logic for transient provider errors |
| Subagents | None | Spawns child `QueryEngine`s with isolated history |
| Cost tracking | None | Accumulates `TokenUsage` from every turn |

The good news: the architecture is the same. Every feature in the right column plugs into the same loop structure. Permissions are checked in `execute_tools` before calling `t.call()`. Compaction runs at the top of the loop when token count is high. Hooks fire around tool execution. You will build all of these in later chapters, and they will slot into the engine you are building now.

## Tests

Run the chapter 4 tests to verify your implementation:

```bash
cargo test -p claw-code test_ch4
```

Here is what each test covers:

**`test_ch4_direct_text_response`** -- The simplest case. The provider returns `StopReason::Stop` on the first call. The engine should return the text without executing any tools.

**`test_ch4_single_tool_call`** -- The provider returns a tool call, then a final text. Tests the full loop: call tool, push results, call provider again, return.

**`test_ch4_multi_step_loop`** -- Two rounds of tool use before the final answer. Verifies the loop handles multiple iterations correctly.

**`test_ch4_unknown_tool`** -- The LLM requests a tool that does not exist. The engine should return an error result (not crash), and the model should see it and respond.

**`test_ch4_max_turns`** -- A provider that always returns `ToolUse`. The engine should bail with a "max turns" error after hitting the configured limit.

**`test_ch4_chat_preserves_history`** -- Calls `chat()` twice on the same message vec. Verifies that history accumulates correctly across calls, enabling multi-turn conversations.

**`test_ch4_events_emitted`** -- Uses `run_with_events` with a channel. Checks that `ToolStart`, `ToolEnd`, and `Done` events are emitted in the correct order.

**`test_ch4_result_truncation`** -- A tool that returns 200 characters of output with `max_result_chars: 50`. Verifies that the result is sliced and the truncation marker is applied.

**`test_ch4_provider_error`** -- An empty `MockProvider` with no responses. The first `provider.chat()` call fails. The engine should propagate the error.

## Implementation checklist

Here is what you need to implement in the starter crate:

1. **`QueryConfig`** -- The struct with `max_turns` and `max_result_chars`, plus the `Default` impl.

2. **`QueryEvent`** -- The five-variant enum.

3. **`QueryEngine`** -- The struct and builder methods (`new`, `config`, `tool`, `tools`).

4. **`execute_tools`** -- Look up each tool, validate, execute, truncate, collect. Return `Vec<(String, ToolResult)>`.

5. **`chat`** -- The core loop. Check max turns, call provider, match stop reason, dispatch tools, push messages, loop.

6. **`run`** -- Create messages, call `chat`.

7. **`chat_with_events`** -- Same loop as `chat` but emit events. Handle errors as events instead of `?`.

8. **`run_with_events`** -- Create messages, call `chat_with_events`, return messages.

Start with `QueryConfig`, `QueryEngine::new`, and the builder methods. Then implement `execute_tools` -- you can test it implicitly through `run`. Then `chat`, then `run`. Save the event methods for last.

## What you have now

After this chapter, you have a working coding agent. Not a complete one -- it has no tools yet (those come in Part II), no permissions (Part III), no context management (Part V) -- but the core loop is done. You can register any tool that implements the `Tool` trait, point it at any provider that implements `Provider`, and the engine will autonomously loop until it has an answer.

This is the skeleton that everything else hangs on. The permission engine (Chapter 10) adds a check inside `execute_tools`. The hook system (Chapter 12) wraps tool calls with pre/post events. Context compaction (Chapter 18) triggers at the top of the loop. Session management (Chapter 19) serializes the message vec. Every feature in this book is a modification to the loop you just built.

Next up: Chapter 5 builds the system prompt -- the instructions that tell the LLM how to behave as a coding agent.
