# Chapter 13: Plan Mode

Your agent can now read files, write code, run shell commands, and do all of it
under a permission system with safety checks and hooks. There is one problem:
it does everything at once. The model reads a file, immediately rewrites it,
runs the tests, and keeps going -- all in a single uninterrupted loop. If the
model misunderstands the task, it has already modified your codebase before you
had a chance to say "wait, that is not what I meant."

Plan mode fixes this by splitting the agent loop into two phases. First, the
agent analyzes the task using only read-only tools -- reading files, searching
code, listing directories. It produces a plan. Then, the caller (you, or your
UI) inspects the plan, approves it, and the agent executes with all tools
available. Think before you act. It is advice that works for humans and agents
alike.

This pattern is not hypothetical. Claude Code ships with a plan mode that
restricts the agent to read-only operations until the user explicitly approves
the plan. Every serious coding agent has some version of this -- a way to let
the model reason about a task before committing to changes. The `is_read_only()`
flag you set on tools back in Chapter 9 has been waiting for exactly this moment.

```bash
cargo test -p claw-code test_ch13
```

---

## Why a separate engine?

You could implement plan mode as a flag on `QueryEngine` -- add a `plan_mode:
bool` field, check it in `execute_tools`, filter definitions accordingly. That
works but tangles two concerns. The `QueryEngine` is the general-purpose agent
loop. Plan mode is a higher-level workflow with distinct phases, transitions,
and a virtual tool that does not exist in the tool set. Mixing them muddies both.

The `PlanEngine` is a separate struct that wraps the same building blocks --
a provider, a `ToolSet`, a `QueryConfig` -- but orchestrates them differently.
Two methods, `plan()` and `execute()`, implement the two phases. The caller
controls the transition between them. This keeps the `QueryEngine` simple and
gives the `PlanEngine` full control over its workflow.

Claude Code takes a similar approach. Its plan mode sets `PermissionMode::Plan`,
which the permission engine enforces (only read-only tools pass). The UI shows
a "Plan Mode" banner and the agent's plan before asking for approval. Our
`PlanEngine` encapsulates the same two-phase pattern with caller-driven approval.

---

## The PlanEngine struct

```rust
use std::collections::HashSet;

use tokio::sync::mpsc;

use crate::engine::{QueryConfig, QueryEvent};
use crate::provider::Provider;
use crate::types::*;

pub struct PlanEngine<P: Provider> {
    provider: P,
    tools: ToolSet,
    config: QueryConfig,
    /// Tool names allowed during planning. Overrides `is_read_only()`.
    plan_tools: HashSet<String>,
    /// System prompt injected during planning phase.
    plan_prompt: Option<String>,
    /// The exit_plan tool definition.
    exit_plan_def: ToolDefinition,
}
```

Six fields, each with a clear role:

- **`provider`** -- The LLM backend, same as `QueryEngine`.
- **`tools`** -- The full tool set. During planning, only a subset is exposed.
  During execution, all tools are available.
- **`config`** -- The `QueryConfig` from Chapter 4. Max turns and result
  truncation apply to both phases.
- **`plan_tools`** -- An explicit override for which tools are allowed during
  planning. If empty, the engine falls back to checking `is_read_only()` on each
  tool. If populated, only the listed tools are allowed, regardless of their
  read-only flag.
- **`plan_prompt`** -- An optional custom system prompt injected at the start of
  the plan phase. If `None`, a default prompt is used.
- **`exit_plan_def`** -- The `ToolDefinition` for the virtual `exit_plan` tool.
  This tool is injected into the plan phase's tool list but does not exist in
  the `ToolSet`. It is a signal, not a real tool.

### The builder

The builder follows the same `new()` + chaining pattern as `QueryEngine`.
The familiar methods -- `config()`, `tool()`, `tools()` -- work identically.
The `new()` constructor creates the `exit_plan_def` with a description that
tells the model what it does. This definition has no parameters -- the model
just calls it to signal "I am done planning." The definition lives on the
struct because it is injected into every `plan()` call but never registered
in the `ToolSet`.

```rust
let engine = PlanEngine::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .plan_tool_names(&["read"])
    .plan_prompt("You are a security auditor.");
```

Two builder methods are specific to `PlanEngine`:

- **`plan_tool_names(&[&str])`** -- Overrides the default read-only filtering.
  If you call `.plan_tool_names(&["bash", "read"])`, only `bash` and `read` are
  available during planning, even if other tools are read-only. This is useful
  for specialized workflows where you want the agent to run commands (like
  `git log` or `cargo test --dry-run`) during analysis.

