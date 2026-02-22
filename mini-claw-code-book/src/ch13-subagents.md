# Chapter 13: Subagents

Complex tasks are hard. Even the best LLM struggles when a single prompt asks
it to research a codebase, design an approach, write the code, and verify the
result -- all while maintaining a coherent conversation. The context window
fills up, the model loses focus, and quality degrades.

**Subagents** solve this with decomposition: the parent agent spawns a child
agent for each subtask. The child has its own message history and tools, runs
to completion, and returns a summary. The parent sees only the final answer --
a clean, focused result without the noise of the child's internal reasoning.

This is exactly how Claude Code's **Task tool** works. When Claude Code needs
to explore a large codebase or handle an independent subtask, it spawns a
subagent that does the work and reports back. OpenCode and the Anthropic Agent
SDK use the same pattern.

In this chapter you'll build `SubagentTool` -- a `Tool` implementation that
spawns ephemeral child agents.

You will:

1. Add a blanket `impl Provider for Arc<P>` so parent and child can share a
   provider.
2. Build `SubagentTool<P: Provider>` with a closure-based tool factory and
   builder methods.
3. Implement the `Tool` trait with an inlined agent loop and turn limit.
4. Wire it up as a module and re-export.

## Why subagents?

Consider this scenario:

```text
User: "Add error handling to all API endpoints"

Agent (no subagents):
  → reads 15 files, context window fills up
  → forgets what it learned from file 3
  → produces inconsistent changes

Agent (with subagents):
  → spawns child: "Add error handling to /api/users.rs"
  → child reads 1 file, writes changes, returns "Done: added Result types"
  → spawns child: "Add error handling to /api/posts.rs"
  → child does the same
  → parent sees clean summaries, coordinates the overall task
```

The key insight: **a subagent is just a Tool**. It takes a task description as
input, does work internally, and returns a string result. The parent's agent
loop doesn't need any special handling -- it calls the subagent tool the same
way it calls `read` or `bash`.

## Provider sharing with `Arc<P>`

The parent and child need to use the same LLM provider. In production this
means sharing an HTTP client, API key, and configuration. Cloning the provider
would duplicate connections. We want to share it cheaply.

The answer is `Arc<P>`. But there's a catch: our `Provider` trait uses RPITIT
(return-position `impl Trait` in trait), which means it's not object-safe. We
can't use `dyn Provider`. We *can* use `Arc<P>` where `P: Provider` -- but
only if `Arc<P>` itself implements `Provider`.

A blanket impl makes this work. In `types.rs`:

```rust
impl<P: Provider> Provider for Arc<P> {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a {
        (**self).chat(messages, tools)
    }
}
```

This delegates to the inner `P` via deref. Now `Arc<MockProvider>` and
`Arc<OpenRouterProvider>` are both valid providers. Existing code is
completely unchanged -- if you were passing `MockProvider` before, it still
works. The `Arc` wrapper is opt-in.

## The `SubagentTool` struct

```rust
pub struct SubagentTool<P: Provider> {
    provider: Arc<P>,
    tools_factory: Box<dyn Fn() -> ToolSet + Send + Sync>,
    system_prompt: Option<String>,
    max_turns: usize,
    definition: ToolDefinition,
}
```

Three design decisions here:

**`Arc<P>` for the provider.** Parent creates `Arc::new(provider)`, keeps a
clone for itself, and passes a clone to `SubagentTool`. Both share the same
underlying provider. Cheap, safe, no cloning of HTTP clients.

**A closure factory for tools.** Tools are `Box<dyn Tool>` -- they're not
cloneable. Each child spawn needs a fresh `ToolSet`. A `Fn() -> ToolSet`
closure produces one on demand. This naturally captures `Arc`s for shared
state:

```rust
let provider = Arc::new(OpenRouterProvider::from_env()?);

SubagentTool::new(provider, || {
    ToolSet::new()
        .with(ReadTool::new())
        .with(WriteTool::new())
        .with(BashTool::new())
})
```

**A `max_turns` safety limit.** Without this, a confused child could loop
forever. Defaults to 10 -- generous enough for real tasks, strict enough to
prevent runaway loops.

## The builder

Construction uses the same fluent builder style as elsewhere in the codebase:

```rust
impl<P: Provider> SubagentTool<P> {
    pub fn new(
        provider: Arc<P>,
        tools_factory: impl Fn() -> ToolSet + Send + Sync + 'static,
    ) -> Self {
        Self {
            provider,
            tools_factory: Box::new(tools_factory),
            system_prompt: None,
            max_turns: 10,
            definition: ToolDefinition::new(
                "subagent",
                "Spawn a child agent to handle a subtask independently. \
                 The child has its own message history and tools.",
            )
            .param(
                "task",
                "string",
                "A clear description of the subtask for the child agent to complete.",
                true,
            ),
        }
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn max_turns(mut self, max: usize) -> Self {
        self.max_turns = max;
        self
    }
}
```

The tool definition exposes a single `task` parameter -- the LLM writes a
clear description of what the child should do. Minimal and effective.

## The `Tool` implementation

