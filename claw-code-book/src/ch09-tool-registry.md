# Chapter 9: Tool Registry

You have six tools. You have a query engine. This chapter wires them together.

Over the past three chapters you built the individual tools that let your agent interact with the world -- file reading and writing (Chapter 6), command execution (Chapter 7), and pattern search (Chapter 8). Each tool implements the `Tool` trait, has a JSON schema, and returns structured results. But they exist in isolation. The agent has no way to discover them, expose their schemas to the LLM, or dispatch calls by name.

The tool registry is the bridge. It holds every available tool in a single `ToolSet`, exposes their schemas to the LLM so it knows what it can call, and dispatches incoming tool calls to the correct implementation by name. By the end of this chapter, you will have a fully functional coding agent that can read, write, edit, search, and execute commands -- the complete tool loop from Chapter 4, now with real tools instead of test doubles.

```bash
cargo test -p claw-code test_ch9
```

---

## The module layout

All tool implementations live under `src/tools/`, one file per tool:

```
src/tools/
  mod.rs       -- re-exports everything
  bash.rs      -- BashTool
  edit.rs      -- EditTool
  glob.rs      -- GlobTool
  grep.rs      -- GrepTool
  read.rs      -- ReadTool
  write.rs     -- WriteTool
```

The `mod.rs` is a flat barrel file:

```rust
mod bash;
mod edit;
mod glob;
mod grep;
mod read;
mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use write::WriteTool;

// Re-export from types
pub use crate::types::{Tool, ToolDefinition, ToolResult, ToolSet, ValidationResult};
```

Every tool is a separate file with a single public struct. The `mod.rs` re-exports the structs and, for convenience, re-exports the core tool types from `crate::types`. This means downstream code can write `use crate::tools::*` and get both the concrete tools and the trait/type machinery in one import.

The flat structure is deliberate. There is no `tools/file/mod.rs` grouping `ReadTool`, `WriteTool`, and `EditTool` together, and no `tools/search/mod.rs` for `GlobTool` and `GrepTool`. Why? Because tools are always referenced individually -- you register `ReadTool::new()`, not `FileTools::all()`. A flat module keeps the import paths short and the mental model simple. When you have 6 tools this is obviously fine. Claude Code has 40+ tools and still uses a similar flat layout -- each tool is its own module with a single export.

---

## Building a ToolSet

The `ToolSet` you defined in Chapter 1 is a `HashMap<String, Box<dyn Tool>>` with a builder API. Now we use it for real. Here is a helper function that assembles the standard tool set:

```rust
fn default_tools() -> ToolSet {
    ToolSet::new()
        .with(ReadTool::new())
        .with(WriteTool::new())
        .with(EditTool::new())
        .with(BashTool::new())
        .with(GlobTool::new())
        .with(GrepTool::new())
}
```

Six calls to `.with()`, one per tool. Each call constructs the tool, extracts its name from the `ToolDefinition`, and inserts it into the internal `HashMap`. The builder pattern means the order does not matter -- the tools are keyed by name, not position.

After construction, the `ToolSet` supports the operations the agent needs:

```rust
let tools = default_tools();

// How many tools are registered?
assert_eq!(tools.len(), 6);

// Look up a tool by name (returns Option<&dyn Tool>)
let read = tools.get("read").unwrap();

// Get all schemas for the LLM
let defs: Vec<&ToolDefinition> = tools.definitions();
assert_eq!(defs.len(), 6);

// List all registered tool names
let names: Vec<&str> = tools.names();
```

The `definitions()` method is what the query engine calls at the start of each loop iteration to tell the LLM which tools are available. Every definition includes the tool's name, description, and JSON Schema for its parameters. The LLM uses this information to decide when and how to call each tool.

The `get()` method is what the engine calls during tool dispatch -- the LLM says `"name": "read"`, the engine does `tools.get("read")`, and calls the returned tool's `.call()` method with the provided arguments.

---

## Tool categories

Not all tools are created equal. The `Tool` trait defines three boolean flags -- `is_read_only()`, `is_concurrent_safe()`, and `is_destructive()` -- that classify each tool's behavior. These flags are not decorative. They drive the permission engine (Chapter 10), plan mode (Chapter 13), and concurrent execution decisions.

Here is how our six tools classify themselves:

### Read-only tools: ReadTool, GlobTool, GrepTool

```rust
let read = ReadTool::new();
assert!(read.is_read_only());       // true
assert!(read.is_concurrent_safe()); // true
assert!(!read.is_destructive());    // false (default)
```

These tools observe the filesystem without changing it. Reading a file, listing paths by glob pattern, and searching content with regex -- none of these have side effects. They are safe to run in parallel (multiple reads cannot race) and they are safe to run in plan mode (the agent can gather information without modifying anything).

