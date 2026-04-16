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

    pub fn matches(&self, tool_name: &str) -> bool {
        if let Ok(pattern) = glob::Pattern::new(&self.tool_pattern) {
            pattern.matches(tool_name)
        } else {
            self.tool_pattern == tool_name
        }
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

    pub fn evaluate(&self, tool_name: &str, _args: &Value) -> Permission {
        if self.session_allows.contains(tool_name) {
            return Permission::Allow;
        }

        for rule in &self.rules {
            if rule.matches(tool_name) {
                return rule.permission.clone();
            }
        }

        self.default_permission.clone()
    }

    pub fn record_session_allow(&mut self, tool_name: &str) {
        self.session_allows.insert(tool_name.to_string());
    }

    pub fn is_allowed(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Allow)
    }

    pub fn needs_approval(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Ask)
    }
}
