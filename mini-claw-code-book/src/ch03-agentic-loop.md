# Chapter 3: The Agentic Loop

> **File(s) to edit:** `src/agent.rs`
> **Tests to run:** `cargo test -p mini-claw-code-starter test_single_turn_` (single_turn), `cargo test -p mini-claw-code-starter test_simple_agent_` (SimpleAgent)
> **Estimated time:** 20 min

You have a provider (talks to the LLM) and a tool (reads files). Now you connect them. This is where the agent comes alive.

## Goal

Implement two things:

1. **`single_turn()`** — handle one prompt with at most one round of tool calls
2. **`SimpleAgent`** — wrap `single_turn` in a loop that keeps going until the LLM is done

### What's in scope for Ch3 (and what isn't)

When you open `src/agent.rs` you'll see five `unimplemented!()` stubs. Only four of them are Chapter 3's job:

| Stub                      | Chapter | Notes                                               |
|---------------------------|---------|-----------------------------------------------------|
| `single_turn`             | Ch3     | one prompt, at most one tool round                  |
| `SimpleAgent::execute_tools` | Ch3  | look up each tool, collect `(id, content)` pairs    |
| `SimpleAgent::push_results`  | Ch3  | push `Assistant` turn, then one `ToolResult` each   |
| `SimpleAgent::chat`       | Ch3     | the main agent loop                                 |
| `SimpleAgent::run_with_history` | Ch7 | events-based loop; **leave stubbed** for now      |

The `run_with_history` / `run_with_events` pair is for Chapter 7 (`AgentEvent`-driven execution). No Ch3 test calls them, so the `unimplemented!()` there will not panic during `test_simple_agent_`. Ignore them until Chapter 7 introduces the events model.

## The core idea

Every coding agent — Claude Code, Cursor, Aider — is this loop:

```
loop {
    response = provider.chat(messages, tools)
    if response.stop_reason == Stop:
        return response.text
    for call in response.tool_calls:
        result = tools.execute(call)
        messages.append(result)
}
```

The LLM decides when to stop. Your code just follows instructions.

```mermaid
flowchart TD
    A["User prompt"] --> B["provider.chat()"]
    B --> C{"stop_reason?"}
    C -- "Stop" --> D["Return text"]
    C -- "ToolUse" --> E["Execute tool calls"]
    E --> F["Append results to messages"]
    F --> B
```

## Part 1: single_turn()

Start simple. `single_turn()` handles one prompt with at most one round of tool calls — no looping yet.

### Key Rust concept: ToolSet

The function takes a `&ToolSet` — a `HashMap<String, Box<dyn Tool>>` that indexes tools by name for O(1) lookup:

```rust
pub async fn single_turn<P: Provider>(
    provider: &P,
    tools: &ToolSet,
    prompt: &str,
) -> anyhow::Result<String>
```

### The flow

```mermaid
flowchart TD
    A["prompt"] --> B["provider.chat()"]
    B --> C{"stop_reason?"}
    C -- "Stop" --> D["Return text"]
    C -- "ToolUse" --> E["Execute each tool call"]
    E --> F{"Tool found?"}
    F -- "Yes" --> G["tool.call() → result"]
    F -- "No" --> H["error: unknown tool"]
    G --> I["Push Assistant + ToolResult messages"]
    H --> I
    I --> J["provider.chat() again"]
    J --> K["Return final text"]
```

### Implementation

```rust
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
            // Execute each tool call, collect results
            let mut results = Vec::new();
            for call in &turn.tool_calls {
                let content = match tools.get(&call.name) {
                    Some(t) => t.call(call.arguments.clone())
                        .await
                        .unwrap_or_else(|e| format!("error: {e}")),
                    None => format!("error: unknown tool `{}`", call.name),
                };
                results.push((call.id.clone(), content));
            }

            // Feed results back to the LLM
            messages.push(Message::Assistant(turn));
            for (id, content) in results {
                messages.push(Message::ToolResult { id, content });
            }

            let final_turn = provider.chat(&messages, &defs).await?;
            Ok(final_turn.text.unwrap_or_default())
        }
    }
}
```

Three key details:

