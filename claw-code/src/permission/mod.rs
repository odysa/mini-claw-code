use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::types::*;

/// The permission engine — evaluates every tool call before execution.
///
/// Mirrors Claude Code's multi-stage permission pipeline:
/// 1. Check permission mode (Bypass/DontAsk/Plan short-circuit)
/// 2. Match against permission rules
/// 3. Apply mode-based defaults (Auto/Default)
/// 4. Check session-level approvals
///
/// The engine is designed to be queried from the QueryEngine's
/// `execute_tools` method. It does not execute tools itself.
pub struct PermissionEngine {
    mode: PermissionMode,
    rules: Vec<PermissionRule>,
    /// Tools the user has approved during this session.
    session_approvals: RwLock<HashSet<String>>,
}

impl PermissionEngine {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: Vec::new(),
            session_approvals: RwLock::new(HashSet::new()),
        }
    }

    pub fn with_rules(mut self, rules: Vec<PermissionRule>) -> Self {
        self.rules = rules;
        self
    }

    pub fn mode(&self) -> &PermissionMode {
        &self.mode
    }

    /// Evaluate a tool call and return a permission decision with its source.
    ///
    /// The pipeline:
    /// 1. Bypass mode → Allow everything
    /// 2. DontAsk mode → Deny everything
    /// 3. Plan mode → Allow read-only, deny everything else
    /// 4. Check rules (first match wins)
    /// 5. Session approvals (user already said yes)
    /// 6. Mode-based default (Auto: allow non-destructive; Default: ask)
    pub fn check(&self, tool_name: &str, tool: &dyn Tool) -> (Permission, PermissionSource) {
        // 1. Short-circuit modes
        match self.mode {
            PermissionMode::Bypass => {
                return (
                    Permission::Allow,
                    PermissionSource::Mode(PermissionMode::Bypass),
                );
            }
            PermissionMode::DontAsk => {
                return (
                    Permission::Deny("permission mode is DontAsk".into()),
                    PermissionSource::Mode(PermissionMode::DontAsk),
                );
            }
            PermissionMode::Plan => {
                return if tool.is_read_only() {
                    (
                        Permission::Allow,
                        PermissionSource::Mode(PermissionMode::Plan),
                    )
                } else {
                    (
                        Permission::Deny(format!(
                            "`{}` is not read-only — blocked in plan mode",
                            tool_name
                        )),
                        PermissionSource::Mode(PermissionMode::Plan),
                    )
                };
            }
            PermissionMode::Auto | PermissionMode::Default => {} // fall through
        }

        // 2. Check rules (first match wins)
        for rule in &self.rules {
            if pattern_matches(&rule.tool_pattern, tool_name) {
                let permission = match &rule.behavior {
                    PermissionBehavior::Allow => Permission::Allow,
                    PermissionBehavior::Deny => {
                        Permission::Deny(format!("denied by rule: {}", rule.tool_pattern))
                    }
                    PermissionBehavior::Ask => {
                        Permission::Ask(format!("rule requires approval: {}", rule.tool_pattern))
                    }
                };
                return (permission, PermissionSource::Rule(rule.clone()));
            }
        }

        // 3. Session approvals
        if self.is_session_approved(tool_name) {
            return (Permission::Allow, PermissionSource::Session);
        }

        // 4. Mode-based defaults
        match self.mode {
            PermissionMode::Auto => {
                if tool.is_destructive() {
                    (
                        Permission::Ask(format!(
                            "`{}` is destructive — requires approval even in auto mode",
                            tool_name
                        )),
                        PermissionSource::Mode(PermissionMode::Auto),
                    )
                } else {
                    (
                        Permission::Allow,
                        PermissionSource::Mode(PermissionMode::Auto),
                    )
                }
            }
            PermissionMode::Default => {
                if tool.is_read_only() {
                    (
                        Permission::Allow,
                        PermissionSource::Mode(PermissionMode::Default),
                    )
                } else {
                    (
                        Permission::Ask(format!("Allow {}?", tool_name)),
                        PermissionSource::Mode(PermissionMode::Default),
                    )
                }
            }
            _ => unreachable!(),
        }
    }

    /// Record that the user approved a tool for this session.
    pub fn approve_session(&self, tool_name: &str) {
        self.session_approvals
            .write()
            .unwrap()
            .insert(tool_name.to_string());
    }

    /// Check if a tool has been approved during this session.
    pub fn is_session_approved(&self, tool_name: &str) -> bool {
        self.session_approvals.read().unwrap().contains(tool_name)
    }

    /// Clear all session approvals.
    pub fn clear_session(&self) {
        self.session_approvals.write().unwrap().clear();
    }
}

/// Simple glob-style pattern matching for tool names.
///
/// Supports:
/// - Exact match: `"bash"` matches `"bash"`
/// - Wildcard: `"*"` matches everything
/// - Prefix wildcard: `"file_*"` matches `"file_read"`, `"file_write"`
fn pattern_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern == name
}

