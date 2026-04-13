# Chapter 6: File Tools

A coding agent that cannot touch the filesystem is just a chatbot with delusions
of grandeur. It can describe code changes, suggest fixes, explain algorithms --
but it cannot do any of it. The tools you built in Chapter 3 gave your agent
hands. In this chapter you give those hands something to hold: files.

File operations are the most fundamental tools in any coding agent's toolkit.
Claude Code ships with Read, Write, and Edit tools (among many others), and
every competitor -- Cursor, Aider, OpenCode -- has its own version. The
operations are simple (read bytes, write bytes, search-and-replace), but the
design choices around them determine whether the agent can reliably modify a
codebase or whether it stumbles over its own edits. You will implement all three
tools in this chapter: `ReadTool`, `WriteTool`, and `EditTool`.

---

## 6.1 ReadTool

Reading a file is the simplest operation, but there are design decisions that
matter. A naive approach would dump the raw file contents and call it done. Our
`ReadTool` does two things differently: it numbers every line, and it supports
partial reads via offset and limit.

### Why line numbers?

When the LLM reads a file, it needs to reference specific locations for later
edits. "Replace the string on line 42" is precise. "Replace the string
somewhere around the middle of the function" is not. By formatting output like
`cat -n` (tab-separated line numbers), we give the model an unambiguous
coordinate system for the file. This becomes critical in the Edit tool, where
the model needs to provide an exact string match -- line numbers help it locate
and copy the right chunk.

### The full implementation

Create `src/tools/read.rs`:

```rust
use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct ReadTool {
    def: ToolDefinition,
}

impl ReadTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("read", "Read the contents of a file")
                .param("path", "string", "Absolute path to the file", true)
                .param("offset", "integer", "Line number to start reading from (1-based)", false)
                .param("limit", "integer", "Maximum number of lines to read", false),
        }
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read '{path}': {e}"))?;

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let start = offset.saturating_sub(1).min(total);

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(total);

        let end = (start + limit).min(total);
        let selected = &lines[start..end];

        let numbered: Vec<String> = selected
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}\t{}", start + i + 1, line))
            .collect();

        Ok(ToolResult::text(numbered.join("\n")))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Reading file...".into())
    }
}
```

### Walking through the code

**The definition.** Three parameters: `path` (required), `offset` (optional),
and `limit` (optional). The LLM sees these as a JSON Schema and knows it must
provide `path` but can omit the others. The parameter types are `"string"` for
the path and `"integer"` for the numeric parameters.

**Reading the file.** We use `tokio::fs::read_to_string` for async file I/O.
If the file does not exist or cannot be read, we return an `Err` -- this is one
of the few cases where we use `Err` rather than `ToolResult::error`, because a
missing file in this context signals a genuine argument error (the LLM provided
a bad path), not a recoverable tool-level issue. The query engine will convert
this to a `ToolResult::error` before the model sees it, so the agent loop still
continues.

**Line slicing.** The `offset` parameter is 1-based (matching `cat -n`
convention and how humans think about line numbers). We convert to 0-based with
`saturating_sub(1)` and clamp to `total` so out-of-range offsets do not panic.
When `offset` is not provided, it defaults to `1` (the first line). When
`limit` is not provided, it defaults to the total number of lines -- read
everything.

**Formatting.** Each line is prefixed with its 1-based line number and a tab
character: `"42\tlet x = 5;"`. This matches the format of `cat -n`, which is
well-represented in the model's training data. The tab separator is important --
it cleanly separates the number from the content even when lines start with
digits.

**Safety flags.** `is_read_only: true` tells the permission system this tool
never modifies anything. `is_concurrent_safe: true` tells the query engine it
is safe to run multiple reads in parallel -- there is no shared mutable state.

**No `summary()` override.** The default `summary()` from the `Tool` trait
checks for common argument names (`command`, `path`, `question`, `pattern`) and
formats them as `[name: detail]`. Since `ReadTool` uses `path`, the default
produces `[read: /path/to/file]` -- exactly what we want. Only override
`summary()` when your tool's primary argument is not one of the standard names.