- **`plan_prompt(impl Into<String>)`** -- Replaces the default planning system
  prompt. The default says "You are in PLANNING mode. Analyze the task using
  read-only tools." A custom prompt can focus the agent on a specific concern:
  security auditing, performance analysis, migration planning.

---

## The two phases

The core of `PlanEngine` is two methods: `plan()` and `execute()`. They share
the same loop structure as `QueryEngine::chat()`, but with different tool sets
and different termination conditions.

```
User prompt
    |
    v
+-------------+       +------------------+       +--------------+
| plan()      |------>| Caller inspects  |------>| execute()    |
| read-only   |       | plan, approves   |       | all tools    |
| + exit_plan |       | or revises       |       |              |
+-------------+       +------------------+       +--------------+
    |                                                  |
    v                                                  v
  Plan text                                        Final result
```

The caller drives the transition. After `plan()` returns, the caller can:
1. Show the plan to the user
2. Push a `Message::user("Approved. Go ahead.")` into the message history
3. Call `execute()` with the same message vec

Or the caller can reject the plan, push feedback, and call `plan()` again.
The `PlanEngine` does not care -- it has no built-in UI, no approval dialog.
It is a workflow engine, not a user interface.

---

## Phase 1: plan()

The planning phase runs a restricted agent loop. Only read-only tools and the
virtual `exit_plan` tool are available. Here is the full implementation:

```rust
pub async fn plan(&self, messages: &mut Vec<Message>) -> anyhow::Result<String> {
    // Inject planning system prompt if not already present
    self.maybe_inject_plan_prompt(messages);

    let plan_defs = self.plan_definitions();
    let mut turns = 0;

    loop {
        if turns >= self.config.max_turns {
            anyhow::bail!("exceeded max turns ({}) during planning", self.config.max_turns);
        }

        let turn = self.provider.chat(messages, &plan_defs).await?;

        match turn.stop_reason {
            StopReason::Stop => {
                let text = turn.text.clone().unwrap_or_default();
                messages.push(Message::Assistant(turn));
                return Ok(text);
            }
            StopReason::ToolUse => {
                // Check for exit_plan
                if turn.tool_calls.iter().any(|c| c.name == "exit_plan") {
                    let text = turn.text.clone().unwrap_or_default();
                    messages.push(Message::Assistant(turn));
                    // Push a tool result for exit_plan
                    let exit_call = messages
                        .iter()
                        .rev()
                        .find_map(|m| {
                            if let Message::Assistant(a) = m {
                                a.tool_calls.iter().find(|c| c.name == "exit_plan")
                            } else {
                                None
                            }
                        })
                        .map(|c| c.id.clone())
                        .unwrap_or_default();
                    messages.push(Message::tool_result(exit_call, "Plan phase complete."));
                    return Ok(text);
                }

                // Execute allowed tools, block others
                let results = self.execute_plan_tools(&turn.tool_calls).await;
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

The structure mirrors `QueryEngine::chat()` from Chapter 4. Same loop, same
max-turns guard, same provider call, same stop-reason match. Three things are
different:

### 1. System prompt injection

Before entering the loop, `plan()` calls `maybe_inject_plan_prompt()`. This
inserts a tagged system message at position 0 of the message history, telling
the model it is in planning mode.

### 2. Filtered tool definitions

Instead of `self.tools.definitions()`, the plan phase uses
`self.plan_definitions()` -- a filtered list that only includes read-only tools
plus the `exit_plan` tool. The model cannot see write tools in its schema, so
it has no reason to call them.

### 3. The exit_plan escape hatch

When the model calls `exit_plan`, the plan phase ends immediately. The engine
pushes the assistant message and a synthetic tool result ("Plan phase complete.")
into the history, then returns. The synthetic result is necessary because the
API requires every tool call to have a corresponding result -- without it, the
next `provider.chat()` call would fail.

The plan phase can end in two ways:
- **`StopReason::Stop`** -- The model produces a text response directly. This
  is the implicit exit.
- **`exit_plan` tool call** -- The model explicitly signals it is done
  analyzing. This is the explicit exit.

Both return the plan text (which may be empty if the model put its plan in
tool calls rather than text).

---

## The exit_plan tool

The `exit_plan` tool deserves its own section because it is unusual. It is not
a real tool. It does not exist in the `ToolSet`. It has no `call()` method. It
is a `ToolDefinition` with a name and description, injected into the plan
phase's tool list so the model sees it as an option.

Why not just rely on `StopReason::Stop`? Because some models are reluctant to
stop when they see tools available. They will keep calling tools in a loop,
gathering more information, never committing to a plan. The `exit_plan` tool
gives the model an explicit action it can take to say "I have enough
information, here is my plan." It is a social contract expressed as a tool
schema.

When the model calls `exit_plan`, the engine detects it by name, pushes the
assistant message, finds the call's ID, and pushes a synthetic `ToolResult`
with "Plan phase complete." The synthetic result is important -- the message
protocol requires every `ToolCall` to have a matching `ToolResult`. Skip it
and the next API call fails with a malformed request.

---

## Phase 2: execute()

The execution phase is a standard agent loop with the full tool set. No
filtering, no virtual tools, no special termination. The implementation is
`QueryEngine::chat()` transplanted into a method -- same max-turns guard, same
provider call, same stop-reason match, same tool dispatch. The only difference
is that it uses `self.tools.definitions()` (all tools) instead of the filtered
plan definitions.

The key point: `execute()` receives the same `&mut Vec<Message>` that `plan()`
used. The message history from planning -- the system prompt, the user request,
the read-only tool calls, the plan text -- is all still there. The model enters
execution with full context of what it analyzed and what it decided to do. This
continuity is what makes the two-phase pattern effective. The model does not
start from scratch; it picks up where it left off.

Between `plan()` and `execute()`, the caller typically pushes a user message:

```rust
let plan = engine.plan(&mut messages).await?;
println!("Plan: {plan}");

