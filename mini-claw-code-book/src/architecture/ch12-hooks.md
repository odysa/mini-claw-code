# Chapter 12: Hooks

> **File(s) to edit:** `src/hooks.rs`
> **Test to run:** `cargo test -p mini-claw-code-starter test_ch20`

The permission engine from Chapter 10 decides whether a tool call runs. The safety checks from Chapter 11 catch dangerous patterns before the user even sees a prompt. But both systems are baked into the agent -- they enforce rules that you, the developer, chose at compile time. What about the user?

Users have policies that the agent author cannot anticipate. A team might require that every bash command is logged to an audit file. A project might enforce that file writes only touch a specific directory. A CI pipeline might need to run a linter after every edit. These are not safety checks in the "prevent `rm -rf /`" sense -- they are workflow hooks that extend the agent's behavior at runtime.

This chapter builds the hook system. Hooks are event-driven: they fire at key lifecycle points (before a tool call, after a tool call, when the agent starts, when it ends) and they can observe, modify, or block execution. The trait-based design means anyone can implement a hook -- a logging hook for debugging, a blocking hook for policy enforcement, a shell hook that delegates decisions to external commands.

```bash
cargo test -p mini-claw-code-starter test_ch20
```

---

## The event model

Before writing any code, let's define when hooks fire. The agent loop from Chapter 4 has a clear lifecycle:

```
User prompt arrives
  -> AgentStart
  -> Provider returns tool calls
    -> PreToolCall (for each tool)
    -> Tool executes
    -> PostToolCall (for each tool)
  -> Provider returns final answer
  -> AgentEnd
```

Four events, four points where external code can intervene:

| Event | When it fires | What hooks can do |
|-------|---------------|-------------------|
| `AgentStart` | Before the first provider call | Log the prompt, initialize state |
| `PreToolCall` | Before each tool execution | Block the call, modify arguments |
| `PostToolCall` | After each tool execution | Log the result, trigger follow-up actions |
| `AgentEnd` | After the final response | Log the response, clean up state |

The asymmetry is deliberate. `PreToolCall` can block or modify because the tool has not run yet -- there is still time to intervene. `PostToolCall` cannot block because the tool already ran -- blocking at this point would be meaningless. It can only observe.

---

## Core types

Open `src/hooks.rs`. The module defines three types: `HookEvent`, `HookAction`, and the `Hook` trait.

### HookEvent

```rust
#[derive(Debug, Clone)]
pub enum HookEvent {
    PreToolCall {
        tool_name: String,
        args: Value,
    },
    PostToolCall {
        tool_name: String,
        args: Value,
        result: String,
    },
    AgentStart {
        prompt: String,
    },
    AgentEnd {
        response: String,
    },
}
```

Each variant carries the data relevant to its lifecycle point. `PreToolCall` carries the tool name and arguments -- everything a hook needs to decide whether to allow or modify the call. `PostToolCall` adds the result string. `AgentStart` and `AgentEnd` carry the user prompt and final response respectively.

The enum derives `Clone` because the `HookRegistry` passes events by shared reference (`&HookEvent`) to each hook in sequence. Hooks that need to store events (like the `LoggingHook`) clone them. Hooks that only inspect events (like the `BlockingHook`) borrow without cloning.

### HookAction

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum HookAction {
    Continue,
    Block(String),
    ModifyArgs(Value),
}
```

Three possible responses, ordered by severity:

- **`Continue`** -- the default. The hook has nothing to say. Execution proceeds normally.
- **`Block(reason)`** -- stop the tool call. The reason string is returned to the LLM as an error message so it can understand why the call was rejected and adjust its approach.
- **`ModifyArgs(new_args)`** -- replace the tool's arguments before execution. This is how hooks can inject defaults, normalize paths, or enforce constraints without blocking the call entirely.

`HookAction` derives `PartialEq` so tests can assert on specific actions with `assert_eq!`. This is purely a testing convenience -- the runtime uses pattern matching, not equality checks.

### The Hook trait

```rust
#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_event(&self, event: &HookEvent) -> HookAction;
}
```

One method. It receives an event reference and returns an action. The trait requires `Send + Sync` because hooks live inside the `HookRunner` and the runner may be shared across async tasks. The `async_trait` attribute handles the usual ceremony of boxing the returned future.

This is the same pattern as the `Tool` trait from Chapter 3 -- a single async method that takes structured input and returns structured output. The difference is scope: tools interact with the outside world (filesystem, shell), while hooks interact with the agent's own execution.

---

## The HookRegistry

Individual hooks are useful, but the real value is composing them. The `HookRegistry` holds a list of hooks and dispatches events to them sequentially.

```rust
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: impl Hook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    pub fn with(mut self, hook: impl Hook + 'static) -> Self {
        self.register(hook);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }
}
```

The builder API should look familiar -- it mirrors `ToolSet` from Chapter 1. The `with()` method takes ownership and returns `self` for chaining. The `register()` method takes `&mut self` for imperative code. Both accept `impl Hook + 'static`, boxing the concrete type into a trait object.

