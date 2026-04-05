# Chapter 3: Tool Interface

In the last chapter we gave our agent a voice by connecting it to an LLM provider. But a model that can only produce text is like a programmer who can only talk about code without ever touching a keyboard. In this chapter we give the agent hands.

We will build the `Tool` trait -- the interface every tool must implement to participate in the agent loop. By the end you will have a `ToolDefinition` schema builder, the `Tool` trait with its full suite of methods, a `ToolResult` type for returning output, and a `ToolSet` registry that holds tools by name. You will wire it all together by implementing a simple `EchoTool`.

## Design context: how Claude Code models tools

Claude Code's TypeScript codebase defines tools with a generic `Tool<Input, Output, Progress>` type. Each tool carries a Zod schema for input validation, returns rich structured output (sometimes including React elements for terminal rendering), and can emit progress events during long-running operations. There are over 40 tools in production, each with permission metadata, cost hints, and UI integration.

We are going to keep the shape but cut the ceremony. In our Rust version:

| Claude Code (TypeScript)            | claw-code (Rust)                      |
|-------------------------------------|---------------------------------------|
| `Tool<Input, Output, Progress>`     | `trait Tool` (no generics)            |
| Zod schema for input                | `serde_json::Value` + builder         |
| Rich `ToolResult<T>`               | `ToolResult { content, is_truncated }`|
| React-rendered progress             | `activity_description()` string       |
| 40+ tools with Zod validation       | 5 tools with JSON schema              |
| `isReadOnly`, `isDestructive`, etc. | Same flags as trait methods            |

The key simplification: we drop the generic parameters. Claude Code needs `<Input, Output, Progress>` because each tool has a distinct strongly-typed input shape and renders different UI. We use `serde_json::Value` for both input and output, which lets us store heterogeneous tools in a single collection without type erasure gymnastics.

## ToolDefinition: telling the LLM what a tool can do

When the agent sends a request to the LLM, it includes a list of tool schemas -- JSON objects describing each tool's name, purpose, and parameters. The LLM uses these schemas to decide which tool to call and what arguments to pass. This is the `ToolDefinition`:

```rust
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}
```

The `name` is what the LLM will use in its tool-call response. The `description` helps the LLM decide when to use the tool. The `parameters` field holds a JSON Schema object describing the expected arguments.

We provide a builder API so you never have to write raw JSON Schema by hand:

```rust
impl ToolDefinition {
    pub fn new(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    pub fn param(
        mut self,
        name: &str,
        type_: &str,
        description: &str,
        required: bool,
    ) -> Self {
        self.parameters["properties"][name] = serde_json::json!({
            "type": type_,
            "description": description
        });
        if required {
            self.parameters["required"]
                .as_array_mut()
                .unwrap()
                .push(Value::String(name.to_string()));
        }
        self
    }
}
```

The `new` constructor starts with an empty object schema. Each `param()` call chains on a property. The builder pattern makes definitions readable:

```rust
ToolDefinition::new("read_file", "Read a file from disk")
    .param("path", "string", "Absolute path to the file", true)
    .param("offset", "integer", "Line to start reading from", false)
    .param("limit", "integer", "Max lines to read", false)
```

There is also `param_raw()` for cases where you need a more complex schema than a simple type/description pair -- for example, an enum constraint or a nested object:

```rust
pub fn param_raw(mut self, name: &str, schema: Value, required: bool) -> Self {
    self.parameters["properties"][name] = schema;
    if required {
        self.parameters["required"]
            .as_array_mut()
            .unwrap()
            .push(Value::String(name.to_string()));
    }
    self
}
```

In Claude Code, tool schemas are defined with Zod and automatically converted to JSON Schema. We skip the intermediate schema library and write JSON Schema directly. It is a few more characters but one fewer dependency, and it keeps the mental model simple: what you build is exactly what the LLM receives.

## The Tool trait

