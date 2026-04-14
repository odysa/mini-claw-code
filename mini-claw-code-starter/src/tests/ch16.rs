use crate::config::{Config, ConfigLoader};

#[test]
fn test_ch16_default_config() {
    let config = Config::default();
    assert_eq!(config.model, "openrouter/free");
    assert_eq!(config.max_context_tokens, 100_000);
    assert_eq!(config.preserve_recent, 6);
    assert!(!config.protected_patterns.is_empty());
    assert!(!config.blocked_commands.is_empty());
}

#[test]
fn test_ch16_load_from_toml() {
    let toml_str = r#"
        model = "anthropic/claude-sonnet-4"
        max_context_tokens = 50000
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.model, "anthropic/claude-sonnet-4");
    assert_eq!(config.max_context_tokens, 50000);
}

#[test]
fn test_ch16_default_fills_missing_fields() {
    let toml_str = r#"
        model = "test-model"
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.model, "test-model");
    // Other fields should have defaults
    assert_eq!(config.preserve_recent, 6);
    assert!(config.instructions.is_none());
}

#[test]
fn test_ch16_load_nonexistent_path() {
    let result = ConfigLoader::load_path(std::path::Path::new("/tmp/__nonexistent_config__.toml"));
    assert!(result.is_none());
}

#[test]
fn test_ch16_mcp_server_config() {
    let toml_str = r#"
        [[mcp_servers]]
        name = "filesystem"
        command = "npx"
        args = ["@anthropic/mcp-server-filesystem"]
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.mcp_servers.len(), 1);
    assert_eq!(config.mcp_servers[0].name, "filesystem");
    assert_eq!(config.mcp_servers[0].command, "npx");
}

#[test]
fn test_ch16_hooks_config() {
    let toml_str = r#"
        [[hooks.pre_tool]]
        command = "echo pre"
        tool_pattern = "bash"
        timeout_ms = 3000
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.hooks.pre_tool.len(), 1);
    assert_eq!(config.hooks.pre_tool[0].command, "echo pre");
    assert_eq!(config.hooks.pre_tool[0].timeout_ms, 3000);
}

#[test]
fn test_ch16_env_override() {
    // In Rust 2024, set_var/remove_var are unsafe (they can cause UB
    // when another thread reads the env concurrently). This is fine in
    // a single-threaded test.
    unsafe {
        std::env::set_var("MINI_CLAW_MODEL", "test/env-model");
    }
    let config = ConfigLoader::load();
    assert_eq!(config.model, "test/env-model");
    unsafe {
        std::env::remove_var("MINI_CLAW_MODEL");
    }
}

#[test]
fn test_ch16_protected_patterns_default() {
    let config = Config::default();
    assert!(config.protected_patterns.contains(&".env".to_string()));
    assert!(config.protected_patterns.contains(&".git/**".to_string()));
}