1. **Collect results before pushing** `Message::Assistant(turn)` — the push moves `turn`, so you can't borrow `turn.tool_calls` after that
2. **Never crash on tool failure** — catch errors with `unwrap_or_else` and return them as strings. The LLM reads the error and adapts
3. **Unknown tools get an error string** — not a panic. The LLM might hallucinate a tool name; your agent handles it gracefully

### Test it

```bash
cargo test -p mini-claw-code-starter test_single_turn_
```

14 tests including:
- **`test_single_turn_direct_response`** — LLM responds immediately, no tools
- **`test_single_turn_one_tool_call`** — LLM reads a file, then answers
- **`test_single_turn_unknown_tool`** — LLM calls a nonexistent tool, gets an error, recovers
- **`test_single_turn_provider_error`** — provider returns an error, propagated correctly

## Part 2: SimpleAgent

`single_turn` handles one round. A real agent loops until the LLM is done. That's `SimpleAgent`.

### The struct

```rust
pub struct SimpleAgent<P: Provider> {
    provider: P,
    tools: ToolSet,
}
```

### Constructor and builder

```rust
pub fn new(provider: P) -> Self {
    Self { provider, tools: ToolSet::new() }
}

pub fn tool(mut self, t: impl Tool + 'static) -> Self {
    self.tools.push(t);
    self
}
```

The builder pattern lets you chain tool registration:

```rust
let agent = SimpleAgent::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(BashTool::new());
```

### The loop: `chat()`

### Aside: who decides `Stop` vs `ToolUse`?

The model does. `StopReason` is not a value we compute from the response; it is
a field the LLM API returns *describing what the model did*. When the model
emitted plain text and stopped, the API reports `stop` (or `end_turn`). When
the model emitted one or more tool-call blocks and paused expecting the
caller to run them, the API reports `tool_use` (OpenAI calls it
`tool_calls`). Our `StopReason` enum is just a thin translation of that API
field into a Rust type; the decision is baked into the model's generation.

Practically, the model decides in a single forward pass: once it begins
writing a tool-call block, most providers force the response to terminate on
that block and return `tool_use` to the caller. It does *not* produce text
and then choose whether to call a tool as a separate step. This is why the
loop below looks so simple -- we never have to second-guess the stop reason,
we just dispatch on it.

---

This is `single_turn` generalized into a loop. Instead of calling the provider twice and returning, it keeps going until `StopReason::Stop`:

```rust
pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
    let defs = self.tools.definitions();

    loop {
        let turn = self.provider.chat(messages, &defs).await?;

        match turn.stop_reason {
            StopReason::Stop => {
                let text = turn.text.clone().unwrap_or_default();
                messages.push(Message::Assistant(turn));
                return Ok(text);
            }
            StopReason::ToolUse => {
                let results = self.execute_tools(&turn.tool_calls).await;
                Self::push_results(messages, turn, results);
            }
        }
    }
}
```

Note: clone `turn.text` **before** pushing `Message::Assistant(turn)` — the push moves `turn`.

**`run()`** is a convenience wrapper:

```rust
pub async fn run(&self, prompt: &str) -> anyhow::Result<String> {
    let mut messages = vec![Message::User(prompt.to_string())];
    self.chat(&mut messages).await
}
```

The helper methods `execute_tools()` and `push_results()` factor out the tool execution and message building — see the stubs in `agent.rs` for the signatures.

### Test it

```bash
cargo test -p mini-claw-code-starter test_simple_agent_
```

16 tests including:
- **`test_simple_agent_simple_text`** — single-turn text response
- **`test_simple_agent_multi_step`** — LLM reads a file, then writes a response
- **`test_simple_agent_three_turn_loop`** — read → edit → verify, three rounds
- **`test_simple_agent_error_recovery`** — tool fails, LLM reads the error and adapts

## What just happened

You built a coding agent.

```rust
let agent = SimpleAgent::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(BashTool::new());

let answer = agent.run("What files are in this directory?").await?;
```

The agent sends the prompt to the LLM, the LLM calls `bash("ls")`, the agent executes it, feeds the output back, and the LLM summarizes the result. The loop handles any number of tool calls across any number of rounds.

That is the architecture. Everything else — streaming, permissions, plan mode, subagents — is built on top of this loop.

---

[← Chapter 2: Your First Tool Call](./ch02-first-tool.md) · [Contents](./ch00-overview.md) · [Chapter 4: Messages & Types →](./ch04-messages-types.md)
