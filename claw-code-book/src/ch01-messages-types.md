# Chapter 1: Messages & Types

Every coding agent is, at its core, a loop over a conversation. The user speaks, the model replies, tools produce results, and those results go back to the model. Before we can build that loop, we need a type system that represents every participant and every kind of payload in the conversation.

In this chapter you will implement the foundational types that the rest of the codebase depends on. By the end, `cargo test -p claw-code-starter test_ch1` should pass.

## Why a rich message type?

If you look at a raw LLM API (OpenAI, Anthropic), messages are JSON blobs with a `role` field: `"system"`, `"user"`, or `"assistant"`. That is fine for a one-shot chatbot, but a coding agent needs more:

- **System instructions** that can be tagged (e.g., `"compact_boundary"`) so the compaction engine knows where to slice.
- **Tool results** that carry the ID of the tool call they answer, so the model can correlate request and response.
- **Attachments** for injected context like CLAUDE.md files or images, distinct from user-typed messages.
- **Progress updates** for long-running tools (e.g., a bash command streaming output) that appear in the TUI but are never sent to the API.

Claude Code models all of these as variants of a single `Message` enum, tagged with a `type` field for serialization. We will do the same.

## File layout

All types live under `src/types/`, split into four files:

```
src/types/
  mod.rs          -- re-exports everything
  message.rs      -- Message enum, constructors, new_id()
  tool.rs         -- ToolDefinition, ToolCall, ToolResult, Tool trait, ToolSet
  permission.rs   -- Permission, PermissionMode, PermissionBehavior
  usage.rs        -- TokenUsage, ModelUsage
```

The `mod.rs` is a simple barrel:

```rust
mod message;
mod permission;
mod tool;
mod usage;

pub use message::*;
pub use permission::*;
pub use tool::*;
pub use usage::*;
```

You will work on one file at a time.

---

## 1.1 MessageId and `new_id()`

Every message in the conversation needs a unique identifier. Session persistence serializes the conversation to JSONL, and the compaction engine references messages by ID when deciding what to summarize. A UUID v4 string is a simple, collision-free choice.

```rust
pub type MessageId = String;

pub fn new_id() -> MessageId {
    uuid::Uuid::new_v4().to_string()
}
```

This uses the `uuid` crate with the `v4` feature (already in Cargo.toml). The type alias keeps things readable -- every `id` field across all message structs is a `MessageId`.

## 1.2 The Message enum

Here is the full enum with its six variants:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    System(SystemMessage),
    User(UserMessage),
    Assistant(AssistantMessage),
    ToolResult(ToolResultMessage),
    Attachment(AttachmentMessage),
    Progress(ProgressMessage),
}
```

The `#[serde(tag = "type")]` attribute is important -- it produces internally-tagged JSON like `{"type": "User", "id": "...", "content": "..."}`, which is the same format Claude Code uses for session transcripts.

Let's walk through each variant.

### System

```rust
pub struct SystemMessage {
    pub id: MessageId,
    pub content: String,
    #[serde(default)]
    pub tag: Option<String>,
}
```

System messages carry instructions injected by the agent, not typed by the user. The optional `tag` field serves a specific purpose: later, the compaction engine will look for a system message tagged `"compact_boundary"` to know where the compacted summary ends and fresh messages begin. Tags also let the system prompt builder mark sections for deduplication.

### User

```rust
pub struct UserMessage {
    pub id: MessageId,
    pub content: String,
}
```

Straightforward -- the human's input. One message per turn.

### Assistant

```rust
pub struct AssistantMessage {
    pub id: MessageId,
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    pub usage: Option<TokenUsage>,
}
```

This is the richest variant. The model can return text, tool calls, or both. `text` is `Option<String>` because when the model decides to use a tool, it may produce no human-readable text at all -- it just emits one or more `ToolCall` entries. The `stop_reason` tells the agent loop whether to execute tools and continue, or to present the response to the user and stop.

The `usage` field is `Option<TokenUsage>` because we attach token counts at parse time from the API response. Mock providers in tests may leave it as `None`.

