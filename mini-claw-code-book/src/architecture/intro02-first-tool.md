# Your First Tool Call

> **File to edit:** `src/tools/read.rs`
> **Test to run:** `cargo test -p mini-claw-code-starter test_ch2`

An LLM can't read files, run commands, or browse the web. It can only generate text. But it can *ask your code* to do those things. That's what tools are.

## How tool calling works

```
1. You send the LLM a prompt + a list of available tools (JSON schemas)
2. The LLM responds with StopReason::ToolUse and a list of tool calls
3. Your code executes each tool call
4. You send the results back to the LLM
5. The LLM generates a final answer using those results
```

The LLM never touches the filesystem. It describes what it wants (`{"name": "read", "arguments": {"path": "foo.txt"}}`), and your code does it.

## The Tool trait

Every tool implements two methods:

```rust
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    async fn call(&self, args: Value) -> anyhow::Result<String>;
}
```

- **`definition()`** returns the JSON schema that tells the LLM what this tool does and what arguments it takes
- **`call()`** executes the tool with the given arguments and returns a string result

## Your task: ReadTool

Open `src/tools/read.rs`. You'll implement a tool that reads files.

### Step 1: The definition

A `ToolDefinition` describes the tool to the LLM using JSON Schema:

```rust
pub fn new() -> Self {
    Self {
        definition: ToolDefinition::new("read", "Read the contents of a file.")
            .param("path", "string", "Absolute path to the file", true),
    }
}
```

The `.param()` builder adds a parameter with its type, description, and whether it's required. When the LLM sees this schema, it knows it can call a tool named `"read"` with a required string argument `"path"`.

### Step 2: The implementation

Extract the path from the JSON arguments, read the file, return the contents:

```rust
async fn call(&self, args: Value) -> anyhow::Result<String> {
    let path = args["path"]
        .as_str()
        .context("missing 'path' argument")?;

    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read '{path}'"))
}
```

Three lines of logic. `args` is a `serde_json::Value` — the parsed JSON arguments from the LLM. The `context()` and `with_context()` methods (from `anyhow`) add human-readable error messages.

## Run the tests

```bash
cargo test -p mini-claw-code-starter test_ch2
```

15 tests verify your tool:
- `test_ch2_read_definition` — schema has the right name and required params
- `test_ch2_read_file` — reads a real file from a temp directory
- `test_ch2_read_missing_file` — returns an error for nonexistent files
- `test_ch2_read_missing_arg` — returns an error when `path` is missing

## The pattern

Every tool in this project follows the same three-step pattern:

1. **Define** — `ToolDefinition::new("name", "description").param(...)`
2. **Extract** — pull arguments from the JSON `Value`
3. **Execute** — do the thing, return a `String`

You'll repeat this pattern for `WriteTool`, `EditTool`, and `BashTool` in later chapters. Once you've written one tool, you've written them all.

## What just happened

You taught the LLM a new capability. By itself, the LLM can only generate text. With your `ReadTool`, it can now read any file on disk. The tool is the bridge between "the LLM wants to read a file" and "the file is actually read."

---

**Next:** [The Agentic Loop →](./intro03-agentic-loop.md)