All three override `is_read_only()` and `is_concurrent_safe()` to return `true`. They leave `is_destructive()` at its default `false`.

### Write tools: WriteTool, EditTool

```rust
let write = WriteTool::new();
assert!(!write.is_read_only());       // false (default)
assert!(!write.is_concurrent_safe()); // false (default)
assert!(!write.is_destructive());     // false -- explicitly set
```

Write and Edit modify files, so they are not read-only. They are not concurrent-safe because two writes to the same file would race. But they are not marked destructive either -- file writes are recoverable (you can revert with git or rewrite the file). The `WriteTool` explicitly overrides `is_destructive()` to return `false`, making this classification visible rather than relying on the default.

`EditTool` uses all defaults: not read-only, not concurrent-safe, not destructive. It modifies files (not read-only), cannot safely run in parallel (two edits to the same file would conflict), and is recoverable (not destructive).

### Destructive tools: BashTool

```rust
let bash = BashTool::new();
assert!(!bash.is_read_only());       // false (default)
assert!(!bash.is_concurrent_safe()); // false (default)
assert!(bash.is_destructive());      // true
```

The BashTool is the only destructive tool. It can run arbitrary shell commands -- `rm -rf /`, `git push --force`, `curl | sh`. There is no way to know at schema time whether a given bash command is safe, so the tool conservatively marks itself as destructive. The permission engine uses this flag to require explicit user approval before execution, even in auto-approve mode.

### Why these categories matter

The categories compose into a permission hierarchy:

| Category | Plan mode | Auto-approve | Default mode |
|----------|-----------|--------------|--------------|
| Read-only | Allowed | Allowed | Allowed |
| Write | Denied | Allowed | Ask user |
| Destructive | Denied | Ask user | Ask user |

In plan mode (Chapter 13), only read-only tools execute. The agent can read files, search code, and list directories, but it cannot write, edit, or run commands. This lets the agent reason about a task without performing it.

In auto-approve mode, read-only and write tools execute without prompting. But destructive tools still require user confirmation -- the `is_destructive()` flag overrides auto-approve for safety.

In default mode, read-only tools execute freely. Everything else asks the user. This is the behavior you see when Claude Code prompts "Allow bash: rm -rf target?" before running a command.

You will implement this logic in Chapters 10-13. For now, the tools self-report their categories, and the information is available to any consumer that needs it.

---

## Wiring tools to the QueryEngine

The `QueryEngine` from Chapter 4 accepts tools through its builder API. You can add tools one at a time:

```rust
let engine = QueryEngine::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new())
    .tool(EditTool::new())
    .tool(BashTool::new())
    .tool(GlobTool::new())
    .tool(GrepTool::new());
```

Or pass a pre-built `ToolSet`:

```rust
let engine = QueryEngine::new(provider)
    .tools(default_tools());
```

Both approaches produce the same result. The `.tool()` method calls `self.tools.push(t)` internally, which extracts the tool's name from its definition and inserts it into the `HashMap`. The `.tools()` method replaces the entire `ToolSet` at once.

Once constructed, the engine handles the full dispatch pipeline. When the LLM responds with `StopReason::ToolUse` and a list of `ToolCall`s, the engine:

1. Looks up each tool by name in the `ToolSet`
2. Validates the input with `validate_input()`
3. Executes the tool with `call()`
4. Truncates the result if it exceeds `max_result_chars`
5. Packages the result as a `ToolResultMessage` and appends it to the conversation

If the LLM requests a tool that does not exist in the registry, the engine returns `ToolResult::error("unknown tool \`foo\`")` -- the errors-as-values pattern from Chapter 3. The model sees the error and can adjust.

---

## Integration: write, read, respond

The `test_ch9_engine_with_file_tools` test demonstrates a complete three-turn interaction with real tools. Let's trace through it step by step.

The setup creates a temp directory and scripts a `MockProvider` with three responses:

```rust
let dir = tempfile::tempdir().unwrap();
let path = dir.path().join("test.txt");
let path_str = path.to_str().unwrap().to_string();

let provider = MockProvider::new(VecDeque::from([
    // Turn 1: write a file
    AssistantMessage {
        id: "1".into(),
        text: None,
        tool_calls: vec![ToolCall {
            id: "c1".into(),
            name: "write".into(),
            arguments: json!({
                "path": path_str,
                "content": "hello from agent"
            }),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    },
    // Turn 2: read it back
    AssistantMessage {
        id: "2".into(),
        text: None,
        tool_calls: vec![ToolCall {
            id: "c2".into(),
            name: "read".into(),
            arguments: json!({ "path": path_str }),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    },
    // Turn 3: final answer
    AssistantMessage {
        id: "3".into(),
        text: Some("Done! I wrote and read the file.".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    },
]));
```

