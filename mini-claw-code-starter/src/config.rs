use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Agent configuration with layered overrides.
///
/// # Chapter 14: Settings Hierarchy
///
/// Priority (highest to lowest):
/// 1. Environment variables
/// 2. User config (~/.config/mini-claw/config.toml)
/// 3. Project config (.mini-claw/config.toml)
/// 4. Defaults
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
///
/// # Chapter 14: Settings Hierarchy
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load config by merging default -> project -> user -> env layers.
    ///
    /// Hints:
    /// 1. Start with Config::default()
    /// 2. Try loading .mini-claw/config.toml (project config)
    /// 3. Try loading ~/.config/mini-claw/config.toml (user config) via dirs::config_dir()
    /// 4. Override with env vars: MINI_CLAW_MODEL, MINI_CLAW_BASE_URL, MINI_CLAW_MAX_TOKENS
    pub fn load() -> Config {
        unimplemented!("Merge layers: defaults -> project -> user -> env")
    }

    /// Load config from a path.
    pub fn load_path(path: &Path) -> Option<Config> {
        unimplemented!("Read file, parse as TOML, return Some(config) or None")
    }

    fn load_file(relative_path: &str) -> Option<Config> {
        let path = PathBuf::from(relative_path);
        Self::load_path(&path)
    }

    /// Merge overlay into base. Non-default values in overlay override base.
    fn merge(base: &mut Config, overlay: Config) {
        unimplemented!("Compare overlay fields against defaults, override base where different")
    }
}
