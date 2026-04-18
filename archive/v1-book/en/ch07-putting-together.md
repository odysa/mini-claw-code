# Chapter 7: A Simple CLI

You have built every component: a mock provider for testing, four tools, the
agent loop, and an HTTP provider. Now it is time to wire them all into a
working CLI.

## Goal

Add a `chat()` method to `SimpleAgent` and write `examples/chat.rs` so that:

1. The agent remembers the conversation -- each prompt builds on the previous
   ones.
2. It prints `> `, reads a line, runs the agent, and prints the result.
3. It shows a `thinking...` indicator while the agent works.
4. It keeps running until the user presses Ctrl+D (EOF).

## The `chat()` method

Open `mini-claw-code-starter/src/agent.rs`. Below `run()` you will see the `chat()`
method signature.

### Why a new method?

`run()` creates a fresh `Vec<Message>` each time it is called. That means the
LLM has no memory of previous exchanges. A real CLI should carry context
forward, so the LLM can say "I already read that file" or "as I mentioned
earlier."

`chat()` solves this by accepting the message history from the caller:

```rust
pub async fn chat(&self, messages: &mut Vec<Message>) -> anyhow::Result<String>
```

The caller pushes `Message::User(…)` before calling, and `chat()` appends the
assistant turns. When it returns, `messages` contains the full conversation
history ready for the next round.

### The implementation

The loop body is identical to `run()`. The only differences are:

1. Use the provided `messages` instead of creating a new vec.
2. On `StopReason::Stop`, clone the text *before* pushing
   `Message::Assistant(turn)` -- the push moves `turn`, so you need the text
   first.
3. Push `Message::Assistant(turn)` so the history includes the final response.
4. Return the cloned text.

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
                // Same tool execution as run() ...
            }
        }
    }
}
```

The `ToolUse` branch is exactly the same as in `run()`: execute each tool,
collect results, push the assistant turn, push the tool results.

### Ownership detail

In `run()` you could do `return Ok(turn.text.unwrap_or_default())` directly
because the function was done with `turn`. In `chat()` you also need to push
`Message::Assistant(turn)` into the history. Since that push moves `turn`, you
must extract the text first:

```rust
let text = turn.text.clone().unwrap_or_default();
messages.push(Message::Assistant(turn));  // moves turn
return Ok(text);                          // return the clone
```

This is a one-line change from `run()`, but it matters.

## The CLI

Open `mini-claw-code-starter/examples/chat.rs`. You will see a skeleton with
`unimplemented!()`. Replace it with the full program.

### Step 1: Imports

```rust
use mini_claw_code_starter::{
    BashTool, EditTool, Message, OpenRouterProvider, ReadTool, SimpleAgent, WriteTool,
};
use std::io::{self, BufRead, Write};
```

Note the `Message` import -- you need it to build the history vector.

### Step 2: Create the provider and agent

```rust
let provider = OpenRouterProvider::from_env()?;
let agent = SimpleAgent::new(provider)
    .tool(BashTool::new())
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(EditTool::new());
```

Same as before -- nothing new here. (In [Chapter 11](./ch11-user-input.md)
you'll add `AskTool` here so the agent can ask you clarifying questions.)

### Step 3: The system prompt and history vector

```rust
let cwd = std::env::current_dir()?.display().to_string();
let mut history: Vec<Message> = vec![Message::System(format!(
    "You are a coding agent. Help the user with software engineering tasks \
     using all available tools. Be concise and precise.\n\n\
     Working directory: {cwd}"
))];
```

The system prompt is the first message in the history. It tells the LLM what
role it should play. Two things to note:

1. **No tool names in the prompt.** Tool definitions are sent separately to the
   API. The system prompt focuses on *behavior* -- be a coding agent, use
   whatever tools are available, be concise.

2. **Working directory is included.** The LLM needs to know where it is so that
   tool calls like `read` and `bash` use correct paths. This is what real
   coding agents do -- Claude Code, OpenCode, and Kimi CLI all inject the
   current directory (and sometimes platform, date, etc.) into their system
   prompts.

The history vector lives outside the loop and accumulates every user prompt,
assistant response, and tool result across the entire session. The system
prompt stays at the front, giving the LLM consistent instructions on every
turn.

### Step 4: The REPL loop

```rust
let stdin = io::stdin();

