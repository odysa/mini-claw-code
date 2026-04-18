# Chapter 11: User Input

Your agent can read files, run commands, and write code -- but it can't ask
*you* a question. If it's unsure which approach to take, which file to target,
or whether to proceed with a destructive operation, it just guesses.

Real coding agents solve this with an **ask tool**. Claude Code has
`AskUserQuestion`, Kimi CLI has approval prompts. The LLM calls a special tool,
the agent pauses, and the user types an answer. The answer goes back as a tool
result and execution continues.

In this chapter you'll build:

1. An **`InputHandler` trait** that abstracts how user input is collected.
2. An **`AskTool`** that the LLM calls to ask the user a question.
3. Three handler implementations: CLI, channel-based (for TUI), and mock (for
   tests).

## Why a trait?

Different UIs collect input differently:

- A **CLI** app prints to stdout and reads from stdin.
- A **TUI** app sends a request through a channel and waits for the event loop
  to collect the answer (maybe with arrow-key selection).
- **Tests** need to provide canned answers without any I/O.

The `InputHandler` trait lets `AskTool` work with all three without knowing
which one it's using:

```rust
#[async_trait::async_trait]
pub trait InputHandler: Send + Sync {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String>;
}
```

The `question` is what the LLM wants to ask. The `options` slice is an optional
list of choices -- if empty, the user types free-text. If non-empty, the UI can
present a selection list.

## AskTool

`AskTool` implements the `Tool` trait. It takes an `Arc<dyn InputHandler>` so
the handler can be shared across threads:

```rust
pub struct AskTool {
    definition: ToolDefinition,
    handler: Arc<dyn InputHandler>,
}
```

### Tool definition

The LLM needs to know what parameters the tool accepts. `question` is required
(a string). `options` is optional (an array of strings).

For `options`, we need a JSON schema for an array type -- something `param()`
can't express since it only handles scalar types. So first, add `param_raw()`
to `ToolDefinition`:

```rust
/// Add a parameter with a raw JSON schema value.
///
/// Use this for complex types (arrays, nested objects) that `param()` can't express.
pub fn param_raw(mut self, name: &str, schema: Value, required: bool) -> Self {
    self.parameters["properties"][name] = schema;
    if required {
        self.parameters["required"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::String(name.to_string()));
    }
    self
}
```

Now the tool definition uses both `param()` and `param_raw()`:

```rust
impl AskTool {
    pub fn new(handler: Arc<dyn InputHandler>) -> Self {
        Self {
            definition: ToolDefinition::new(
                "ask_user",
                "Ask the user a clarifying question...",
            )
            .param("question", "string", "The question to ask the user", true)
            .param_raw(
                "options",
                json!({
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of choices to present to the user"
                }),
                false,
            ),
            handler,
        }
    }
}
```

### Tool::call

The `call` implementation extracts `question`, parses options with a helper,
and delegates to the handler:

```rust
#[async_trait::async_trait]
impl Tool for AskTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter: question"))?;

        let options = parse_options(&args);

        self.handler.ask(question, &options).await
    }
}

/// Extract the optional `options` array from tool arguments.
fn parse_options(args: &Value) -> Vec<String> {
    args.get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
```

The `parse_options` helper keeps `call()` focused on the happy path. If
`options` is missing or not an array, it defaults to an empty vec -- the
handler treats this as free-text input.

## Three handlers

### CliInputHandler

The simplest handler. Prints the question, lists numbered choices (if any),
reads a line from stdin, and resolves numbered answers:

```rust
pub struct CliInputHandler;

#[async_trait::async_trait]
impl InputHandler for CliInputHandler {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String> {
        let question = question.to_string();
        let options = options.to_vec();

        // spawn_blocking because stdin is synchronous
        tokio::task::spawn_blocking(move || {
            // Display the question and numbered choices (if any)
            println!("\n  {question}");
            for (i, opt) in options.iter().enumerate() {
                println!("    {}) {opt}", i + 1);
            }

            // Read the answer
            print!("  > ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            let answer = line.trim().to_string();

            // If the user typed a valid option number, resolve it
            Ok(resolve_option(&answer, &options))
        }).await?
    }
}

/// If `answer` is a number matching one of the options, return that option.
/// Otherwise return the raw answer.
fn resolve_option(answer: &str, options: &[String]) -> String {
    if let Ok(n) = answer.parse::<usize>()
        && n >= 1
        && n <= options.len()
    {
        return options[n - 1].clone();
    }
    answer.to_string()
}
```

The `resolve_option` helper keeps the closure body clean. It uses **let-chain
syntax** (stabilized in Rust 1.87 / edition 2024): multiple conditions joined
with `&&` including `let Ok(n) = ...` pattern bindings. If the user types `"2"`
and there are three options, it resolves to `options[1]`. Otherwise the raw text
is returned.

