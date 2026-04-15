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
        unimplemented!("Canonicalize the path and store both raw and canonical versions")
    }

    pub fn validate_path(&self, path: &str) -> Result<(), String> {
        unimplemented!("Resolve path, canonicalize, check starts_with allowed_dir")
    }
}

impl SafetyCheck for PathValidator {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        unimplemented!("Match on tool_name, extract path arg, call validate_path")
    }
}

/// Filters dangerous shell commands.
pub struct CommandFilter {
    blocked_patterns: Vec<glob::Pattern>,
}

impl CommandFilter {
    pub fn new(patterns: &[String]) -> Self {
        unimplemented!("Parse each pattern string into a glob::Pattern")
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

    pub fn is_blocked(&self, command: &str) -> Option<&str> {
        unimplemented!("Trim command, check against each pattern, return matching pattern")
    }
}

impl SafetyCheck for CommandFilter {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        unimplemented!("Only check bash tool, extract command, call is_blocked")
    }
}

/// Checks file paths against protected glob patterns.
pub struct ProtectedFileCheck {
    patterns: Vec<glob::Pattern>,
}

impl ProtectedFileCheck {
    pub fn new(patterns: &[String]) -> Self {
        unimplemented!("Parse each pattern string into a glob::Pattern")
    }
}

impl SafetyCheck for ProtectedFileCheck {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        unimplemented!("Match on write/edit, extract path, check against patterns")
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

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        unimplemented!(
            "Run checks, if all pass call inner.call(args), otherwise return error string"
        )
    }
}
