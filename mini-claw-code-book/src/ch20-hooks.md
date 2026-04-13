# Chapter 20: Hooks

Your agent can run tools, stream responses, ask the user questions, and plan
before acting. But every new behavior -- logging, auditing, blocking dangerous
commands, running shell scripts on tool events -- requires touching the agent
loop directly. That does not scale.

Claude Code solves this with **hooks**: 12+ event types that let users and
extensions inject custom behavior at key points without rebuilding the agent.
Want to log every tool call? Register a hook. Want to block `bash` in
production? Register a hook. Want to run a linter after every file write?
Register a hook. The agent itself does not change.

In this chapter you will walk through:

1. A **`HookEvent` enum** for the events hooks respond to.
2. A **`HookAction` enum** for what hooks tell the agent to do.
3. A **`Hook` trait** -- the async interface every hook implements.
4. A **`HookRegistry`** that stores hooks and dispatches events.
5. Three **built-in hooks**: `LoggingHook`, `BlockingHook`, and `ShellHook`.
6. How hooks integrate with the agent loop.

## The event model

Open `mini-claw-code/src/hooks.rs`. At the top you will find two enums that
define the vocabulary between hooks and the agent.

### HookEvent

`HookEvent` describes *what happened*:

```rust
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Before a tool is executed.
    PreToolCall {
        tool_name: String,
        args: Value,
    },
    /// After a tool finishes executing.
    PostToolCall {
        tool_name: String,
        args: Value,
        result: String,
    },
    /// The agent is starting a new run.
    AgentStart {
        prompt: String,
    },
    /// The agent finished with a final response.
    AgentEnd {
        response: String,
    },
}
```

Four variants, each carrying the data a hook might need:

- **`PreToolCall`** fires *before* a tool runs. It carries the tool name and
  the arguments the LLM chose. A hook can inspect these, log them, or decide
  to block the call entirely.
- **`PostToolCall`** fires *after* a tool completes. It adds the `result`
  string so hooks can audit what happened.
- **`AgentStart`** fires once when the agent begins a new run, carrying the
  user's prompt.
- **`AgentEnd`** fires once when the agent produces its final response.

This gives hooks four natural insertion points: two per tool call (before and
after), plus the boundaries of the entire run.

### HookAction

`HookAction` describes *what should happen next*:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum HookAction {
    /// Continue normally.
    Continue,
    /// Block the tool call with a reason.
    Block(String),
    /// Modify the tool arguments (PreToolCall only).
    ModifyArgs(Value),
}
```

Three options:

- **`Continue`** -- do nothing special, proceed as normal.
- **`Block(reason)`** -- abort the tool call. The reason string becomes the
  tool result so the LLM knows what happened and can adjust.
- **`ModifyArgs(new_args)`** -- replace the tool arguments before execution.
  This only makes sense for `PreToolCall` events (you cannot retroactively
  change args after the tool ran).

The combination of `HookEvent` and `HookAction` is the entire contract. Hooks
receive events and return actions. Nothing more.

## The Hook trait

```rust
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
    /// Handle an event and return an action.
    async fn on_event(&self, event: &HookEvent) -> HookAction;
}
```

One method. It takes an immutable reference to a `HookEvent` and returns a
`HookAction`. The trait requires `Send + Sync` because hooks live inside the
agent, which may be shared across threads (e.g. wrapped in `Arc` for TUI
apps).

The method is `async` because some hooks need I/O -- `ShellHook` spawns a
child process, and future hooks might call HTTP endpoints. But simple hooks
like `LoggingHook` just push to a `Vec` and return immediately.

## HookRegistry

Individual hooks are useful, but you typically want multiple hooks active at
once -- a logger *and* a blocker *and* a shell script. `HookRegistry` manages
the collection:

```rust
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}
```

It stores hooks as trait objects in registration order. `register()` takes
`&mut self` for imperative use. `with()` takes `self` and returns it for
builder-pattern chaining:

```rust
let registry = HookRegistry::new()
    .with(LoggingHook::new())
    .with(BlockingHook::new(vec!["bash".into()], "blocked"));
