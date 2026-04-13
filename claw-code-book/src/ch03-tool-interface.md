# Chapter 3: Tool Interface

In the last chapter we gave our agent a voice by connecting it to an LLM provider. But a model that can only produce text is like a programmer who can only talk about code without ever touching a keyboard. In this chapter we give the agent hands.

You already defined the tool types in Chapter 1 -- `ToolDefinition`, `Tool` trait, `ToolResult`, `ValidationResult`, and `ToolSet`. In this chapter we will understand *why* those types are designed the way they are, explore the critical distinction between `#[async_trait]` and RPITIT, and then wire everything together by implementing your first concrete tool: an `EchoTool`.

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

## Why `#[async_trait]` for tools but not for providers

This is the most important design decision in the type system, and it is worth understanding deeply.

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

Note that in the `MockProvider` impl from Chapter 2, we wrote `async fn chat(...)` directly. That works because Rust 1.75+ allows `async fn` in trait impls even when the trait signature uses the RPITIT form. The compiler desugars it correctly. You can do the same for `Tool` impls -- write `async fn call(...)` and the `#[async_trait]` macro handles the rest.

## Why errors are values, not `Err`

When a tool fails -- a file doesn't exist, a command exits non-zero, an edit can't be applied -- we do **not** return `Err(...)`. Instead, we return `Ok(ToolResult::error("..."))`.

Why? Because a tool failure is not an agent failure. If the LLM asks to read a file that doesn't exist, the correct behavior is to tell the LLM "error: file not found" and let it recover -- try a different path, ask the user, or move on. Returning `Err(...)` would bubble up through the agent loop and terminate the conversation.

Reserve `Err(...)` for genuinely unrecoverable situations: a network failure talking to the LLM, a serialization bug, or a programming error. Tool-level "this didn't work" is always `Ok(ToolResult::error(...))`.

This distinction will become critical in the next chapter when we build the query engine loop. The loop matches on `Ok` vs `Err` to decide whether to continue or abort -- and we want tool failures to continue, not abort.

## Hands-on: building an EchoTool

Time to implement your first concrete tool. We will build a minimal `EchoTool` that takes a `text` argument and returns it unchanged. This covers the full lifecycle: defining a schema, implementing the trait, and registering with a `ToolSet`.

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

Here is a quick reference for the `Tool` trait methods you defined in Chapter 1:

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

Despite these differences, the core protocol is identical. An LLM sees a list of tool schemas, decides to call one, the agent executes it, and the result goes back to the LLM. Everything else -- validation, permissions, progress, rendering -- is orchestration around that loop. Understanding the `Tool` trait gives you the foundation to understand Claude Code's full system.

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

This chapter focused on the *why* behind the tool types you defined in Chapter 1:

- **`#[async_trait]` vs RPITIT** -- the critical distinction. Tools need object safety for heterogeneous storage; providers need zero-cost generics. The two-strategy approach gives you both.
- **Errors are values** -- tool failures return `Ok(ToolResult::error(...))`, not `Err(...)`. The agent loop continues. The model adapts.
- **EchoTool** -- your first concrete tool, demonstrating the full lifecycle: schema definition, trait implementation, registration, execution.

In the next chapter we build the query engine -- the loop that ties providers and tools together into a functioning agent.