// User approves
messages.push(Message::user("Approved. Go ahead."));

let result = engine.execute(&mut messages).await?;
```

This approval message becomes part of the context for execution. The model sees
it and knows it has permission to proceed with modifications.

---

## Defense in depth: tool filtering

The plan phase uses two layers of protection to prevent write operations:

### Layer 1: Definition filtering

The `plan_definitions()` method filters the tool schemas sent to the model. Only
tools that pass `is_plan_allowed()` are included, plus `exit_plan`:

```rust
fn plan_definitions(&self) -> Vec<&ToolDefinition> {
    let mut defs: Vec<&ToolDefinition> = self
        .tools
        .definitions()
        .into_iter()
        .filter(|d| self.is_plan_allowed(d.name))
        .collect();
    defs.push(&self.exit_plan_def);
    defs
}
```

If the model does not see a tool in its schema, it has no reason to call it.
This is the primary defense -- remove the temptation.

### Layer 2: Execution guard

Even if the model somehow requests a blocked tool (hallucination, prompt
injection, or a creative interpretation of the schema), the
`execute_plan_tools()` method catches it. For each tool call, three things
happen:

1. **`exit_plan` is skipped** -- It is handled by the caller (the `plan()`
   method itself), not by the execution pipeline. A `continue` ensures we
   do not try to look up a tool that does not exist in the `ToolSet`.

2. **Blocked tools return errors** -- If `is_plan_allowed()` returns false, the
   tool is not executed. Instead, a `ToolResult::error` is returned with a clear
   message: `` `write` is not available in planning mode ``. The model sees this
   error, understands the constraint, and adjusts.

3. **Allowed tools execute normally** -- Lookup, call, truncate. The same
   pipeline as `QueryEngine::execute_tools()`.

Both layers must fail for a write operation to slip through during planning.

### The is_plan_allowed() check

```rust
fn is_plan_allowed(&self, tool_name: &str) -> bool {
    if !self.plan_tools.is_empty() {
        return self.plan_tools.contains(tool_name);
    }
    // Default: check is_read_only on the tool
    self.tools
        .get(tool_name)
        .map(|t| t.is_read_only())
        .unwrap_or(false)
}
```

Two modes:

- **Default** -- If `plan_tools` is empty (no override), check the tool's
  `is_read_only()` flag. `ReadTool`, `GlobTool`, and `GrepTool` return `true`.
  `WriteTool`, `EditTool`, and `BashTool` return `false`. This is the safe
  default that works for most use cases.

- **Custom override** -- If `plan_tool_names()` was called on the builder, the
  `plan_tools` set is non-empty. Only tools in that set are allowed, regardless
  of their `is_read_only()` flag. This lets you allow `BashTool` during planning
  (for read-only commands like `git log`) or restrict normally read-only tools.

---

## System prompt injection

The plan phase injects a system message to tell the model it is in planning
mode. This is handled by `maybe_inject_plan_prompt()`:

```rust
fn maybe_inject_plan_prompt(&self, messages: &mut Vec<Message>) {
    let prompt = self.plan_prompt.as_deref().unwrap_or(
        "You are in PLANNING mode. Analyze the task using read-only tools. \
         Do NOT modify any files. When your analysis is complete, call exit_plan \
         or provide your plan as a text response.",
    );

    // Don't inject if already present
    let already_has = messages.iter().any(|m| {
        matches!(m, Message::System(s) if s.tag.as_deref() == Some("plan_mode"))
    });

    if !already_has {
        messages.insert(
            0,
            Message::System(SystemMessage {
                id: crate::types::new_id(),
                content: prompt.to_string(),
                tag: Some("plan_mode".into()),
            }),
        );
    }
}
```

Three design decisions here:

1. **Tagged message** -- The `tag: Some("plan_mode")` identifies the message
   for deduplication. If `plan()` is called twice (the user asks the agent to
   revise), the second call finds the existing tagged message and skips
   injection. Without the tag, you would get duplicate system prompts.

2. **Position 0** -- The planning prompt is inserted at the beginning of the
   message list, before any existing messages. System prompts at position 0
   have the strongest influence on model behavior.

3. **Custom or default** -- If `plan_prompt()` was called on the builder, that
   text is used. Otherwise, the default tells the model it is in planning mode,
   should use read-only tools, and should call `exit_plan` when done.

---

## The full plan-execute flow

Let's trace through a realistic scenario to see how everything fits together.
The user wants to copy a source file to a new location.

**Setup:**

```rust
let engine = PlanEngine::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new());