```

There is also `is_empty()` so the agent loop can skip dispatch entirely when
no hooks are registered -- a minor optimization, but a nice one.

### Dispatch logic

The heart of the registry is `dispatch()`:

```rust
pub async fn dispatch(&self, event: &HookEvent) -> HookAction {
    let mut modified_args: Option<Value> = None;

    for hook in &self.hooks {
        match hook.on_event(event).await {
            HookAction::Continue => {}
            HookAction::Block(reason) => return HookAction::Block(reason),
            HookAction::ModifyArgs(new_args) => {
                modified_args = Some(new_args);
            }
        }
    }

    match modified_args {
        Some(args) => HookAction::ModifyArgs(args),
        None => HookAction::Continue,
    }
}
```

Three rules govern dispatch:

1. **Iterate in order.** Hooks fire in the order they were registered.
   Registration order is your priority system.

2. **Short-circuit on Block.** The moment any hook returns `Block`, dispatch
   stops immediately and returns that `Block`. Hooks registered *after* the
   blocking hook never see the event. This is important for correctness -- if
   a security hook blocks `bash`, a logging hook registered later should not
   log a call that never happened.

3. **Collect ModifyArgs.** If multiple hooks modify args, the last one wins
   (each overwrites `modified_args`). If no hook blocked and at least one
   modified args, `ModifyArgs` is returned. If nobody did anything,
   `Continue` is returned.

This gives you a clean priority chain: blocking hooks should be registered
before logging hooks so they can short-circuit first.

## Built-in hooks

The module provides three hooks out of the box. They cover the most common
patterns and serve as examples for writing your own.

### LoggingHook

```rust
pub struct LoggingHook {
    log: std::sync::Mutex<Vec<String>>,
}
```

`LoggingHook` records a one-line summary of every event into a `Vec<String>`.
Its `on_event` formats each variant into a compact tag -- `"pre:bash"`,
`"post:read"`, `"agent:start"`, `"agent:end"` -- pushes it into the vec
behind the mutex, and returns `Continue`. Logging is observation, not
intervention.

The `messages()` method clones and returns the accumulated log.

Notice this uses `std::sync::Mutex`, not `tokio::sync::Mutex`. The lock is
held only long enough to push a string or clone the vec -- no `.await` inside
the critical section. A `std::sync::Mutex` is cheaper than a `tokio::sync::Mutex`
for these short, synchronous operations. Compare this with `MockInputHandler`
from Chapter 11, which needed `tokio::sync::Mutex` because its lock guard was
held across an `.await` boundary.

`LoggingHook` is particularly useful in tests. Register it alongside other
hooks, run the agent, and then inspect `messages()` to verify exactly which
events fired and in what order.

### BlockingHook

```rust
pub struct BlockingHook {
    blocked_tools: Vec<String>,
    reason: String,
}
```

`BlockingHook` takes a list of tool names and a reason string. If a
`PreToolCall` event matches any blocked tool, it returns `Block`:

```rust
#[async_trait::async_trait]
impl Hook for BlockingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        if let HookEvent::PreToolCall { tool_name, .. } = event
            && self.blocked_tools.iter().any(|b| b == tool_name)
        {
            return HookAction::Block(self.reason.clone());
        }
        HookAction::Continue
    }
}
```

This uses a **let-chain** (same syntax as `resolve_option` in Chapter 11):
the `if let` pattern match and the `.any()` check are joined with `&&`. If
either condition fails, the hook returns `Continue`.

Use this for safety rails. For example, block `bash` in a read-only review
mode:

```rust
let registry = HookRegistry::new()
    .with(BlockingHook::new(
        vec!["bash".into(), "write".into(), "edit".into()],
        "read-only mode: mutation tools are disabled",
    ));
```

The LLM receives the reason string as the tool result, so it knows *why* the
call was blocked and can adapt its approach.

### ShellHook

```rust
pub struct ShellHook {
    command: String,
    tool_pattern: Option<glob::Pattern>,
}
```

`ShellHook` runs a shell command whenever a tool event fires. It is the escape
hatch: anything you can do in a shell script, you can do in a hook.

The `for_tool()` builder method restricts the hook to tools matching a glob
pattern. Without it, the hook fires on every tool event. With it, only
matching tool names trigger the command. The `glob` crate provides
Unix-style pattern matching -- `"write*"` would match `write` and
`write_file`, `"*"` matches everything.

The `Hook` implementation only responds to `PreToolCall` and `PostToolCall`
events (it ignores `AgentStart` and `AgentEnd`). It extracts the tool name,
checks `matches_tool()`, then spawns the command with
`tokio::process::Command::new("sh").arg("-c").arg(&self.command)`:

```rust
match result {
    Ok(output) => {
        if output.status.success() {
            HookAction::Continue
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            HookAction::Block(format!("hook failed: {stderr}"))
        }
    }
    Err(e) => HookAction::Block(format!("hook error: {e}")),
}
```

If the command succeeds (exit code 0), the hook returns `Continue`. If it
fails, the hook returns `Block` with the stderr output. This means a
`ShellHook` can act as a gate: run a linter after a file write, and block the
result if the linter fails.

Example -- run `cargo fmt --check` after every `write` or `edit`:

```rust
let registry = HookRegistry::new()
    .with(ShellHook::new("cargo fmt --check").for_tool("write"))
    .with(ShellHook::new("cargo fmt --check").for_tool("edit"));