Here is the full trait:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    // --- Identity ---
    fn definition(&self) -> &ToolDefinition;

    // --- Execution ---
    async fn call(&self, args: Value) -> anyhow::Result<ToolResult>;

    // --- Validation ---
    fn validate_input(&self, _args: &Value) -> ValidationResult {
        ValidationResult::Ok
    }

    // --- Safety & Behavior ---
    fn is_read_only(&self) -> bool { false }
    fn is_concurrent_safe(&self) -> bool { false }
    fn is_destructive(&self) -> bool { false }

    // --- Display ---
    fn summary(&self, args: &Value) -> String {
        let name = self.definition().name;
        let detail = args
            .get("command")
            .or_else(|| args.get("path"))
            .or_else(|| args.get("question"))
            .or_else(|| args.get("pattern"))
            .and_then(|v| v.as_str());
        match detail {
            Some(s) => format!("[{name}: {s}]"),
            None => format!("[{name}]"),
        }
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        None
    }
}
```

Let's walk through each method.

### `definition(&self) -> &ToolDefinition`

The only identity method. Returns the tool's schema. Every tool must implement this -- there is no default. The agent loop calls `definition()` on every registered tool to build the schema list sent to the LLM.

Returning a reference (`&ToolDefinition`) means the tool struct owns its definition. Typically you store a `ToolDefinition` as a field and return a reference to it.

### `call(&self, args: Value) -> anyhow::Result<ToolResult>`

The core execution method. `args` is the JSON object the LLM produced. You parse out the fields you need, do the work, and return a `ToolResult`. This is `async` because most tools perform I/O (reading files, running subprocesses, making network calls).

Note that `call` takes `&self`, not `&mut self`. Tools are shared across the agent loop and potentially across concurrent executions. If a tool needs mutable state, use interior mutability (`Mutex`, `RwLock`, etc.).

### `validate_input(&self, args: &Value) -> ValidationResult`

Optional pre-execution validation. The default implementation returns `ValidationResult::Ok` unconditionally. Override it when you want to catch bad arguments before doing any work -- for example, rejecting an empty `path` argument or checking that a file exists before attempting to edit it.

### `is_read_only(&self) -> bool`

Returns `true` if the tool never modifies anything -- files, environment, network state. The permission system uses this flag: in plan mode, only read-only tools are allowed to execute. Default: `false` (conservative -- assume a tool writes until told otherwise).

### `is_concurrent_safe(&self) -> bool`

Whether it is safe to run this tool in parallel with other tools. A read-file tool is concurrent-safe; a bash tool that might write to the filesystem is not. The query engine checks this flag before dispatching multiple tool calls simultaneously. Default: `false`.

### `is_destructive(&self) -> bool`

Whether the tool performs operations that are hard to undo -- deleting files, force-pushing branches, dropping databases. The permission system can require explicit user confirmation for destructive tools. Default: `false`.

### `summary(&self, args: &Value) -> String`

Produces a one-line string for terminal display, like `[bash: ls -la]` or `[read_file: src/main.rs]`. The default implementation looks for common argument names (`command`, `path`, `question`, `pattern`) and formats them. Override it if your tool has a different primary argument or needs custom formatting.

### `activity_description(&self, args: &Value) -> Option<String>`

Returns a short description for spinner display while the tool is executing, like `"Reading file..."` or `"Running command..."`. Default: `None` (the TUI will show a generic spinner). Override this when you want the user to see what the tool is actively doing.

## Why `#[async_trait]` for tools but not for providers

Look at the `Provider` trait from Chapter 2:

```rust
pub trait Provider: Send + Sync {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantMessage>> + Send + 'a;
}
```

This uses **RPITIT** (return-position `impl Trait` in traits), a feature stabilized in Rust 1.75. The compiler generates a unique future type for each implementation. It is zero-cost and avoids boxing.

But RPITIT has a catch: it makes the trait non-object-safe. You cannot write `Box<dyn Provider>` because the compiler needs to know the concrete future type at compile time. That is fine for providers -- we use them as generic parameters (`struct QueryEngine<P: Provider>`), so the concrete type is always known.

Tools are different. We need to store a heterogeneous collection of tools -- `BashTool`, `ReadTool`, `WriteTool`, all in one `HashMap`. That requires `Box<dyn Tool>`, which requires object safety. And object safety requires that async methods return a known type, not an opaque `impl Future`.

The `#[async_trait]` macro from the `async-trait` crate solves this by rewriting `async fn call(...)` into a method that returns `Pin<Box<dyn Future<...> + Send + '_>>`. The boxing has a small cost (one heap allocation per tool call), but tool calls involve I/O that dwarfs the allocation.