### The dispatch method

The interesting part is how actions compose:

```rust
pub async fn dispatch(&self, event: &HookEvent) -> HookAction {
    // Iterate hooks in order
    // If any hook returns Block, return Block immediately
    // If any hook returns ModifyArgs, remember the new args
    // If all hooks return Continue (and no ModifyArgs), return Continue
    unimplemented!()
}
```

Three rules:

1. **`Block` short-circuits.** The moment any hook returns `Block`, the registry stops and returns that action immediately. Later hooks never see the event. This is the right behavior -- if a policy says "no bash," there is no point asking the logging hook for its opinion.

2. **`ModifyArgs` accumulates.** If multiple hooks return `ModifyArgs`, the last one wins. Each hook that modifies arguments overwrites the previous modification. This is simple but effective -- if you need more complex composition (merging argument objects), you can implement it in a single hook that encapsulates the logic.

3. **`Continue` is the default.** If no hook has an opinion, execution proceeds unchanged. An empty registry always returns `Continue`.

The sequential evaluation order means hook priority is determined by registration order. Hooks registered first run first. If you want a blocking hook to take precedence over a logging hook, register it first.

---

## Built-in hooks

The module provides three ready-made hooks. Each demonstrates a different pattern of hook usage.

### LoggingHook

```rust
pub struct LoggingHook {
    log: std::sync::Mutex<Vec<String>>,
}

impl LoggingHook {
    pub fn new() -> Self {
        Self {
            log: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn messages(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

#[async_trait]
impl Hook for LoggingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        // Format as "pre:{tool_name}", "post:{tool_name}", "agent:start", "agent:end"
        unimplemented!()
    }
}
```

The simplest possible hook: record a short description of every event, never interfere. It always returns `Continue`, meaning it never blocks or modifies anything. The `Mutex<Vec<String>>` allows interior mutability -- the `on_event` method takes `&self` (not `&mut self`), so we need a lock to push into the vector.

In the starter, the `LoggingHook` records string descriptions rather than cloned events. The format is compact: `"pre:bash"`, `"post:write"`, `"agent:start"`, `"agent:end"`. This makes test assertions simpler -- you compare strings rather than matching enum variants.

The `LoggingHook` is invaluable for testing. You can construct a registry with a `LoggingHook`, fire some events, and then inspect what was recorded. This is exactly what the tests do.

### BlockingHook

```rust
pub struct BlockingHook {
    blocked_tools: Vec<String>,
    reason: String,
}

impl BlockingHook {
    pub fn new(blocked_tools: Vec<String>, reason: impl Into<String>) -> Self {
        Self {
            blocked_tools,
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl Hook for BlockingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        if let HookEvent::PreToolCall { tool_name, .. } = event {
            if self.blocked_tools.iter().any(|b| b == tool_name) {
                return HookAction::Block(self.reason.clone());
            }
        }
        HookAction::Continue
    }
}
```

A policy hook: it takes a list of tool names and blocks any `PreToolCall` event that matches. Everything else -- `PostToolCall`, `AgentStart`, `AgentEnd`, and pre-tool events for tools not on the list -- passes through as `Continue`.

The pattern match is deliberate. The hook only inspects `PreToolCall` events. On a `PostToolCall` for a blocked tool, it does nothing -- the tool has already run and blocking would be meaningless. This is the asymmetry from the event model table above, enforced in code.

You could use `BlockingHook` to implement workspace-level policies. For example, a read-only project might block `write`, `edit`, and `bash`:

```rust
let hook = BlockingHook::new(
    vec!["write".into(), "edit".into(), "bash".into()],
    "this workspace is read-only",
);
```

The LLM would see the block reason in the tool result and switch to read-only tools for the rest of the session.

### ShellHook

```rust
pub struct ShellHook {
    command: String,
    tool_pattern: Option<glob::Pattern>,
}

impl ShellHook {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            tool_pattern: None,
        }
    }

    pub fn for_tool(mut self, pattern: &str) -> Self {
        self.tool_pattern = glob::Pattern::new(pattern).ok();
        self
    }

    fn matches_tool(&self, tool_name: &str) -> bool {
        match &self.tool_pattern {
            Some(pattern) => pattern.matches(tool_name),
            None => true,
        }
    }
}
```

The `ShellHook` bridges the gap between Rust code and external commands. Instead of implementing policy in Rust, it delegates to a shell command. The command signals its decision through its exit code.

