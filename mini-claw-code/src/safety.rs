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
pub struct PathValidator {
    /// Pre-canonicalized allowed directory (resolved once at construction).
    allowed_dir: PathBuf,
    /// Original (non-canonicalized) path, used for joining relative paths.
    raw_dir: PathBuf,
}

impl PathValidator {
    pub fn new(allowed_dir: impl Into<PathBuf>) -> Self {
        let raw_dir: PathBuf = allowed_dir.into();
        let allowed_dir = raw_dir
            .canonicalize()
            .unwrap_or_else(|_| raw_dir.clone());
        Self {
            allowed_dir,
            raw_dir,
        }
    }

    /// Check whether a path is within the allowed directory.
    pub fn validate_path(&self, path: &str) -> Result<(), String> {
        let target = Path::new(path);

        // Resolve to absolute path
        let resolved = if target.is_absolute() {
            target.to_path_buf()
        } else {
            self.raw_dir.join(target)
        };

        let canonical_target = if resolved.exists() {
            resolved
                .canonicalize()
                .map_err(|e| format!("cannot resolve path: {e}"))?
        } else {
            // For new files, check the parent directory
            let parent = resolved.parent().ok_or("invalid path")?;
            if parent.exists() {
                let mut canonical = parent
                    .canonicalize()
                    .map_err(|e| format!("cannot resolve parent: {e}"))?;
                if let Some(filename) = resolved.file_name() {
                    canonical.push(filename);
                }
                canonical
            } else {
                return Err(format!(
                    "parent directory does not exist: {}",
                    parent.display()
                ));
            }
        };

        if canonical_target.starts_with(&self.allowed_dir) {
            Ok(())
        } else {
            Err(format!(
                "path {} is outside allowed directory {}",
                canonical_target.display(),
                self.allowed_dir.display()
            ))
        }
    }
}

impl SafetyCheck for PathValidator {
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        // Only check tools that take a "path" argument
        match tool_name {
            "read" | "write" | "edit" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.validate_path(path)
                } else {
                    Ok(()) // No path argument, nothing to check
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

    pub fn is_blocked(&self, command: &str) -> Option<&str> {
        let trimmed = command.trim();
        for pattern in &self.blocked_patterns {
            if pattern.matches(trimmed) {
                return Some(pattern.as_str());
            }
        }
        None
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
    fn check(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        match tool_name {
            "write" | "edit" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    for pattern in &self.patterns {
                        if pattern.matches(path)
                            || pattern.matches(
                                Path::new(path)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_str()
                                    .unwrap_or(""),
                            )
                        {
                            return Err(format!(
                                "file `{path}` is protected (matches pattern `{}`)",
                                pattern.as_str()
                            ));
                        }
                    }
                    Ok(())
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
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

    /// Wrap a tool with a single safety check.
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
        let tool_name = self.inner.definition().name;
        for check in &self.checks {
            if let Err(reason) = check.check(tool_name, &args) {
                return Ok(format!("error: safety check failed: {reason}"));
            }
        }
        self.inner.call(args).await
    }
}