```
Provider: generic param P       -> RPITIT (zero-cost, not object-safe)
Tool:     stored in Box<dyn>    -> #[async_trait] (boxed future, object-safe)
```

This split is a deliberate design choice. If Rust stabilizes `dyn async fn` in the future, we could drop `async_trait` entirely. Until then, the two-strategy approach gives us the best of both worlds.

## ToolResult: why errors are values, not `Err`

When a tool fails -- a file doesn't exist, a command exits non-zero, an edit can't be applied -- we do **not** return `Err(...)`. Instead, we return `Ok(ToolResult::error("..."))`.

```rust
pub struct ToolResult {
    pub content: String,
    pub is_truncated: bool,
}

impl ToolResult {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_truncated: false,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            content: format!("error: {}", msg.into()),
            is_truncated: false,
        }
    }
}
```

Why? Because a tool failure is not an agent failure. If the LLM asks to read a file that doesn't exist, the correct behavior is to tell the LLM "error: file not found" and let it recover -- try a different path, ask the user, or move on. Returning `Err(...)` would bubble up through the agent loop and terminate the conversation.

Reserve `Err(...)` for genuinely unrecoverable situations: a network failure talking to the LLM, a serialization bug, or a programming error. Tool-level "this didn't work" is always `Ok(ToolResult::error(...))`.

The `is_truncated` field signals that the output was cut short (e.g., a file was too large to return in full). The LLM can use this information to request a specific range.

## ValidationResult

```rust
pub enum ValidationResult {
    Ok,
    Error { message: String, code: u32 },
}
```

Two variants, no surprises. When validation fails, the error message is returned to the LLM and the tool is not executed. The `code` field allows categorizing errors (invalid argument, missing field, permission denied) if you need to handle them programmatically.

Most tools leave `validate_input` at its default and handle bad arguments inside `call` instead. The validation hook is most useful when you want to reject a call before any side effects occur -- for example, blocking a bash command that matches a dangerous pattern.

## ToolSet: the tool registry

Tools need a home. The `ToolSet` is a `HashMap<String, Box<dyn Tool>>` with a convenience API:

```rust
pub struct ToolSet {
    tools: HashMap<String, Box<dyn Tool>>,
}
```

### Building a ToolSet

Two styles, depending on whether you are building the set all at once or adding tools incrementally:

```rust
// Builder style (immutable chain)
let tools = ToolSet::new()
    .with(BashTool::new())
    .with(ReadTool::new())
    .with(WriteTool::new());

// Incremental style (mutable push)
let mut tools = ToolSet::new();
tools.push(BashTool::new());
tools.push(ReadTool::new());
```

The `with` method takes `self` by value and returns `Self`, enabling chaining. The `push` method takes `&mut self` for when you need to add tools conditionally.

### Lookup and enumeration

```rust
// Get a tool by name (returns Option<&dyn Tool>)
if let Some(tool) = tools.get("bash") {
    let result = tool.call(args).await?;
}

// Get all definitions (for sending to the LLM)
let defs: Vec<&ToolDefinition> = tools.definitions();

// List tool names
let names: Vec<&str> = tools.names();

// Size checks
assert!(!tools.is_empty());
assert_eq!(tools.len(), 3);
```

The `definitions()` method is what the query engine calls before each LLM request. It collects references to every registered tool's `ToolDefinition`, which get serialized into the request payload. The LLM sees the full catalog of available tools on every turn.

## Hands-on: building an EchoTool

Time to implement. We will build a minimal `EchoTool` that takes a `text` argument and returns it unchanged. This covers the full lifecycle: defining a schema, implementing the trait, and registering with a `ToolSet`.

### Step 1: the struct and definition

```rust
struct EchoTool {
    def: ToolDefinition,
}

impl EchoTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition::new("echo", "Echo the input")
                .param("text", "string", "Text to echo", true),
        }
    }
}
```

The `ToolDefinition` is built once in the constructor and stored as a field. The schema tells the LLM: "this tool is called `echo`, it takes a required string parameter called `text`."

### Step 2: implement the Tool trait

```rust
#[async_trait::async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let text = args["text"].as_str().unwrap_or("(no text)");
        Ok(ToolResult::text(text))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
}
```

A few things to note:

- `definition()` returns a reference to the stored `ToolDefinition`.
- `call()` indexes into the JSON `args` to extract `text`. If the key is missing or not a string, we fall back to `"(no text)"` rather than panicking. Always be defensive with LLM-provided arguments.
- We override `is_read_only` and `is_concurrent_safe` to `true` because echoing text has no side effects. We leave `is_destructive` at its default `false`.
- We do **not** override `validate_input`, `summary`, or `activity_description`. The defaults work fine here.

### Step 3: register and use

```rust
let tools = ToolSet::new().with(EchoTool::new());

// The agent loop would do this:
let defs = tools.definitions();
// ... send defs to LLM, get back a ToolCall ...

let tool = tools.get("echo").unwrap();
let result = tool.call(serde_json::json!({"text": "hello"})).await?;
assert_eq!(result.content, "hello");
```

That is the full round-trip. Definition goes to the LLM, the LLM produces a `ToolCall`, we look up the tool by name, call it, and feed the result back.

## Default methods: what to override and when

Here is a quick reference:

| Method | Default | Override when... |
|--------|---------|-----------------|
| `definition()` | (none -- required) | Always |
| `call()` | (none -- required) | Always |
| `validate_input()` | `ValidationResult::Ok` | You want to reject bad args before execution |
| `is_read_only()` | `false` | Your tool never modifies anything |
| `is_concurrent_safe()` | `false` | Your tool is safe to run in parallel |
| `is_destructive()` | `false` | Your tool does hard-to-undo operations |
| `summary()` | `[name: detail]` | Your tool's primary arg isn't `command`/`path`/`question`/`pattern` |
| `activity_description()` | `None` | You want a custom spinner message |

The defaults are conservative: not read-only, not concurrent-safe, not destructive. A tool that forgets to override these flags will be treated cautiously by the permission system and query engine. That is the right failure mode -- being too cautious is better than accidentally running destructive operations in parallel.

## How this compares to Claude Code

Claude Code's tool system is substantially larger:

- **40+ tools** spanning file operations, git, search, browser, notebook, MCP, and more. We build 5.
- **Zod schemas** provide runtime validation with TypeScript type inference. We use `serde_json::Value` with a builder.
- **React rendering** -- tools can return React elements that render rich terminal UI (diffs, tables, progress bars). We return plain strings.
- **Progress events** -- tools emit typed progress events during execution. We have `activity_description()` for a simple spinner.
- **Tool groups and permissions** -- tools are organized into permission groups with allow/deny lists. We will build our permission system in Chapter 10, but it will be simpler.
- **Cost hints** -- tools can declare estimated token costs to help the context manager. We will add token tracking in Chapter 17 at the session level.

Despite these differences, the core protocol is identical. An LLM sees a list of tool schemas, decides to call one, the agent executes it, and the result goes back to the LLM. Everything else -- validation, permissions, progress, rendering -- is orchestration around that loop. Understanding the `Tool` trait in this chapter gives you the foundation to understand Claude Code's full system.

## Run the tests

```bash
cargo test -p claw-code test_ch3
```

You should see these tests pass:

- `test_ch3_tool_definition` -- the `EchoTool` produces the correct name and description
- `test_ch3_tool_call` -- calling with `{"text": "hello"}` returns `"hello"`
- `test_ch3_tool_is_read_only` -- the safety flags are set correctly
- `test_ch3_tool_summary` -- the default summary includes the tool name
- `test_ch3_tool_default_validation` -- validation returns `Ok` by default
- `test_ch3_toolset_register_and_get` -- registering and looking up a tool by name
- `test_ch3_toolset_definitions` -- `definitions()` returns the registered tool schemas
- `test_ch3_toolset_names` -- `names()` lists registered tool names
- `test_ch3_toolset_push` -- incremental registration with `push()`

## Summary

You now have the complete tool interface:

- **ToolDefinition** describes a tool's schema for the LLM, built with a chainable API.
- **Tool** is the async trait every tool implements. Two required methods (`definition`, `call`), seven optional methods with sensible defaults.
- **ToolResult** carries output back to the LLM. Errors are values, not panics.
- **ValidationResult** gates execution before side effects.
- **ToolSet** is a registry that maps names to boxed tools for the agent loop.
- **`#[async_trait]`** makes the trait object-safe so heterogeneous tools can coexist in a `HashMap`.

In the next chapter we build the query engine -- the loop that ties providers and tools together into a functioning agent.
