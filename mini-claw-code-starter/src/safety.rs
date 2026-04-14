use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;

use crate::types::{Tool, ToolDefinition};

/// A check that runs before a tool call is executed.
///
/// Implementations validate tool arguments and return `Ok(())` to allow
/// execution or `Err(reason)` to block it.
pub trait SafetyCheck: Send + Sync {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String>;
}

/// Validates that file paths stay within an allowed directory.
///
/// # Chapter 11: Safety Checks
pub struct PathValidator {
    allowed_dir: PathBuf,
    raw_dir: PathBuf,
}

impl PathValidator {
    /// Create a new PathValidator for the given directory.
    ///
    /// Hint: Canonicalize the path, falling back to the raw path on error.
    pub fn new(allowed_dir: impl Into<PathBuf>) -> Self {
        unimplemented!("Canonicalize the path and store both raw and canonical versions")
    }

    /// Check whether a path is within the allowed directory.
    ///
    /// Hints:
    /// - Resolve relative paths against raw_dir
    /// - Canonicalize the target (for existing files) or its parent (for new files)
    /// - Check if the canonical path starts_with the allowed_dir
    pub fn validate_path(&self, path: &str) -> Result<(), String> {
        unimplemented!("Resolve path, canonicalize, check starts_with allowed_dir")
    }
}

impl SafetyCheck for PathValidator {
    /// Only check tools that take a "path" argument (read, write, edit).
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        unimplemented!("Match on tool_name, extract path arg, call validate_path")
    }
}

/// Filters dangerous shell commands.
///
/// # Chapter 11: Safety Checks
pub struct CommandFilter {
    blocked_patterns: Vec<glob::Pattern>,
}

impl CommandFilter {
    /// Create a filter with the given blocked patterns.
    pub fn new(patterns: &[String]) -> Self {
        unimplemented!("Parse each pattern string into a glob::Pattern")
    }

    /// Default set of dangerous command patterns.
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

    /// Check if a command matches any blocked pattern.
    pub fn is_blocked(&self, command: &str) -> Option<&str> {
        unimplemented!("Trim command, check against each pattern, return matching pattern")
    }
}

impl SafetyCheck for CommandFilter {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        unimplemented!("Only check 'bash' tool, extract command, call is_blocked")
    }
}

/// Checks file paths against protected glob patterns.
///
/// # Chapter 11: Safety Checks
pub struct ProtectedFileCheck {
    patterns: Vec<glob::Pattern>,
}

impl ProtectedFileCheck {
    pub fn new(patterns: &[String]) -> Self {
        unimplemented!("Parse each pattern string into a glob::Pattern")
    }
}

impl SafetyCheck for ProtectedFileCheck {
    /// Block write/edit to files matching protected patterns.
    ///
    /// Hint: Check both the full path and just the filename against each pattern.
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        unimplemented!("Match on write/edit, extract path, check against patterns")
    }
}

/// Wraps a tool with safety checks that run before each call.
///
/// # Chapter 11: Safety Checks
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

    /// Run all safety checks before calling the inner tool.
    ///
    /// Hint: If any check returns Err, return Ok(format!("error: safety check failed: {reason}"))
    /// instead of calling the inner tool.
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "Run checks, if all pass call inner.call(args), otherwise return error string"
        )
    }
}
