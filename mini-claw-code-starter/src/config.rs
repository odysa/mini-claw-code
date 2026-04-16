use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Agent configuration with layered overrides.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: String,
    pub base_url: String,
    pub max_context_tokens: u64,
    pub preserve_recent: usize,
    pub allowed_directory: Option<String>,
    pub protected_patterns: Vec<String>,
    pub blocked_commands: Vec<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub hooks: HooksConfig,
    pub instructions: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "openrouter/free".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            max_context_tokens: 100_000,
            preserve_recent: 6,
            allowed_directory: None,
            protected_patterns: vec![".env".into(), ".env.*".into(), ".git/**".into()],
            blocked_commands: vec![
                "rm -rf /".into(),
                "sudo *".into(),
                "curl * | bash".into(),
                "curl * | sh".into(),
            ],
            mcp_servers: Vec::new(),
            hooks: HooksConfig::default(),
            instructions: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HooksConfig {
    pub pre_tool: Vec<ShellHookConfig>,
    pub post_tool: Vec<ShellHookConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellHookConfig {
    pub tool_pattern: Option<String>,
    pub command: String,
    #[serde(default = "default_hook_timeout")]
    pub timeout_ms: u64,
}

fn default_hook_timeout() -> u64 {
    5000
}

/// Loads and merges configuration from multiple sources.
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load config by merging default -> project -> user -> env layers.
    pub fn load() -> Config {
        let mut config = Config::default();

        if let Some(project_config) = Self::load_file(".mini-claw/config.toml") {
            Self::merge(&mut config, project_config);
        }

        if let Some(user_dir) = dirs::config_dir() {
            let user_path = user_dir.join("mini-claw/config.toml");
            if let Some(user_config) = Self::load_path(&user_path) {
                Self::merge(&mut config, user_config);
            }
        }

        if let Ok(model) = std::env::var("MINI_CLAW_MODEL") {
            config.model = model;
        }
        if let Ok(url) = std::env::var("MINI_CLAW_BASE_URL") {
            config.base_url = url;
        }
        if let Ok(tokens) = std::env::var("MINI_CLAW_MAX_TOKENS")
            && let Ok(n) = tokens.parse()
        {
            config.max_context_tokens = n;
        }

        config
    }

    /// Load config from a path.
    pub fn load_path(path: &Path) -> Option<Config> {
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    fn load_file(relative_path: &str) -> Option<Config> {
        let path = PathBuf::from(relative_path);
        Self::load_path(&path)
    }

    /// Merge overlay into base. Non-default values in overlay override base.
    fn merge(base: &mut Config, overlay: Config) {
        let defaults = Config::default();

        if overlay.model != defaults.model {
            base.model = overlay.model;
        }
        if overlay.base_url != defaults.base_url {
            base.base_url = overlay.base_url;
        }
        if overlay.max_context_tokens != defaults.max_context_tokens {
            base.max_context_tokens = overlay.max_context_tokens;
        }
        if overlay.preserve_recent != defaults.preserve_recent {
            base.preserve_recent = overlay.preserve_recent;
        }
        if overlay.allowed_directory.is_some() {
            base.allowed_directory = overlay.allowed_directory;
        }
        if !overlay.protected_patterns.is_empty()
            && overlay.protected_patterns != defaults.protected_patterns
        {
            base.protected_patterns = overlay.protected_patterns;
        }
        if !overlay.blocked_commands.is_empty()
            && overlay.blocked_commands != defaults.blocked_commands
        {
            base.blocked_commands = overlay.blocked_commands;
        }
        if !overlay.mcp_servers.is_empty() {
            base.mcp_servers = overlay.mcp_servers;
        }
        if overlay.instructions.is_some() {
            base.instructions = overlay.instructions;
        }
    }
}
