use serde_json::Value;

/// A permission decision for a tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    Allow,
    Deny,
    Ask,
}

/// A rule that matches tool calls and assigns a permission.
#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub tool_pattern: String,
    pub permission: Permission,
}

impl PermissionRule {
    pub fn new(tool_pattern: impl Into<String>, permission: Permission) -> Self {
        Self {
            tool_pattern: tool_pattern.into(),
            permission,
        }
    }

    /// Check whether this rule matches the given tool name.
    ///
    /// Hint: Parse `self.tool_pattern` as a `glob::Pattern` (for wildcards like
    /// `"bash:*"`). Fall back to exact equality if the pattern is invalid.
    pub fn matches(&self, _tool_name: &str) -> bool {
        unimplemented!("TODO ch13: glob-match tool_pattern against tool_name (exact fallback)")
    }
}

/// Evaluates permission rules to decide whether a tool call should proceed.
pub struct PermissionEngine {
    rules: Vec<PermissionRule>,
    default_permission: Permission,
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

    pub fn ask_by_default(rules: Vec<PermissionRule>) -> Self {
        Self::new(rules, Permission::Ask)
    }

    pub fn allow_all() -> Self {
        Self::new(vec![], Permission::Allow)
    }

    /// Decide the permission for a tool call.
    ///
    /// Hints:
    /// - If `session_allows` already contains `tool_name`, return `Allow`.
    /// - Otherwise iterate `rules`; the first rule that matches wins.
    /// - Fall back to `self.default_permission`.
    pub fn evaluate(&self, _tool_name: &str, _args: &Value) -> Permission {
        unimplemented!("TODO ch13: session_allow → first matching rule → default_permission")
    }

    /// Remember that the user granted session-wide allow for this tool.
    pub fn record_session_allow(&mut self, _tool_name: &str) {
        unimplemented!("TODO ch13: insert tool_name into self.session_allows")
    }

    pub fn is_allowed(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Allow)
    }

    pub fn needs_approval(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Ask)
    }
}