The engine is built with only the tools it needs:

```rust
let engine = QueryEngine::new(provider)
    .tool(ReadTool::new())
    .tool(WriteTool::new());
```

Now trace the loop:

**Turn 1 -- Write.** The engine calls `provider.chat()`, gets back `StopReason::ToolUse` with a `write` tool call. It looks up `"write"` in the `ToolSet`, finds `WriteTool`, calls it with `{"path": "/tmp/.../test.txt", "content": "hello from agent"}`. The `WriteTool` creates the file on disk. The engine pushes the assistant message and the tool result into the conversation history.

Message history after turn 1:
```
[User]         "write and read a file"
[Assistant]    tool_calls: [write(path, content)]
[ToolResult]   "Wrote 16 bytes to /tmp/.../test.txt"
```

**Turn 2 -- Read.** The engine calls `provider.chat()` again with the updated history. The mock returns a `read` tool call. The engine looks up `"read"`, calls `ReadTool` with `{"path": "/tmp/.../test.txt"}`. The `ReadTool` reads the file that `WriteTool` created in the previous turn and returns its content.

Message history after turn 2:
```
[User]         "write and read a file"
[Assistant]    tool_calls: [write(path, content)]
[ToolResult]   "Wrote 16 bytes to /tmp/.../test.txt"
[Assistant]    tool_calls: [read(path)]
[ToolResult]   "1\thello from agent\n"
```

**Turn 3 -- Final answer.** The engine calls `provider.chat()` one more time. The mock returns `StopReason::Stop` with text. The engine pushes the final assistant message and returns the text to the caller.

The test verifies two things: the returned text contains "Done!", and the file actually exists on disk with the expected content. This confirms that real tools executed with real side effects inside the engine loop.

```rust
let result = engine.run("write and read a file").await.unwrap();
assert!(result.contains("Done!"));
assert_eq!(
    std::fs::read_to_string(&path).unwrap(),
    "hello from agent"
);
```

---

## Error recovery: the hallucinated tool

The `test_ch9_engine_unknown_tool_recovery` test demonstrates what happens when the LLM requests a tool that does not exist. This is not a hypothetical scenario -- models regularly hallucinate tool names, especially smaller models or when the tool list is long.

The mock provider scripts two responses:

```rust
let provider = MockProvider::new(VecDeque::from([
    // LLM hallucinates a tool
    AssistantMessage {
        id: "1".into(),
        text: None,
        tool_calls: vec![ToolCall {
            id: "c1".into(),
            name: "imaginary_tool".into(),
            arguments: json!({}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: None,
    },
    // LLM recovers after seeing the error
    AssistantMessage {
        id: "2".into(),
        text: Some("Sorry, that tool doesn't exist.".into()),
        tool_calls: vec![],
        stop_reason: StopReason::Stop,
        usage: None,
    },
]));

let engine = QueryEngine::new(provider).tool(ReadTool::new());
let result = engine.run("do something").await.unwrap();
assert!(result.contains("doesn't exist"));
```

Here is what happens:

**Turn 1.** The LLM asks to call `"imaginary_tool"`. The engine does `tools.get("imaginary_tool")`, gets `None`, and returns `ToolResult::error("unknown tool \`imaginary_tool\`")`. This error message is pushed into the conversation as a `ToolResultMessage`. The loop continues.

**Turn 2.** The LLM sees the error in the conversation history and produces a text response acknowledging the mistake. The engine returns normally.

The agent did not crash. It did not panic. It did not return an `Err`. It treated the unknown tool as a tool-level error -- the errors-as-values pattern from Chapter 3 -- and let the model recover. This is the correct behavior for a production agent. Models make mistakes. The agent should be resilient to them.

The same pattern handles other failure modes: a tool that rejects its input via `validate_input()`, a tool that returns an execution error, or a tool whose output exceeds the truncation limit. In every case, the model sees a descriptive error message and can adjust its approach.

---

## How Claude Code does it

Claude Code's tool registry is substantially larger, but the architecture is the same.

**Scale.** Claude Code registers 40+ tools spanning file operations, git, browser, notebooks, MCP (Model Context Protocol), and more. Each tool has permission metadata, cost hints, and rich terminal rendering. Our six tools cover the essential capabilities -- the same protocol, less surface area.

