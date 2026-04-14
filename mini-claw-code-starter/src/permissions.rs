use serde_json::Value;

/// A permission decision for a tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    /// Tool call is allowed without asking.
    Allow,
    /// Tool call is blocked without asking.
    Deny,
    /// User must be prompted for approval.
    Ask,
}

/// A rule that matches tool calls and assigns a permission.
#[derive(Debug, Clone)]
pub struct PermissionRule {
    /// Glob pattern matching tool names (e.g. "bash", "write", "*").
    pub tool_pattern: String,
    /// The permission to assign when the pattern matches.
    pub permission: Permission,
}

impl PermissionRule {
    pub fn new(tool_pattern: impl Into<String>, permission: Permission) -> Self {
        Self {
            tool_pattern: tool_pattern.into(),
            permission,
        }
    }

    /// Check if this rule matches a tool name.
    ///
    /// # Chapter 10: Permission Engine
    ///
    /// Hint: Use `glob::Pattern::new(&self.tool_pattern)` for pattern matching.
    /// Fall back to exact string comparison if the pattern is invalid.
    pub fn matches(&self, tool_name: &str) -> bool {
        unimplemented!("Use glob::Pattern to match tool_name against self.tool_pattern")
    }
}

/// Evaluates permission rules to decide whether a tool call should proceed.
///
/// Rules are evaluated in order. The first matching rule wins. If no rule
/// matches, the default permission applies.
///
/// # Chapter 10: Permission Engine
pub struct PermissionEngine {
    rules: Vec<PermissionRule>,
    default_permission: Permission,
    /// Session-level overrides (tool calls the user has already approved).
    session_allows: std::collections::HashSet<String>,
}

impl PermissionEngine {
    pub fn new(rules: Vec<PermissionRule>, default_permission: Permission) -> Self {
        unimplemented!("Store rules, default_permission, and empty session_allows HashSet")
    }

    /// Create an engine that asks for everything by default.
    pub fn ask_by_default(rules: Vec<PermissionRule>) -> Self {
        Self::new(rules, Permission::Ask)
    }

    /// Create an engine that allows everything (no permission checks).
    pub fn allow_all() -> Self {
        Self::new(vec![], Permission::Allow)
    }

    /// Evaluate permission for a tool call.
    ///
    /// Hints:
    /// 1. Check session_allows first — if tool_name is in the set, return Allow
    /// 2. Check rules in order — first matching rule wins
    /// 3. If no rule matches, return self.default_permission
    pub fn evaluate(&self, tool_name: &str, _args: &Value) -> Permission {
        unimplemented!("Check session_allows, then rules in order, then default")
    }

    /// Record that the user approved a tool for this session.
    pub fn record_session_allow(&mut self, tool_name: &str) {
        unimplemented!("Insert tool_name into session_allows")
    }

    /// Check if a tool is allowed (returns true for Allow, false for Deny/Ask).
    pub fn is_allowed(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Allow)
    }

    /// Check if a tool requires user approval.
    pub fn needs_approval(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Ask)
    }
}