The core of `SubagentTool` is its `Tool::call()` method. It inlines a minimal
agent loop -- the same protocol as `SimpleAgent::chat()` (call provider, execute
tools, loop), but with a turn limit, no terminal output, and a locally-owned
message vec:

```rust
#[async_trait::async_trait]
impl<P: Provider + 'static> Tool for SubagentTool<P> {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter: task"))?;

        let tools = (self.tools_factory)();
        let defs = tools.definitions();

        let mut messages = Vec::new();
        if let Some(ref prompt) = self.system_prompt {
            messages.push(Message::System(prompt.clone()));
        }
        messages.push(Message::User(task.to_string()));

        for _ in 0..self.max_turns {
            let turn = self.provider.chat(&messages, &defs).await?;

            match turn.stop_reason {
                StopReason::Stop => {
                    return Ok(turn.text.unwrap_or_default());
                }
                StopReason::ToolUse => {
                    let mut results = Vec::with_capacity(turn.tool_calls.len());
                    for call in &turn.tool_calls {
                        let content = match tools.get(&call.name) {
                            Some(t) => t
                                .call(call.arguments.clone())
                                .await
                                .unwrap_or_else(|e| format!("error: {e}")),
                            None => format!("error: unknown tool `{}`", call.name),
                        };
                        results.push((call.id.clone(), content));
                    }
                    messages.push(Message::Assistant(turn));
                    for (id, content) in results {
                        messages.push(Message::ToolResult { id, content });
                    }
                }
            }
        }

        Ok("error: max turns exceeded".to_string())
    }
}
```

A few things to notice:

**No `tokio::spawn`.** The child runs within the parent's `Tool::call()`
future. This is deliberate -- spawning a background task would add
coordination complexity (channels, join handles, cancellation). Running
inline keeps things simple and deterministic.

**Fresh message history.** The child starts with only a system prompt
(optional) and the task as a `User` message. It never sees the parent's
conversation. When the child finishes, only its final text is returned to the
parent as a tool result. The child's internal messages are dropped.

**Turn limit as a soft error.** When `max_turns` is exceeded, the tool
returns an error string rather than `Err(...)`. This lets the parent LLM see
the failure and decide what to do (retry with a simpler task, try a different
approach, etc.), rather than crashing the entire agent loop.

**Provider errors propagate.** If the LLM API fails during a child turn, the
error bubbles up through `?` to the parent. This is intentional -- API errors
are infrastructure failures, not task failures.

## Wiring it up

Add the module and re-export in `mini-claw-code/src/lib.rs`:

```rust
pub mod subagent;
// ...
pub use subagent::SubagentTool;
```

## Usage example

Here's how you'd wire up a parent agent with a subagent tool:

```rust
use std::sync::Arc;
use mini_claw_code::*;

let provider = Arc::new(OpenRouterProvider::from_env()?);
let p = provider.clone();

let agent = SimpleAgent::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(BashTool::new())
    .tool(SubagentTool::new(p, || {
        ToolSet::new()
            .with(ReadTool::new())
            .with(WriteTool::new())
            .with(BashTool::new())
    }));

let result = agent.run("Refactor the auth module").await?;
```

The parent LLM sees `subagent` in its tool list alongside `read`, `write`,
and `bash`. When the task is complex enough, the LLM can choose to delegate
via `subagent` -- or handle it directly with the other tools. The LLM
decides.

You can also give the child a specialized system prompt:

```rust
SubagentTool::new(provider, || {
    ToolSet::new()
        .with(ReadTool::new())
        .with(BashTool::new())
})
.system_prompt("You are a security auditor. Review code for vulnerabilities.")
.max_turns(15)
```

## Running the tests

```bash
cargo test -p mini-claw-code ch13
```

The tests verify:

- **Text response**: child returns text immediately (no tool calls).
- **With tool**: child uses `ReadTool` before answering.
- **Multi-step**: child makes multiple tool calls across turns.
- **Max turns exceeded**: turn limit enforced, returns error string.
- **Missing task**: error on missing `task` parameter.
- **Provider error**: child provider error propagates to parent.
- **Unknown tool**: child handles unknown tools gracefully.
- **Builder pattern**: chaining `.system_prompt().max_turns()` compiles.
- **System prompt**: child runs correctly with a system prompt configured.
- **Write tool**: child writes a file, parent continues afterward.
- **Parent continues**: parent resumes its own work after subagent completes.
- **Isolated history**: child messages don't leak into parent's message vec.

## Recap

- **`SubagentTool`** is a `Tool` that spawns ephemeral child agents. The
  parent sees only the final answer.
- **`Arc<P>`** blanket impl lets parent and child share a provider without
  cloning. Fully backward-compatible.
- **Closure factory** produces a fresh `ToolSet` per child spawn, since
  `Box<dyn Tool>` isn't cloneable.
- **Inlined agent loop** with `max_turns` guard keeps `SimpleAgent` unchanged.
  No `tokio::spawn` needed -- the child runs within `Tool::call()`.
- **Message isolation**: the child's internal messages are local to the
  `call()` future. Only the final text crosses back to the parent.
- **Single `task` parameter**: the LLM writes a clear task description; the
  child handles the rest.
- **Purely additive**: the only existing change is the blanket impl in
  `types.rs`. Everything else is new code.