**`activity_description`** returns `"Reading file..."` for the TUI spinner.

### What the output looks like

Given a file with three lines:

```
alpha
beta
gamma
```

The tool returns:

```
1	alpha
2	beta
3	gamma
```

With `offset: 2, limit: 1`:

```
2	beta
```

The line numbers in the output always reflect the actual position in the file,
not the position in the sliced result. This is essential -- when the LLM sees
`2\tbeta`, it knows that `beta` is on line 2 of the file, not "the first line
of what I requested."

---

## 6.2 WriteTool

Writing a file is conceptually simple: take a path and content, write the
content to the path. But there is one practical detail that makes a big
difference: creating parent directories automatically.

When the LLM writes `src/handlers/auth/middleware.rs`, the `src/handlers/auth/`
directory might not exist yet. A naive tool would fail with "No such file or
directory." The agent would then need to call `bash("mkdir -p ...")` and retry.
This wastes a tool-use round and confuses the model. Better to handle it
silently.

### The full implementation

Create `src/tools/write.rs`:

```rust
use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct WriteTool {
    def: ToolDefinition,
}

impl WriteTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("write", "Write content to a file, creating directories as needed")
                .param("path", "string", "Absolute path to write to", true)
                .param("content", "string", "Content to write", true),
        }
    }
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

        // Create parent directories
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to create directories for '{path}': {e}"))?;
            }
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write '{path}': {e}"))?;

        let bytes = content.len();
        Ok(ToolResult::text(format!("wrote {bytes} bytes to {path}")))
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Writing file...".into())
    }
}
```

### Walking through the code

**Two required parameters.** Both `path` and `content` are required. There is no
optional behavior here -- you always need both.

**Auto-creating directories.** The `create_dir_all` call is the key design
choice. It mirrors `mkdir -p` -- if the directory already exists, it is a no-op.
If intermediate directories are missing, it creates them all. The guard
`!parent.as_os_str().is_empty()` handles the edge case where the path has no
parent component (e.g., a bare filename like `"file.txt"`), where calling
`create_dir_all("")` would fail.

**Overwrite semantics.** `tokio::fs::write` overwrites the file if it already
exists and creates it if it does not. There is no append mode, no conflict
detection. This is deliberate -- the tool is a clean write, not a merge. If the
LLM wants to modify an existing file, it should use the Edit tool.

**Byte count confirmation.** The result reports `"wrote 42 bytes to /path/to/file"`.
This gives the model confirmation that the write succeeded and how much data was
written. It is a small detail that helps the model verify its own work.

**Not destructive.** `WriteTool` uses all the default safety flags: not
read-only, not concurrent-safe, not destructive. This might seem wrong for
a tool that overwrites files, but in practice any file the agent writes
is either new (no data loss) or already tracked by git (recoverable with
`git checkout`). Claude Code makes the same classification. Truly destructive
operations are things like `rm -rf` or database drops -- irreversible even with
version control.

---

## 6.3 EditTool

The Edit tool is the most interesting of the three, and it teaches the most
important design lesson in this book: **errors are values, not exceptions**.

The Edit tool performs a search-and-replace on a file. It takes a path, an
`old_string` to find, and a `new_string` to replace it with. The critical
constraint: `old_string` must appear exactly once in the file. Zero matches
means the model got the string wrong. More than one match means the replacement
is ambiguous -- we do not know which occurrence to change.

Both of these are expected failure modes, not bugs. The model frequently gets
strings slightly wrong (missing whitespace, wrong indentation, stale content
from a previous edit). The tool must report these failures clearly so the model
can correct itself.

### The full implementation

Create `src/tools/edit.rs`:

