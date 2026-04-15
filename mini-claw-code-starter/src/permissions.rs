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
        unimplemented!("Use glob::Pattern to match tool_name against self.tool_pattern")
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
        unimplemented!("Store rules, default_permission, and empty session_allows HashSet")
    }

    pub fn ask_by_default(rules: Vec<PermissionRule>) -> Self {
        Self::new(rules, Permission::Ask)
    }

    pub fn allow_all() -> Self {
        Self::new(vec![], Permission::Allow)
    }

    pub fn evaluate(&self, tool_name: &str, _args: &Value) -> Permission {
        unimplemented!("Check session_allows, then rules in order, then default")
    }

    pub fn record_session_allow(&mut self, tool_name: &str) {
        unimplemented!("Insert tool_name into session_allows")
    }

    pub fn is_allowed(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Allow)
    }

    pub fn needs_approval(&self, tool_name: &str, args: &Value) -> bool {
        matches!(self.evaluate(tool_name, args), Permission::Ask)
    }
}