let mut messages = vec![Message::user("Copy src.txt to dst.txt")];
```

**Plan phase** -- `plan()` injects the planning system prompt, filters
definitions to `[read, exit_plan]` (write is excluded), and enters the loop.
The model calls `read(path="src.txt")`, sees the contents, and returns
"I'll copy src.txt to dst.txt."

**Approval** -- The caller prints the plan and pushes a user message:

```rust
println!("Plan: {}", plan);
messages.push(Message::user("Approved. Go ahead."));
```

**Execute phase** -- `execute()` exposes all tools. The model calls
`write(path="dst.txt", content="source content")`, the file is created on disk,
and the model returns "Done! Copied the file."

The message history at the end contains the complete trace: planning system
prompt, user request, read-only analysis, plan text, approval, write operation,
final confirmation. The model had full context at every step.

---

## Event streaming: plan_with_events()

Like `QueryEngine`, the `PlanEngine` has an event-streaming variant.
`plan_with_events()` takes an `mpsc::UnboundedSender<QueryEvent>` and emits
`ToolStart`, `ToolEnd`, `Done`, and `Error` events as the plan phase runs.
The signature mirrors `chat_with_events()` from Chapter 4 -- errors are sent
as `QueryEvent::Error` events rather than propagated, and the return type is
`Option<String>` instead of `Result<String>`.

A TUI would use this to show a spinner while the agent reads files during
planning, display the plan text as it streams, and prompt the user for approval
before calling `execute()`.

---

## How Claude Code does it

Claude Code's plan mode follows the same two-phase pattern but integrates more
deeply with the permission system.

| Feature | Our PlanEngine | Claude Code |
|---------|---------------|-------------|
| Tool filtering | `is_read_only()` or explicit set | `PermissionMode::Plan` flag |
| UI integration | Caller-driven (no built-in UI) | "Plan Mode" banner in TUI |
| Approval flow | Caller pushes user message | UI dialog with approve/reject |
| System prompt | Tagged `plan_mode` message | Mode-specific prompt section |
| Exit signal | `exit_plan` virtual tool | Mode transition in permission engine |
| Write blocking | Two layers (definitions + execution) | Permission engine rejects non-read-only |

The biggest difference is where the enforcement happens. In Claude Code, the
permission engine handles it -- plan mode is just another permission mode that
rejects non-read-only tool calls. The `QueryEngine` does not need to know about
plan mode at all. Our approach is simpler and self-contained: everything about
plan mode lives in one struct, at the cost of less flexibility for "semi-plan"
modes that allow some writes but not others.

---

## Tests

Run the chapter 13 tests:

```bash
cargo test -p claw-code test_ch13
```

Here is what each test covers:

**`test_ch13_plan_text_only`** -- The simplest plan scenario. The provider
returns `StopReason::Stop` with text. The plan phase should return that text
without executing any tools. Verifies the basic plan-as-text path.

**`test_ch13_plan_allows_read_only`** -- A `ReadTool` is registered. During
planning, the model reads a file and gets the contents back. The plan text
references what was read. Verifies that read-only tools work in plan mode.

**`test_ch13_plan_blocks_write_tools`** -- A `WriteTool` is registered, and the
model tries to call it during planning. The engine should return a
`ToolResult::error` containing "not available in planning mode". The model
recovers after seeing the error. Verifies the execution guard.

**`test_ch13_exit_plan_ends_planning`** -- The model calls `exit_plan`. The plan
phase should terminate immediately and return. Verifies the virtual tool
mechanism.

**`test_ch13_execute_allows_write`** -- During execution (not planning), the
model calls `WriteTool`. The file should be written to disk. Verifies that the
execution phase has no tool restrictions.

**`test_ch13_full_plan_execute_flow`** -- The complete workflow: read a file
during planning, produce a plan, receive approval, write during execution,
produce a final answer. Verifies the end-to-end two-phase pattern with real
file I/O.

**`test_ch13_message_continuity`** -- Calls `plan()` then `execute()` on the
same message vec. Verifies that messages accumulate across phases -- the
execution phase sees everything from the planning phase.

**`test_ch13_custom_plan_tools`** -- Uses `plan_tool_names(&["bash"])` to
override the default read-only filter. During planning, `BashTool` is allowed
and executes successfully. Verifies the custom override mechanism.

**`test_ch13_system_prompt_injected`** -- After calling `plan()`, the message
history should contain a `Message::System` with `tag == Some("plan_mode")`.
Verifies system prompt injection.

**`test_ch13_system_prompt_not_duplicated`** -- Calls `plan()` twice on the same
message vec (simulating a plan revision). The message history should contain
exactly one `plan_mode` system message, not two. Verifies the deduplication
logic.

**`test_ch13_custom_plan_prompt`** -- Uses `plan_prompt("You are a security
auditor.")`. After calling `plan()`, the `plan_mode` system message should
contain the custom text, not the default. Verifies the prompt override.

**`test_ch13_provider_error_in_plan`** -- An empty `MockProvider` with no
responses. The `plan()` call should return `Err`. Verifies that provider errors
propagate correctly from the plan phase.

**`test_ch13_provider_error_in_execute`** -- Same as above, but for `execute()`.
Verifies that provider errors propagate from the execution phase.

---

## Recap

Plan mode completes Part III -- Safety & Control. Over four chapters you built
the layers that turn a reckless agent into a disciplined one:

- **Chapter 10: Permission Engine** -- Checks every tool call against permission
  rules before execution. Ask, allow, or deny based on the tool and the mode.
- **Chapter 11: Safety Checks** -- Static analysis of tool arguments. Catches
  dangerous patterns before the permission prompt appears.
- **Chapter 12: Hook System** -- Pre-tool and post-tool hooks for custom
  policies. Run linters after edits, block certain paths, enforce project rules.
- **Chapter 13: Plan Mode** -- A two-phase workflow that separates analysis from
  action. The agent reads and reasons first, then modifies only after approval.

The key architectural insight is **caller-driven approval**. The `PlanEngine`
does not prompt the user, display a dialog, or make assumptions about the UI.
It runs the plan, returns the text, and waits. The caller decides what to do
next. This separation of concerns -- engine logic vs. user interaction -- is
what makes the same `PlanEngine` work in a CLI, a TUI, a web interface, or a
test harness.

---

## What's next

Part III gave your agent safety and control. Part IV -- Configuration & Context
-- builds the systems that make your agent project-aware:

- **Chapter 14: Settings Hierarchy** -- Layered configuration from global
  defaults to project-specific overrides.
- **Chapter 15: Project Instructions** -- Loading and assembling CLAUDE.md files
  that tell the agent how to work with this specific codebase.
- **Chapter 16: Memory System** -- Persistent memory that survives across
  sessions.
- **Chapter 17: Token & Cost Tracking** -- Monitoring how much context and money
  each interaction consumes.

The safety infrastructure you built in Part III protects the agent from doing
harm. The configuration infrastructure in Part IV teaches it to do good.
