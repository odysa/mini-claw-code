use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;

use crate::types::{Tool, ToolDefinition};

/// A check that runs before a tool call is executed.
pub trait SafetyCheck: Send + Sync {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String>;
}

/// Validates that file paths stay within an allowed directory.
pub struct PathValidator {
    allowed_dir: PathBuf,
    raw_dir: PathBuf,
}

impl PathValidator {
    pub fn new(allowed_dir: impl Into<PathBuf>) -> Self {
        let raw_dir: PathBuf = allowed_dir.into();
        let allowed_dir = raw_dir.canonicalize().unwrap_or_else(|_| raw_dir.clone());
        Self {
            allowed_dir,
            raw_dir,
        }
    }

    /// Validate that `path` resolves to somewhere inside `self.allowed_dir`.
    ///
    /// Hints:
    /// - Build the resolved path: absolute stays absolute, relative joins `self.raw_dir`.
    /// - If it exists, canonicalize; otherwise canonicalize the parent and append
    ///   the filename (so new files in allowed dirs still validate).
    /// - Return Ok if the canonical result starts with `self.allowed_dir`, Err otherwise.
    pub fn validate_path(&self, _path: &str) -> Result<(), String> {
        unimplemented!(
            "TODO ch11: canonicalize path (or its parent) and check it stays under allowed_dir"
        )
    }
}

impl SafetyCheck for PathValidator {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        match tool_name {
            "read" | "write" | "edit" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.validate_path(path)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}

/// Filters dangerous shell commands.
pub struct CommandFilter {
    blocked_patterns: Vec<glob::Pattern>,
}

impl CommandFilter {
    pub fn new(patterns: &[String]) -> Self {
        Self {
            blocked_patterns: patterns
                .iter()
                .filter_map(|p| glob::Pattern::new(p).ok())
                .collect(),
        }
    }

    pub fn default_filters() -> Self {
        Self::new(&[
            "rm -rf /".into(),
            "rm -rf /*".into(),
            "sudo *".into(),
            "> /dev/sda*".into(),
            "mkfs.*".into(),
            "dd if=*of=/dev/*".into(),
            ":(){:|:&};:".into(),
        ])
    }

    /// Return the first blocked pattern that matches `command`, or `None`.
    ///
    /// Hint: Trim the command, then check each `glob::Pattern` in `self.blocked_patterns`.
    pub fn is_blocked(&self, _command: &str) -> Option<&str> {
        unimplemented!(
            "TODO ch11: return the first blocked pattern that matches the trimmed command"
        )
    }
}

impl SafetyCheck for CommandFilter {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        if tool_name != "bash" {
            return Ok(());
        }
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            if let Some(pattern) = self.is_blocked(command) {
                Err(format!("blocked command matching pattern `{pattern}`"))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}

/// Checks file paths against protected glob patterns.
pub struct ProtectedFileCheck {
    patterns: Vec<glob::Pattern>,
}

impl ProtectedFileCheck {
    pub fn new(patterns: &[String]) -> Self {
        Self {
            patterns: patterns
                .iter()
                .filter_map(|p| glob::Pattern::new(p).ok())
                .collect(),
        }
    }
}

impl SafetyCheck for ProtectedFileCheck {
    /// Block `write`/`edit` calls whose path matches a protected pattern.
    ///
    /// Hints:
    /// - Only react to tool_name "write" or "edit"; other tools pass.
    /// - Match each pattern against both the full path and the bare file name.
    fn check(&self, _tool_name: &str, _args: &Value) -> Result<(), String> {
        unimplemented!(
            "TODO ch11: for write/edit, fail if path (or file name) matches any protected pattern"
        )
    }
}

/// Wraps a tool with safety checks that run before each call.
pub struct SafeToolWrapper {
    inner: Box<dyn Tool>,
    checks: Vec<Box<dyn SafetyCheck>>,
}

impl SafeToolWrapper {
    pub fn new(tool: Box<dyn Tool>, checks: Vec<Box<dyn SafetyCheck>>) -> Self {
        Self {
            inner: tool,
            checks,
        }
    }

    pub fn with_check(tool: Box<dyn Tool>, check: impl SafetyCheck + 'static) -> Self {
        Self::new(tool, vec![Box::new(check)])
    }
}

#[async_trait]
impl Tool for SafeToolWrapper {
    fn definition(&self) -> &ToolDefinition {
        self.inner.definition()
    }

    /// Run every check before delegating to the inner tool.
    ///
    /// Hint: On the first failure, return `Ok(format!("error: safety check failed: {reason}"))`
    /// so the agent sees the rejection as a normal tool result instead of crashing.
    async fn call(&self, _args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "TODO ch11: run each SafetyCheck; on first failure return error-shaped Ok, else call inner"
        )
    }
}