```rust
use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct EditTool {
    def: ToolDefinition,
}

impl EditTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new(
                "edit",
                "Replace an exact string in a file. The old_string must appear exactly once.",
            )
            .param("path", "string", "Absolute path to the file to edit", true)
            .param("old_string", "string", "The exact string to find", true)
            .param("new_string", "string", "The replacement string", true),
        }
    }
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
        let old = args["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'old_string' argument"))?;
        let new = args["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'new_string' argument"))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read '{path}': {e}"))?;

        let count = content.matches(old).count();
        if count == 0 {
            return Ok(ToolResult::error(format!(
                "old_string not found in '{path}'"
            )));
        }
        if count > 1 {
            return Ok(ToolResult::error(format!(
                "old_string appears {count} times in '{path}', must be unique"
            )));
        }

        let updated = content.replacen(old, new, 1);
        tokio::fs::write(path, &updated)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write '{path}': {e}"))?;

        Ok(ToolResult::text(format!("edited {path}")))
    }

    fn validate_input(&self, args: &Value) -> ValidationResult {
        if args.get("old_string").and_then(|v| v.as_str()).is_none() {
            return ValidationResult::Error {
                message: "missing 'old_string' argument".into(),
                code: 400,
            };
        }
        if args.get("new_string").and_then(|v| v.as_str()).is_none() {
            return ValidationResult::Error {
                message: "missing 'new_string' argument".into(),
                code: 400,
            };
        }
        ValidationResult::Ok
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Editing file...".into())
    }
}
```

### Walking through the code

**Three required parameters.** `path`, `old_string`, and `new_string` are all
required. The model must specify exactly what to find and what to replace it
with. There is no regex, no line-number-based editing, no diff format. Just
plain string replacement. This simplicity is a feature -- it is unambiguous and
easy for the model to use correctly.

**The uniqueness check.** This is the heart of the tool:

```rust
let count = content.matches(old).count();
if count == 0 {
    return Ok(ToolResult::error(format!(
        "old_string not found in '{path}'"
    )));
}
if count > 1 {
    return Ok(ToolResult::error(format!(
        "old_string appears {count} times in '{path}', must be unique"
    )));
}
```

Two branches, both returning `Ok(ToolResult::error(...))`. Not `Err(...)`. This
is the most important pattern in the entire tool system. Let me explain why.

### Errors are values: the key design lesson

When the model asks to edit a file and the old string is not found, what should
happen? There are three possible designs:

1. **Panic** -- crash the agent. Obviously wrong.
2. **Return `Err(...)`** -- propagate an error up the call stack.
3. **Return `Ok(ToolResult::error(...))`** -- tell the model what went wrong.

Option 2 seems reasonable, but look at what happens in the query engine from
Chapter 4. The `execute_tools` method calls `t.call(...)`:

```rust
match t.call(call.arguments.clone()).await {
    Ok(mut r) => { /* truncate and use */ }
    Err(e) => ToolResult::error(e.to_string()),
}
```

An `Err` from `call()` gets converted to `ToolResult::error(...)` anyway. So
both paths end up in the same place. But option 3 is better for two reasons:

First, it makes the tool's intent clear. A not-found error is a **normal
outcome**, not an exceptional condition. The file exists, the tool ran, the
string just was not there. Returning `Ok` signals "I executed successfully;
here is what I found (which is an error condition)." Returning `Err` signals
"something went wrong during execution" -- which is misleading.

Second, it gives the tool control over the error message. `ToolResult::error`
produces a message prefixed with `"error: "`. The model sees
`"error: old_string not found in 'foo.rs'"` and knows to try a different
string. If we returned `Err(anyhow!(...))`, the message would go through
`e.to_string()` and might lose formatting or context.

This pattern applies throughout the codebase. Reserve `Err` for genuinely
unrecoverable situations: I/O failures reading the file, serialization bugs,
permissions errors at the OS level. Tool-level "this did not work" is always
`Ok(ToolResult::error(...))`.

### Input validation

The Edit tool is the first tool that overrides `validate_input`:

```rust
fn validate_input(&self, args: &Value) -> ValidationResult {
    if args.get("old_string").and_then(|v| v.as_str()).is_none() {
        return ValidationResult::Error {
            message: "missing 'old_string' argument".into(),
            code: 400,
        };
    }
    if args.get("new_string").and_then(|v| v.as_str()).is_none() {
        return ValidationResult::Error {
            message: "missing 'new_string' argument".into(),
            code: 400,
        };
    }
    ValidationResult::Ok
}
```