### ToolResult

```rust
pub struct ToolResultMessage {
    pub id: MessageId,
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_truncated: bool,
}
```

After the agent executes a tool, it packages the output into a `ToolResultMessage` and appends it to the conversation. The `tool_use_id` field links this result back to the specific `ToolCall` it answers -- without this, the model cannot correlate which result belongs to which call when multiple tools run in a single turn.

The `is_truncated` flag tells the model that the output was cut short. This matters for tools like Bash or Read that can produce enormous output -- the agent truncates to stay within context limits and sets this flag so the model knows it is seeing an incomplete picture.

### Attachment

```rust
pub struct AttachmentMessage {
    pub id: MessageId,
    pub path: String,
    pub content: String,
    pub content_type: String,
}
```

Attachments represent injected context that is neither user input nor system instructions. When Claude Code discovers a `CLAUDE.md` file in the project, it reads it and injects an `Attachment` with `content_type: "instructions"`. Images get `content_type: "file"`. The `path` field records where the content came from, which the TUI uses for display.

### Progress

```rust
pub struct ProgressMessage {
    pub tool_use_id: String,
    pub data: serde_json::Value,
}
```

Progress messages are UI-only -- they are never sent to the LLM API. When a long-running tool (like a bash command) produces incremental output, it emits `Progress` messages so the TUI can show a live spinner or streaming text. The `data` field is an unstructured `Value` because different tools report different shapes of progress.

Notice that `Progress` has no `id` field -- these are ephemeral and never persisted to the session transcript.

## 1.3 Message constructors

Rather than manually constructing each struct variant, we provide convenience methods on `Message`:

```rust
impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::System(SystemMessage {
            id: new_id(),
            content: content.into(),
            tag: None,
        })
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::User(UserMessage {
            id: new_id(),
            content: content.into(),
        })
    }

    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::ToolResult(ToolResultMessage {
            id: new_id(),
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_truncated: false,
        })
    }

    pub fn assistant(
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
        stop_reason: StopReason,
        usage: Option<TokenUsage>,
    ) -> Self {
        Self::Assistant(AssistantMessage {
            id: new_id(),
            text,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}
```

Every constructor calls `new_id()` to generate a fresh UUID. The `impl Into<String>` parameters let callers pass `&str` or `String` without explicit conversion.

**Implement these** in `src/types/message.rs`. You can verify with:

```bash
cargo test -p claw-code-starter test_ch1_create_user_message
cargo test -p claw-code-starter test_ch1_create_system_message
cargo test -p claw-code-starter test_ch1_create_tool_result
cargo test -p claw-code-starter test_ch1_create_assistant_message
cargo test -p claw-code-starter test_ch1_unique_message_ids
```

---

## 1.4 StopReason

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StopReason {
    Stop,
    ToolUse,
}
```

This tiny enum drives the entire agent loop. When the provider parses the LLM response:

- **`Stop`** means the model is done -- its `text` field contains the final answer for the user.
- **`ToolUse`** means the model wants to invoke tools -- the agent should look at `tool_calls`, execute them, append the results, and call the provider again.

The `PartialEq` derive is essential -- the agent loop literally matches `stop_reason == StopReason::Stop` to decide whether to break.

```bash
cargo test -p claw-code-starter test_ch1_stop_reason_equality
```

---

## 1.5 ToolCall

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
```

When the LLM responds with `StopReason::ToolUse`, it includes one or more `ToolCall` entries. Each has:

- **`id`** -- a unique identifier assigned by the API (e.g., `"call_abc123"`). This is what `ToolResultMessage::tool_use_id` references.
- **`name`** -- which tool to invoke (e.g., `"bash"`, `"read"`, `"edit"`).
- **`arguments`** -- a JSON object whose shape matches the tool's parameter schema.

The agent loop uses `name` to look up the tool in the `ToolSet`, passes `arguments` to `tool.call()`, and wraps the output in a `ToolResultMessage` whose `tool_use_id` matches the `ToolCall`'s `id`.

```bash
cargo test -p claw-code-starter test_ch1_assistant_with_tool_calls
```

