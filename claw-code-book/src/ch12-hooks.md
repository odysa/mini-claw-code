# Chapter 12: Hook System

The permission engine from Chapter 10 decides whether a tool call runs. The safety checks from Chapter 11 catch dangerous patterns before the user even sees a prompt. But both systems are baked into the agent -- they enforce rules that you, the developer, chose at compile time. What about the user?

Users have policies that the agent author cannot anticipate. A team might require that every bash command is logged to an audit file. A project might enforce that file writes only touch a specific directory. A CI pipeline might need to run a linter after every edit. These are not safety checks in the "prevent `rm -rf /`" sense -- they are workflow hooks that extend the agent's behavior at runtime.

This chapter builds the hook system. Hooks are event-driven: they fire at key lifecycle points (before a tool call, after a tool call, when the agent starts, when it ends) and they can observe, modify, or block execution. The trait-based design means anyone can implement a hook -- a logging hook for debugging, a blocking hook for policy enforcement, a shell hook that delegates decisions to external commands.

```bash
cargo test -p claw-code test_ch12
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

Open `src/hooks/mod.rs`. The module defines three types: `HookEvent`, `HookAction`, and the `Hook` trait.

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

The enum derives `Clone` because the `HookRunner` passes events by shared reference (`&HookEvent`) to each hook in sequence. Hooks that need to store events (like the `LoggingHook`) clone them. Hooks that only inspect events (like the `BlockingHook`) borrow without cloning.

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

## The HookRunner

Individual hooks are useful, but the real value is composing them. The `HookRunner` holds a list of hooks and evaluates them sequentially for each event.

```rust
pub struct HookRunner {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRunner {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn with(mut self, hook: impl Hook + 'static) -> Self {
        self.hooks.push(Box::new(hook));
        self
    }

    pub fn push(&mut self, hook: impl Hook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }
}
```

The builder API should look familiar -- it mirrors `ToolSet` from Chapter 1. The `with()` method takes ownership and returns `self` for chaining. The `push()` method takes `&mut self` for imperative code. Both accept `impl Hook + 'static`, boxing the concrete type into a trait object.

### The run method

The interesting part is how actions compose:

```rust
pub async fn run(&self, event: &HookEvent) -> HookAction {
    let mut final_action = HookAction::Continue;

    for hook in &self.hooks {
        let action = hook.on_event(event).await;
        match action {
            HookAction::Block(_) => return action,
            HookAction::ModifyArgs(_) => final_action = action,
            HookAction::Continue => {}
        }
    }

    final_action
}
```

Three rules:

1. **`Block` short-circuits.** The moment any hook returns `Block`, the runner stops and returns that action immediately. Later hooks never see the event. This is the right behavior -- if a policy says "no bash," there is no point asking the logging hook for its opinion.

2. **`ModifyArgs` accumulates.** If multiple hooks return `ModifyArgs`, the last one wins. Each hook that modifies arguments overwrites the previous modification. This is simple but effective -- if you need more complex composition (merging argument objects), you can implement it in a single hook that encapsulates the logic.

3. **`Continue` is the default.** If no hook has an opinion, execution proceeds unchanged. An empty runner always returns `Continue`.

The sequential evaluation order means hook priority is determined by insertion order. Hooks added first run first. If you want a blocking hook to take precedence over a logging hook, add it first.

---

## Built-in hooks

The module provides three ready-made hooks. Each demonstrates a different pattern of hook usage.

### LoggingHook

```rust
pub struct LoggingHook {
    events: Mutex<Vec<HookEvent>>,
}

impl LoggingHook {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<HookEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn event_count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[async_trait]
impl Hook for LoggingHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        self.events.lock().unwrap().push(event.clone());
        HookAction::Continue
    }
}
```

The simplest possible hook: record every event, never interfere. It always returns `Continue`, meaning it never blocks or modifies anything. The `Mutex<Vec<HookEvent>>` allows interior mutability -- the `on_event` method takes `&self` (not `&mut self`), so we need a lock to push into the vector.

Why `Mutex` and not `RwLock`? Because every access is a write. The `events()` and `event_count()` methods read, but they also hold the lock briefly -- the contention is negligible for a debugging tool.

The `LoggingHook` is invaluable for testing. You can construct a runner with a `LoggingHook`, fire some events, and then inspect what was recorded. This is exactly what the tests do.

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
    event_filter: Vec<String>,
    timeout: std::time::Duration,
}

