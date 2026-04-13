use serde_json::json;

use crate::permission::SafetyChecker;
use crate::types::Permission;

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

#[test]
fn test_ch11_path_inside_allowed_directory() {
    let checker = SafetyChecker::new().with_allowed_directory("/home/user/project");
    let result = checker.check_path("/home/user/project/src/main.rs");
    assert_eq!(result, Permission::Allow);
}

#[test]
fn test_ch11_path_outside_allowed_directory() {
    let checker = SafetyChecker::new().with_allowed_directory("/home/user/project");
    let result = checker.check_path("/etc/passwd");
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_no_allowed_directory_allows_all() {
    let checker = SafetyChecker::new();
    let result = checker.check_path("/etc/passwd");
    assert_eq!(result, Permission::Allow);
}

// ---------------------------------------------------------------------------
// Protected patterns
// ---------------------------------------------------------------------------

#[test]
fn test_ch11_protected_env_file() {
    let checker = SafetyChecker::new().with_protected_patterns(vec![".env".into()]);
    let result = checker.check_path("/home/user/project/.env");
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_protected_env_wildcard() {
    let checker = SafetyChecker::new().with_protected_patterns(vec![".env.*".into()]);
    let result = checker.check_path("/home/user/project/.env.local");
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_protected_git_config() {
    let checker = SafetyChecker::new().with_protected_patterns(vec![".git/config".into()]);
    let result = checker.check_path("/home/user/project/.git/config");
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_unprotected_file_allowed() {
    let checker =
        SafetyChecker::new().with_protected_patterns(vec![".env".into(), ".git/config".into()]);
    let result = checker.check_path("/home/user/project/src/main.rs");
    assert_eq!(result, Permission::Allow);
}

// ---------------------------------------------------------------------------
// Command validation
// ---------------------------------------------------------------------------

#[test]
fn test_ch11_blocked_rm_rf() {
    let checker = SafetyChecker::new().with_blocked_commands(vec!["rm -rf /".into()]);
    let result = checker.check_command("rm -rf /");
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_blocked_sudo() {
    let checker = SafetyChecker::new().with_blocked_commands(vec!["sudo ".into()]);
    let result = checker.check_command("sudo rm -rf /tmp");
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_allowed_command() {
    let checker =
        SafetyChecker::new().with_blocked_commands(vec!["rm -rf /".into(), "sudo ".into()]);
    let result = checker.check_command("ls -la");
    assert_eq!(result, Permission::Allow);
}

#[test]
fn test_ch11_blocked_fork_bomb() {
    let checker = SafetyChecker::default_checks();
    let result = checker.check_command(":(){:|:&};: && echo done");
    // The fork bomb pattern is a substring match
    assert!(matches!(result, Permission::Deny(_)));
}

// ---------------------------------------------------------------------------
// Integrated check() method
// ---------------------------------------------------------------------------

#[test]
fn test_ch11_check_bash_tool() {
    let checker = SafetyChecker::default_checks();
    let result = checker.check("bash", &json!({"command": "rm -rf /"}));
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_check_bash_safe_command() {
    let checker = SafetyChecker::default_checks();
    let result = checker.check("bash", &json!({"command": "cargo test"}));
    assert_eq!(result, Permission::Allow);
}

#[test]
fn test_ch11_check_write_protected_file() {
    let checker = SafetyChecker::default_checks();
    let result = checker.check("write", &json!({"path": "/project/.env"}));
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_check_write_safe_file() {
    let checker = SafetyChecker::default_checks();
    let result = checker.check("write", &json!({"path": "/project/src/main.rs"}));
    assert_eq!(result, Permission::Allow);
}

#[test]
fn test_ch11_check_edit_protected_file() {
    let checker = SafetyChecker::default_checks();
    let result = checker.check("edit", &json!({"path": "/project/.env.local"}));
    assert!(matches!(result, Permission::Deny(_)));
}

#[test]
fn test_ch11_check_read_tool_not_checked() {
    // Read tools are not subject to safety checks on path
    let checker = SafetyChecker::default_checks();
    let result = checker.check("read", &json!({"path": "/project/.env"}));
    assert_eq!(result, Permission::Allow);
}

#[test]
fn test_ch11_default_checks_has_protections() {
    let checker = SafetyChecker::default_checks();

    // Has blocked commands
    assert!(matches!(
        checker.check_command("sudo apt install"),
        Permission::Deny(_)
    ));

    // Has protected patterns
    assert!(matches!(
        checker.check_path("/project/.env"),
        Permission::Deny(_)
    ));
}

#[test]
fn test_ch11_combined_directory_and_pattern() {
    let checker = SafetyChecker::new()
        .with_allowed_directory("/home/user/project")
        .with_protected_patterns(vec![".env".into()]);

    // Inside directory, not protected — allowed
    assert_eq!(
        checker.check_path("/home/user/project/src/main.rs"),
        Permission::Allow
    );

    // Outside directory — denied (directory check first)
    assert!(matches!(
        checker.check_path("/etc/passwd"),
        Permission::Deny(_)
    ));

    // Inside directory, but protected — denied
    assert!(matches!(
        checker.check_path("/home/user/project/.env"),
        Permission::Deny(_)
    ));
}
