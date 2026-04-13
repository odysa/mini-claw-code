/// Permission decision for a tool call.
///
/// Mirrors Claude Code's multi-level permission system.
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    /// Tool call is allowed without asking.
    Allow,
    /// Tool call is blocked.
    Deny(String),
    /// User must be prompted for approval.
    Ask(String),
}

/// Permission mode controlling the overall behavior.
///
/// Mirrors Claude Code's PermissionMode.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    /// Interactive prompts for unrecognized operations.
    Default,
    /// Auto-approve based on classifier confidence.
    Auto,
    /// Skip all permission checks.
    Bypass,
    /// Only allow read-only operations.
    Plan,
    /// Deny everything without prompting.
    DontAsk,
}

/// A rule that matches tool calls and assigns a permission.
#[derive(Debug, Clone)]
pub struct PermissionRule {
    /// Glob pattern matching tool names.
    pub tool_pattern: String,
    /// The behavior when matched.
    pub behavior: PermissionBehavior,
}

/// What to do when a permission rule matches.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// The source of a permission decision.
#[derive(Debug, Clone)]
pub enum PermissionSource {
    Rule(PermissionRule),
    Mode(PermissionMode),
    Hook(String),
    Safety(String),
    Session,
}