---

## 1.6 ToolDefinition and the builder pattern

Every tool must describe itself to the LLM with a JSON Schema so the model knows what parameters are available. `ToolDefinition` holds this schema and provides a builder API for constructing it without hand-writing JSON:

```rust
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}
```

The constructor initializes an empty object schema:

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
}
```

### `.param()` -- add a simple parameter

```rust
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
```

This is the workhorse. Most tool parameters are simple types -- a `"string"` for a file path, a `"number"` for a line offset. The builder takes `self` by value and returns it, enabling chained calls:

```rust
ToolDefinition::new("read", "Read a file from disk")
    .param("path", "string", "Absolute path to the file", true)
    .param("offset", "number", "Line number to start reading from", false)
    .param("limit", "number", "Maximum number of lines to read", false)
```

### `.param_raw()` -- add a complex parameter

```rust
pub fn param_raw(
    mut self,
    name: &str,
    schema: Value,
    required: bool,
) -> Self {
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

Some parameters need richer schemas -- enums, arrays, nested objects. `param_raw` lets you pass an arbitrary `serde_json::Value` as the schema. For example, an edit tool might define:

```rust
.param_raw("changes", serde_json::json!({
    "type": "array",
    "items": {
        "type": "object",
        "properties": {
            "old_string": { "type": "string" },
            "new_string": { "type": "string" }
        }
    }
}), true)
```

**Implement `ToolDefinition`** in `src/types/tool.rs`, then verify:

```bash
cargo test -p claw-code-starter test_ch1_tool_definition_builder
cargo test -p claw-code-starter test_ch1_tool_definition_optional_param
```

---

## 1.7 ToolResult

When a tool finishes executing, it returns a `ToolResult`:

```rust
pub struct ToolResult {
    pub content: String,
    pub is_truncated: bool,
}
```

Two convenience constructors cover the common cases:

```rust
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

Note how `error()` prefixes the message with `"error: "`. This convention is important -- the model sees the tool result as plain text, and the prefix signals that the operation failed. Claude Code uses the same pattern so the LLM can distinguish success from failure and decide whether to retry or report the error to the user.

```bash
cargo test -p claw-code-starter test_ch1_tool_result_text
cargo test -p claw-code-starter test_ch1_tool_result_error
```

---

## 1.8 ValidationResult

Before executing a tool, the agent can validate its input:

```rust
pub enum ValidationResult {
    Ok,
    Error { message: String, code: u32 },
}
```

This lets tools reject malformed arguments (e.g., a missing required field, an invalid path) before any side effects occur. The `code` field is reserved for structured error reporting -- the permission engine can use it to distinguish "bad input" from "permission denied."

---

## 1.9 The Tool trait

This is the central abstraction. Every tool -- Bash, Read, Write, Edit, Grep, Glob -- implements this trait:

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
    fn summary(&self, args: &Value) -> String { ... }
    fn activity_description(&self, _args: &Value) -> Option<String> { None }
}
```

Let's break down each method:

**`definition()`** returns the tool's schema. This is called once when registering tools and whenever the agent needs to send tool definitions to the LLM. It returns a reference (`&ToolDefinition`) because the definition is static for the lifetime of the tool.

**`call()`** is the execution entry point. It receives the JSON arguments the LLM provided and returns a `ToolResult`. This is `async` because most tools do I/O -- reading files, running subprocesses, making HTTP requests.

**`validate_input()`** is a pre-execution check. The default implementation accepts everything. Tools override this to catch errors early -- for example, the Edit tool checks that `old_string` and `new_string` are present before attempting the edit.

**`is_read_only()`** is used by Plan Mode. When the agent runs in plan mode, it only executes read-only tools (Read, Glob, Grep) and skips write tools (Write, Edit, Bash). Each tool self-reports via this method.

**`is_concurrent_safe()`** tells the agent whether this tool can run in parallel with other tools in the same turn. Read is concurrent-safe; Write is not (two concurrent writes to the same file would race).

**`is_destructive()`** flags tools that perform irreversible operations. The permission engine uses this to require explicit user approval even in auto-approve mode.

**`summary()`** produces a one-line string for the terminal, like `[bash: ls -la]` or `[read: src/main.rs]`. The default implementation looks for common argument keys (`command`, `path`, `question`, `pattern`) and formats them:

```rust
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
```

**`activity_description()`** returns an optional string for the TUI spinner, like `"Reading file..."` or `"Running command..."`. Returns `None` by default, meaning the TUI falls back to the summary.

The trait is marked `Send + Sync` (required by `#[async_trait]` for object safety) so tools can be stored in the `ToolSet` and called from async contexts. You do not need to implement any concrete tools yet -- that comes in later chapters. For now, just define the trait.

---

## 1.10 ToolSet

The agent needs to look up tools by name when the LLM requests a tool call. `ToolSet` is a `HashMap`-backed registry:

```rust
pub struct ToolSet {
    tools: HashMap<String, Box<dyn Tool>>,
}
```

The key methods:

```rust
impl ToolSet {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    /// Builder-style: add a tool and return self.
    pub fn with(mut self, tool: impl Tool + 'static) -> Self {
        self.push(tool);
        self
    }

    /// Add a tool, keyed by its definition name.
    pub fn push(&mut self, tool: impl Tool + 'static) {
        let name = tool.definition().name.to_string();
        self.tools.insert(name, Box::new(tool));
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Collect all tool schemas for the provider.
    pub fn definitions(&self) -> Vec<&ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolSet {
    fn default() -> Self {
        Self::new()
    }
}
```

A few design points:

- **`with()` enables builder-style chaining**: `ToolSet::new().with(ReadTool::new()).with(BashTool::new())`.
- **`push()` extracts the name from the tool's definition**, so you never pass the name manually -- one source of truth.
- **`definitions()`** collects all schemas into a `Vec` that the provider sends to the LLM at the start of each turn.
- **`Box<dyn Tool>`** is the trait object that makes heterogeneous storage possible. The `'static` bound on `push`/`with` ensures the tool lives long enough.

```bash
cargo test -p claw-code-starter test_ch1_toolset_empty
```

---

## 1.11 TokenUsage and ModelUsage

LLM APIs report token counts with each response. Tracking these is essential for cost management and for knowing when to trigger context compaction.

### Per-request usage

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}
```

The `cache_read_tokens` and `cache_creation_tokens` fields reflect Anthropic's prompt caching feature. When a prefix of the prompt matches the cache, those tokens are served at reduced cost (`cache_read_tokens`). The first time a prompt is cached, the API reports `cache_creation_tokens`. These fields default to zero via `#[serde(default)]` for providers that do not support caching.

