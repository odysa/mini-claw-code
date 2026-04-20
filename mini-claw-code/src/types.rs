use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use serde_json::Value;

pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

impl ToolDefinition {
    /// Create a new tool definition with no parameters.
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

    /// Add a parameter to the tool definition.
    ///
    /// - `name`: parameter name (e.g. "path")
    /// - `type_`: JSON schema type (e.g. "string")
    /// - `description`: what this parameter is for
    /// - `required`: whether the parameter is required
    pub fn param(mut self, name: &str, type_: &str, description: &str, required: bool) -> Self {
        self.parameters["properties"][name] = serde_json::json!({
            "type": type_,
            "description": description
        });
        if required {
            self.parameters["required"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::Value::String(name.to_string()));
        }
        self
    }

    /// Add a parameter with a raw JSON schema value.
    ///
    /// Use this for complex types (arrays, nested objects) that `param()` can't express.
    pub fn param_raw(mut self, name: &str, schema: Value, required: bool) -> Self {
        self.parameters["properties"][name] = schema;
        if required {
            self.parameters["required"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::Value::String(name.to_string()));
        }
        self
    }
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Why the model stopped generating.
pub enum StopReason {
    /// The model finished — check `text` for the response.
    Stop,
    /// The model wants to use tools — check `tool_calls`.
    ToolUse,
}

/// Token usage reported by the API for a single request.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub struct AssistantTurn {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    /// Token usage for this turn, if reported by the provider.
    pub usage: Option<TokenUsage>,
}

pub enum Message {
    System(String),
    User(String),
    Assistant(AssistantTurn),
    ToolResult { id: String, content: String },
}

/// Result of executing a tool.
///
/// Wraps the content returned to the LLM with metadata the agent loop
/// needs — currently just a truncation flag, but this is the place to add
/// injected-message or context-mutation support later.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_truncated: bool,
}

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

/// Outcome of a tool's `validate_input`.
pub enum ValidationResult {
    Ok,
    Error { message: String, code: u32 },
}

/// The `Tool` trait uses `#[async_trait]` (instead of RPITIT like `Provider`)
/// because tools are stored as `Box<dyn Tool>` in `ToolSet`, which requires
/// object safety. RPITIT methods (`-> impl Future`) are not object-safe,
/// so `async_trait` desugars them into `-> Pin<Box<dyn Future>>` which is.
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    // --- Identity & execution ---

    fn definition(&self) -> &ToolDefinition;
    async fn call(&self, args: Value) -> anyhow::Result<ToolResult>;

    // --- Validation ---

    /// Validate input before execution. Default: always valid. Override when
    /// you want to fail fast before any side effect runs.
    fn validate_input(&self, _args: &Value) -> ValidationResult {
        ValidationResult::Ok
    }

    // --- Safety metadata ---

    /// True if the tool only reads data (safe in plan mode, no approval needed).
    fn is_read_only(&self) -> bool {
        false
    }

    /// True if the tool is safe to run concurrently with other tools.
    fn is_concurrent_safe(&self) -> bool {
        false
    }

    /// True if the tool performs irreversible operations.
    fn is_destructive(&self) -> bool {
        false
    }

    // --- Display ---

    /// One-line summary for terminal output (e.g. `[bash: ls -la]`).
    ///
    /// Default picks the first argument from a short list of common keys.
    /// Override when your tool wants a better caption.
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
}

/// A named collection of tools backed by a HashMap for O(1) lookup.
pub struct ToolSet {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolSet {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Add a tool (builder pattern).
    pub fn with(mut self, tool: impl Tool + 'static) -> Self {
        self.push(tool);
        self
    }

    /// Add a tool by mutable reference.
    pub fn push(&mut self, tool: impl Tool + 'static) {
        let name = tool.definition().name.to_string();
        self.tools.insert(name, Box::new(tool));
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Collect all tool definitions.
    pub fn definitions(&self) -> Vec<&ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Execute a list of tool calls end-to-end: lookup, validate, invoke, truncate.
    ///
    /// Returns one `(call_id, ToolResult)` pair per input call, in order. Unknown
    /// tools, validation failures, and call errors all surface as
    /// `ToolResult::error` so the model sees them as tool output rather than
    /// bubbling up and breaking the agent loop.
    pub async fn execute_calls(
        &self,
        calls: &[ToolCall],
        max_result_chars: usize,
    ) -> Vec<(String, ToolResult)> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            let result = match self.get(&call.name) {
                Some(t) => match t.validate_input(&call.arguments) {
                    ValidationResult::Error { message, .. } => ToolResult::error(message),
                    ValidationResult::Ok => match t.call(call.arguments.clone()).await {
                        Ok(mut r) => {
                            if r.content.len() > max_result_chars {
                                let at = truncate_utf8(&r.content, max_result_chars);
                                r.content = format!(
                                    "{}... [truncated, {} chars total]",
                                    &r.content[..at],
                                    r.content.len()
                                );
                                r.is_truncated = true;
                            }
                            r
                        }
                        Err(e) => ToolResult::error(e.to_string()),
                    },
                },
                None => ToolResult::error(format!("unknown tool `{}`", call.name)),
            };
            results.push((call.id.clone(), result));
        }
        results
    }
}

impl Default for ToolSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the largest byte index `<= max_bytes` that falls on a UTF-8 char
/// boundary. Used to truncate tool output without slicing a multi-byte codepoint.
pub fn truncate_utf8(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Limits that bound the agent loop so a runaway model can't blow up
/// context or spin forever on tool calls.
#[derive(Debug, Clone, Copy)]
pub struct QueryConfig {
    /// Maximum agent loop iterations before bailing with an error.
    pub max_turns: usize,
    /// Maximum `content` length (in bytes) for each tool result before truncation.
    pub max_result_chars: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            max_turns: 50,
            max_result_chars: 100_000,
        }
    }
}

/// `Provider` uses RPITIT (return-position `impl Trait` in trait) because it
/// is always used as a generic parameter (`P: Provider`), never as `dyn Provider`.
/// This avoids the heap allocation that `#[async_trait]` requires.
pub trait Provider: Send + Sync {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a;
}

/// Blanket impl: `Arc<P>` is a `Provider` whenever `P` is.
///
/// This lets parent and child agents share the same provider via `Arc`
/// without cloning. Needed for subagents (Chapter 13).
impl<P: Provider> Provider for Arc<P> {
    fn chat<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [&'a ToolDefinition],
    ) -> impl Future<Output = anyhow::Result<AssistantTurn>> + Send + 'a {
        (**self).chat(messages, tools)
    }
}