The `for_tool` builder method restricts which tools the hook fires for, using a glob pattern. Without it, the hook fires for all tools. `ShellHook::new("cargo fmt --check").for_tool("write")` only fires when the write tool is called.

The `on_event` implementation handles `PreToolCall` and `PostToolCall` events:

```rust
#[async_trait]
impl Hook for ShellHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        // Only handle PreToolCall and PostToolCall events
        // Check matches_tool() first
        // Run: tokio::process::Command::new("sh").arg("-c").arg(&self.command).output()
        // Exit code 0 -> Continue, non-zero -> Block with stderr
        unimplemented!()
    }
}
```

The execution flow:

1. **Extract tool name.** Only `PreToolCall` and `PostToolCall` events are handled. `AgentStart` and `AgentEnd` return `Continue` immediately.

2. **Check the tool pattern.** If a `tool_pattern` is set and does not match the tool name, return `Continue`.

3. **Run the command.** Uses `tokio::process::Command` to spawn `sh -c <command>`.

4. **Interpret the exit code.** A non-zero exit means "block this call." The stderr is captured and included in the block reason. A zero exit means `Continue`.

Here is a concrete example. Run a linter after every file edit:

```rust
let hook = ShellHook::new("cargo fmt --check")
    .for_tool("write");
```

---

## How Claude Code does it

Claude Code's hook system shares the same event-driven architecture but is configured declaratively through `settings.json` rather than Rust code.

In Claude Code, hooks are defined as JSON objects with matchers and commands:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "bash",
        "command": "/path/to/check-bash-command.sh"
      }
    ],
    "PostToolUse": [
      {
        "matcher": "*",
        "command": "echo 'Tool $TOOL_NAME completed'"
      }
    ]
  }
}
```

The matcher field supports glob patterns against tool names. The command field is a shell command that receives context through environment variables -- the same pattern as our `ShellHook`. Non-zero exits on pre-tool hooks block the call. Claude Code's hooks can also modify tool arguments by writing JSON to stdout, which the agent parses and applies.

Our trait-based approach provides the same extensibility through a different mechanism. Instead of JSON configuration, hooks are Rust types that implement the `Hook` trait. This gives us compile-time type safety and the ability to write hooks with complex logic (the `BlockingHook` matches against a list of tool names; the `LoggingHook` records structured events). The trade-off is that adding a new hook requires writing Rust code rather than editing a config file.

The `ShellHook` bridges this gap -- it delegates to external commands just like Claude Code's JSON-configured hooks do. A production agent would likely combine both approaches: built-in hooks for core policies (implemented in Rust) and shell hooks for user-defined customization (configured at runtime).

---

## Tests

Run the hook system tests:

```bash
cargo test -p mini-claw-code-starter test_ch20
```

Note: The hook tests are in `test_ch20`, following the V1 chapter numbering
where hooks were Chapter 20.

The tests verify each hook type and the registry's dispatch behavior.

---

## Recap

This chapter added an event-driven hook system that lets external code observe, modify, and block agent behavior at runtime:

- **`HookEvent`** defines four lifecycle points: `AgentStart`, `PreToolCall`, `PostToolCall`, and `AgentEnd`. Each carries the context relevant to its point in the agent loop.

- **`HookAction`** defines three responses: `Continue` (proceed normally), `Block` (cancel the tool call with a reason), and `ModifyArgs` (replace the tool arguments). The asymmetry between pre and post events is enforced in the hook implementations -- only pre-tool hooks can meaningfully block.

- **`HookRegistry`** dispatches events to hooks sequentially. `Block` short-circuits immediately. `ModifyArgs` accumulates (last writer wins). `Continue` is the default for an empty registry.

- **`LoggingHook`** records all events in a `Mutex<Vec<HookEvent>>` for debugging and testing. It never interferes with execution.

- **`BlockingHook`** blocks specific tools by name on `PreToolCall` events. It ignores everything else.

- **`ShellHook`** delegates to an external shell command via `tokio::process::Command`. Non-zero exits block the call. The `for_tool()` method restricts which tools trigger the command using `glob::Pattern`.

The hook system completes the safety and control layer. The permission engine (Chapter 10) enforces mode-based access rules. Safety checks (Chapter 11) catch dangerous patterns statically. Hooks (this chapter) provide the escape hatch for policies that are too specific or too dynamic to hardcode.

---

## What's next

Chapter 13 -- Plan Mode -- ties together everything from Part III. Plan mode is a restricted execution mode where only read-only tools run. The agent can read files, search code, and reason about a task, but it cannot write, edit, or execute commands. The permission engine checks tool categories. Safety checks validate arguments. Hooks fire for observation. But nothing destructive happens. It is the ultimate guardrail: the agent plans, the user reviews, and only then does execution begin.