### Accumulated usage

```rust
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    pub turn_count: u64,
}

impl ModelUsage {
    pub fn record(&mut self, usage: &TokenUsage, cost: f64) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cache_read_tokens += usage.cache_read_tokens;
        self.cache_creation_tokens += usage.cache_creation_tokens;
        self.cost_usd += cost;
        self.turn_count += 1;
    }
}
```

`ModelUsage` accumulates totals across an entire session. The `record()` method is called after each provider response. The `cost_usd` is computed by the caller (provider layer) because pricing varies per model. The `turn_count` tracks how many API calls have been made -- useful for the TUI's status line and for cost-per-turn calculations.

```bash
cargo test -p claw-code-starter test_ch1_token_usage_default
cargo test -p claw-code-starter test_ch1_token_usage_total
cargo test -p claw-code-starter test_ch1_model_usage_record
```

---

## 1.12 Permission types

The permission system deserves its own chapter (Chapter 10), but we define the types here because they are referenced by the tool trait and the agent loop. Think of this section as forward declarations -- you are laying down the vocabulary now and will wire up the logic later.

### Permission

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    Allow,
    Deny(String),
    Ask(String),
}
```

Three outcomes for any tool call:

- **`Allow`** -- execute immediately, no user prompt.
- **`Deny(reason)`** -- block execution and tell the model why it was rejected.
- **`Ask(prompt)`** -- show the user a permission dialog with the given prompt text and wait for approval.

This mirrors Claude Code's permission model exactly. When you run Claude Code and it asks "Allow bash: rm -rf target?" -- that is an `Ask` permission being surfaced to the TUI.

### PermissionMode

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    Default,
    Auto,
    Bypass,
    Plan,
    DontAsk,
}
```

