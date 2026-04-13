use crate::permission::PermissionEngine;
use crate::types::*;

// ---------------------------------------------------------------------------
// Helpers: minimal Tool impls for testing permission decisions
// ---------------------------------------------------------------------------

struct ReadOnlyTool;

#[async_trait::async_trait]
impl Tool for ReadOnlyTool {
    fn definition(&self) -> &ToolDefinition {
        // We only need the flags, not the full definition, so use a leak trick
        // to get a static reference for testing.
        static DEF: std::sync::OnceLock<ToolDefinition> = std::sync::OnceLock::new();
        DEF.get_or_init(|| ToolDefinition::new("read", "Read a file"))
    }
    async fn call(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::text(""))
    }
    fn is_read_only(&self) -> bool {
        true
    }
}

struct WriteTool;

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: std::sync::OnceLock<ToolDefinition> = std::sync::OnceLock::new();
        DEF.get_or_init(|| ToolDefinition::new("write", "Write a file"))
    }
    async fn call(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::text(""))
    }
}

struct DestructiveTool;

#[async_trait::async_trait]
impl Tool for DestructiveTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: std::sync::OnceLock<ToolDefinition> = std::sync::OnceLock::new();
        DEF.get_or_init(|| ToolDefinition::new("bash", "Run a command"))
    }
    async fn call(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::text(""))
    }
    fn is_destructive(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// PermissionMode tests
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_bypass_allows_everything() {
    let engine = PermissionEngine::new(PermissionMode::Bypass);
    let tool = DestructiveTool;
    let (perm, source) = engine.check("bash", &tool);
    assert_eq!(perm, Permission::Allow);
    assert!(matches!(
        source,
        PermissionSource::Mode(PermissionMode::Bypass)
    ));
}

#[test]
fn test_ch10_dontask_denies_everything() {
    let engine = PermissionEngine::new(PermissionMode::DontAsk);
    let tool = ReadOnlyTool;
    let (perm, _) = engine.check("read", &tool);
    assert!(matches!(perm, Permission::Deny(_)));
}

#[test]
fn test_ch10_plan_allows_read_only() {
    let engine = PermissionEngine::new(PermissionMode::Plan);
    let read = ReadOnlyTool;
    let (perm, _) = engine.check("read", &read);
    assert_eq!(perm, Permission::Allow);
}

#[test]
fn test_ch10_plan_denies_write() {
    let engine = PermissionEngine::new(PermissionMode::Plan);
    let write = WriteTool;
    let (perm, _) = engine.check("write", &write);
    assert!(matches!(perm, Permission::Deny(_)));
}

#[test]
fn test_ch10_plan_denies_destructive() {
    let engine = PermissionEngine::new(PermissionMode::Plan);
    let bash = DestructiveTool;
    let (perm, _) = engine.check("bash", &bash);
    assert!(matches!(perm, Permission::Deny(_)));
}

// ---------------------------------------------------------------------------
// Auto mode
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_auto_allows_read_only() {
    let engine = PermissionEngine::new(PermissionMode::Auto);
    let tool = ReadOnlyTool;
    let (perm, _) = engine.check("read", &tool);
    assert_eq!(perm, Permission::Allow);
}

#[test]
fn test_ch10_auto_allows_non_destructive_write() {
    let engine = PermissionEngine::new(PermissionMode::Auto);
    let tool = WriteTool;
    let (perm, _) = engine.check("write", &tool);
    assert_eq!(perm, Permission::Allow);
}

#[test]
fn test_ch10_auto_asks_for_destructive() {
    let engine = PermissionEngine::new(PermissionMode::Auto);
    let tool = DestructiveTool;
    let (perm, _) = engine.check("bash", &tool);
    assert!(matches!(perm, Permission::Ask(_)));
}

// ---------------------------------------------------------------------------
// Default mode
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_default_allows_read_only() {
    let engine = PermissionEngine::new(PermissionMode::Default);
    let tool = ReadOnlyTool;
    let (perm, _) = engine.check("read", &tool);
    assert_eq!(perm, Permission::Allow);
}

#[test]
fn test_ch10_default_asks_for_write() {
    let engine = PermissionEngine::new(PermissionMode::Default);
    let tool = WriteTool;
    let (perm, _) = engine.check("write", &tool);
    assert!(matches!(perm, Permission::Ask(_)));
}

#[test]
fn test_ch10_default_asks_for_destructive() {
    let engine = PermissionEngine::new(PermissionMode::Default);
    let tool = DestructiveTool;
    let (perm, _) = engine.check("bash", &tool);
    assert!(matches!(perm, Permission::Ask(_)));
}

// ---------------------------------------------------------------------------
// Permission rules
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_rule_allow_overrides_default() {
    let engine = PermissionEngine::new(PermissionMode::Default).with_rules(vec![PermissionRule {
        tool_pattern: "write".into(),
        behavior: PermissionBehavior::Allow,
    }]);
    let tool = WriteTool;
    let (perm, source) = engine.check("write", &tool);
    assert_eq!(perm, Permission::Allow);
    assert!(matches!(source, PermissionSource::Rule(_)));
}

#[test]
fn test_ch10_rule_deny_overrides_auto() {
    let engine = PermissionEngine::new(PermissionMode::Auto).with_rules(vec![PermissionRule {
        tool_pattern: "write".into(),
        behavior: PermissionBehavior::Deny,
    }]);
    let tool = WriteTool;
    let (perm, _) = engine.check("write", &tool);
    assert!(matches!(perm, Permission::Deny(_)));
}

#[test]
fn test_ch10_wildcard_rule() {
    let engine = PermissionEngine::new(PermissionMode::Default).with_rules(vec![PermissionRule {
        tool_pattern: "*".into(),
        behavior: PermissionBehavior::Allow,
    }]);
    let tool = DestructiveTool;
    let (perm, _) = engine.check("bash", &tool);
    assert_eq!(perm, Permission::Allow);
}

#[test]
fn test_ch10_prefix_wildcard_rule() {
    let engine = PermissionEngine::new(PermissionMode::Default).with_rules(vec![PermissionRule {
        tool_pattern: "file_*".into(),
        behavior: PermissionBehavior::Allow,
    }]);

    // Matches
    let tool = ReadOnlyTool;
    let (perm, _) = engine.check("file_read", &tool);
    assert_eq!(perm, Permission::Allow);

    // Does not match
    let (perm, _) = engine.check("bash", &DestructiveTool);
    assert!(matches!(perm, Permission::Ask(_)));
}

#[test]
fn test_ch10_first_rule_wins() {
    let engine = PermissionEngine::new(PermissionMode::Default).with_rules(vec![
        PermissionRule {
            tool_pattern: "bash".into(),
            behavior: PermissionBehavior::Deny,
        },
        PermissionRule {
            tool_pattern: "*".into(),
            behavior: PermissionBehavior::Allow,
        },
    ]);
    let tool = DestructiveTool;
    let (perm, _) = engine.check("bash", &tool);
    assert!(matches!(perm, Permission::Deny(_)));
}

// ---------------------------------------------------------------------------
// Session approvals
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_session_approval() {
    let engine = PermissionEngine::new(PermissionMode::Default);
    let tool = WriteTool;

    // Before approval — asks
    let (perm, _) = engine.check("write", &tool);
    assert!(matches!(perm, Permission::Ask(_)));

    // Approve for session
    engine.approve_session("write");
    assert!(engine.is_session_approved("write"));

    // After approval — allowed
    let (perm, source) = engine.check("write", &tool);
    assert_eq!(perm, Permission::Allow);
    assert!(matches!(source, PermissionSource::Session));
}

#[test]
fn test_ch10_session_approval_clear() {
    let engine = PermissionEngine::new(PermissionMode::Default);
    engine.approve_session("write");
    assert!(engine.is_session_approved("write"));

    engine.clear_session();
    assert!(!engine.is_session_approved("write"));
}

#[test]
fn test_ch10_session_approval_does_not_cross_tools() {
    let engine = PermissionEngine::new(PermissionMode::Default);
    engine.approve_session("write");

    assert!(engine.is_session_approved("write"));
    assert!(!engine.is_session_approved("bash"));
}

// ---------------------------------------------------------------------------
// Permission hierarchy summary table
// ---------------------------------------------------------------------------

#[test]
fn test_ch10_permission_hierarchy() {
    // This test verifies the complete permission table from Chapter 9:
    //
    // | Category    | Plan mode | Auto-approve | Default mode |
    // |-------------|-----------|--------------|--------------|
    // | Read-only   | Allowed   | Allowed      | Allowed      |
    // | Write       | Denied    | Allowed      | Ask user     |
    // | Destructive | Denied    | Ask user     | Ask user     |

    let read = ReadOnlyTool;
    let write = WriteTool;
    let bash = DestructiveTool;

    // Plan mode
    let plan = PermissionEngine::new(PermissionMode::Plan);
    assert_eq!(plan.check("read", &read).0, Permission::Allow);
    assert!(matches!(plan.check("write", &write).0, Permission::Deny(_)));
    assert!(matches!(plan.check("bash", &bash).0, Permission::Deny(_)));

    // Auto mode
    let auto = PermissionEngine::new(PermissionMode::Auto);
    assert_eq!(auto.check("read", &read).0, Permission::Allow);
    assert_eq!(auto.check("write", &write).0, Permission::Allow);
    assert!(matches!(auto.check("bash", &bash).0, Permission::Ask(_)));

    // Default mode
    let default = PermissionEngine::new(PermissionMode::Default);
    assert_eq!(default.check("read", &read).0, Permission::Allow);
    assert!(matches!(
        default.check("write", &write).0,
        Permission::Ask(_)
    ));
    assert!(matches!(default.check("bash", &bash).0, Permission::Ask(_)));
}
