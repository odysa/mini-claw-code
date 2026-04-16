use serde_json::json;

use crate::permissions::*;

#[test]
fn test_ch19_allow_all() {
    let engine = PermissionEngine::allow_all();
    assert_eq!(engine.evaluate("bash", &json!({})), Permission::Allow);
    assert_eq!(engine.evaluate("write", &json!({})), Permission::Allow);
}

#[test]
fn test_ch19_ask_by_default() {
    let engine = PermissionEngine::ask_by_default(vec![]);
    assert_eq!(engine.evaluate("bash", &json!({})), Permission::Ask);
}

#[test]
fn test_ch19_rule_matching() {
    let rules = vec![
        PermissionRule::new("read", Permission::Allow),
        PermissionRule::new("bash", Permission::Ask),
        PermissionRule::new("write", Permission::Deny),
    ];
    let engine = PermissionEngine::new(rules, Permission::Ask);

    assert_eq!(engine.evaluate("read", &json!({})), Permission::Allow);
    assert_eq!(engine.evaluate("bash", &json!({})), Permission::Ask);
    assert_eq!(engine.evaluate("write", &json!({})), Permission::Deny);
}

#[test]
fn test_ch19_glob_pattern() {
    let rules = vec![PermissionRule::new("mcp__*", Permission::Ask)];
    let engine = PermissionEngine::new(rules, Permission::Allow);

    assert_eq!(
        engine.evaluate("mcp__fs__read", &json!({})),
        Permission::Ask
    );
    assert_eq!(engine.evaluate("read", &json!({})), Permission::Allow);
}

#[test]
fn test_ch19_first_rule_wins() {
    let rules = vec![
        PermissionRule::new("bash", Permission::Allow),
        PermissionRule::new("bash", Permission::Deny), // Should be ignored
    ];
    let engine = PermissionEngine::new(rules, Permission::Ask);

    assert_eq!(engine.evaluate("bash", &json!({})), Permission::Allow);
}

#[test]
fn test_ch19_session_allow() {
    let mut engine = PermissionEngine::ask_by_default(vec![]);

    // Initially asks
    assert_eq!(engine.evaluate("bash", &json!({})), Permission::Ask);

    // After recording session allow, should be allowed
    engine.record_session_allow("bash");
    assert_eq!(engine.evaluate("bash", &json!({})), Permission::Allow);
}

#[test]
fn test_ch19_session_allow_per_tool() {
    let mut engine = PermissionEngine::ask_by_default(vec![]);
    engine.record_session_allow("read");

    assert_eq!(engine.evaluate("read", &json!({})), Permission::Allow);
    assert_eq!(engine.evaluate("write", &json!({})), Permission::Ask);
}

#[test]
fn test_ch19_is_allowed() {
    let rules = vec![
        PermissionRule::new("read", Permission::Allow),
        PermissionRule::new("write", Permission::Deny),
    ];
    let engine = PermissionEngine::new(rules, Permission::Ask);

    assert!(engine.is_allowed("read", &json!({})));
    assert!(!engine.is_allowed("write", &json!({})));
    assert!(!engine.is_allowed("bash", &json!({})));
}

#[test]
fn test_ch19_needs_approval() {
    let rules = vec![PermissionRule::new("read", Permission::Allow)];
    let engine = PermissionEngine::new(rules, Permission::Ask);

    assert!(!engine.needs_approval("read", &json!({})));
    assert!(engine.needs_approval("bash", &json!({})));
}

#[test]
fn test_ch19_wildcard_rule() {
    let rules = vec![PermissionRule::new("*", Permission::Allow)];
    let engine = PermissionEngine::new(rules, Permission::Deny);

    assert_eq!(engine.evaluate("anything", &json!({})), Permission::Allow);
}

#[test]
fn test_ch19_deny_overrides_default() {
    let rules = vec![PermissionRule::new("dangerous", Permission::Deny)];
    let engine = PermissionEngine::new(rules, Permission::Allow);

    assert_eq!(engine.evaluate("dangerous", &json!({})), Permission::Deny);
    assert_eq!(engine.evaluate("safe", &json!({})), Permission::Allow);
}