```

## Integrating with the agent loop

Hooks are designed to sit at two points in the agent loop: **before** and
**after** tool execution. Here is how the dispatch points look conceptually
in a hook-aware agent:

```rust
for call in &turn.tool_calls {
    // 1. Dispatch PreToolCall
    let pre_action = registry.dispatch(&HookEvent::PreToolCall {
        tool_name: call.name.clone(),
        args: call.arguments.clone(),
    }).await;

    let result = match pre_action {
        HookAction::Block(reason) => reason, // skip the tool entirely
        HookAction::ModifyArgs(new_args) => {
            tool.call(new_args).await.unwrap_or_else(|e| format!("error: {e}"))
        }
        HookAction::Continue => {
            tool.call(call.arguments.clone()).await.unwrap_or_else(|e| format!("error: {e}"))
        }
    };

    // 2. Dispatch PostToolCall
    registry.dispatch(&HookEvent::PostToolCall {
        tool_name: call.name.clone(),
        args: call.arguments.clone(),
        result: result.clone(),
    }).await;
}
```

The pattern is:

1. **Before execution**: dispatch `PreToolCall`. If the action is `Block`,
   skip the tool entirely and use the reason as the result. If `ModifyArgs`,
   execute with the new args. If `Continue`, execute normally.

2. **After execution**: dispatch `PostToolCall` with the result. The return
   action is typically `Continue` (you cannot undo a tool call), but hooks
   can still log, audit, or trigger side effects.

3. **Run boundaries**: dispatch `AgentStart` at the beginning of `run()` and
   `AgentEnd` when the agent produces its final response.

The existing `SimpleAgent` and `StreamingAgent` do not have hooks wired in --
this is an extension point you would add when building a production agent. The
`HookRegistry` is intentionally separate so you can compose it into whatever
agent architecture you have.

## Tests

Run the tests with:

```bash
cargo test -p mini-claw-code ch20
```

The tests verify each component in isolation, then test composition:

- **LoggingHook**: fires a single `PreToolCall` and checks
  `messages() == ["pre:bash"]`. A second test fires all four event types and
  asserts the log matches `["agent:start", "pre:read", "post:read", "agent:end"]`.
- **BlockingHook**: `PreToolCall` for a blocked tool returns
  `Block("bash is disabled")`; the same hook returns `Continue` for `read`.
- **Registry dispatch**: a registry with only `LoggingHook` returns `Continue`.
  Adding a `BlockingHook` produces `Block` for the targeted tool.
- **Multiple hooks**: two `LoggingHook`s both see the event (both logs have
  length 1).
- **Short-circuit**: the most important test. A `BlockingHook` is registered
  *first*, a `LoggingHook` *second*:

```rust
let registry = HookRegistry::new()
    .with(BlockingHook::new(vec!["bash".into()], "blocked"))
    .with(ArcHook(log.clone()));

let action = registry.dispatch(&event).await;
assert_eq!(action, HookAction::Block("blocked".into()));

// The second hook should NOT have been called
assert_eq!(log.messages().len(), 0);
```

The logger never saw the event -- `Block` stopped iteration. Registration
order matters.

- **PostToolCall**: `LoggingHook` correctly logs `"post:write"`.
- **is_empty**: an empty registry returns `true`; adding a hook flips it to
  `false`.

## The observer/middleware pattern

If you have worked with web frameworks, hooks will feel familiar. They
implement two overlapping patterns:

- **Observer pattern**: hooks *observe* events without affecting them.
  `LoggingHook` is a pure observer -- it watches everything and changes
  nothing.

- **Middleware pattern**: hooks can *intercept and modify* the pipeline.
  `BlockingHook` short-circuits execution. `ModifyArgs` rewrites the request
  before it reaches the tool. This is middleware.

The `HookRegistry` is a middleware chain with observer capabilities. The
dispatch loop is the pipeline, `Block` is early return, and `ModifyArgs` is
request transformation.

This design keeps the agent loop clean. Instead of scattering `if` statements
for every new behavior, you register hooks. The agent loop just calls
`dispatch()` at two points and obeys the returned action. New behaviors are
added by implementing `Hook`, not by modifying the agent.

## Recap

- **`HookEvent`** represents four lifecycle points: `PreToolCall`,
  `PostToolCall`, `AgentStart`, `AgentEnd`.
- **`HookAction`** gives hooks three options: `Continue`, `Block`, or
  `ModifyArgs`.
- **`Hook` trait** has a single async method: `on_event`.
- **`HookRegistry`** dispatches events to hooks in order, short-circuiting on
  `Block` and collecting `ModifyArgs`.
- **`LoggingHook`** records events for inspection -- ideal for testing.
- **`BlockingHook`** blocks specific tools by name -- ideal for safety rails.
- **`ShellHook`** runs arbitrary shell commands on tool events -- the escape
  hatch for anything else.
- Hooks follow the **observer/middleware pattern**: observe without changing,
  or intercept and modify the pipeline.
- The agent loop stays clean -- just call `dispatch()` before and after tool
  execution and obey the returned action.
