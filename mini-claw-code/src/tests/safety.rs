use serde_json::json;

use crate::safety::*;
use crate::types::Tool;

// -- PathValidator tests --

#[test]
fn test_safety_path_within_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.txt");
    std::fs::write(&file, "hello").unwrap();

    let validator = PathValidator::new(dir.path());
    assert!(validator.validate_path(file.to_str().unwrap()).is_ok());
}

#[test]
fn test_safety_path_outside_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let validator = PathValidator::new(dir.path());
    assert!(validator.validate_path("/etc/passwd").is_err());
}

#[test]
fn test_safety_path_traversal_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();

    let validator = PathValidator::new(&sub);
    let traversal = format!("{}/../../../etc/passwd", sub.display());
    assert!(validator.validate_path(&traversal).is_err());
}

#[test]
fn test_safety_path_new_file_in_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let validator = PathValidator::new(dir.path());
    let new_file = dir.path().join("new.txt");
    assert!(validator.validate_path(new_file.to_str().unwrap()).is_ok());
}

#[test]
fn test_safety_safety_check_read_tool() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ok.txt");
    std::fs::write(&file, "ok").unwrap();

    let validator = PathValidator::new(dir.path());
    assert!(
        validator
            .check("read", &json!({"path": file.to_str().unwrap()}))
            .is_ok()
    );
}

#[test]
fn test_safety_safety_check_ignores_bash() {
    let dir = tempfile::tempdir().unwrap();
    let validator = PathValidator::new(dir.path());
    // Bash tool doesn't have a path arg, should be ignored
    assert!(validator.check("bash", &json!({"command": "ls"})).is_ok());
}

// -- CommandFilter tests --

#[test]
fn test_safety_command_filter_blocks_rm_rf() {
    let filter = CommandFilter::default_filters();
    assert!(filter.is_blocked("rm -rf /").is_some());
    assert!(filter.is_blocked("rm -rf /*").is_some());
}

#[test]
fn test_safety_command_filter_blocks_sudo() {
    let filter = CommandFilter::default_filters();
    assert!(filter.is_blocked("sudo rm file").is_some());
}

#[test]
fn test_safety_command_filter_allows_safe() {
    let filter = CommandFilter::default_filters();
    assert!(filter.is_blocked("ls -la").is_none());
    assert!(filter.is_blocked("echo hello").is_none());
    assert!(filter.is_blocked("cargo test").is_none());
}

#[test]
fn test_safety_command_filter_safety_check() {
    let filter = CommandFilter::default_filters();
    assert!(
        filter
            .check("bash", &json!({"command": "sudo reboot"}))
            .is_err()
    );
    assert!(
        filter
            .check("bash", &json!({"command": "echo safe"}))
            .is_ok()
    );
}

// -- ProtectedFileCheck tests --

#[test]
fn test_safety_protected_file_blocks_env() {
    let check = ProtectedFileCheck::new(&[".env".into(), ".env.*".into()]);
    assert!(check.check("write", &json!({"path": ".env"})).is_err());
    assert!(
        check
            .check("write", &json!({"path": ".env.local"}))
            .is_err()
    );
}

#[test]
fn test_safety_protected_file_allows_normal() {
    let check = ProtectedFileCheck::new(&[".env".into()]);
    assert!(
        check
            .check("write", &json!({"path": "src/main.rs"}))
            .is_ok()
    );
}

// -- SafeToolWrapper tests --

#[tokio::test]
async fn test_safety_wrapper_blocks_on_check_failure() {
    use crate::tools::ReadTool;

    let dir = tempfile::tempdir().unwrap();
    let validator = PathValidator::new(dir.path());

    let tool: Box<dyn Tool> = Box::new(ReadTool::new());
    let wrapped = SafeToolWrapper::with_check(tool, validator);

    // Try to read a file outside the allowed directory
    let result = Tool::call(&wrapped, json!({"path": "/etc/passwd"}))
        .await
        .unwrap();
    assert!(result.content.contains("safety check failed"));
}

#[tokio::test]
async fn test_safety_wrapper_allows_valid_call() {
    use crate::tools::ReadTool;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("allowed.txt");
    std::fs::write(&file, "content").unwrap();

    let validator = PathValidator::new(dir.path());
    let tool: Box<dyn Tool> = Box::new(ReadTool::new());
    let wrapped = SafeToolWrapper::with_check(tool, validator);

    let result = Tool::call(&wrapped, json!({"path": file.to_str().unwrap()}))
        .await
        .unwrap();
    assert_eq!(result.content, "content");
}

#[test]
fn test_safety_custom_blocked_commands() {
    let filter = CommandFilter::new(&["docker rm *".into(), "npm publish*".into()]);
    assert!(filter.is_blocked("docker rm container").is_some());
    assert!(filter.is_blocked("npm publish").is_some());
    assert!(filter.is_blocked("docker ps").is_none());
}
