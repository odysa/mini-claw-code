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
    pub fn matches(&self, tool_name: &str) -> bool {
        if let Ok(pattern) = glob::Pattern::new(&self.tool_pattern) {
            pattern.matches(tool_name)
        } else {
            self.tool_pattern == tool_name
        }
    }
}

/// Evaluates permission rules to decide whether a tool call should proceed.
///
/// Rules are evaluated in order. The first matching rule wins. If no rule
/// matches, the default permission applies.
pub struct PermissionEngine {
    rules: Vec<PermissionRule>,
    default_permission: Permission,
    /// Session-level overrides (tool calls the user has already approved).
    session_allows: std::collections::HashSet<String>,
}

impl PermissionEngine {
    pub fn new(rules: Vec<PermissionRule>, default_permission: Permission) -> Self {
        Self {
            rules,
            default_permission,
            session_allows: std::collections::HashSet::new(),
        }
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
    /// Returns the permission decision. If the result is `Ask`, the caller
    /// should prompt the user and then call `record_session_allow` if approved.
    pub fn evaluate(&self, tool_name: &str, _args: &Value) -> Permission {
        // Check session-level overrides first
        if self.session_allows.contains(tool_name) {
            return Permission::Allow;
        }

        // Check rules in order
        for rule in &self.rules {
            if rule.matches(tool_name) {
                return rule.permission.clone();
            }
        }

        self.default_permission.clone()
    }

    /// Record that the user approved a tool for this session.
    pub fn record_session_allow(&mut self, tool_name: &str) {
        self.session_allows.insert(tool_name.to_string());
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