Why validate here when `call()` also checks for these fields? Because
`validate_input` runs **before** `call`, in the query engine's `execute_tools`
method. If validation fails, `call()` is never invoked. This matters when the
tool has side effects -- you do not want to read the file, start processing, and
then discover a required argument is missing.

For the Read and Write tools, the `call()` method handles missing arguments
with `ok_or_else` and the default `validate_input` (which always returns `Ok`)
is fine. But for Edit, where the operation is more complex and the error modes
are richer, explicit validation catches the simplest failures early.

The `code: 400` is a convention borrowed from HTTP status codes. 400 means "bad
request" -- the caller (in this case, the LLM) sent invalid input. The
permission engine can use this code to distinguish "bad input" from "permission
denied" (which might use 403).

---

## 6.4 Integration: Write, Edit, Read

The real power of these tools comes from combining them. A typical agent
workflow looks like this:

1. **Write** a new file
2. **Edit** to fix a bug or refine the code
3. **Read** to verify the result

Here is what that looks like as tool calls:

```
Agent: I'll create the handler file.
-> write(path: "/tmp/project/handler.rs", content: "fn main() { println!(\"hello\"); }")
<- "wrote 35 bytes to /tmp/project/handler.rs"

Agent: Let me update the greeting.
-> edit(path: "/tmp/project/handler.rs", old_string: "hello", new_string: "goodbye")
<- "edited /tmp/project/handler.rs"

Agent: Let me verify the change.
-> read(path: "/tmp/project/handler.rs")
<- "1	fn main() { println!(\"goodbye\"); }"
```

Each tool does one thing and communicates its result clearly. The agent sees
the output of each step and decides what to do next. If the edit had failed
(wrong string), the agent would see the error and retry with the correct string.

This write-edit-read pattern is how Claude Code modifies files in practice. It
does not generate a complete file and overwrite -- that would lose any content
outside the modified section. Instead, it uses surgical edits on the specific
lines that need to change, then reads the result to confirm. This is more
reliable and produces smaller diffs.

---

## 6.5 How Claude Code does it

Claude Code's file tools follow the same protocol but with more sophistication:

**Read** supports images and PDFs. It detects binary files and renders them
appropriately (base64-encoded images are sent as multimodal content blocks).
It has smarter truncation with token counting rather than character counting,
and it warns when a file is empty.

**Write** checks for protected files. Claude Code maintains a list of files
that should never be overwritten (`.env`, `credentials.json`, etc.) and blocks
writes to them. It also integrates with the permission system to require user
approval before overwriting existing files in certain modes.

**Edit** is considerably more powerful. It supports multiple edits in a single
call, has a diff preview mode, handles encoding detection, and validates that
the edit produces syntactically valid code (for supported languages). It also
has a more nuanced uniqueness check that considers context lines around the
match to disambiguate.

But the core protocol is identical to what you just built. A struct holds the
definition. The `Tool` trait provides the interface. The `call` method does
the work. The agent loop dispatches and collects results. Understanding our
three simple tools gives you the foundation to understand Claude Code's full
tool suite.

---

## 6.6 Tool file organization

All three tools live in `src/tools/`, alongside the other tools you will build
in later chapters. The module structure:

```
src/tools/
  mod.rs    -- re-exports all tools
  read.rs   -- ReadTool
  write.rs  -- WriteTool
  edit.rs   -- EditTool
  bash.rs   -- (Chapter 7)
  glob.rs   -- (Chapter 8)
  grep.rs   -- (Chapter 8)
```

The `mod.rs` barrel re-exports everything:

```rust
mod edit;
mod read;
mod write;

pub use edit::EditTool;
pub use read::ReadTool;
pub use write::WriteTool;

// Re-export from types for convenience
pub use crate::types::{Tool, ToolDefinition, ToolResult, ToolSet, ValidationResult};
```

This lets consumers write `use crate::tools::{ReadTool, WriteTool, EditTool}`
without reaching into individual modules.

---

## 6.7 Tests

Run the chapter 6 tests:

```bash
cargo test -p claw-code test_ch6
```

Here is what each test verifies:

