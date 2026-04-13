use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct GlobTool {
    def: ToolDefinition,
}

impl GlobTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("glob", "Find files matching a glob pattern")
                .param("pattern", "string", "Glob pattern (e.g. \"**/*.rs\")", true)
                .param(
                    "path",
                    "string",
                    "Base directory to search in (default: current directory)",
                    false,
                ),
        }
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' argument"))?;

        let base = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let full_pattern = if pattern.starts_with('/') || pattern.starts_with('.') {
            pattern.to_string()
        } else {
            format!("{base}/{pattern}")
        };

        let entries: Vec<String> = glob::glob(&full_pattern)
            .map_err(|e| anyhow::anyhow!("invalid glob pattern: {e}"))?
            .filter_map(|entry| entry.ok())
            .map(|p| p.display().to_string())
            .collect();

        if entries.is_empty() {
            Ok(ToolResult::text("no files matched"))
        } else {
            Ok(ToolResult::text(entries.join("\n")))
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Searching files...".into())
    }
}