loop {
    print!("> ");
    io::stdout().flush()?;

    let mut line = String::new();
    if stdin.lock().read_line(&mut line)? == 0 {
        println!();
        break;
    }

    let prompt = line.trim();
    if prompt.is_empty() {
        continue;
    }

    history.push(Message::User(prompt.to_string()));
    print!("    thinking...");
    io::stdout().flush()?;
    match agent.chat(&mut history).await {
        Ok(text) => {
            print!("\x1b[2K\r");
            println!("{}\n", text.trim());
        }
        Err(e) => {
            print!("\x1b[2K\r");
            println!("error: {e}\n");
        }
    }
}
```

A few things to note:

- **`history.push(Message::User(…))`** adds the prompt before calling the
  agent. `chat()` will append the rest.
- **`print!("    thinking...")`** shows a status while the agent works. The
  `flush()` is needed because `print!` (no newline) does not flush
  automatically.
- **`\x1b[2K\r`** is an ANSI escape sequence: "erase entire line, move cursor
  to column 1." This clears the `thinking...` text before printing the
  response. It also gets cleared automatically when the agent prints a tool
  summary (since `tool_summary()` uses the same escape).
- **`stdout.flush()?`** after `print!` ensures the prompt and thinking
  indicator appear immediately.
- `read_line` returns `0` on EOF (Ctrl+D), which breaks the loop.
- Errors from the agent are printed instead of crashing -- this keeps the
  loop alive even if one request fails.

### The main function

Wrap everything in an async main:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Steps 1-4 go here
    Ok(())
}
```

### The complete program

Putting it all together, the entire program is about 45 lines. That is the
beauty of the framework you built -- the final assembly is straightforward
because each component has a clean interface.

## Running the full test suite

Run the full test suite:

```bash
cargo test -p mini-claw-code-starter
```

This runs all tests from chapters 1 through 7. If everything passes,
congratulations -- your agent framework is complete and fully tested.

### What the tests verify

The Chapter 7 tests are integration tests that combine all components:

- **Write-then-read flows**: Write a file, read it back, verify contents.
- **Edit flows**: Write a file, edit it, read back the result.
- **Multi-tool pipelines**: Use bash, write, edit, and read across multiple turns.
- **Long conversations**: Five-step tool-call sequences.

There are about 10 integration tests that exercise the full agent pipeline.

## Running the chat example

To try it with a real LLM, you need an API key. Create a `.env` file in the
workspace root:

```
OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

Then run:

```bash
cargo run -p mini-claw-code-starter --example chat
```

You will get an interactive prompt. Try a multi-turn conversation:

```text
> List the files in the current directory
    thinking...
    [bash: ls]
Cargo.toml  src/  examples/  ...

> What is in Cargo.toml?
    thinking...
    [read: Cargo.toml]
The Cargo.toml contains the package definition for mini-claw-code-starter...

> Add a new dependency for serde
    thinking...
    [read: Cargo.toml]
    [edit: Cargo.toml]
Done! I added serde to the dependencies.

>
```

Notice how the second prompt ("What is in Cargo.toml?") works without
repeating context -- the LLM already knows the directory listing from the
first exchange. That is conversation history at work.

Press Ctrl+D (or Ctrl+C) to exit.

## What you have built

Let's step back and look at the complete picture:

```text
examples/chat.rs
    |
    | creates
    v
SimpleAgent<OpenRouterProvider>
    |
    | holds
    +---> OpenRouterProvider (HTTP to LLM API)
    +---> ToolSet (HashMap<String, Box<dyn Tool>>)
              |
              +---> BashTool
              +---> ReadTool
              +---> WriteTool
              +---> EditTool
```

The `chat()` method drives the interaction:

```text
User prompt
    |
    v
history: [User, Assistant, ToolResult, ..., User]
    |
    v
Provider.chat() ---HTTP---> LLM API
    |
    | AssistantTurn
    v
Tool calls? ----yes---> Execute tools ---> append to history ---> loop
    |
    no
    |
    v
Append final Assistant to history, return text
```

In about 300 lines of Rust across all files, you have:

- A trait-based tool system with JSON schema definitions.
- A generic agent loop that works with any provider.
- A mock provider for deterministic testing.
- An HTTP provider for real LLM APIs.
- A CLI with conversation memory that ties it all together.

## Where to go from here

This framework is intentionally minimal. Here are ideas for extending it:

**Streaming responses** -- Instead of waiting for the full response, stream
tokens as they arrive. This means changing `chat()` to return a `Stream`
instead of a single `AssistantTurn`.

**Token limits** -- Track token usage and truncate old messages when the context
window fills up.

**More tools** -- Add a web search tool, a database query tool, or anything
else you can imagine. The `Tool` trait makes it easy to plug in new
capabilities.

**A richer UI** -- Add a spinner animation, markdown rendering, or collapsed
tool call display. See `mini-claw-code/examples/tui.rs` for an example that does
all three using `termimad`.

The foundation you built is solid. Every extension is a matter of adding to the
existing patterns, not rewriting them. The `Provider` trait, the `Tool` trait,
and the agent loop are the building blocks for anything you want to build next.

## What's next

Head to [Chapter 8: The Singularity](./ch08-singularity.md) -- your
agent can now modify its own source code, and we will talk about what that
means and where to go from here.