### ReadTool tests

- **`test_ch6_read_file`** -- Reads a three-line file and verifies all lines appear in the output.
- **`test_ch6_read_with_line_numbers`** -- Reads a file and checks that the output contains tab-separated line numbers (`1\t`, `2\t`, `3\t`).
- **`test_ch6_read_with_offset_and_limit`** -- Reads lines 2-3 of a five-line file using `offset: 2, limit: 2`. Verifies the correct lines are included and others are excluded.
- **`test_ch6_read_nonexistent`** -- Attempts to read a file that does not exist. Verifies that the result is an `Err` (not a `ToolResult::error`), because a missing file is an I/O failure.
- **`test_ch6_read_is_read_only`** -- Checks the safety flags: `is_read_only` and `is_concurrent_safe` are `true`, `is_destructive` is `false`.

### WriteTool tests

- **`test_ch6_write_file`** -- Writes "hello world" to a new file, verifies the result contains "wrote", and reads back the file to confirm the content.
- **`test_ch6_write_creates_directories`** -- Writes to `a/b/c/deep.txt` inside a temp directory. All intermediate directories are created automatically.
- **`test_ch6_write_overwrites`** -- Writes to a file that already has content. Verifies the old content is replaced.

### EditTool tests

- **`test_ch6_edit_replace`** -- Edits "world" to "rust" in a file containing "hello world". Verifies the result says "edited" and the file now reads "hello rust".
- **`test_ch6_edit_not_found`** -- Attempts to replace a string that does not exist. Verifies the result starts with `"error:"` and contains `"not found"`. Critically, this is an `Ok` result, not an `Err`.
- **`test_ch6_edit_ambiguous`** -- Attempts to replace "aa" in a file containing "aa bb aa" (two occurrences). Verifies the error mentions "2 times".
- **`test_ch6_edit_validation`** -- Tests `validate_input` directly. Missing `old_string` returns `ValidationResult::Error`. Providing all three fields returns `ValidationResult::Ok`.

### Integration tests

- **`test_ch6_write_then_read`** -- Writes a two-line file, then reads it back. Verifies the round-trip preserves content.
- **`test_ch6_write_edit_read`** -- The full workflow: writes a file containing `println!("hello")`, edits "hello" to "goodbye", reads it back, and verifies "goodbye" is present and "hello" is gone.

### Definition and summary tests

- **`test_ch6_tool_definitions`** -- Checks that each tool's definition returns the correct name: "read", "write", "edit".
- **`test_ch6_tool_summaries`** -- Checks that `summary` produces the expected format: `[read: foo.rs]`, `[write: bar.rs]`, `[edit: baz.rs]`.

---

## Recap

Three tools, one pattern. Every tool in this chapter follows the same structure:

1. **A struct** with a `def: ToolDefinition` field.
2. **A `new()` constructor** that builds the definition with the parameter builder from Chapter 1.
3. **A `Tool` impl** with `definition()`, `call()`, and optional overrides for safety flags, validation, summary, and activity description.

The pattern scales. When you add Bash in Chapter 7 and Glob/Grep in Chapter 8,
the shape is identical -- only the `call()` logic changes. This is the power of
the `Tool` trait: a uniform interface that makes every tool interchangeable from
the query engine's perspective.

The key lessons from this chapter:

- **Line numbers matter.** The `ReadTool` formats output with tab-separated
  line numbers so the LLM has an unambiguous coordinate system for edits.
- **Automate the obvious.** The `WriteTool` creates parent directories
  automatically, saving the agent a wasted tool-use round.
- **Errors are values.** The `EditTool` returns `Ok(ToolResult::error(...))`
  for not-found and ambiguous matches. The agent loop continues. The model
  adapts. Reserve `Err` for I/O failures and programming errors.
- **Validate early.** The `EditTool` uses `validate_input` to catch missing
  arguments before `call()` runs, preventing wasted work.

In [Chapter 7: Bash Tool](./ch07-bash-tool.md), you will build the most
powerful (and most dangerous) tool in the agent's arsenal -- one that can run
arbitrary shell commands.