/// Static analysis of tool arguments for safety violations.
///
/// Catches dangerous patterns before the permission prompt even appears:
/// - Paths outside the allowed directory (boundary- and symlink-aware)
/// - Paths matching protected patterns (.env, .git)
/// - Blocked bash commands matched as glob patterns
pub struct SafetyChecker {
    /// Canonicalized allowed directory. Paths resolving outside this tree
    /// are rejected. `None` disables the boundary check.
    allowed_directory: Option<PathBuf>,
    /// Original (pre-canonicalization) allowed directory, used to resolve
    /// relative target paths.
    raw_allowed_directory: Option<PathBuf>,
    /// Glob patterns for files that cannot be modified.
    protected_patterns: Vec<String>,
    /// Compiled glob patterns blocked in bash commands.
    blocked_commands: Vec<glob::Pattern>,
}

impl SafetyChecker {
    pub fn new() -> Self {
        Self {
            allowed_directory: None,
            raw_allowed_directory: None,
            protected_patterns: Vec::new(),
            blocked_commands: Vec::new(),
        }
    }

    pub fn with_allowed_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        let raw: PathBuf = dir.into();
        let canonical = raw.canonicalize().unwrap_or_else(|_| raw.clone());
        self.allowed_directory = Some(canonical);
        self.raw_allowed_directory = Some(raw);
        self
    }

    pub fn with_protected_patterns(mut self, patterns: Vec<String>) -> Self {
        self.protected_patterns = patterns;
        self
    }

    pub fn with_blocked_commands(mut self, commands: Vec<String>) -> Self {
        self.blocked_commands = commands
            .iter()
            .filter_map(|p| glob::Pattern::new(p).ok())
            .collect();
        self
    }

    /// Default safety checker with common protections.
    pub fn default_checks() -> Self {
        Self::new()
            .with_protected_patterns(vec![".env".into(), ".env.*".into(), ".git/config".into()])
            .with_blocked_commands(vec![
                "rm -rf /*".into(),
                "sudo *".into(),
                "* > /dev/sd*".into(),
                "mkfs.*".into(),
                ":(){:|:&};:*".into(),
            ])
    }

    /// Check a tool call for safety violations.
    ///
    /// Returns `Permission::Allow` if safe, `Permission::Deny` if not.
    pub fn check(&self, tool_name: &str, args: &serde_json::Value) -> Permission {
        match tool_name {
            "bash" => {
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    return self.check_command(cmd);
                }
            }
            "write" | "edit" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    return self.check_path(path);
                }
            }
            _ => {}
        }
        Permission::Allow
    }

    /// Check if a path violates safety rules.
    pub fn check_path(&self, path: &str) -> Permission {
        if let Some(ref allowed) = self.allowed_directory {
            let canonical = match self.resolve_target(path) {
                Ok(p) => p,
                Err(reason) => {
                    return Permission::Deny(format!(
                        "path `{}` cannot be validated: {}",
                        path, reason
                    ));
                }
            };

            if !canonical.starts_with(allowed) {
                return Permission::Deny(format!(
                    "path `{}` is outside allowed directory `{}`",
                    path,
                    allowed.display()
                ));
            }
        }

        for pattern in &self.protected_patterns {
            if path_matches_pattern(path, pattern) {
                return Permission::Deny(format!(
                    "path `{}` matches protected pattern `{}`",
                    path, pattern
                ));
            }
        }

        Permission::Allow
    }

    /// Resolve `path` to a canonical absolute form that can be compared
    /// against `allowed_directory` with `Path::starts_with`. For paths
    /// that do not yet exist (e.g. new-file writes), the parent is
    /// canonicalized and the filename appended — this follows symlinks
    /// on the parent and normalizes `..` components.
    fn resolve_target(&self, path: &str) -> Result<PathBuf, String> {
        let target = Path::new(path);
        let base = self.raw_allowed_directory.as_deref();
        let absolute = if target.is_absolute() {
            target.to_path_buf()
        } else if let Some(base) = base {
            base.join(target)
        } else {
            target.to_path_buf()
        };

        if absolute.exists() {
            return absolute
                .canonicalize()
                .map_err(|e| format!("canonicalize failed: {e}"));
        }

        let parent = absolute.parent().ok_or("no parent directory")?;
        if !parent.exists() {
            return Err(format!("parent does not exist: {}", parent.display()));
        }
        let mut canonical = parent
            .canonicalize()
            .map_err(|e| format!("parent canonicalize failed: {e}"))?;
        if let Some(name) = absolute.file_name() {
            canonical.push(name);
        }
        Ok(canonical)
    }

    /// Check if a bash command matches any blocked glob pattern.
    pub fn check_command(&self, command: &str) -> Permission {
        let trimmed = command.trim();
        for pattern in &self.blocked_commands {
            if pattern.matches(trimmed) {
                return Permission::Deny(format!(
                    "command matches blocked pattern: `{}`",
                    pattern.as_str()
                ));
            }
        }
        Permission::Allow
    }
}

impl Default for SafetyChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple path matching against a pattern.
///
/// Supports:
/// - Exact filename match: `.env` matches any path ending in `.env`
/// - Prefix with wildcard: `.env.*` matches `.env.local`, `.env.production`
/// - Path prefix: `.git/config` matches paths containing `.git/config`
fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(".*") {
        // Pattern like ".env.*" — check if any path component starts with prefix + "."
        let target = format!("{}.", prefix);
        return path.contains(&target) || path.ends_with(prefix);
    }
    // Exact match on filename or path suffix
    path.ends_with(pattern)
}
