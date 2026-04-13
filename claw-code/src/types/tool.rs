use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Schema definition for a tool, sent to the LLM.
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

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

    pub fn param(mut self, name: &str, type_: &str, description: &str, required: bool) -> Self {
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

    pub fn param_raw(mut self, name: &str, schema: Value, required: bool) -> Self {
        self.parameters["properties"][name] = schema;
        if required {
            self.parameters["required"]
                .as_array_mut()
                .unwrap()
                .push(Value::String(name.to_string()));
        }
        self
    }
}

/// A tool call requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Result of executing a tool.
///
/// Mirrors Claude Code's `ToolResult<T>` — the tool can return data,
/// inject messages, or modify context.
pub struct ToolResult {
    /// The string content returned to the LLM.
    pub content: String,
    /// Whether the content was truncated.
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

/// Validation result for tool input.
pub enum ValidationResult {
    Ok,
    Error { message: String, code: u32 },
}

/// The full tool interface, mirroring Claude Code's Tool type.
///
/// Tools implement this trait to participate in the agent loop. The trait
/// is object-safe (via `#[async_trait]`) so tools can be stored in a `ToolSet`.
#[async_trait]
pub trait Tool: Send + Sync {
    // --- Identity ---

    /// The tool's JSON schema definition for the LLM.
    fn definition(&self) -> &ToolDefinition;

    // --- Execution ---

    /// Execute the tool with the given arguments.
    async fn call(&self, args: Value) -> anyhow::Result<ToolResult>;

    // --- Validation ---

    /// Validate input before execution. Default: always valid.
    fn validate_input(&self, _args: &Value) -> ValidationResult {
        ValidationResult::Ok
    }

    // --- Safety & Behavior ---

    /// Whether this tool only reads data (doesn't modify anything).
    /// Used by plan mode to filter tools.
    fn is_read_only(&self) -> bool {
        false
    }

    /// Whether this tool is safe to run concurrently with other tools.
    fn is_concurrent_safe(&self) -> bool {
        false
    }

    /// Whether this tool performs destructive operations.
    fn is_destructive(&self) -> bool {
        false
    }

    // --- Display ---

    /// One-line summary for terminal display (e.g., "[bash: ls -la]").
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

    /// Activity description for spinner display (e.g., "Reading file...").
    fn activity_description(&self, _args: &Value) -> Option<String> {
        None
    }
}

/// A named collection of tools backed by a HashMap.
pub struct ToolSet {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolSet {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn with(mut self, tool: impl Tool + 'static) -> Self {
        self.push(tool);
        self
    }

    pub fn push(&mut self, tool: impl Tool + 'static) {
        let name = tool.definition().name.to_string();
        self.tools.insert(name, Box::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

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

    /// Execute a list of tool calls, returning results paired with call IDs.
    ///
    /// Handles lookup, validation, execution, and truncation in one place.
    /// Both `QueryEngine` and `PlanEngine` delegate to this.
    pub async fn execute_calls(
        &self,
        calls: &[ToolCall],
        max_result_chars: usize,
    ) -> Vec<(String, ToolResult)> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            let result = match self.get(&call.name) {
                Some(t) => {
                    match t.validate_input(&call.arguments) {
                        ValidationResult::Ok => {}
                        ValidationResult::Error { message, .. } => {
                            results.push((call.id.clone(), ToolResult::error(message)));
                            continue;
                        }
                    }
                    match t.call(call.arguments.clone()).await {
                        Ok(mut r) => {
                            if r.content.len() > max_result_chars {
                                let truncate_at = truncate_utf8(&r.content, max_result_chars);
                                r.content = format!(
                                    "{}... [truncated, {} chars total]",
                                    &r.content[..truncate_at],
                                    r.content.len()
                                );
                                r.is_truncated = true;
                            }
                            r
                        }
                        Err(e) => ToolResult::error(e.to_string()),
                    }
                }
                None => ToolResult::error(format!("unknown tool `{}`", call.name)),
            };
            results.push((call.id.clone(), result));
        }
        results
    }
}

/// Find the largest byte index <= `max_bytes` that falls on a UTF-8 char boundary.
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

impl Default for ToolSet {
    fn default() -> Self {
        Self::new()
    }
}