Note the `for` loop over `options` does nothing when the slice is empty -- no
special `if` branch needed.

Use this in simple CLI apps like `examples/chat.rs`:

```rust
let agent = SimpleAgent::new(provider)
    .tool(BashTool::new())
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(EditTool::new())
    .tool(AskTool::new(Arc::new(CliInputHandler)));
```

### ChannelInputHandler

For TUI apps, input collection happens in the event loop, not in the tool. The
`ChannelInputHandler` bridges the gap with a channel:

```rust
pub struct UserInputRequest {
    pub question: String,
    pub options: Vec<String>,
    pub response_tx: oneshot::Sender<String>,
}

pub struct ChannelInputHandler {
    tx: mpsc::UnboundedSender<UserInputRequest>,
}
```

When `ask()` is called, it sends a `UserInputRequest` through the channel and
awaits the oneshot response:

```rust
#[async_trait::async_trait]
impl InputHandler for ChannelInputHandler {
    async fn ask(&self, question: &str, options: &[String]) -> anyhow::Result<String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx.send(UserInputRequest {
            question: question.to_string(),
            options: options.to_vec(),
            response_tx,
        })?;
        Ok(response_rx.await?)
    }
}
```

The TUI event loop receives the request and renders it however it likes --
a simple text prompt, or an arrow-key-navigable selection list using
`crossterm` in raw terminal mode.

### MockInputHandler

For tests, pre-configure answers in a queue:

```rust
pub struct MockInputHandler {
    answers: Mutex<VecDeque<String>>,
}

#[async_trait::async_trait]
impl InputHandler for MockInputHandler {
    async fn ask(&self, _question: &str, _options: &[String]) -> anyhow::Result<String> {
        self.answers.lock().await.pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockInputHandler: no more answers"))
    }
}
```

This follows the same pattern as `MockProvider` -- pop from the front, error
when empty. Note that this uses `tokio::sync::Mutex` (with `.lock().await`),
not `std::sync::Mutex`. The reason: `ask()` is an `async fn`, and the lock
guard must be held across the `.await` boundary. A `std::sync::Mutex` guard is
`!Send`, so holding it across `.await` won't compile. `tokio::sync::Mutex`
produces a `Send`-safe guard that works in async contexts. Compare this with
`MockProvider` from Chapter 1, which uses `std::sync::Mutex` because its
`chat()` method doesn't hold the guard across an `.await`.

## Tool summary

Update `tool_summary()` in `agent.rs` to display `"question"` for `ask_user`
calls in the terminal output:

```rust
let detail = call.arguments
    .get("command")
    .or_else(|| call.arguments.get("path"))
    .or_else(|| call.arguments.get("question"))  // <-- new
    .and_then(|v| v.as_str());
```

## Plan mode integration

`ask_user` is read-only -- it collects information without mutating anything.
Add it to `PlanAgent`'s default `read_only` set (see
[Chapter 12](./ch12-plan-mode.md)) so the LLM can ask questions during
planning:

```rust
read_only: HashSet::from(["bash", "read", "ask_user"]),
```

## Wiring it up

Add the module to `mini-claw-code/src/tools/mod.rs`:

```rust
mod ask;
pub use ask::*;
```

And re-export from `lib.rs`:

```rust
pub use tools::{
    AskTool, BashTool, ChannelInputHandler, CliInputHandler,
    EditTool, InputHandler, MockInputHandler, ReadTool,
    UserInputRequest, WriteTool,
};
```

## Running the tests

```bash
cargo test -p mini-claw-code ch11
```

The tests verify:

- **Tool definition**: schema has `question` (required) and `options` (optional
  array).
- **Question only**: `MockInputHandler` returns answer for a question-only call.
- **With options**: tool passes options to the handler correctly.
- **Missing question**: missing `question` argument returns an error.
- **Handler exhausted**: empty `MockInputHandler` returns an error.
- **Agent loop**: LLM calls `ask_user`, gets an answer, then returns final
  text.
- **Ask then tool**: `ask_user` followed by another tool call (e.g. `read`).
- **Multiple asks**: two sequential `ask_user` calls with different answers.
- **Channel roundtrip**: `ChannelInputHandler` sends request and receives
  response via oneshot channel.
- **param_raw**: `param_raw()` adds array parameter to `ToolDefinition`
  correctly.

## Recap

- **`InputHandler` trait** abstracts input collection across CLI, TUI, and
  tests.
- **`AskTool`** lets the LLM pause execution and ask the user a question.
- **`param_raw()`** extends `ToolDefinition` to support complex JSON schema
  types like arrays.
- **Three handlers**: `CliInputHandler` for simple apps,
  `ChannelInputHandler` for TUI apps, `MockInputHandler` for tests.
- **Plan mode**: `ask_user` is read-only by default, so it works during
  planning.
- **Purely additive**: no changes to `SimpleAgent`, `StreamingAgent`, or any
  existing tool.