This controls the agent's overall permission posture:

| Mode | Behavior |
|------|----------|
| `Default` | Prompt the user for unrecognized operations |
| `Auto` | Auto-approve based on safety classifier confidence |
| `Bypass` | Skip all permission checks (testing / CI) |
| `Plan` | Only allow read-only tools; deny everything else |
| `DontAsk` | Deny anything that would normally prompt the user |

Claude Code defaults to `Default` for interactive use. The `--dangerously-skip-permissions` flag sets `Bypass`. Plan mode uses `Plan` to let the agent read and reason but never write.

### PermissionBehavior and PermissionRule

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub tool_pattern: String,
    pub behavior: PermissionBehavior,
}
```

Rules are glob patterns that match tool names. A rule like `{ tool_pattern: "bash", behavior: Deny }` blocks all bash invocations. Rules are evaluated in order; the first match wins. This is how `.claude/settings.json` configures per-project permissions -- you might allow `read` and `glob` but require a prompt for `bash`.

### PermissionSource

```rust
#[derive(Debug, Clone)]
pub enum PermissionSource {
    Rule(PermissionRule),
    Mode(PermissionMode),
    Hook(String),
    Safety(String),
    Session,
}
```

When a permission decision is made, the source records *why*:

- **`Rule`** -- a config rule matched.
- **`Mode`** -- the global permission mode decided.
- **`Hook`** -- a pre-tool hook returned a permission override.
- **`Safety`** -- a safety classifier flagged the operation.
- **`Session`** -- the user previously approved this exact operation in this session.

This is used for logging and debugging. When a tool call is unexpectedly denied, the source tells you where to look.

**Implement all permission types** in `src/types/permission.rs`. These are pure data types with no logic beyond the derives.

---

## Putting it all together

After implementing all four files, run the full chapter test suite:

```bash
cargo test -p claw-code-starter test_ch1
```

You should see all 15 tests pass:

```
test_ch1_create_user_message ........... ok
test_ch1_create_system_message ......... ok
test_ch1_create_tool_result ............ ok
test_ch1_create_assistant_message ...... ok
test_ch1_assistant_with_tool_calls ..... ok
test_ch1_unique_message_ids ............ ok
test_ch1_tool_definition_builder ....... ok
test_ch1_tool_definition_optional_param  ok
test_ch1_token_usage_default ........... ok
test_ch1_token_usage_total ............. ok
test_ch1_toolset_empty ................. ok
test_ch1_tool_result_text .............. ok
test_ch1_tool_result_error ............. ok
test_ch1_stop_reason_equality .......... ok
test_ch1_model_usage_record ............ ok
```

## What you built

This chapter established the type vocabulary for the entire agent:

- **`Message`** -- a six-variant tagged enum carrying every kind of conversation entry, from user input to ephemeral progress updates.
- **`StopReason`** -- the binary signal that drives the agent loop: keep going or stop.
- **`ToolDefinition`** -- a builder for JSON Schema tool descriptions that the LLM uses to understand what tools are available.
- **`ToolCall` / `ToolResult`** -- the request-response pair for tool execution, linked by ID.
- **`Tool` trait** -- the full interface every tool must implement, covering identity, execution, validation, safety, and display.
- **`ToolSet`** -- a `HashMap`-backed registry for looking up tools by name at runtime.
- **`TokenUsage` / `ModelUsage`** -- per-request and per-session token tracking for cost management and compaction triggers.
- **Permission types** -- the vocabulary for the safety pipeline: decisions, modes, rules, and sources.

None of these types do anything on their own -- they are the nouns of the system. In the next chapter, we will build the `Provider` trait and connect to a real LLM via Server-Sent Events streaming, giving these types their first verbs.