impl ShellHook {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            event_filter: Vec::new(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn on_events(mut self, events: Vec<String>) -> Self {
        self.event_filter = events;
        self
    }

    fn should_fire(&self, event_name: &str) -> bool {
        self.event_filter.is_empty() || self.event_filter.iter().any(|e| e == event_name)
    }
}
```

The `ShellHook` bridges the gap between Rust code and external commands. Instead of implementing policy in Rust, it delegates to a shell command. The command receives context through environment variables and signals its decision through its exit code.

The `timeout` field defaults to 30 seconds. A shell command that takes longer is killed and treated as a failure -- this prevents a hung hook from stalling the agent indefinitely. The `timeout()` builder method overrides the default.

The `on_events` builder method restricts which events the hook fires for. Without it, the hook fires for all tool events. With it, you can say "only fire for `pre_tool_call`" or "only fire for `post_tool_call`". The `should_fire` method implements this: an empty filter means "fire for everything"; a non-empty filter means "fire only for listed event names".

Here is the `on_event` implementation:

```rust
#[async_trait]
impl Hook for ShellHook {
    async fn on_event(&self, event: &HookEvent) -> HookAction {
        let (event_name, tool_name) = match event {
            HookEvent::PreToolCall { tool_name, .. } => ("pre_tool_call", tool_name.as_str()),
            HookEvent::PostToolCall { tool_name, .. } => ("post_tool_call", tool_name.as_str()),
            HookEvent::AgentStart { .. } => ("agent_start", ""),
            HookEvent::AgentEnd { .. } => ("agent_end", ""),
        };

        if !self.should_fire(event_name) {
            return HookAction::Continue;
        }

        let fut = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .env("HOOK_TOOL_NAME", tool_name)
            .env("HOOK_EVENT", event_name)
            .output();

        match tokio::time::timeout(self.timeout, fut).await {
            Ok(Ok(output)) => {
                if !output.status.success() && event_name == "pre_tool_call" {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    HookAction::Block(format!(
                        "hook command failed: {}",
                        stderr.trim()
                    ))
                } else {
                    HookAction::Continue
                }
            }
            Ok(Err(e)) => HookAction::Block(format!("hook command error: {}", e)),
            Err(_) => HookAction::Block(format!(
                "hook command timed out after {:?}",
                self.timeout
            )),
        }
    }
}
```

The execution flow:

1. **Map event to names.** Each event variant maps to a string name (`"pre_tool_call"`, `"post_tool_call"`, `"agent_start"`, `"agent_end"`) and extracts the tool name if applicable.

2. **Check the filter.** If the event name is not in the filter, return `Continue` immediately.

3. **Run the command with a timeout.** Uses `tokio::process::Command` to spawn `sh -c <command>` with two environment variables: `HOOK_TOOL_NAME` (the tool being called) and `HOOK_EVENT` (which lifecycle point). The command is wrapped in `tokio::time::timeout(self.timeout, ...)` -- if the command takes longer than the configured timeout (default 30 seconds), it is killed and treated as a failure.

4. **Interpret the exit code.** A non-zero exit on a `pre_tool_call` event means "block this call." The stderr is captured and included in the block reason, so the hook author can provide a human-readable explanation. A non-zero exit on any other event (including `post_tool_call`) is ignored -- the tool already ran, so blocking would be pointless. A zero exit always means `Continue`.

5. **Handle errors.** If the command itself fails to spawn (binary not found, permission denied) or times out, the hook blocks with an error message. This is conservative -- if the hook system cannot evaluate the policy, it refuses to proceed.

Here is a concrete example. Suppose you want to block any bash command that touches production databases:

```rust
let hook = ShellHook::new(r#"
    echo "$HOOK_TOOL_NAME" | grep -q "bash" && exit 1 || exit 0
"#).on_events(vec!["pre_tool_call".into()]);
```

Or run a linter after every file edit:

```rust
let hook = ShellHook::new("cargo fmt --check")
    .on_events(vec!["post_tool_call".into()]);
```

The post-tool linter hook will run after every tool call. If `cargo fmt --check` fails, the failure is silently ignored (post-tool hooks do not block). But you could pair it with a logging hook to record the linter output for review.

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

Run all chapter 12 tests:

```bash
cargo test -p claw-code test_ch12
```

There are 17 tests organized into five groups.

### HookAction basics

- **`test_ch12_hook_action_continue`** -- Constructs `HookAction::Continue` and verifies it equals itself. Confirms the `PartialEq` derive works for the unit variant.

- **`test_ch12_hook_action_block`** -- Constructs `HookAction::Block("reason")` and verifies equality with an identical value. Confirms the tuple variant carries its payload correctly.

- **`test_ch12_hook_action_modify_args`** -- Constructs `HookAction::ModifyArgs(json!({"path": "/new/path"}))` and verifies equality. Confirms `serde_json::Value` comparison works inside the enum.

### LoggingHook

- **`test_ch12_logging_hook_records_events`** -- Creates a `LoggingHook`, verifies it starts empty, fires one `PreToolCall` event, verifies the action is `Continue`, checks that `event_count()` is 1, and inspects the recorded event to confirm it is a `PreToolCall` for `"bash"`.

- **`test_ch12_logging_hook_multiple_events`** -- Fires all four event types (`AgentStart`, `PreToolCall`, `PostToolCall`, `AgentEnd`) through a single `LoggingHook`. Verifies the event count is 4.

### BlockingHook

- **`test_ch12_blocking_hook_blocks_tool`** -- Creates a `BlockingHook` that blocks `"bash"`. Fires a `PreToolCall` for bash. Verifies the action is `Block("bash is blocked")`.

- **`test_ch12_blocking_hook_allows_other_tools`** -- Same hook, but fires a `PreToolCall` for `"read"`. Verifies the action is `Continue` -- the hook only blocks tools on its list.

- **`test_ch12_blocking_hook_ignores_post_events`** -- Fires a `PostToolCall` for `"bash"` through the same hook. Verifies `Continue` -- the blocking hook only cares about pre-tool events.

### HookRunner

- **`test_ch12_runner_empty`** -- Creates an empty runner. Verifies `is_empty()` is true. Fires a `PreToolCall` event. Verifies the result is `Continue` -- no hooks means no interference.

- **`test_ch12_runner_logging_continues`** -- Builds a runner with one `LoggingHook`. Verifies `len()` is 1. Fires a `PreToolCall`. Verifies the result is `Continue` -- a logging hook never blocks.

- **`test_ch12_runner_block_short_circuits`** -- Builds a runner with a `BlockingHook` first and a `LoggingHook` second. Fires a `PreToolCall` for bash. Verifies the result is `Block`. The `BlockingHook` fires first and short-circuits -- the `LoggingHook` never sees the event.

- **`test_ch12_runner_multiple_hooks`** -- Builds a runner with two `LoggingHook` instances. Verifies `len()` is 2. Fires a `PreToolCall`. Verifies the result is `Continue` -- two passive hooks compose to passive behavior.

### ShellHook

- **`test_ch12_shell_hook_success`** -- Creates a `ShellHook` with the command `"true"` (always exits 0). Fires a `PreToolCall`. Verifies `Continue`.

- **`test_ch12_shell_hook_failure_blocks`** -- Creates a `ShellHook` with `"false"` (always exits 1). Fires a `PreToolCall`. Verifies the result is `Block(...)`.

- **`test_ch12_shell_hook_post_failure_continues`** -- Same `"false"` command, but fires a `PostToolCall`. Verifies `Continue` -- post-tool failures do not block because the tool already ran.

- **`test_ch12_shell_hook_event_filter`** -- Creates a `ShellHook` with `on_events(vec!["pre_tool_call"])`. Fires a `PreToolCall` (matches filter, fires, returns `Continue`). Fires an `AgentStart` (does not match filter, returns `Continue` without running the command).

- **`test_ch12_shell_hook_receives_env_vars`** -- Creates a `ShellHook` whose command checks that `$HOOK_TOOL_NAME` equals `"bash"` and `$HOOK_EVENT` equals `"pre_tool_call"`. If either variable is wrong, the command exits non-zero and the test fails. Verifies `Continue`, confirming the environment variables are set correctly.

---

## Recap

This chapter added an event-driven hook system that lets external code observe, modify, and block agent behavior at runtime:

- **`HookEvent`** defines four lifecycle points: `AgentStart`, `PreToolCall`, `PostToolCall`, and `AgentEnd`. Each carries the context relevant to its point in the agent loop.

- **`HookAction`** defines three responses: `Continue` (proceed normally), `Block` (cancel the tool call with a reason), and `ModifyArgs` (replace the tool arguments). The asymmetry between pre and post events is enforced in the hook implementations -- only pre-tool hooks can meaningfully block.

- **`HookRunner`** evaluates hooks sequentially. `Block` short-circuits immediately. `ModifyArgs` accumulates (last writer wins). `Continue` is the default for an empty runner.

- **`LoggingHook`** records all events in a `Mutex<Vec<HookEvent>>` for debugging and testing. It never interferes with execution.

- **`BlockingHook`** blocks specific tools by name on `PreToolCall` events. It ignores everything else.

- **`ShellHook`** delegates to an external shell command, passing `HOOK_TOOL_NAME` and `HOOK_EVENT` as environment variables. Non-zero exits on pre-tool events block the call. Post-tool failures are ignored. An event filter restricts which lifecycle points trigger the command.

The hook system completes the safety and control layer. The permission engine (Chapter 10) enforces mode-based access rules. Safety checks (Chapter 11) catch dangerous patterns statically. Hooks (this chapter) provide the escape hatch for policies that are too specific or too dynamic to hardcode.

---

## What's next

Chapter 13 -- Plan Mode -- ties together everything from Part III. Plan mode is a restricted execution mode where only read-only tools run. The agent can read files, search code, and reason about a task, but it cannot write, edit, or execute commands. The permission engine checks tool categories. Safety checks validate arguments. Hooks fire for observation. But nothing destructive happens. It is the ultimate guardrail: the agent plans, the user reviews, and only then does execution begin.