**Dynamic registration.** Our `ToolSet` is built at startup and never changes. Claude Code's registry is dynamic -- MCP tools are discovered and registered at runtime when a user configures an MCP server. A tool can appear or disappear mid-session. The `ToolSet::push()` method you built in Chapter 1 supports this pattern, though we do not exercise it yet.

**Tool groups.** Claude Code organizes tools into permission groups. File tools, git tools, and shell tools each have group-level allow/deny rules. Our flat `ToolSet` with per-tool flags is simpler but achieves the same effect -- the permission engine (Chapter 10) will check `is_read_only()` and `is_destructive()` on each tool individually.

**Usage statistics.** Claude Code tracks how often each tool is called, how long each call takes, and how many tokens each result consumes. This data feeds into the TUI's status display and helps with cost estimation. We track token usage at the session level (Chapter 17) but not per-tool.

Despite these differences, the core protocol is identical. The LLM sees a list of tool schemas. It decides to call one. The agent looks up the tool by name, executes it, and feeds the result back. Everything else -- permissions, groups, statistics, dynamic registration -- is orchestration around that lookup.

---

## Tests

Run all chapter 9 tests:

```bash
cargo test -p claw-code test_ch9
```

Here is what each test covers:

- **`test_ch9_registry_all_tools`** -- Builds a `ToolSet` with all 6 tools. Verifies `len()` is 6 and each tool is retrievable by name. Also checks that looking up a nonexistent name returns `None`.

- **`test_ch9_registry_definitions`** -- Builds a `ToolSet` with 3 tools. Calls `definitions()` and verifies all 3 schemas are present with correct names.

- **`test_ch9_registry_names`** -- Builds a `ToolSet` with `GlobTool` and `GrepTool`. Calls `names()`, sorts the result, and verifies the expected names.

- **`test_ch9_read_only_tools`** -- Builds a `ToolSet` with 5 tools. Filters by `is_read_only()`. Verifies that `read`, `glob`, and `grep` are read-only, while `write` and `bash` are not.

- **`test_ch9_destructive_tools`** -- Checks the `is_destructive()` flag on individual tools. `BashTool` is destructive. `WriteTool` and `ReadTool` are not.

- **`test_ch9_engine_with_file_tools`** -- The full integration test described above. Three-turn interaction: write a file, read it back, return a final answer. Verifies both the engine output and the file on disk.

- **`test_ch9_engine_with_bash`** -- Two-turn interaction. The LLM calls `bash("echo hello-from-bash")`, the engine executes it, and the final answer references the output.

- **`test_ch9_engine_unknown_tool_recovery`** -- The error recovery test described above. The LLM hallucinates a tool, gets an error, and recovers gracefully.

- **`test_ch9_default_toolset_builder`** -- Defines a `default_tools()` helper function, builds the tool set, and verifies that all 6 tools have non-empty names and descriptions.

---

## Recap

Part II is complete. Over four chapters you built every tool a basic coding agent needs:

- **ReadTool** reads files with line numbers, offsets, and limits.
- **WriteTool** creates and overwrites files, creating parent directories as needed.
- **EditTool** performs surgical string replacements within existing files.
- **BashTool** executes shell commands with timeout support and exit code reporting.
- **GlobTool** finds files by pattern matching across the directory tree.
- **GrepTool** searches file contents with regex and context lines.

In this chapter you wired them all together through the `ToolSet` registry and connected them to the `QueryEngine`. The engine can now receive a user prompt, send it to the LLM with all six tool schemas, execute whatever tools the model requests, and loop until the model produces a final answer. You have a working coding agent.

But a working agent is not a safe agent. Right now, the engine executes every tool call the LLM requests without question. If the model decides to `bash("rm -rf /")`, the engine runs it. If it writes over your source files with garbage, the engine writes. There are no guardrails, no confirmation prompts, no safety checks. The tool flags (`is_read_only`, `is_destructive`) exist but nothing enforces them.

---

## What's next

Part III -- Safety & Control -- adds the guardrails that turn a working agent into a trustworthy one:

- **Chapter 10: Permission Engine** -- The system that checks every tool call before execution. It evaluates permission rules, respects the permission mode, and asks the user when needed.
- **Chapter 11: Safety Checks** -- Static analysis of tool arguments. Catches dangerous patterns (`rm -rf`, `git push --force`) before the permission prompt even appears.
- **Chapter 12: Hook System** -- Pre-tool and post-tool hooks that run shell commands around tool execution. Lets users enforce custom policies (run linters after edits, block certain paths).
- **Chapter 13: Plan Mode** -- A restricted execution mode where only read-only tools run. The agent can analyze and plan but never modify. This is where `is_read_only()` finally gets enforced.

The tools you built in Part II are the hands. Part III teaches the agent when to use them -- and when not to.
